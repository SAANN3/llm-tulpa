#!/usr/bin/env python3
"""Runs scripted scenarios against a backend whose model server is `fake_llama.py`.

  run_scenarios.py SETTINGS_JSON PROFILE_ID [BACKEND_URL] [FAKE_URL] [scenario names...]

SETTINGS_JSON is the backend's own settings file (for the JWT secret), PROFILE_ID a launch profile of a registered
model (the chat is bound to it; the fake server answers whatever the model). Each scenario makes its own chat, scripts the
fake server, drives the backend through its public API and checks what was stored and what the model was sent.
"""
import base64, hashlib, hmac, json, os, sys, time, traceback, urllib.error, urllib.request

SETTINGS, PROFILE = sys.argv[1], int(sys.argv[2])
BACKEND = sys.argv[3] if len(sys.argv) > 3 else "http://127.0.0.1:3105"
FAKE = sys.argv[4] if len(sys.argv) > 4 else "http://127.0.0.1:18300"
ONLY = sys.argv[5:]

_b64 = lambda d: base64.urlsafe_b64encode(d).rstrip(b"=")
_h = _b64(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
_c = _b64(json.dumps({"sub": 1, "username": "scenarios", "role": "owner", "exp": int(time.time()) + 86400}).encode())
_secret = json.load(open(SETTINGS))["jwt_secret"]
TOKEN = (_h + b"." + _c + b"." + _b64(hmac.new(_secret.encode(), _h + b"." + _c, hashlib.sha256).digest())).decode()


def request(base, path, body=None, method=None, token=True):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + TOKEN
    req = urllib.request.Request(base + path, data=json.dumps(body).encode() if body is not None else None,
                                 method=method or ("POST" if body is not None else "GET"), headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            raw = r.read()
            return r.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            return e.code, json.loads(raw)
        except Exception:
            return e.code, raw


api = lambda path, body=None, method=None: request(BACKEND, path, body, method)
fake = lambda path, body=None: request(FAKE, path, body, token=False)


def ok(path, body=None, method=None):
    status, data = api(path, body, method)
    assert status < 300, f"{path} -> {status} {data}"
    return data


def script(*items):
    fake("/__script", list(items))


def calls(kind=None):
    items = fake("/__calls")[1]
    return [c for c in items if kind is None or c["kind"] == kind]


def new_chat(name, tools=True):
    return ok("/api/chats", {"name": name, "launch_profile_id": PROFILE, "tools_enabled": tools})["id"]


def state(chat):
    return ok(f"/api/agent/turn?chat_id={chat}")


def wait_idle(chat, limit=60):
    t0 = time.time()
    while time.time() - t0 < limit:
        s = state(chat)
        if s["status"] != "running":
            return s
        time.sleep(0.1)
    raise AssertionError("the run did not end")


def wait_for(cond, limit=30, what="a condition"):
    t0 = time.time()
    while time.time() - t0 < limit:
        if cond():
            return
        time.sleep(0.1)
    raise AssertionError(f"timed out waiting for {what}")


def turn(chat, prompt, **extra):
    status, data = api("/api/agent/turn", {"chat_id": chat, "prompt": prompt, **extra})
    assert status == 202, f"POST /turn -> {status} {data}"
    return wait_idle(chat)


def messages(chat):
    return sorted(ok(f"/api/chats/messages?chat_id={chat}&limit=500")["messages"], key=lambda m: m["id"])


def roles(chat):
    return [m["role"] for m in messages(chat)]


def settings(**kw):
    status, _ = api("/api/settings", {"auto_confirm": True, "max_turn_steps": 0, "use_tools": True, "trim_old_thinking": False, **kw})
    assert status < 300


def eq(actual, expected, what):
    assert actual == expected, f"{what}: expected {expected!r}, got {actual!r}"


def has(text, part, what):
    assert part in (text or ""), f"{what}: {part!r} not in {str(text)[:200]!r}"


SCENARIOS = {}


def scenario(fn):
    SCENARIOS[fn.__name__] = fn
    return fn


# ---------------------------------------------------------------------------------------------------------------------

@scenario
def plain_answer():
    """A prompt is stored, the model is asked once, its reply is stored, the run ends answered."""
    settings()
    chat = new_chat("plain")
    script({"text": "Hello there."})
    s = turn(chat, "Hi")
    eq(s["last_end"]["reason"], "answered", "run end")
    eq(roles(chat), ["user", "assistant"], "stored roles")
    eq(messages(chat)[-1]["content"], "Hello there.", "reply")
    main = calls("main")
    eq(len(main), 1, "model calls")
    assert main[0]["has_tools"], "a chat with tools sends them"
    eq(main[0]["messages"][0]["role"], "system", "system message first")
    has(main[0]["messages"][-1]["content"], "Hi", "the prompt reaches the model")
    has(main[0]["messages"][-1]["content"], "Current real date and time", "the date note is on the newest message")


@scenario
def tool_call_then_answer():
    """A tool call is run by the backend, its result goes back to the model, then the model answers."""
    settings()
    chat = new_chat("tool")
    script({"tool_calls": [{"name": "os.get_date", "arguments": {}}]}, {"text": "Done."})
    s = turn(chat, "What day is it?")
    eq(s["last_end"]["reason"], "answered", "run end")
    eq(roles(chat), ["user", "assistant", "tool", "assistant"], "stored roles")
    eq(messages(chat)[1]["tool_calls"][0]["tool_name"], "os.get_date", "the call")
    second = calls("main")[1]["messages"]
    eq([m["role"] for m in second][-3:], ["user", "assistant", "tool"], "the second request ends with the tool result")
    has(second[-1]["content"], "utc", "the result is in the second request")
    assert not any("Current real date" in str(m["content"]) for m in second[:-1]), "the date note only rides on the newest message"


@scenario
def tools_off_sends_no_tools():
    """A chat without tools sends none, and its system prompt is the short one."""
    settings()
    chat = new_chat("no-tools", tools=False)
    script({"text": "Hi."})
    turn(chat, "Hello")
    main = calls("main")[0]
    assert not main["has_tools"], "no tool definitions"
    system = main["messages"][0]["content"]
    assert "tool" not in system.lower(), "the short prompt names no tool"
    assert len(system) < 3000, f"the short prompt is short ({len(system)})"


@scenario
def step_limit_with_a_tool_call_only():
    """At the step limit a reply that is only a tool call is not run, and not stored empty."""
    settings(max_turn_steps=2)
    chat = new_chat("limit")
    script({"tool_calls": [{"name": "os.get_date", "arguments": {}}]}, {"tool_calls": [{"name": "os.get_date", "arguments": {}}]})
    s = turn(chat, "go")
    eq(s["last_end"]["reason"], "step_limit", "run end")
    last = messages(chat)[-1]
    eq(last["role"], "assistant", "last message")
    eq(last["tool_calls"], [], "the refused call is not stored")
    has(last["content"], "step limit", "a placeholder instead of an empty reply")
    has(calls("main")[1]["messages"][-1]["content"], "Step limit reached", "the last request carries the note")
    assert "Step limit reached" not in str(calls("main")[0]["messages"][-1]["content"]), "the first request does not"
    settings()


@scenario
def empty_reply_is_asked_again_once():
    settings()
    chat = new_chat("empty")
    script({"text": ""}, {"text": "Now an answer."})
    s = turn(chat, "say something")
    eq(messages(chat)[-1]["content"], "Now an answer.", "the second reply is the one stored")
    eq(len(calls("main")), 2, "model calls")
    eq(roles(chat), ["user", "assistant"], "stored roles")


@scenario
def tool_call_written_as_text_is_asked_again():
    settings()
    chat = new_chat("astext")
    bad = {"text": "<tool_call>\n<function=os.get_date>\n</function>\n</tool_call>"}
    script(bad, bad, {"text": "Fine."})
    turn(chat, "go")
    eq(messages(chat)[-1]["content"], "Fine.", "reply")
    eq(len(calls("main")), 3, "two retries")
    script(bad, bad, bad)
    chat2 = new_chat("astext-2")
    turn(chat2, "go")
    eq(len(calls("main")), 6, "after two retries the third is kept")


@scenario
def cut_off_in_thinking_continues_once():
    settings()
    chat = new_chat("cutoff")
    script({"text": "", "thinking": "thinking and thinking " * 50, "finish": "length"}, {"text": "The answer."})
    s = turn(chat, "hard question")
    eq(s["last_end"]["reason"], "answered", "run end")
    eq(roles(chat), ["user", "assistant", "notice", "assistant"], "stored roles")
    has(messages(chat)[1]["content"], "thought process", "the cut thoughts are kept as a message")
    eq(messages(chat)[-1]["content"], "The answer.", "reply")


@scenario
def context_overflow_fails_the_run_with_the_reason():
    settings()
    chat = new_chat("overflow")
    script({"status": 400, "body": {"error": {"code": 400, "message": "request (50000 tokens) exceeds the available context size (40960 tokens)",
                                               "type": "exceed_context_size_error"}}})
    s = turn(chat, "go")
    eq(s["last_end"]["reason"], "failed", "run end")
    has(s["last_end"]["detail"], "exceeds the available context size", "the reason is kept")
    eq(roles(chat), ["user"], "only the user's message is stored")
    # and the chat works afterwards
    script({"text": "Back."})
    s = turn(chat, "again")
    eq(s["last_end"]["reason"], "answered", "run end after the failure")


@scenario
def stop_drops_the_call_in_flight():
    settings()
    chat = new_chat("stop")
    script({"text": "never stored", "delay": 20})
    status, _ = api("/api/agent/turn", {"chat_id": chat, "prompt": "go"})
    eq(status, 202, "start")
    wait_for(lambda: len(calls("main")) == 1, what="the model call")
    status, _ = api("/api/agent/turn", {"chat_id": chat, "prompt": "second"})
    eq(status, 409, "a second turn while one runs")
    status, _ = api("/api/chats/rewind", {"chat_id": chat, "message_id": 1})
    eq(status, 409, "rewind during a run")
    t0 = time.time()
    eq(api("/api/agent/stop", {"chat_id": chat})[0], 204, "stop")
    s = wait_idle(chat, 10)
    assert time.time() - t0 < 3, "the run ends at once, not when the model call would have"
    eq(s["last_end"]["reason"], "stopped", "run end")
    eq(roles(chat), ["user"], "nothing of the reply is stored")
    eq(api("/api/agent/stop", {"chat_id": chat})[0], 409, "stopping again")
    # the chat is usable at once
    script({"text": "Back."})
    eq(turn(chat, "again")["last_end"]["reason"], "answered", "next run")


@scenario
def permission_waits_and_answers_continue():
    settings(auto_confirm=False)
    chat = new_chat("permission")
    write = {"name": "storage.write_file", "arguments": {"path": "/tmp/fake_perm_%d.txt" % os.getpid(), "content": "x"}}
    script({"tool_calls": [write]}, {"text": "Written."})
    s = turn(chat, "write it")
    eq(s["status"], "waiting_for_permission", "state")
    eq(s["last_end"]["reason"], "waiting_for_permission", "run end")
    eq(len(s["pending"]), 1, "pending calls")
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 3, "allowance": "deny"}]})[0], 400, "a decision pointing at nothing")
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "deny"}]})[0], 202, "deny")
    s = wait_idle(chat)
    eq(s["last_end"]["reason"], "answered", "run end after the answer")
    tool = [m for m in messages(chat) if m["role"] == "tool"][0]
    eq(tool["tool_denied"], True, "the call was recorded as denied")
    assert not os.path.exists(write["arguments"]["path"]), "the file was not written"
    has(calls("main")[1]["messages"][-1]["content"], "declined", "the model is told it was declined")
    settings()


