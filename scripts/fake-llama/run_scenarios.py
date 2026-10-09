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


def token_for(user_id, username, role):
    claims = _b64(json.dumps({"sub": user_id, "username": username, "role": role, "exp": int(time.time()) + 86400}).encode())
    return (_h + b"." + claims + b"." + _b64(hmac.new(_secret.encode(), _h + b"." + claims, hashlib.sha256).digest())).decode()


TOKEN = token_for(1, "scenarios", "owner")


def request(base, path, body=None, method=None, token=True):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + (TOKEN if token is True else token)
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
def every_command_of_a_shell_line_needs_approval_and_allow_once_adds_to_the_chats_grants():
    settings(auto_confirm=False)
    chat = new_chat("shell-grants")
    first = {"name": "os.execute_command", "arguments": {"command": "cd /tmp"}}
    both = {"name": "os.execute_command", "arguments": {"command": "cd /tmp && echo once-ok"}}
    script({"tool_calls": [first]}, {"text": "ok"}, {"tool_calls": [both]}, {"text": "done"})
    eq(turn(chat, "go")["status"], "waiting_for_permission", "cd needs approving")
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "permanent"}]})[0], 202, "approve cd")
    eq(wait_idle(chat)["last_end"]["reason"], "answered", "first turn")
    s = turn(chat, "again")
    # `cd` is approved for the chat, but the line also runs `echo`
    eq(s["status"], "waiting_for_permission", "a chained command still asks")
    has(s["pending"][0]["permission"]["escalation"]["ui_message"], "`echo`", "the prompt names the missing command")
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})[0], 202, "allow echo once")
    eq(wait_idle(chat)["last_end"]["reason"], "answered", "second turn")
    tool = [m for m in messages(chat) if m["role"] == "tool"][-1]
    has(str(tool["content"]), "once-ok", "the line ran with the stored grant plus the one-time one")


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
def the_context_is_measured_by_part_and_memory_edits_reach_the_next_request():
    settings()
    chat = new_chat("memory")
    context = lambda: ok(f"/api/chats/context?chat_id={chat}")
    edit = lambda what, body: api(f"/api/chats/{what}", {"chat_id": chat, **body})[0]
    eq(context()["memory"]["folded"], False, "folded before the first fold")
    eq(edit("facts", {"facts": ["x"]}), 409, "facts before the first fold")
    eq(edit("summary", {"summary": "x"}), 409, "a summary before the first fold")
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000},
           {"text": "Final.", "prompt_tokens": 9000, "completion_tokens": 500}, {"text": "slow", "delay": 1.5}, {"text": "Edited."})
    for i in range(3):
        turn(chat, f"part {i}: " + BIG)
    turn(chat, "what now?")

    c = context()
    memory, parts = c["memory"], {p["kind"]: p for p in c["parts"]}
    eq((memory["folded"], memory["goal"], memory["facts"]), (True, "the fake goal", ["fake fact one", "fake fact two"]), "the memory after a fold")
    has(memory["summary"], "The fake summary of the excerpt", "the summary")
    has(memory["notes"], "the fake goal", "the notes")
    eq((c["measured"], c["used"]), (True, 9500), "the measured prompt and the reply, which the next prompt carries")
    assert abs(sum(p["tokens"] for p in c["parts"]) - 9500) <= len(c["parts"]), "the parts add up to the measurement"
    eq(parts["tools"]["count"], len(calls("main")[-1]["tool_names"]), "the tools counted")
    # The parts of the system message are what the request carried, apart from the joins and the summary's heading
    system = calls("main")[-1]["messages"][0]["content"].encode()
    lead = sum(parts[k]["chars"] for k in ("system_prompt", "key_facts", "pinned", "summary", "notes"))
    assert 0 <= len(system) - lead < 100, f"the system message measured: {len(system)} sent, {lead} counted"
    sent = calls("main")[-1]["messages"][1:]
    eq((parts["user_messages"]["count"], parts["replies"]["count"]),
       (sum(m["role"] == "user" for m in sent), sum(m["role"] == "assistant" for m in sent) + 1), "the messages since the fold, and the reply to the last")

    eq(edit("facts", {"facts": ["the user's fact", "  ", "fake fact two"]}), 204, "facts saved")
    eq(edit("summary", {"summary": "The user's summary."}), 204, "summary saved")
    eq(edit("notes", {"notes": "The user's notes."}), 204, "notes saved")
    c = context()
    eq((c["memory"]["goal"], c["memory"]["facts"]), ("the fake goal", ["the user's fact", "fake fact two"]), "the goal kept, blank facts dropped")
    eq(c["measured"], False, "an edit drops the measurement")

    api("/api/agent/turn", {"chat_id": chat, "prompt": "go"})
    wait_for(lambda: len(calls("main")) == 5, what="the slow call")
    for what, body in (("facts", {"facts": []}), ("summary", {"summary": "x"}), ("notes", {"notes": "x"})):
        eq(edit(what, body), 409, f"{what} during a run")
    wait_idle(chat)
    turn(chat, "and now?")
    system = calls("main")[-1]["messages"][0]["content"]
    for part in ("The user's summary.", "- the user's fact", "Goal: the fake goal", "The user's notes."):
        has(system, part, "the edit in the next request")
    assert "fake fact one" not in system and "The fake summary" not in system, "the replaced text is gone"


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


