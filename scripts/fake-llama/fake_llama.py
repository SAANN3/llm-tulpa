#!/usr/bin/env python3
"""A scripted stand-in for llama-server, for testing the backend's turn logic in seconds and without a model.

The backend is pointed at it with `llama_cpp.external_url` (it then neither starts nor stops a server). What it answers on
`/v1/chat/completions` is scripted over its own control endpoints:

  POST /__script   a JSON list of replies, used one per main-chat request, in order (empty: the reply "OK.")
  POST /__reset    forget the script and the recorded calls
  GET  /__calls    every request seen so far: its kind, whether it carried tools, the messages, the reply cap

A reply item: {"text", "thinking", "tool_calls": [{"name", "arguments"}], "finish": "stop"|"length"|"tool_calls",
"prompt_tokens", "completion_tokens", "delay": seconds, "status": 400, "body": <error body>,
"match": {"system_contains": "...", "system_lacks": "..."}}. An item with `match` is only used for a request whose system
message fits it (the first fitting item in the queue is taken), so two chats talking to the fake at once, a parent and its
sub-agent, each get their own replies.

Requests that are not the chat's own (the compaction summary, the key facts, the notes request) are told apart by their
instruction and answered with a fixed valid reply, so a script only has to describe the conversation itself. They are
recorded with their own kind.
"""
import json
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
SCRIPT = []
CALLS = []
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 18300
N_CTX = int(sys.argv[2]) if len(sys.argv) > 2 else 40960

SUMMARY = ("1. ESTABLISHED FACTS & FINDINGS\n- The fake summary of the excerpt.\n\n2. COMPLETED CHANGES\n- Nothing real.\n\n"
           "3. CURRENT UNSOLVED OBJECTIVE\n- Carry on with the task.")
FACTS = json.dumps({"goal": "the fake goal", "facts": ["fake fact one", "fake fact two"]})
NOTES = "Goal: the fake goal. Plan: step 1 done, step 2 next."
TEMPLATE = ("{%- if tools %}<tools>{{ tools }}</tools>{%- endif %}{%- if enable_thinking is defined and enable_thinking is false %}"
            "<think>\n\n</think>{%- endif %}<tool_call>\n</tool_call><think>\n</think>")


def text_of(message):
    content = message.get("content")
    if isinstance(content, list):
        return " ".join(part.get("text", "") for part in content if isinstance(part, dict))
    return content or ""


def kind_of(body):
    messages = body.get("messages") or []
    system = text_of(messages[0]) if messages and messages[0].get("role") == "system" else ""
    last = text_of(messages[-1]) if messages else ""
    # The notes request is the live prompt (tools included, so the server's cache serves it) plus one question
    if last.startswith("The older part of this conversation is about to be compacted"):
        return "notes"
    if not body.get("tools"):
        if system.startswith("Summarize the conversation excerpt"):
            return "summary"
        if system.startswith("Extract key facts"):
            return "facts"
    return "main"


def tokens_of(body):
    return max(1, int(len(json.dumps(body.get("messages") or [])) / 3.3) + int(len(json.dumps(body.get("tools") or [])) / 3.3))


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def send_json(self, status, payload, raw=False):
        data = payload.encode() if raw else json.dumps(payload).encode()
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return True
        except (BrokenPipeError, ConnectionResetError):
            return False

    def read_body(self):
        length = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(length) or b"null") if length else None

    def do_GET(self):
        if self.path == "/health":
            return self.send_json(200, {"status": "ok"})
        if self.path == "/props":
            return self.send_json(200, {"chat_template": TEMPLATE, "model_path": "/models/fake.gguf", "total_slots": 1,
                                        "default_generation_settings": {"n_ctx": N_CTX}})
        if self.path == "/slots":
            return self.send_json(200, [{"id": 0, "n_ctx": N_CTX, "is_processing": False}])
        if self.path == "/metrics":
            return self.send_json(200, "", raw=True)
        if self.path == "/__calls":
            with LOCK:
                return self.send_json(200, CALLS)
        return self.send_json(404, {"error": "not found"})

    def do_POST(self):
        body = self.read_body()
        if self.path == "/__script":
            with LOCK:
                SCRIPT.extend(body)
            return self.send_json(200, {"queued": len(SCRIPT)})
        if self.path == "/__reset":
            with LOCK:
                SCRIPT.clear()
                CALLS.clear()
            return self.send_json(200, {})
        if self.path != "/v1/chat/completions":
            return self.send_json(404, {"error": "not found"})

        kind = kind_of(body)
        item = {}
        messages = body.get("messages") or []
        if kind == "main":
            system = text_of(messages[0]) if messages and messages[0].get("role") == "system" else ""
            with LOCK:
                for index, candidate in enumerate(SCRIPT):
                    want = candidate.get("match") or {}
                    if want.get("system_contains", system) in system and (not want.get("system_lacks") or want["system_lacks"] not in system):
                        item = SCRIPT.pop(index)
                        break
        with LOCK:
            CALLS.append({
                "n": len(CALLS) + 1, "kind": kind, "has_tools": bool(body.get("tools")), "tool_names": [t["function"]["name"] for t in body.get("tools") or []],
                "max_tokens": body.get("max_tokens"), "messages": messages, "scripted": bool(item), "at": time.time(),
                "chat_template_kwargs": body.get("chat_template_kwargs"),
            })
        delay = float(item.get("delay", 0))
        end = time.time() + delay
        while time.time() < end:
            time.sleep(0.05)
        if "status" in item:
            return self.send_json(item["status"], item.get("body", {"error": {"message": "scripted error"}}))

        if kind == "summary":
            text = SUMMARY
        elif kind == "facts":
            text = FACTS
        elif kind == "notes":
            text = NOTES
        else:
            text = item.get("text", "OK." if not item.get("tool_calls") else "")
        calls = item.get("tool_calls") or []
        prompt_tokens = item.get("prompt_tokens", tokens_of(body))
        completion_tokens = item.get("completion_tokens", max(1, (len(text) + len(item.get("thinking", ""))) // 4))
        message = {"role": "assistant", "content": text}
        if item.get("thinking"):
            message["reasoning_content"] = item["thinking"]
        if calls:
            message["tool_calls"] = [{"id": f"call_{i}", "type": "function",
                                      "function": {"name": c["name"], "arguments": json.dumps(c.get("arguments", {}))}} for i, c in enumerate(calls)]
        finish = item.get("finish", "tool_calls" if calls else "stop")
        self.send_json(200, {
            "id": "fake", "object": "chat.completion", "model": "fake",
            "choices": [{"index": 0, "finish_reason": finish, "message": message}],
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens},
            "timings": {"prompt_ms": 5.0, "predicted_ms": 10.0, "prompt_n": prompt_tokens},
        })


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    server.daemon_threads = True
    print(f"fake llama-server on 127.0.0.1:{PORT} (n_ctx {N_CTX})", flush=True)
    server.serve_forever()