@scenario
def allowed_call_runs_before_the_wait_and_the_step_count_carries():
    settings(auto_confirm=False, max_turn_steps=10)
    chat = new_chat("order")
    path = "/tmp/fake_order_%d.txt" % os.getpid()
    script({"tool_calls": [{"name": "os.get_date", "arguments": {}}, {"name": "storage.write_file", "arguments": {"path": path, "content": "y"}}]},
           {"text": "Both done."})
    s = turn(chat, "two calls")
    eq(s["status"], "waiting_for_permission", "state")
    eq([p["name"] for p in s["pending"]], ["storage.write_file"], "only the call needing permission is pending")
    msgs = messages(chat)
    eq([m["role"] for m in msgs], ["user", "assistant", "tool"], "the allowed call ran")
    assert "Interrupted" not in msgs[-1]["content"], "and was not taken for a cut-short one"
    state(chat)  # reading changes nothing
    eq(len(messages(chat)), 3, "reading the state writes nothing")
    api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})
    time.sleep(0.2)
    s = wait_idle(chat)
    eq(s["last_end"]["reason"], "answered", "run end")
    assert os.path.exists(path), "the file was written"
    os.remove(path)
    settings()


BIG = "word " * 3000   # ~15,000 characters: three of them are more than a 40k window's tail budget


