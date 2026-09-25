#!/usr/bin/env python3
"""Translates Ollama's `/api/chat` and `/api/generate` wire format to llama-server's
OpenAI-compatible `/v1/chat/completions`, so `backend/src/services/llm.rs`'s
`OllamaService` can talk to `llama-mtp` without any change to that module — see
`llm/compose.yaml`'s `mtp-proxy`/`llama-mtp` services and project memory for why
this exists. Stdlib-only on purpose: this is a thin, low-throughput (`llama-mtp`
itself runs `-np 1`, one request at a time) JSON-reshaping shim, not a
performance-sensitive service, so it isn't worth a dependency (FastAPI/etc) to
maintain.

Translation contract, confirmed empirically against a real `llama-mtp` instance
before writing this (see project memory for the exact test transcripts), not
assumed from docs alone:
  - `think: bool` -> `chat_template_kwargs.enable_thinking` (bool).
  - `options.num_predict` -> `max_tokens` (confirmed this exact field name caps
    generation, via a request capped at 5 tokens that came back at exactly 5).
  - `tools` passes through unchanged — Ollama's tool-definition JSON schema is
    already identical to OpenAI's (confirmed by sending our own Ollama-shaped
    tool defs straight through and getting a real tool call back).
  - Outgoing tool-call arguments (Ollama's `Value`, already a real JSON
    object/array/etc, since `backend/src/services/llm.rs`'s `OllamaToolCall`
    types it that way) get `json.dumps`'d into the JSON *string* OpenAI expects.
    Incoming tool-call arguments (confirmed via a real tool-call response: a JSON
    *string* like `"{\"city\":\"Paris\"}"`) get `json.loads`'d back into a real
    object, since Ollama's own `OllamaToolCall.arguments: serde_json::Value` is a
    real value, not a string-wrapping-JSON — sending a string there would let it
    deserialize into `Value::String(...)` instead of an object, breaking whatever
    downstream code expects the tool's actual argument fields.
  - `tool_call_id` (which OpenAI's tool-result messages require, keyed to a
    preceding assistant message's `tool_calls[].id`) is synthesized per assistant
    message here, matched to following `tool`-role messages purely by order. This
    is safe specifically because `backend/src/facade/agent.rs`'s
    `to_ollama_message` already discards Ollama's own per-call `id` on replay
    (leaves it `""`) and matches tool results by `tool_name` + position instead
    (see that function's own doc comment) — so the backend never round-trips or
    checks a real id, only order, which this proxy's synthesized ids preserve.
  - Images: Ollama's `images: [base64, ...]` (no data-URL prefix) -> OpenAI's
    multi-part `content: [{type:"text",...}, {type:"image_url",
    image_url:{url:"data:image/png;base64,..."}}]` — confirmed working through a
    real `--mmproj`-loaded instance with a real test image before wiring this in.
  - Duration fields: llama-server's `timings.{prompt,predicted}_ms` (float
    milliseconds) -> Ollama's `{prompt_eval,eval}_duration` (integer
    nanoseconds — `backend/src/services/llm.rs`'s `log_ollama_metrics` divides by
    1e9 to get seconds, so this must be ns, not ms or s).
"""

from __future__ import annotations

import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPSTREAM = os.environ.get("LLAMA_SERVER_URL", "http://llama-mtp:8080")
LISTEN_PORT = int(os.environ.get("PORT", "8080"))

# Generous, not the real safety net — `max_tokens` (translated from the
# backend's own context-derived cap) and the server's own `-n` fallback are what
# actually bound a generation. This just needs to outlast any legitimate one.
UPSTREAM_TIMEOUT_SECONDS = 4 * 60 * 60


def _now_iso() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def call_upstream(payload: dict) -> dict:
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        f"{UPSTREAM}/v1/chat/completions",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=UPSTREAM_TIMEOUT_SECONDS) as resp:
        return json.loads(resp.read())


def build_openai_messages(messages: list[dict]) -> list[dict]:
    """Ollama-shaped history/messages -> OpenAI-shaped. See module docstring for
    the `tool_call_id` synthesis/matching rule — `pending_tool_ids` here is that
    per-assistant-message queue, consumed in order by the `tool`-role messages
    that follow it, exactly mirroring how the Rust side itself matches them."""
    out: list[dict] = []
    pending_tool_ids: list[str] = []

    for idx, msg in enumerate(messages):
        role = msg.get("role", "user")

        if role == "tool":
            tool_call_id = pending_tool_ids.pop(0) if pending_tool_ids else f"call_unmatched_{idx}"
            out.append({"role": "tool", "tool_call_id": tool_call_id, "content": msg.get("content", "")})
            continue

        content = msg.get("content", "")
        images = msg.get("images")
        if images:
            parts = [{"type": "text", "text": content}] if content else []
            for b64 in images:
                parts.append({"type": "image_url", "image_url": {"url": f"data:image/png;base64,{b64}"}})
            out_msg: dict = {"role": role, "content": parts}
        else:
            out_msg = {"role": role, "content": content}

        tool_calls = msg.get("tool_calls")
        if tool_calls:
            synthesized = []
            pending_tool_ids = []
            for i, tc in enumerate(tool_calls):
                tool_call_id = f"call_{idx}_{i}"
                fn = tc.get("function", {})
                synthesized.append(
                    {
                        "id": tool_call_id,
                        "type": "function",
                        "function": {
                            "name": fn.get("name", ""),
                            "arguments": json.dumps(fn.get("arguments", {})),
                        },
                    }
                )
                pending_tool_ids.append(tool_call_id)
            out_msg["tool_calls"] = synthesized

        out.append(out_msg)

    return out