def collect_events(seconds=10):
    """Every event the backend sends this user while the stream is open, in a list that fills in the background"""
    import threading
    events = []

    def listen():
        req = urllib.request.Request(BACKEND + "/api/live", headers={"Authorization": "Bearer " + TOKEN, "Accept": "text/event-stream"})
        try:
            with urllib.request.urlopen(req, timeout=seconds) as r:
                for line in r:
                    if line.startswith(b"data:"):
                        events.append(json.loads(line[5:]))
        except Exception:
            pass

    threading.Thread(target=listen, daemon=True).start()
    time.sleep(0.5)
    return events


@scenario
def seen_is_announced_only_when_there_was_something_to_clear():
    settings()
    chat = new_chat("seen-event")
    events = collect_events()
    script({"text": "Done."})
    turn(chat, "go")
    eq(api("/api/chats/seen", {"chat_id": chat})[0], 204, "seen")
    eq(api("/api/chats/seen", {"chat_id": chat})[0], 204, "seen again")
    time.sleep(0.5)
    mine = [e for e in events if e.get("chat_id") == chat]
    eq([e["type"] for e in mine if e["type"] == "chat_seen"], ["chat_seen"], "one chat_seen for two calls")
    assert [e["type"] for e in mine].index("run_ended") < [e["type"] for e in mine].index("chat_seen"), "after the end"
    # another user's chat can't be marked, so nothing goes out for it
    status, _ = request(BACKEND, "/api/chats/seen", {"chat_id": chat}, token=token_for(2, "testuser", "user"))
    assert status in (404, 401), status


@scenario
def the_chat_list_changes_are_announced():
    settings()
    events = collect_events()
    chat = new_chat("listed")
    eq(api("/api/chats/rename", {"chat_id": chat, "name": "Renamed"})[0], 204, "rename")
    eq(api(f"/api/chats?id={chat}", None, "DELETE")[0], 204, "delete")
    time.sleep(0.5)
    mine = [e for e in events if e.get("chat_id") == chat]
    eq([e["type"] for e in mine], ["chat_created", "chat_renamed", "chat_deleted"], "the list changes, in order")
    eq(mine[1]["name"], "Renamed", "the new name")


@scenario
def removed_messages_are_announced():
    settings()
    chat = new_chat("removed")
    events = collect_events()
    script({"text": "First."}, {"text": "Second."})
    turn(chat, "one")
    reply = messages(chat)[-1]["id"]
    eq(api("/api/agent/regenerate", {"chat_id": chat, "message_id": reply})[0], 202, "regenerate")
    wait_idle(chat)
    kept = messages(chat)
    eq(api("/api/chats/rewind", {"chat_id": chat, "message_id": kept[0]["id"]})[0], 200, "rewind")
    time.sleep(0.5)
    removed = [sorted(e["message_ids"]) for e in events if e.get("chat_id") == chat and e["type"] == "messages_removed"]
    eq(removed, [[reply], sorted(m["id"] for m in kept)], "the replaced reply, then everything the rewind took")