@scenario
def a_fold_asks_for_notes_summary_and_facts_and_the_next_request_carries_them():
    settings()
    chat = new_chat("fold")
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000},
           {"text": "Final."})
    for i in range(3):
        turn(chat, f"part {i}: " + BIG)
    kinds = [c["kind"] for c in calls()]
    for kind in ("notes", "summary", "facts"):
        assert kind in kinds, f"a {kind} request was made ({kinds})"
    # notes are asked while the prompt is still the cached one: before the summary
    assert kinds.index("notes") < kinds.index("summary"), "notes before the summary"
    turn(chat, "what now?")
    system = calls("main")[-1]["messages"][0]["content"]
    has(system, "Earlier parts of this conversation were summarized", "the fold header")
    has(system, "never reconstruct a value from memory", "the header tells the model not to fill gaps")
    has(system, "The fake summary of the excerpt", "the summary")
    has(system, "fake fact one", "the key facts")
    has(system, "Your own working notes", "the notes")
    has(system, "part 0:", "the user's first message is pinned")


@scenario
def a_fold_in_a_tools_off_chat_has_no_notes_and_no_tool_pointers():
    settings()
    chat = new_chat("fold-off", tools=False)
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000},
           {"text": "Final."})
    for i in range(3):
        turn(chat, f"part {i}: " + BIG)
    kinds = [c["kind"] for c in calls()]
    assert "notes" not in kinds, "no notes request without tools"
    assert "summary" in kinds, "the fold itself still happens"
    turn(chat, "what now?")
    system = calls("main")[-1]["messages"][0]["content"]
    has(system, "Earlier parts of this conversation were summarized", "the fold header")
    assert "chat.get_messages" not in system and "chat.list_messages" not in system, "no pointer to a tool"
    assert "Your own working notes" not in system, "no notes block"