def parse_tool_call_arguments(raw):
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        # Malformed/non-JSON arguments from the model — pass the raw string
        # through rather than guessing at a fix. Downstream tool-arg
        # deserialization on the Rust side will then fail loudly with a real
        # error, matching this project's established "fail loud, don't guess"
        # convention rather than silently swallowing a bad tool call.
        return raw


def openai_message_to_ollama(message: dict) -> dict:
    result: dict = {
        "role": message.get("role", "assistant"),
        "content": message.get("content") or "",
    }
    reasoning = message.get("reasoning_content")
    if reasoning:
        result["thinking"] = reasoning

    tool_calls = message.get("tool_calls")
    if tool_calls:
        result["tool_calls"] = [
            {
                "id": tc.get("id") or f"call_{i}",
                "function": {
                    "name": tc.get("function", {}).get("name", ""),
                    "arguments": parse_tool_call_arguments(tc.get("function", {}).get("arguments", "{}")),
                },
            }
            for i, tc in enumerate(tool_calls)
        ]

    return result


def metrics_fields(openai_resp: dict) -> dict:
    usage = openai_resp.get("usage") or {}
    timings = openai_resp.get("timings") or {}
    prompt_ms = timings.get("prompt_ms", 0.0)
    predicted_ms = timings.get("predicted_ms", 0.0)
    return {
        "prompt_eval_count": usage.get("prompt_tokens"),
        "prompt_eval_duration": int(prompt_ms * 1e6),
        "eval_count": usage.get("completion_tokens"),
        "eval_duration": int(predicted_ms * 1e6),
        "total_duration": int((prompt_ms + predicted_ms) * 1e6),
        "load_duration": 0,
    }


def build_upstream_payload(model: str, messages: list[dict], think, num_predict, tools=None) -> dict:
    # `think` mirrors the backend's own `ThinkChoice` (backend/src/services/llm.rs):
    # `False` disables thinking; `True` enables it at the model's own default effort
    # (no `reasoning_effort` sent — this is deliberate, not an oversight: forcing a
    # default here was tried and reverted, see project memory for why hardcoding one
    # was wrong); a string (e.g. `"low"`) requests that specific effort level,
    # exactly as sent — never rewritten to a different value. The frontend's
    # thinking-mode selector is what actually produces a string here, populated from
    # `GET /api/llm/thinking_capability`, which itself parses this same model's real
    # chat template rather than assuming any particular level exists.
    chat_template_kwargs: dict = {"enable_thinking": bool(think)}
    if isinstance(think, str):
        chat_template_kwargs["reasoning_effort"] = think

    payload: dict = {
        "model": model,
        "stream": False,
        "messages": messages,
        "chat_template_kwargs": chat_template_kwargs,
    }
    if num_predict is not None:
        payload["max_tokens"] = num_predict
    if tools:
        payload["tools"] = tools
    return payload


def handle_chat(body: dict) -> dict:
    messages = build_openai_messages(body.get("messages", []))
    options = body.get("options") or {}
    payload = build_upstream_payload(
        model=body.get("model", "local"),
        messages=messages,
        think=body.get("think", True),
        num_predict=options.get("num_predict"),
        tools=body.get("tools"),
    )

    openai_resp = call_upstream(payload)
    choice = (openai_resp.get("choices") or [{}])[0]
    message = openai_message_to_ollama(choice.get("message", {}))

    result = {
        "model": body.get("model", "local"),
        "created_at": _now_iso(),
        "message": message,
        "done": True,
        "done_reason": choice.get("finish_reason", "stop"),
    }
    result.update(metrics_fields(openai_resp))
    return result


def handle_generate(body: dict) -> dict:
    options = body.get("options") or {}
    payload = build_upstream_payload(
        model=body.get("model", "local"),
        messages=[{"role": "user", "content": body.get("prompt", "")}],
        think=body.get("think", True),
        num_predict=options.get("num_predict"),
    )

    openai_resp = call_upstream(payload)
    choice = (openai_resp.get("choices") or [{}])[0]
    message = choice.get("message", {})

    result = {
        "model": body.get("model", "local"),
        "created_at": _now_iso(),
        "response": message.get("content") or "",
        "done": True,
    }
    reasoning = message.get("reasoning_content")
    if reasoning:
        result["thinking"] = reasoning
    result.update(metrics_fields(openai_resp))
    return result