@scenario
def messages_after_an_id_and_the_run_events():
    settings()
    chat = new_chat("events")
    import threading
    seen = []

    def listen():
        req = urllib.request.Request(BACKEND + "/api/live", headers={"Authorization": "Bearer " + TOKEN, "Accept": "text/event-stream"})
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
def running_chats_are_listed_for_their_owner_only():
    settings()
    a, b = new_chat("run-a"), new_chat("run-b")
    eq(ok("/api/agent/runs"), [], "nothing runs yet")
    script({"text": "A", "delay": 1.5}, {"text": "B", "delay": 1.5})
    api("/api/agent/turn", {"chat_id": a, "prompt": "go a"})
    api("/api/agent/turn", {"chat_id": b, "prompt": "go b"})
    wait_for(lambda: len(ok("/api/agent/runs")) == 2, what="both runs listed")
    listed = ok("/api/agent/runs")
    eq(sorted(r["chat_id"] for r in listed), sorted([a, b]), "the running chats")
    assert all(r["state"]["status"] == "running" for r in listed), listed
    # another user (the second account of the database, when there is one) sees none of them
    status, other = request(BACKEND, "/api/agent/runs", token=token_for(2, "testuser", "user"))
    if status == 200:
        eq(other, [], "another user's list")
    else:
        print(f"      (no second user in this database: {status}, ownership not checked)")
    wait_idle(a)
    wait_idle(b)
    wait_for(lambda: ok("/api/agent/runs") == [], what="the list empty again")


def unseen(chat):
    return ok(f"/api/chats?id={chat}")["unseen_end"]


@scenario
def the_end_of_a_run_is_marked_until_the_chat_is_seen():
    settings(auto_confirm=False)
    chat = new_chat("unseen")
    eq(unseen(chat), None, "a new chat has nothing new")
    script({"text": "Done."})
    turn(chat, "go")
    eq(unseen(chat), "answered", "marked when the run ends")
    chats = ok("/api/chats?limit=50")["chats"]
    eq([c["unseen_end"] for c in chats if c["id"] == chat], ["answered"], "the list carries it")
    eq(api("/api/chats/seen", {"chat_id": chat})[0], 204, "seen")
    eq(unseen(chat), None, "cleared")
    # a stopped run has nothing to tell
    script({"text": "never stored", "delay": 20})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "again"})
    wait_for(lambda: len(calls("main")) == 2, what="the second model call")
    eq(api("/api/agent/stop", {"chat_id": chat})[0], 204, "stop")
    wait_idle(chat, 10)
    eq(unseen(chat), None, "a stopped run leaves no mark")
    # a wait for permission is marked, and the answer's run clears it
    write = {"name": "storage.write_file", "arguments": {"path": "/tmp/fake_seen_%d.txt" % os.getpid(), "content": "x"}}
    script({"tool_calls": [write]}, {"text": "Written."})
    turn(chat, "write it")
    eq(unseen(chat), "waiting_for_permission", "marked while waiting")
    api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "deny"}]})
    wait_for(lambda: unseen(chat) in (None, "answered"), what="the answer's run to start")
    wait_idle(chat)
    eq(unseen(chat), "answered", "marked again at the end")
    # a failed run is marked as failed; another user's chat can't be marked
    script({"status": 500, "body": "not json at all"})
    turn(chat, "fail")
    eq(unseen(chat), "failed", "failed run")
    status, _ = request(BACKEND, "/api/chats/seen", {"chat_id": chat}, token=token_for(2, "testuser", "user"))
    assert status in (404, 401), f"another user marking the chat: {status}"
    eq(unseen(chat), "failed", "still marked")


@scenario
def a_sub_agents_run_names_its_parent():
    settings()
    parent = new_chat("parent-of-run")
    sub = {"system_contains": "You are a sub-agent"}
    notsub = {"system_lacks": "You are a sub-agent"}
    script({"match": notsub, "tool_calls": [{"name": "llm.run_agent", "arguments": {"prompt": "find the number"}}]},
           {"match": sub, "delay": 2.0, "tool_calls": [{"name": "llm.return_agent", "arguments": {"output": "42"}}]},
           {"match": notsub, "text": "Delegated."},
           {"match": notsub, "text": "It is 42."})
    events = collect_events()
    api("/api/agent/turn", {"chat_id": parent, "prompt": "delegate"})
    wait_for(lambda: any(r["parent_chat_id"] == parent for r in ok("/api/agent/runs")), what="the sub-agent listed with its parent")
    wait_for(lambda: ok("/api/agent/runs") == [] and state(parent)["status"] == "idle", limit=60, what="every run over")
    starts = [e for e in events if e["type"] == "run_started"]
    assert all(e["parent_chat_id"] is None for e in starts if e["chat_id"] == parent), "the parent's own starts name no parent"
    assert any(e["parent_chat_id"] == parent for e in starts), f"the sub-agent's start names its parent ({starts})"


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
        # A run the last scenario left going (a wake-up after a sub-agent's result) would take the next one's scripted replies
        wait_for(lambda: ok("/api/agent/runs") == [], what="no run going on")
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