@scenario
def a_huge_tool_result_is_stored_cut():
    settings()
    chat = new_chat("cap")
    script({"tool_calls": [{"name": "os.execute_command", "arguments": {"command": "python3 -c \"print('x' * 60000)\""}}]}, {"text": "Seen."})
    s = turn(chat, "print a lot")
    eq(s["last_end"]["reason"], "answered", "run end")
    tool = [m for m in messages(chat) if m["role"] == "tool"][0]
    has(tool["content"], "This result was cut", "the cut is said")
    assert len(tool["content"]) < 31500, f"stored result is {len(tool['content'])} characters"


@scenario
def a_finished_job_wakes_the_chat_with_nothing_open():
    settings()
    chat = new_chat("wake")
    script({"tool_calls": [{"name": "os.start_job", "arguments": {"command": "sleep 2; echo woke", "wait_seconds": 1}}]}, {"text": "Started."},
           {"text": "The job is done."})
    s = turn(chat, "start a job")
    eq(s["last_end"]["reason"], "answered", "first run")
    wait_for(lambda: roles(chat)[-2:] == ["notice", "assistant"], limit=30, what="the notice and the reply to it")
    eq(messages(chat)[-1]["content"], "The job is done.", "the reply the backend asked for by itself")
    has(messages(chat)[-2]["content"], "finished", "the notice")
    wait_for(lambda: state(chat)["status"] == "idle", what="idle")


@scenario
def regenerate_replaces_the_reply():
    settings()
    chat = new_chat("regen")
    script({"text": "First."}, {"text": "Second."})
    turn(chat, "question")
    old = messages(chat)[-1]
    eq(api("/api/agent/regenerate", {"chat_id": chat, "message_id": old["id"]})[0], 202, "regenerate")
    time.sleep(0.2)
    wait_idle(chat)
    eq([(m["role"], m["content"][:20]) for m in messages(chat)], [("user", "question"), ("assistant", "Second.")], "stored messages")
    sent = calls("main")[1]["messages"]
    eq([m["role"] for m in sent], ["system", "user"], "the old reply is not sent again")
    # a reply that is not the newest can't be regenerated
    eq(api("/api/agent/regenerate", {"chat_id": chat, "message_id": old["id"]})[0], 409, "a stale reply")


@scenario
def a_sub_agent_runs_and_its_result_wakes_the_parent():
    settings()
    parent = new_chat("parent")
    sub = {"system_contains": "You are a sub-agent"}
    notsub = {"system_lacks": "You are a sub-agent"}
    script({"match": notsub, "tool_calls": [{"name": "llm.run_agent", "arguments": {"prompt": "find the number"}}]},
           {"match": sub, "tool_calls": [{"name": "llm.return_agent", "arguments": {"output": "the number is 42"}}]},
           {"match": notsub, "text": "Delegated."},
           {"match": notsub, "text": "It is 42."})
    s = turn(parent, "please delegate")
    eq(s["last_end"]["reason"], "answered", "first run")
    wait_for(lambda: roles(parent)[-2:] == ["notice", "assistant"], limit=30, what="the result reaching the parent")
    eq(messages(parent)[-1]["content"], "It is 42.", "the parent's reply to the result")
    has(messages(parent)[-2]["content"], "the number is 42", "the sub-agent's result is in the notice")
    wait_for(lambda: state(parent)["status"] == "idle", what="idle")