def handle_tags() -> dict:
    """Translates Ollama's `GET /api/tags` (the list of installed models —
    `backend/src/services/llm.rs`'s `list_local_models`, which feeds the model
    picker/switcher) to llama-server's `/props`, same source `handle_show` reads.
    llama-server only ever serves the one model it was started with — no real
    registry to list — so this reports exactly that one, under the same tag
    Ollama itself uses for a `.env`-configured `MODEL_FILE` (`llm/start.sh`
    always registers it as `local-llm`, regardless of the actual filename), so a
    chat already bound to that tag keeps showing as installed under either
    backend. `quantization_level` is parsed from `model_ftype` (e.g.
    `"IQ3_S - 3.4375 bpw"` -> `"IQ3_S"`, confirmed against this project's own
    GGUFs); `size`/`family`/`parameter_size` are left out rather than guessed —
    all optional on the Rust side (`LocalModel`/`LocalModelDetails`)."""
    r = urllib.request.Request(f"{UPSTREAM}/props", method="GET")
    with urllib.request.urlopen(r, timeout=30) as resp:
        props = json.loads(resp.read())

    quant = (props.get("model_ftype") or "").split(" - ")[0].strip() or None

    return {
        "models": [
            {
                "name": "local-llm:latest",
                "details": {"quantization_level": quant},
            }
        ]
    }


def handle_show(_body: dict) -> dict:
    """Translates Ollama's `POST /api/show` to llama-server's `GET /props` — this
    project's own backend uses `/api/show` specifically to read a model's raw Jinja
    `template` text (to detect what it supports for `think`, see
    `OllamaService.thinking_capability` on the Rust side), so that's the only field
    actually populated here. Confirmed live that `/props`'s `chat_template` is
    byte-for-byte the same raw template text Ollama's own `/api/show` returns for the
    same GGUF — both read straight from the model's embedded chat_template, neither
    is some backend-specific rewrite of it — so the same parsing logic on the Rust
    side works unchanged against either backend's response."""
    r = urllib.request.Request(f"{UPSTREAM}/props", method="GET")
    with urllib.request.urlopen(r, timeout=30) as resp:
        props = json.loads(resp.read())
    return {"template": props.get("chat_template", "")}


class Handler(BaseHTTPRequestHandler):
    def _send_json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler's own naming
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw)
        except json.JSONDecodeError as e:
            self._send_json(400, {"error": f"invalid JSON: {e}"})
            return

        try:
            if self.path == "/api/chat":
                self._send_json(200, handle_chat(body))
            elif self.path == "/api/generate":
                self._send_json(200, handle_generate(body))
            elif self.path == "/api/show":
                self._send_json(200, handle_show(body))
            else:
                self._send_json(404, {"error": f"unknown path {self.path}"})
        except urllib.error.HTTPError as e:
            upstream_body = e.read().decode(errors="replace")
            sys.stderr.write(f"upstream error {e.code}: {upstream_body}\n")
            self._send_json(502, {"error": f"upstream returned {e.code}", "body": upstream_body})
        except Exception as e:  # this is the request's own error boundary
            sys.stderr.write(f"proxy error: {e!r}\n")
            self._send_json(500, {"error": str(e)})

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/health":
            self._send_json(200, {"status": "ok"})
            return

        try:
            if self.path == "/api/tags":
                self._send_json(200, handle_tags())
            else:
                self._send_json(404, {"error": "not found"})
        except urllib.error.HTTPError as e:
            upstream_body = e.read().decode(errors="replace")
            sys.stderr.write(f"upstream error {e.code}: {upstream_body}\n")
            self._send_json(502, {"error": f"upstream returned {e.code}", "body": upstream_body})
        except Exception as e:  # this request's own error boundary
            sys.stderr.write(f"proxy error: {e!r}\n")
            self._send_json(500, {"error": str(e)})

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))


def warmup_upstream() -> None:
    time.sleep(1)
    for _ in range(60):
        try:
            req = urllib.request.Request(f"{UPSTREAM}/health")
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status == 200:
                    break
        except Exception:
            time.sleep(1)
    try:
        sys.stderr.write(f"warming up upstream llama-server at {UPSTREAM}...\n")
        warmup_payload = json.dumps({"prompt": "Hi", "n_predict": 1}).encode("utf-8")
        req = urllib.request.Request(
            f"{UPSTREAM}/completion",
            data=warmup_payload,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            pass
        sys.stderr.write("upstream llama-server warmed up successfully.\n")
    except Exception as e:
        sys.stderr.write(f"warmup notice: {e}\n")


if __name__ == "__main__":
    threading.Thread(target=warmup_upstream, daemon=True).start()
    server = ThreadingHTTPServer(("0.0.0.0", LISTEN_PORT), Handler)
    print(f"mtp-proxy listening on :{LISTEN_PORT}, upstream={UPSTREAM}", flush=True)
    server.serve_forever()