@scenario
def the_tools_switch_is_refused_during_a_run_and_applies_after():
    settings()
    chat = new_chat("switch")
    script({"text": "slow", "delay": 1.5}, {"text": "Without tools."})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "go"})
    wait_for(lambda: len(calls("main")) == 1, what="the call")
    s = state(chat)
    eq(s["status"], "running", "state")
    eq(s["step"], 1, "step")
    assert s["call_started_at"] is not None and s["running_tool"] is None, "a model call is in flight, no tool"
    eq(api("/api/chats/tools", {"chat_id": chat, "enabled": False})[0], 409, "the switch during a run")
    wait_idle(chat)
    eq(api("/api/chats/tools", {"chat_id": chat, "enabled": False})[0], 204, "the switch afterwards")
    turn(chat, "again")
    assert calls("main")[0]["has_tools"] and not calls("main")[1]["has_tools"], "the next request has no tools"


@scenario
def messages_after_an_id_and_the_run_events():
    settings()
    chat = new_chat("events")
    import threading
    seen = []

    def listen():
        req = urllib.request.Request(BACKEND + "/api/events", headers={"Authorization": "Bearer " + TOKEN, "Accept": "text/event-stream"})
        try:
            with urllib.request.urlopen(req, timeout=8) as r:
                for line in r:
                    if line.startswith(b"data:"):
                        event = json.loads(line[5:])
                        if event.get("chat_id") == chat:
                            seen.append(event)
        except Exception:
            pass

    threading.Thread(target=listen, daemon=True).start()
    time.sleep(0.5)
    script({"tool_calls": [{"name": "os.get_date", "arguments": {}}]}, {"text": "Done."})
    turn(chat, "go")
    time.sleep(1)
    types = [e["type"] for e in seen]
    for wanted in ("run_started", "messages_changed", "tool_started", "turn_progress", "run_ended"):
        assert wanted in types, f"{wanted} was sent ({types})"
    ended = [e for e in seen if e["type"] == "run_ended"][-1]
    eq(ended["reason"], "answered", "run_ended reason")
    assert ended["eval_tokens"] > 0 and ended["started_at"], "run_ended carries its start and tokens"
    assert types.index("run_started") < types.index("run_ended"), "start before end"
    msgs = messages(chat)
    newer = ok(f"/api/chats/messages?chat_id={chat}&after_id={msgs[1]['id']}")["messages"]
    eq([m["id"] for m in newer], [m["id"] for m in reversed(msgs[2:])], "messages after an id, newest first")


@scenario
def two_chats_run_at_once():
    settings()
    a, b = new_chat("a"), new_chat("b")
    script({"text": "A", "delay": 1.0, "match": {"system_contains": "private, self-hosted"}}, {"text": "B", "delay": 1.0})
    api("/api/agent/turn", {"chat_id": a, "prompt": "go a"})
    api("/api/agent/turn", {"chat_id": b, "prompt": "go b"})
    t0 = time.time()
    wait_idle(a)
    wait_idle(b)
    assert time.time() - t0 < 3, "the two runs went on together"
    eq(sorted([messages(a)[-1]["content"], messages(b)[-1]["content"]]), ["A", "B"], "each chat got a reply")


@scenario
def a_crash_in_the_model_server_connection_fails_the_run_cleanly():
    settings()
    chat = new_chat("badbody")
    script({"status": 500, "body": "not json at all"})
    s = turn(chat, "go")
    eq(s["last_end"]["reason"], "failed", "run end")
    eq(s["status"], "idle", "the chat is free")
    script({"text": "OK again."})
    eq(turn(chat, "again")["last_end"]["reason"], "answered", "the next run")


def main():
    names = ONLY or list(SCENARIOS)
    failed = 0
    for name in names:
        fake("/__reset", {})
        t0 = time.time()
        try:
            SCENARIOS[name]()
            print(f"PASS  {name}  ({time.time() - t0:.1f}s)", flush=True)
        except Exception as e:
            failed += 1
            print(f"FAIL  {name}: {e}", flush=True)
            if os.environ.get("TRACE"):
                traceback.print_exc()
    print(f"{len(names) - failed} of {len(names)} passed", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
