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
    status, _ = api("/api/settings", {"auto_confirm": True, "max_turn_steps": 0, "use_tools": True, "trim_old_thinking": False,
                                      "memory_enabled": True, "memory_max_entries": 300, "memory_slice_chars": 3000, "suggest_next": False, **kw})
    assert status < 300


def eq(actual, expected, what):
    assert actual == expected, f"{what}: expected {expected!r}, got {actual!r}"


def has(text, part, what):
    assert part in (text or ""), f"{what}: {part!r} not in {str(text)[:200]!r}"


SCENARIOS = {}


def scenario(fn):
    SCENARIOS[fn.__name__] = fn
    return fn


class Skip(Exception):
    """Raised by a scenario that needs something this backend was not set up with; reported, not counted as a failure."""


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
def stop_in_the_middle_of_a_fold_ends_the_run_and_clears_the_steps():
    """A fold is part of the run: stopping while its calls are going drops them, the run ends as stopped and the checklist
    is gone from the state (and from the pages, which are told no list is left)."""
    settings()
    chat = new_chat("fold-stop")
    events = collect_events()
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000})
    turn(chat, "part 0: " + BIG)
    turn(chat, "part 1: " + BIG)
    request(FAKE, "/__slow", {"prompt_delay": 1.0, "piece_delay": 0.02})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "part 2: " + BIG})
    wait_for(lambda: state(chat).get("steps"), what="a fold under way")
    t0 = time.time()
    eq(api("/api/agent/stop", {"chat_id": chat})[0], 204, "stop")
    s = wait_idle(chat, 10)
    assert time.time() - t0 < 3, "the run ends at once, not when the fold would have"
    eq(s["last_end"]["reason"], "stopped", "run end")
    eq(s["steps"], [], "no steps in the state")
    time.sleep(0.3)
    last = [e["lists"] for e in events if e.get("chat_id") == chat and e["type"] == "run_steps"][-1]
    eq(last, [], "the pages are told no list is left")


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
    for kind in ("memory", "notes", "summary", "facts"):
        assert kind in kinds, f"a {kind} request was made ({kinds})"
    # what to remember and the notes are asked while the prompt is still the cached one: before the summary
    assert kinds.index("memory") < kinds.index("notes") < kinds.index("summary"), "memory, then notes, then the summary"
    turn(chat, "what now?")
    system = calls("main")[-1]["messages"][0]["content"]
    has(system, "Earlier parts of this conversation were summarized", "the fold header")
    has(system, "never reconstruct a value from memory", "the header tells the model not to fill gaps")
    has(system, "The fake summary of the excerpt", "the summary")
    has(system, "fake fact one", "the key facts")
    has(system, "Your own working notes", "the notes")
    has(system, "part 0:", "the user's first message is pinned")


@scenario
def a_fold_in_a_run_of_one_request_and_many_tool_calls_still_sends_a_user_message():
    settings()
    chat = new_chat("fold-one-ask")
    call = {"name": "os.get_date", "arguments": {}}
    script({"text": BIG, "tool_calls": [call], "prompt_tokens": 25000}, {"text": BIG, "tool_calls": [call], "prompt_tokens": 25000},
           {"text": BIG, "tool_calls": [call], "prompt_tokens": 25000}, {"text": "Done."})
    state_after = turn(chat, "port the page to the phone")
    assert "summary" in [c["kind"] for c in calls()], "the run folded"
    mains = calls("main")
    folded = [c for c in mains if "Earlier parts of this conversation were summarized" in c["messages"][0]["content"]]
    assert folded, "a request went out after the fold"
    for c in folded:
        users = [m for m in c["messages"] if m["role"] == "user"]
        assert users, "a request after the fold carries a user message"
    eq(folded[0]["messages"][1]["role"], "user", "the user message leads what the fold left")
    has(folded[0]["messages"][1]["content"], "port the page to the phone", "it is the user's own words")
    eq(roles(chat)[-1], "assistant", "the run ended on the reply")
    eq(messages(chat)[-1]["content"], "Done.", "the final reply")


@scenario
def a_chat_keeps_the_thinking_level_it_was_sent_with_and_a_model_has_a_default_level():
    settings()
    chat = new_chat("think-level")
    chat_out = lambda: ok(f"/api/chats?id={chat}")
    eq(chat_out()["think_level"], None, "no level before a turn names one")
    turn(chat, "first", think="low")
    eq(chat_out()["think_level"], "low", "the level the turn was sent with")
    turn(chat, "second", think=False)
    eq(chat_out()["think_level"], "off", "no thinking is a level too")
    turn(chat, "third", think=True)
    eq(chat_out()["think_level"], "on", "plain thinking")
    turn(chat, "fourth")
    eq(chat_out()["think_level"], "on", "a turn that names none leaves the level as it was")

    model = {"model": "fake.gguf", "provider": "llama-cpp"}
    default = lambda: [d for d in ok("/api/llm/thinking_defaults") if d["model"] == "fake.gguf"]
    eq(api("/api/llm/thinking_default", model)[0], 204, "a default left by an earlier run is taken away")
    eq(default(), [], "no default level to begin with")
    eq(api("/api/llm/thinking_default", {**model, "level": "low"})[0], 204, "a default level is saved")
    eq(default(), [{**model, "level": "low"}], "the default is listed")
    eq(api("/api/llm/thinking_default", {**model, "level": "medium"})[0], 204, "a second one replaces the first")
    eq([d["level"] for d in default()], ["medium"], "one level per model")
    eq(api("/api/llm/thinking_default", {**model, "level": "  "})[0], 400, "an empty level is refused")
    eq(api("/api/llm/thinking_default", {**model, "level": "x" * 40})[0], 400, "an overlong level is refused")
    eq(api("/api/llm/thinking_default", {"model": "nothing.gguf", "level": "low"})[0], 404, "a model that isn't registered")
    eq(api("/api/llm/thinking_default", model)[0], 204, "no level takes the choice away")
    eq(default(), [], "and the list is empty again")


@scenario
def a_reply_leaves_a_suggestion_for_the_next_message_that_the_next_message_replaces():
    chat = new_chat("suggest")
    suggestion = lambda: ok(f"/api/chats?id={chat}")["suggestion"]
    settings()
    turn(chat, "first, with the setting off")
    time.sleep(1.5)
    eq(calls("suggestion"), [], "nothing is asked while the setting is off")
    eq(suggestion(), None, "and nothing is kept")

    settings(suggest_next=True)
    turn(chat, "second")
    wait_for(suggestion, what="a suggestion")
    eq(suggestion(), "And what about the themes?", "the line the model wrote")
    main, asked = calls("main")[-1], calls("suggestion")[-1]
    eq(asked["messages"][0], main["messages"][0], "the system message is the turn's own, so the server's cached prompt serves it")
    eq(asked["tool_names"], main["tool_names"], "and so are the tools")
    eq(asked["messages"][-2]["role"], "assistant", "the reply is the last message before the question")
    eq(asked["max_tokens"] is not None and asked["max_tokens"] <= 64, True, "a few tokens to spend")
    eq(asked["chat_template_kwargs"], {"enable_thinking": False}, "with thinking off")

    # a message of the user's makes it stale at once, and the run's own reply brings a new one
    script({"text": "Slow.", "delay": 2.0})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "third"})
    eq(suggestion(), None, "the line is about a reply that is no longer the newest message")
    wait_idle(chat)
    wait_for(suggestion, what="the suggestion for the new reply")

    # a run the user stops, and a chat of a sub-agent or a plugin, get none
    before = len(calls("suggestion"))
    script({"text": "Never finished.", "delay": 5.0})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "fourth"})
    wait_for(lambda: state(chat)["status"] == "running", what="the run")
    eq(api("/api/agent/stop", {"chat_id": chat})[0], 204, "stop")
    wait_idle(chat, 10)
    time.sleep(1.5)
    eq(len(calls("suggestion")), before, "a stopped run asks for none")
    eq(suggestion(), None, "and keeps none")
    settings()


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
    # (the memory and a plugin's prompt part are in the message when there are any)
    lead = sum(parts[k]["chars"] for k in ("system_prompt", "key_facts", "pinned", "summary", "notes", "memory", "plugin_sections") if k in parts)
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
    events = collect_events()
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000},
           {"text": "Final."})
    for i in range(3):
        turn(chat, f"part {i}: " + BIG)
    kinds = [c["kind"] for c in calls()]
    assert "notes" not in kinds, "no notes request without tools"
    assert "summary" in kinds, "the fold itself still happens"
    # The checklist says why the notes step was skipped, and keeps what the other steps did
    time.sleep(0.5)
    last = [e["lists"][0]["steps"] for e in events if e.get("chat_id") == chat and e["type"] == "run_steps" and e["lists"]][-1]
    by_kind = {step["id"]: step for step in last}
    eq(by_kind["notes"]["state"], "skipped", "the notes step")
    has(by_kind["notes"]["detail"], "no tools", "why it was skipped")
    assert by_kind["summary"]["call"]["prompt"]["total"] > 0, "the summary step keeps what its call read"
    assert by_kind["summary"]["call"]["generation"]["tokens"] > 0, "and what it wrote"
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


def collect_events(seconds=10, token=None):
    """Every event the backend sends this user (or the one `token` signs in) while the stream is open, in a list that
    fills in the background"""
    import threading
    events = []

    def listen():
        req = urllib.request.Request(BACKEND + "/api/live", headers={"Authorization": "Bearer " + (token or TOKEN), "Accept": "text/event-stream"})
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
def the_reply_is_streamed_and_the_stored_one_matches_the_pieces():
    """Only the turn's own request streams; its thinking and text reach the page as reply_piece events, in order, and
    put together they are what gets stored. A tool call's arguments arrive in pieces too and still parse."""
    settings()
    chat = new_chat("stream")
    events = collect_events()
    thinking = "Let me think about this carefully, step by step."
    text = "Here is a fairly long answer, written out in many small pieces."
    script({"tool_calls": [{"name": "os.get_date", "arguments": {"note": "some argument text"}}]},
           {"thinking": thinking, "text": text, "piece_delay": 0.03})
    s = turn(chat, "stream please")
    eq(s["last_end"]["reason"], "answered", "run end")
    eq(messages(chat)[1]["tool_calls"][0]["tool_name"], "os.get_date", "the streamed call")
    eq(messages(chat)[-1]["content"], text, "the stored reply")
    eq(messages(chat)[-1].get("thinking"), thinking, "the stored thinking")
    time.sleep(0.3)
    pieces = [e for e in events if e.get("chat_id") == chat and e["type"] == "reply_piece"]
    eq("".join(p["thinking"] for p in pieces), thinking, "the thinking pieces")
    eq("".join(p["text"] for p in pieces), text, "the text pieces")
    assert 2 <= len(pieces) < len(text) // 7, f"pieces are batched, not one per token: {len(pieces)}"
    assert all(c["stream"] == (c["kind"] == "main") for c in calls()), "only the turn streams"


@scenario
def the_prompt_being_read_and_the_speed_are_reported_for_the_call_in_flight():
    """A streamed call asks the server for its progress on the prompt and its speed: a page is told how far the prompt is
    read (the cached part counted as read), then how fast the reply comes, and the call ends with nothing. The same
    picture is in the turn's state for a page that opens midway."""
    settings()
    chat = new_chat("progress")
    events = collect_events()
    script({"text": "A reply with some length to it, so there are several pieces.", "prompt_tokens": 2000, "cached": 800,
            "prompt_delay": 0.4, "piece_delay": 0.02})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "go"})
    wait_for(lambda: ((state(chat).get("call") or {}).get("prompt") or {}).get("processed", 0) > 800, what="the prompt being read")
    midway = state(chat)["call"]
    eq(midway["kind"], "reply", "the call in the state")
    assert midway["prompt"]["total"] == 2000 and midway["prompt"]["cached"] == 800, midway
    assert midway["prompt"]["processed"] < 2000 and midway["generation"] is None, midway
    wait_idle(chat)
    time.sleep(0.3)
    calls_seen = [e["call"] for e in events if e.get("chat_id") == chat and e["type"] == "call_progress"]
    assert calls_seen[0] == {"kind": "reply", "prompt": None, "generation": None}, calls_seen[0]
    processed = [c["prompt"]["processed"] for c in calls_seen if c and c["prompt"] and c["generation"] is None]
    eq(processed[0], 800, "the first report is the cached part")
    assert processed == sorted(processed) and processed[-1] == 2000, processed
    speeds = [c["generation"] for c in calls_seen if c and c["generation"]]
    assert speeds and speeds[0]["tokens"] >= 1 and speeds[0]["tokens_per_second"] == 20.0, speeds
    eq(calls_seen[-1], None, "the call ends with nothing")
    eq(state(chat)["call"], None, "no call in the state once it is over")


@scenario
def a_fold_reports_its_steps_and_each_calls_progress():
    """The fold's four model calls are steps of a checklist a page can show: every one is waiting, then running, then
    done, in order, and each call reports its own prompt and speed. When the fold is over the checklist is gone."""
    settings()
    chat = new_chat("fold-steps")
    events = collect_events()
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000})
    for i in range(3):
        turn(chat, f"part {i}: " + BIG)
    time.sleep(0.5)
    snapshots = [e["lists"] for e in events if e.get("chat_id") == chat and e["type"] == "run_steps"]
    assert snapshots, "a fold was reported"
    # Each fold is a run of snapshots ending with the one that says no list is left
    folds, current = [], []
    for snapshot in snapshots:
        if not snapshot:
            folds.append(current)
            current = []
        else:
            eq(len(snapshot), 1, "one list at a time here")
            current.append(snapshot[0])
    eq(current, [], "the last fold was ended")
    assert folds, "a fold ended"
    ids = ["memory", "notes", "summary", "facts"]
    for fold in folds:
        eq(fold[0]["title"], "Compacting the chat", "the work is named")
        eq(len({snap["id"] for snap in fold}), 1, "one list throughout")
        eq([(step["id"], step["state"]) for step in fold[0]["steps"]], [(i, "pending") for i in ids], "all waiting at the start")
        eq([step["label"] for step in fold[0]["steps"]], ["Saving memory", "Saving notes", "Summarizing", "Extracting facts"], "the labels come from the backend")
        for step_id in ids:
            states = [next(step["state"] for step in snap["steps"] if step["id"] == step_id) for snap in fold]
            # Each step is only ever waiting, then running, then done
            eq([a for i, a in enumerate(states) if i == 0 or a != states[i - 1]], ["pending", "running", "done"], f"the {step_id} step")
    # A finished step says what it produced, and keeps what its call came to
    for fold in folds:
        done = {step["id"]: step for step in fold[-1]["steps"]}
        assert "messages folded" in (done["summary"]["detail"] or ""), done["summary"]
        assert "facts found" in (done["facts"]["detail"] or ""), done["facts"]
        assert done["summary"]["call"]["prompt"], done["summary"]
    # A step's call is reported with its step: the prompt, then the speed
    for step_id in ids:
        mine = [next(step for step in e["lists"][0]["steps"] if step["id"] == step_id)["call"]
                for e in events if e.get("chat_id") == chat and e["type"] == "run_steps" and e["lists"]]
        mine = [c for c in mine if c]
        assert any(c["prompt"] for c in mine) and any(c["generation"] for c in mine), f"the {step_id} call reported its prompt and speed"
    # ... and not as the run's own call
    assert not any(e["call"] and e["call"]["kind"] != "reply" for e in events if e.get("chat_id") == chat and e["type"] == "call_progress")
    eq(state(chat)["steps"], [], "no checklist in the state afterwards")


@scenario
def a_page_opened_during_a_fold_gets_its_steps_from_the_state():
    """The steps of the work going on are in the turn's state, so a page that is opened (or a chat switched to) in the
    middle of a fold shows what a page that watched it from the start does: its title, the steps done, the one running."""
    settings()
    chat = new_chat("fold-restore")
    script({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000}, {"text": "And again.", "prompt_tokens": 25000})
    turn(chat, "part 0: " + BIG)
    turn(chat, "part 1: " + BIG)
    request(FAKE, "/__slow", {"prompt_delay": 0.4, "piece_delay": 0.02})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "part 2: " + BIG})
    wait_for(lambda: state(chat).get("steps"), what="a fold under way")
    midway = state(chat)["steps"]
    eq(len(midway), 1, "one list")
    eq(midway[0]["title"], "Compacting the chat", "the work's title in the state")
    assert any(step["state"] == "running" for step in midway[0]["steps"]), midway
    wait_for(lambda: any(step["state"] == "done" and step["detail"] for lst in state(chat)["steps"] for step in lst["steps"]), what="a step done")
    wait_idle(chat)
    eq(state(chat)["steps"], [], "no steps in the state once it is over")


@scenario
def a_page_opened_midway_gets_the_reply_so_far_and_the_pieces_continue_it():
    """The state read during a reply carries what was sent of it; the pieces numbered after its `seq` add exactly the
    rest, nothing twice, nothing missing. Once the reply is stored the state has none."""
    settings()
    chat = new_chat("midway")
    events = collect_events()
    thinking = "thinking it over " * 6
    text = "A reply long enough to be read in the middle of being written, piece by piece."
    script({"thinking": thinking, "text": text, "piece_delay": 0.04})
    api("/api/agent/turn", {"chat_id": chat, "prompt": "go"})
    wait_for(lambda: (state(chat).get("reply") or {}).get("seq", 0) >= 3, what="a reply under way")
    so = state(chat)["reply"]
    assert so["number"] == 1 and so["thinking"] and len(so["thinking"]) < len(thinking), so
    wait_idle(chat)
    time.sleep(0.3)
    later = [e for e in events if e.get("chat_id") == chat and e["type"] == "reply_piece" and e["seq"] > so["seq"]]
    eq([e["seq"] for e in later], list(range(so["seq"] + 1, so["seq"] + 1 + len(later))), "the pieces after it, in order")
    eq(so["thinking"] + "".join(e["thinking"] for e in later), thinking, "the thinking put together")
    eq(so["text"] + "".join(e["text"] for e in later), text, "the text put together")
    eq(state(chat).get("reply"), None, "no reply in the state once it is stored")


@scenario
def another_users_stream_hears_nothing_of_a_streamed_reply():
    """Each stream remembers whose chats are whose: the owner's gets every piece, another user's none."""
    settings()
    chat = new_chat("private")
    other = token_for(2, "testuser", "user")
    if request(BACKEND, "/api/agent/runs", token=other)[0] == 401:
        print("      (no second user in this database: 401, ownership not checked)")
        return
    mine, theirs = collect_events(), collect_events(token=other)
    script({"thinking": "private thoughts " * 5, "text": "A private reply, in pieces.", "piece_delay": 0.02})
    turn(chat, "go")
    time.sleep(0.3)
    assert any(e["type"] == "reply_piece" and e.get("chat_id") == chat for e in mine), "the owner's stream has the pieces"
    eq([e["type"] for e in theirs if e.get("chat_id") == chat], [], "another user's stream")


@scenario
def a_regenerated_reply_restarts_the_live_one():
    """An empty reply is asked again; the page is told to drop what it showed of the first one."""
    settings()
    chat = new_chat("restart")
    events = collect_events()
    script({"thinking": "hmm", "text": ""}, {"text": "Second try."})
    eq(turn(chat, "go")["last_end"]["reason"], "answered", "run end")
    time.sleep(0.3)
    mine = [e["type"] for e in events if e.get("chat_id") == chat and e["type"] in ("reply_piece", "reply_restart")]
    eq(mine, ["reply_piece", "reply_restart", "reply_piece"], "a piece, the restart, the new reply")


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


def memory_clear():
    ok("/api/memory/clear", {})


def system_of_last_main_call():
    content = calls("main")[-1]["messages"][0]["content"]
    return content if isinstance(content, str) else " ".join(part.get("text", "") for part in content)


FOLD_REPLIES = ({"text": "Noted.", "prompt_tokens": 25000}, {"text": "Noted again.", "prompt_tokens": 25000},
                {"text": "And again.", "prompt_tokens": 25000}, {"text": "Final."})


@scenario
def a_fold_asks_what_to_remember_first_on_the_live_prompt_and_keeps_only_quoted_entries():
    settings()
    memory_clear()
    chat = new_chat("memory-fold")
    fake("/__memory", [json.dumps({"memories": [
        {"category": "preferences", "text": "Answers in Russian.", "quote": "I always want answers in Russian", "replaces": None},
        {"category": "about_you", "text": "Runs Arch with Hyprland.", "quote": "i run arch with hyprland", "replaces": None},
        {"category": "about_you", "text": "Is a senior engineer.", "quote": "I am a senior engineer", "replaces": None},
    ]})])
    script(*FOLD_REPLIES)
    turn(chat, "part 0: I always want answers in Russian. I run Arch with Hyprland. " + BIG)
    for i in (1, 2):
        turn(chat, f"part {i}: " + BIG)
    kinds = [c["kind"] for c in calls()]
    assert kinds.index("memory") < kinds.index("notes") < kinds.index("summary"), f"memory before the notes and the summary ({kinds})"
    memory = next(c for c in calls() if c["kind"] == "memory")
    notes = next(c for c in calls() if c["kind"] == "notes")
    # The same live prompt as the notes request, so the model server's cache serves it: only the question differs
    eq(memory["messages"][:-1], notes["messages"][:-1], "the live prompt in front of the question")
    eq((memory["tool_names"], memory["chat_template_kwargs"]), (notes["tool_names"], notes["chat_template_kwargs"]), "the same tools and thinking setting")
    assert memory["max_tokens"] and memory["max_tokens"] <= 6000, f"the reply is capped ({memory['max_tokens']})"
    has(memory["messages"][-1]["content"], "(nothing yet)", "the question says nothing is remembered yet")

    entries = ok("/api/memory")["entries"]
    eq({e["text"] for e in entries}, {"Answers in Russian.", "Runs Arch with Hyprland."}, "only the entries whose quote the user wrote")
    russian = next(e for e in entries if e["text"] == "Answers in Russian.")
    eq((russian["learned"], russian["category"]), (True, "preferences"), "a learned entry")
    has(russian["quote"], "answers in Russian", "its quote")

    # The fold took the memory into the chat's prompt, how they like things done first
    turn(chat, "what now?")
    system = system_of_last_main_call()
    has(system, "What is remembered about the user", "the memory in the prompt")
    assert system.index("Answers in Russian.") < system.index("Runs Arch with Hyprland."), "preferences before about_you"
    parts = {p["kind"]: p for p in ok(f"/api/chats/context?chat_id={chat}")["parts"]}
    assert parts["memory"]["chars"] > 0, "the context shows the memory as a part of its own"
    memory_clear()


@scenario
def a_new_chat_takes_the_memory_once_and_a_later_entry_waits_for_the_next_chat():
    settings()
    memory_clear()
    ok("/api/memory", {"category": "preferences", "text": "Answers in Russian."})
    chat = new_chat("memory-start")
    script({"text": "one"}, {"text": "two"}, {"text": "three"}, {"text": "four"})
    turn(chat, "hi")
    first = system_of_last_main_call()
    has(first, "Answers in Russian.", "the memory at the chat's start")
    ok("/api/memory", {"category": "about_you", "text": "Runs Arch."})
    turn(chat, "again")
    eq(system_of_last_main_call(), first, "the same bytes on the next turn: the prompt cache stays valid")
    later = new_chat("memory-start-2")
    turn(later, "hi")
    has(system_of_last_main_call(), "Runs Arch.", "a chat started after the entry has it")
    # Switching memory off takes it out of the prompt at once, and on brings the same snapshot back
    settings(memory_enabled=False)
    turn(chat, "with memory off")
    assert "What is remembered" not in system_of_last_main_call(), "memory off: not in the prompt"
    settings()
    turn(chat, "and on")
    eq(system_of_last_main_call(), first, "memory on again: the chat's snapshot")
    memory_clear()


@scenario
def with_memory_off_nothing_is_taken_in_and_a_fold_does_not_ask():
    settings(memory_enabled=False)
    memory_clear()
    ok("/api/memory", {"category": "preferences", "text": "Answers in Russian."})
    chat = new_chat("memory-off")
    events = collect_events()
    script(*FOLD_REPLIES)
    turn(chat, "part 0: " + BIG)
    assert "What is remembered" not in system_of_last_main_call(), "memory off: not in a new chat's prompt"
    for i in (1, 2):
        turn(chat, f"part {i}: " + BIG)
    kinds = [c["kind"] for c in calls()]
    assert "memory" not in kinds, f"no memory request with memory off ({kinds})"
    assert "summary" in kinds, "the fold itself still happens"
    time.sleep(0.5)
    last = [e["lists"][0]["steps"] for e in events if e.get("chat_id") == chat and e["type"] == "run_steps" and e["lists"]][-1]
    step = next(s for s in last if s["id"] == "memory")
    eq(step["state"], "skipped", "the memory step")
    has(step["detail"], "memory is off", "why")
    settings()
    memory_clear()


@scenario
def the_memory_api_adds_changes_and_forgets_and_the_limits_are_checked():
    settings()
    memory_clear()
    entry = ok("/api/memory", {"category": "misc", "text": "  Likes   tea. "})
    eq((entry["text"], entry["learned"], entry["quote"]), ("Likes tea.", False, None), "an entry the user wrote, tidied")
    eq(api("/api/memory", {"category": "nope", "text": "x"})[0], 400, "an unknown category")
    eq(api("/api/memory", {"category": "misc", "text": "   "})[0], 400, "an empty text")
    eq(api("/api/memory", {"category": "misc", "text": "x" * 1001})[0], 400, "a text that is too long")
    changed = ok("/api/memory/update", {"id": entry["id"], "category": "preferences", "text": "Likes green tea."})
    eq((changed["category"], changed["text"], changed["learned"]), ("preferences", "Likes green tea.", False), "a changed entry")
    eq(api("/api/memory/update", {"id": 999999, "category": "misc", "text": "x"})[0], 404, "changing an entry that is not there")
    eq(api(f"/api/memory?id={entry['id']}", None, "DELETE")[0], 204, "deleting")
    eq(api(f"/api/memory?id={entry['id']}", None, "DELETE")[0], 404, "deleting it twice")
    ok("/api/memory", {"category": "misc", "text": "One."})
    ok("/api/memory", {"category": "misc", "text": "Two."})
    eq(len(ok("/api/memory")["entries"]), 2, "two entries")
    eq(ok("/api/memory/clear", {})["removed"], 2, "cleared")
    eq(ok("/api/memory")["entries"], [], "nothing left")
    for bad in ({"memory_max_entries": 9}, {"memory_max_entries": 2001}, {"memory_slice_chars": 499}, {"memory_slice_chars": 12001}):
        eq(api("/api/settings", bad)[0], 400, f"{bad} is out of range")
    ok("/api/settings", {"memory_max_entries": 2000, "memory_slice_chars": 12000})
    got = ok("/api/settings")
    eq((got["memory_enabled"], got["memory_max_entries"], got["memory_slice_chars"]), (True, 2000, 12000), "the limits as set")
    settings()


@scenario
def the_learned_limit_drops_the_oldest_learned_entries_and_the_slice_limit_caps_a_prompt():
    settings(memory_max_entries=10, memory_slice_chars=500)
    memory_clear()
    ok("/api/memory", {"category": "misc", "text": "My own entry that is never dropped."})
    chat = new_chat("memory-limits")
    sentences = [f"I like fruit number {i:02d} very much." for i in range(1, 13)]
    fake("/__memory", [json.dumps({"memories": [
        {"category": "about_you", "text": f"Likes fruit {i:02d}.", "quote": sentence, "replaces": None} for i, sentence in enumerate(sentences, 1)
    ]})])
    script(*FOLD_REPLIES)
    turn(chat, "part 0: " + " ".join(sentences) + " " + BIG)
    for i in (1, 2):
        turn(chat, f"part {i}: " + BIG)
    entries = ok("/api/memory")["entries"]
    learned = sorted(e["text"] for e in entries if e["learned"])
    eq(learned, [f"Likes fruit {i:02d}." for i in range(3, 13)], "the ten newest of the twelve the model wrote are kept")
    assert any("never dropped" in e["text"] for e in entries), "the user's own entry is not counted and not dropped"
    turn(chat, "what now?")
    system = system_of_last_main_call()
    block = system[system.index("What is remembered"):].split("\n\n")[0]
    assert len(block) <= 500, f"the memory in the prompt stays within the slice ({len(block)})"
    has(block, "never dropped", "the user's own entry goes first")
    settings()
    memory_clear()


def tool_results(chat):
    return [m for m in messages(chat) if m["role"] == "tool"]


class SkillsVariant:
    """One of the two skills plugins the same scenarios run against: the built-in one and its port in plugin-sdk/examples/skills."""

    def __init__(self, name, subname, tool_domain, folder, tab_id):
        self.plugin = {"plugin_name": name, "plugin_subname": subname}
        self.tool_domain, self.folder, self.tab_id = tool_domain, folder, tab_id
        self.tab_url = f"/api/plugins/tab?plugin_name={name}&plugin_subname={subname}"

    def t(self, tool):
        return f"{self.tool_domain}.{tool}"

    def installed(self):
        return any((p["plugin_name"], p["plugin_subname"]) == tuple(self.plugin.values()) for p in ok("/api/plugins"))


BUILT_IN_SKILLS = SkillsVariant("knowledge", "skills", "skills", "/knowledge/skills/", "skills")
SKILLS_PORT = SkillsVariant("example", "skills", "example_skills", "/example/skills/", "example-skills")


def both_skills_plugins(fn):
    """Registers the scenario for the built-in skills plugin under its own name, and for the example port under the same name
    with `_in_the_example_port`: the port has to behave the same, which is what makes it a port. The port's run is skipped
    when this backend has not got it installed."""
    SCENARIOS[fn.__name__] = lambda: fn(BUILT_IN_SKILLS)

    def on_the_port():
        if not SKILLS_PORT.installed():
            raise Skip("the example/skills plugin is not installed in this backend's external_plugins_dir")
        fn(SKILLS_PORT)

    SCENARIOS[fn.__name__ + "_in_the_example_port"] = on_the_port
    return fn


@both_skills_plugins
def skills_tools_ask_for_the_users_folder_and_the_prompt_lists_what_was_written(V):
    settings(auto_confirm=False)
    eq(api("/api/plugins/enable", {**V.plugin, "enabled": True})[0], 204, "enable the skills plugin")
    chat = new_chat("skills")
    source = "/tmp/fake_skill_src_%d" % os.getpid()
    os.makedirs(source + "/notes", exist_ok=True)
    open(source + "/SKILL.md", "w").write("---\nname: from-folder\ndescription: Installed from a folder.\n---\n\nDo the folder thing.\n")
    open(source + "/notes/a.md", "w").write("note A")
    write = {"name": V.t("write"), "arguments": {"name": "lab-report", "description": "Use for a lab report.\nSecond line.", "content": "# Steps\n1. Do it"}}
    script({"tool_calls": [write]})
    s = turn(chat, "save a skill")
    eq(s["status"], "waiting_for_permission", "a write asks first")
    has(s["pending"][0]["permission"]["escalation"]["ui_message"], V.folder + "1", "it names the user's own skills folder")
    script({"tool_calls": [{"name": V.t("read"), "arguments": {"name": "lab-report"}},
                           {"name": V.t("add_file"), "arguments": {"name": "lab-report", "path": "scripts/run.sh", "content": "echo hi"}},
                           {"name": V.t("read_file"), "arguments": {"name": "lab-report", "path": "scripts/run.sh"}},
                           {"name": V.t("read_file"), "arguments": {"name": "lab-report", "path": "../../2/x"}},
                           {"name": V.t("list"), "arguments": {}}]},
           {"tool_calls": [{"name": V.t("install"), "arguments": {"path": source}}]})
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "permanent"}]})[0], 202, "approve the folder")
    s = wait_idle(chat)
    # the read-only tools and the add ran on the grant; the install also reads a folder nobody has approved
    eq(s["status"], "waiting_for_permission", "an install asks to read its source")
    has(s["pending"][0]["permission"]["escalation"]["ui_message"], source, "it names the source")
    results = [str(m["content"]) for m in tool_results(chat)]
    has(results[1], "Do it", "skills.read gives the instructions")
    has(results[1], "lab-report", "skills.read names the skill")
    has(results[3], "echo hi", "a file added to the skill is read back")
    has(results[4], "climbs out", "a path leaving the skill is refused")
    has(results[5], "Use for a lab report. Second line.", "the list shows the description on one line")
    script({"tool_calls": [{"name": V.t("delete"), "arguments": {"name": "lab-report"}}]}, {"text": "gone"})
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})[0], 202, "approve reading the source")
    s = wait_idle(chat)
    eq(s["status"], "waiting_for_permission", "a delete asks on its own")
    eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})[0], 202, "approve the delete")
    wait_idle(chat)
    # a new chat's prompt lists the skill that is left, and not the deleted one
    chat2 = new_chat("skills-2")
    script({"text": "ok"})
    turn(chat2, "hello")
    system = system_of_last_main_call()
    has(system, "- from-folder: Installed from a folder.", "the prompt lists the installed skill")
    assert "lab-report" not in system, "a deleted skill is not listed"
    chat3 = new_chat("skills-3")
    script({"tool_calls": [{"name": V.t("delete"), "arguments": {"name": "from-folder"}}]})
    eq(turn(chat3, "clean up")["status"], "waiting_for_permission", "cleaning up asks too")
    script({"text": "ok"})
    api("/api/agent/answer", {"chat_id": chat3, "decisions": [{"index": 0, "allowance": "only_now"}]})
    wait_idle(chat3)
    api("/api/plugins/enable", {**V.plugin, "enabled": False})
    os.system("rm -rf " + source)
    settings()


@both_skills_plugins
def another_users_skills_are_not_listed_and_not_readable(V):
    made = ok("/api/users", {"username": "skills-bob-%d" % os.getpid(), "password": "bob-pass-123"})
    bob_id = made["id"]
    BOB = token_for(bob_id, made["username"], "user")
    try:
        settings(auto_confirm=True)
        full = {"auto_confirm": True, "max_turn_steps": 0, "use_tools": True, "trim_old_thinking": False,
                "memory_enabled": True, "memory_max_entries": 300, "memory_slice_chars": 3000}
        eq(request(BACKEND, "/api/settings", full, token=BOB)[0] // 100, 2, "bob's settings")
        eq(api("/api/plugins/enable", {**V.plugin, "enabled": True})[0], 204, "enable the skills plugin")
        owner = new_chat("skills-owner")
        script({"tool_calls": [{"name": V.t("write"), "arguments": {"name": "owner-secret", "description": "The owner's own skill.", "content": "Private."}}]}, {"text": "saved"})
        turn(owner, "save it")

        def bob_turn(prompt):
            status, chat = request(BACKEND, "/api/chats", {"name": "skills-bob", "launch_profile_id": PROFILE, "tools_enabled": True}, token=BOB)
            eq(status, 200, "bob's chat")
            eq(request(BACKEND, "/api/agent/turn", {"chat_id": chat["id"], "prompt": prompt}, token=BOB)[0], 202, "bob's turn")
            wait_for(lambda: request(BACKEND, f"/api/agent/turn?chat_id={chat['id']}", token=BOB)[1]["status"] != "running", what="bob's run")
            msgs = request(BACKEND, f"/api/chats/messages?chat_id={chat['id']}&limit=200", token=BOB)[1]["messages"]
            return sorted(msgs, key=lambda m: m["id"])

        script({"tool_calls": [{"name": V.t("read"), "arguments": {"name": "owner-secret"}},
                               {"name": V.t("delete"), "arguments": {"name": "owner-secret"}},
                               {"name": V.t("list"), "arguments": {}}]}, {"text": "done"})
        results = [str(m["content"]) for m in bob_turn("read it") if m["role"] == "tool"]
        has(results[0], "not found", "bob cannot read the owner's skill")
        has(results[1], "not found", "bob cannot delete it")
        eq(results[2], "[]", "bob's list is empty")
        assert "owner-secret" not in system_of_last_main_call(), "bob's prompt does not list the owner's skill"

        again = new_chat("skills-owner-again")
        script({"tool_calls": [{"name": V.t("read"), "arguments": {"name": "owner-secret"}}]}, {"text": "ok"})
        turn(again, "check")
        has(str([m["content"] for m in tool_results(again)]), "Private.", "the owner still reads their own skill")
        has(system_of_last_main_call(), "- owner-secret: The owner's own skill.", "and their prompt lists it")
        eq(api("/api/plugins/tab/action", {**V.plugin, "action": "delete", "args": {"row": "owner-secret"}})[0], 200, "clean up")
    finally:
        api("/api/plugins/enable", {**V.plugin, "enabled": False})
        status, _ = api(f"/api/users?id={bob_id}", None, "DELETE")
        eq(status, 204, "the user is deleted again")
        settings()


@both_skills_plugins
def switching_the_skills_plugin_off_hides_it_and_keeps_the_skills(V):
    settings(auto_confirm=True)
    enable = lambda on: api("/api/plugins/enable", {**V.plugin, "enabled": on})[0]
    eq(enable(True), 204, "enable")
    chat = new_chat("skills-onoff")
    script({"tool_calls": [{"name": V.t("write"), "arguments": {"name": "keeper", "description": "Kept while off.", "content": "Body."}}]}, {"text": "saved"})
    turn(chat, "save it")
    eq(enable(False), 204, "disable")
    eq([t for t in ok("/api/plugins/tabs") if t["id"] == V.tab_id], [], "the tab is gone while it is off")
    off = new_chat("skills-off")
    script({"tool_calls": [{"name": V.t("read"), "arguments": {"name": "keeper"}}]}, {"text": "ok"})
    turn(off, "read it")
    has(str([m["content"] for m in tool_results(off)]), "no tool named", "its tools are gone while it is off")
    assert "# Skills" not in system_of_last_main_call(), "and its part of the prompt"
    eq(enable(True), 204, "enable again")
    on = new_chat("skills-on")
    script({"text": "ok"})
    turn(on, "hello")
    has(system_of_last_main_call(), "- keeper: Kept while off.", "the skill is listed again, nothing was lost")
    eq(api("/api/plugins/tab/action", {**V.plugin, "action": "delete", "args": {"row": "keeper"}})[0], 200, "clean up")
    enable(False)
    settings()


@both_skills_plugins
def a_skill_switched_off_is_hidden_from_the_model_and_kept_for_the_user(V):
    settings(auto_confirm=True)
    eq(api("/api/plugins/enable", {**V.plugin, "enabled": True})[0], 204, "enable")
    tab = lambda action, args: api("/api/plugins/tab/action", {**V.plugin, "action": action, "args": args})
    chat = new_chat("skill-switch")
    script({"tool_calls": [{"name": V.t("write"), "arguments": {"name": "sleeper", "description": "Only for the sleeper case.", "content": "Body."}}]}, {"text": "saved"})
    turn(chat, "save it")
    eq(tab("enable", {"row": "sleeper", "on": False})[0], 200, "switch off")

    off = new_chat("skill-off")
    script(
        {"tool_calls": [{"name": V.t("read"), "arguments": {"name": "sleeper"}}]},
        {"tool_calls": [{"name": V.t("list"), "arguments": {}}]},
        {"tool_calls": [{"name": V.t("write"), "arguments": {"name": "sleeper", "description": "Mine now.", "content": "Overwritten."}}]},
        {"text": "ok"},
    )
    turn(off, "use it")
    assert "sleeper" not in system_of_last_main_call(), "a skill that is off is not in the prompt"
    results = [m["content"] for m in tool_results(off)]
    has(results[0], "not found", "skills.read does not open it")
    assert "sleeper" not in results[1], "skills.list does not show it"
    has(results[2], "turned off", "the model cannot write over it")
    row = [r for r in ok(V.tab_url)["sections"][1]["blocks"][0]["rows"] if r["id"] == "sleeper"][0]
    eq(row["switch"]["on"], False, "the user's list still shows it, off")

    eq(tab("enable", {"row": "sleeper", "on": True})[0], 200, "switch on")
    # The model can turn off what it made, never what the user added, and cannot turn anything back on
    mine = new_chat("skill-self-off")
    script(
        {"tool_calls": [{"name": V.t("disable"), "arguments": {"name": "sleeper"}}]},
        {"tool_calls": [{"name": V.t("disable"), "arguments": {"name": "sleeper"}}]},
        {"text": "ok"},
    )
    turn(mine, "retire it")
    results = [m["content"] for m in tool_results(mine)]
    has(results[0], "turned off", "the model turned off its own skill")
    has(results[1], "turned off by the user", "and cannot touch it once it is off")
    row = [r for r in ok(V.tab_url)["sections"][1]["blocks"][0]["rows"] if r["id"] == "sleeper"][0]
    eq(row["switch"]["on"], False, "the user sees it off")
    eq(tab("enable", {"row": "sleeper", "on": True})[0], 200, "the user turns it back on")
    on = new_chat("skill-on")
    script({"text": "ok"})
    turn(on, "hello")
    has(system_of_last_main_call(), "- sleeper: Only for the sleeper case.", "listed again, unchanged by the model's attempt")
    eq(tab("delete", {"row": "sleeper"})[0], 200, "clean up")
    api("/api/plugins/enable", {**V.plugin, "enabled": False})
    settings()


@both_skills_plugins
def the_model_cannot_turn_off_or_change_a_skill_the_user_added(V):
    settings(auto_confirm=True)
    eq(api("/api/plugins/enable", {**V.plugin, "enabled": True})[0], 204, "enable")
    # A skill the user added goes through the tab's upload, a multipart form
    boundary = "scenarioboundary"
    skill_md = "---\nname: theirs\ndescription: Added by the user.\n---\nBody.\n"
    parts = "".join(
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        for name, value in [("plugin_name", V.plugin["plugin_name"]), ("plugin_subname", V.plugin["plugin_subname"]), ("action", "upload")]
    ) + f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"theirs/SKILL.md\"\r\n\r\n{skill_md}\r\n--{boundary}--\r\n"
    req = urllib.request.Request(BACKEND + "/api/plugins/tab/upload", data=parts.encode(), method="POST",
                                 headers={"Authorization": "Bearer " + TOKEN, "Content-Type": "multipart/form-data; boundary=" + boundary})
    eq(urllib.request.urlopen(req, timeout=30).status, 200, "the user adds a skill")

    chat = new_chat("skill-user-owned")
    script(
        {"tool_calls": [{"name": V.t("disable"), "arguments": {"name": "theirs"}}]},
        {"tool_calls": [{"name": V.t("delete"), "arguments": {"name": "theirs"}}]},
        {"text": "ok"},
    )
    turn(chat, "retire it")
    results = [m["content"] for m in tool_results(chat)]
    has(results[0], "was added by the user", "turning it off is refused")
    has(results[1], "was added by the user", "and so is deleting it")
    row = [r for r in ok(V.tab_url)["sections"][1]["blocks"][0]["rows"] if r["id"] == "theirs"][0]
    eq(row["switch"]["on"], True, "it is still on")
    eq(api("/api/plugins/tab/action", {**V.plugin, "action": "delete", "args": {"row": "theirs"}})[0], 200, "clean up")
    api("/api/plugins/enable", {**V.plugin, "enabled": False})
    settings()


@both_skills_plugins
def auto_confirm_grants_every_folder_a_call_needs_before_it_runs(V):
    settings(auto_confirm=True)
    eq(api("/api/plugins/enable", {**V.plugin, "enabled": True})[0], 204, "enable")
    # skills.install needs read access to the zip's folder and write access to the skills folder, which are offered one at a time
    import tempfile, zipfile
    folder = tempfile.mkdtemp(prefix="scenario-zip-")
    path = os.path.join(folder, "zipped.zip")
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("zipped/SKILL.md", "---\nname: zipped\ndescription: From a zip.\n---\nBody.\n")
    chat = new_chat("auto-confirm-two-folders")
    script({"tool_calls": [{"name": V.t("install"), "arguments": {"path": path}}]}, {"text": "done"})
    turn(chat, "install it")
    results = tool_results(chat)
    eq(len(results), 1, "one call, no retry")
    has(str(results[0]["content"]), "zipped", "the install ran on the first call")
    eq(api("/api/plugins/tab/action", {**V.plugin, "action": "delete", "args": {"row": "zipped"}})[0], 200, "clean up")
    api("/api/plugins/enable", {**V.plugin, "enabled": False})
    settings()


@scenario
def auto_confirm_grants_every_command_of_a_shell_line_before_it_runs():
    settings(auto_confirm=True)
    chat = new_chat("auto-confirm-shell-line")
    script({"tool_calls": [{"name": "os.execute_command", "arguments": {"command": "cd /tmp && echo first-ok && ls -d /tmp | wc -l"}}]}, {"text": "done"})
    turn(chat, "run it")
    results = tool_results(chat)
    eq(len(results), 1, "one call, no retry")
    has(str(results[0]["content"]), "first-ok", "the whole line ran on the first call")
    settings()


HELLO = {"plugin_name": "example", "plugin_subname": "hello"}


def hello_plugin():
    """The example/hello plugin as the plugin list shows it, or a skip when this backend has not got it installed."""
    listed = {(p["plugin_name"], p["plugin_subname"]): p for p in ok("/api/plugins")}
    if ("example", "hello") not in listed:
        raise Skip("the example/hello plugin is not installed in this backend's external_plugins_dir")
    return listed[("example", "hello")]


def hello_on(greeting):
    ok("/api/plugins/settings", {**HELLO, "settings": {"greeting": greeting}})
    ok("/api/plugins/enable", {**HELLO, "enabled": True})


@scenario
def external_plugin_tool_runs_through_the_model_loop():
    """A tool of an external plugin (the hello-world-rust example, installed in the backend's external_plugins_dir) is called by
    the model like any built-in one: the backend sends the call to the plugin's process, which answers, and the answer goes back
    to the model."""
    hello_plugin()
    hello_on("Howdy")
    settings()
    chat = new_chat("external-tool")
    script({"tool_calls": [{"name": "hello.greet", "arguments": {"name": "Ada"}}]}, {"text": "Greeted."})
    s = turn(chat, "Greet Ada with the hello plugin")
    eq(s["last_end"]["reason"], "answered", "run end")
    results = tool_results(chat)
    eq(len(results), 1, "one tool result")
    has(str(results[0]["content"]), "Howdy, Ada!", "the plugin's greeting, with the greeting from its settings, reached the model")
    has(str(results[0]["content"]), "for user 1", "the call carried the chat's user")
    second = calls("main")[1]["messages"]
    has(str(second[-1]["content"]), "Howdy, Ada!", "the second request carries the result")


@scenario
def an_external_plugins_settings_tab_prompt_section_and_routes_behave_like_a_built_ins():
    """The hello plugin declares settings and a tab in its manifest and implements the other hooks: the backend checks the
    settings before the plugin sees them, draws its tab, adds its prompt section to a chat that has tools, forwards its routes
    and stops serving all of it when it is switched off."""
    hello_plugin()
    status, body = api("/api/plugins/settings", {**HELLO, "settings": {"greeting": 5}})
    eq(status, 400, "a setting of the wrong type is refused")
    has(str(body), "greeting", "the refusal names the setting")
    hello_on("Hey")
    tabs = [t for t in ok("/api/plugins/tabs") if t["plugin_subname"] == "hello"]
    eq([t["id"] for t in tabs], ["hello"], "the plugin's tab is listed")

    tab = ok("/api/plugins/tab?plugin_name=example&plugin_subname=hello")
    blocks = tab["sections"][0]["blocks"]
    has(blocks[0]["text"], "\"Hey\"", "the tab shows the greeting from the settings")
    eq(blocks[1]["on"], False, "the switch starts off")
    tab = ok("/api/plugins/tab/action", {**HELLO, "action": "loud", "args": {"on": True}})
    eq(tab["sections"][0]["blocks"][1]["on"], True, "the action's answer is the tab as it is after")
    has(tab["message"], "capitals", "the action's message")

    reply = ok("/api/plugins/example/hello/greeting")
    eq(reply, {"greeting": "Hey", "asked_by": 1}, "the plugin's own route, with the caller")

    settings()
    chat = new_chat("external-hooks")
    script({"tool_calls": [{"name": "hello.greet", "arguments": {"name": "Ada"}}]}, {"text": "Greeted."})
    turn(chat, "Greet Ada")
    has(system_of_last_main_call(), "The hello plugin is on", "the plugin's prompt section is in the prompt of a chat with tools")
    has(str(tool_results(chat)[0]["content"]), "HEY, ADA!", "the switch in the tab changed the greeting")
    bare = new_chat("external-hooks-bare", tools=False)
    script({"text": "Hi."})
    turn(bare, "Say hi")
    assert "hello plugin" not in system_of_last_main_call(), "a chat without tools gets no section about tools"

    ok("/api/plugins/tab/action", {**HELLO, "action": "loud", "args": {"on": False}})
    ok("/api/plugins/enable", {**HELLO, "enabled": False})
    eq([t for t in ok("/api/plugins/tabs") if t["plugin_subname"] == "hello"], [], "a switched-off plugin has no tab")
    eq(api("/api/plugins/example/hello/greeting")[0], 404, "nor routes")
    hello_on("Hello")


def hello_process():
    """The example/hello plugin as the external-plugin list shows it, with its process."""
    return [p for p in ok("/api/plugins/external") if p["name"] == "example" and p["subname"] == "hello"][0]


def greet(chat_name, name):
    chat = new_chat(chat_name)
    script({"tool_calls": [{"name": "hello.greet", "arguments": {"name": name}}]}, {"text": "Greeted."})
    turn(chat, "Greet " + name)
    return str(tool_results(chat)[0]["content"])


@scenario
def an_external_plugin_that_is_killed_comes_back_with_its_settings_and_its_tool_works_again():
    """The process of an external plugin ends without being asked to: the backend starts a new one, tells it the settings and that
    it is switched on, and the plugin's tool works again, with nothing done by the owner."""
    hello_plugin()
    hello_on("Back")
    settings()
    first = hello_process()
    eq(first["process"]["state"], "running", "it runs")
    os.kill(first["process"]["pid"], 9)
    wait_for(lambda: hello_process()["process"]["state"] == "running" and hello_process()["process"]["restarts"] == 1, limit=20, what="the plugin to be started again")
    again = hello_process()
    assert again["process"]["pid"] != first["process"]["pid"], "a new process"
    eq(again["state"], "ok", "it is not marked failing")
    has(greet("external-after-kill", "Ada"), "Back, Ada!", "the greeting from the settings, which the new process was told")
    eq([p["enabled"] for p in ok("/api/plugins") if p["plugin_subname"] == "hello"], [True], "still switched on")
    hello_on("Hello")


@scenario
def an_external_plugin_has_a_process_only_while_it_is_switched_on():
    """A plugin that is off is loaded and listed but nothing of it runs: switching it off ends its process, switching it on starts
    a new one, and its tool works at once."""
    hello_plugin()
    hello_on("Lazy")
    settings()
    first = hello_process()["process"]
    eq(first["state"], "running", "on: it runs")
    eq(api("/api/plugins/enable", {**HELLO, "enabled": False})[0], 204, "switch off")
    off = hello_process()
    eq((off["state"], off["process"]["state"], off["process"]["pid"]), ("ok", "stopped", None), "off: no process")
    wait_for(lambda: not os.path.exists(f"/proc/{first['pid']}"), limit=10, what="the process to end")
    eq([p["enabled"] for p in ok("/api/plugins") if p["plugin_subname"] == "hello"], [False], "it is listed, off")
    eq(api("/api/plugins/enable", {**HELLO, "enabled": True})[0], 204, "switch on")
    on = hello_process()["process"]
    eq(on["state"], "running", "on again: it runs")
    assert on["pid"] != first["pid"], "a new process"
    has(greet("external-lazy", "Ada"), "Lazy, Ada!", "its settings reached the new process")
    hello_on("Hello")


@scenario
def an_external_plugin_can_be_unloaded_and_loaded_again_without_restarting_the_backend():
    """The owner unloads an external plugin: its process ends and its tool and route are gone, while its files and settings stay;
    loading it again brings it back switched on with the settings it had."""
    hello_plugin()
    hello_on("Again")
    settings()
    pid = hello_process()["process"]["pid"]
    eq(api("/api/plugins/external/unload", HELLO)[0], 204, "unload")
    eq([p for p in ok("/api/plugins") if p["plugin_subname"] == "hello"], [], "it is not in the plugin list")
    eq(hello_process()["state"], "unloaded", "it is recorded as unloaded")
    eq(api("/api/plugins/example/hello/greeting")[0], 404, "its route is gone")
    wait_for(lambda: not os.path.exists(f"/proc/{pid}"), limit=10, what="its process to end")
    chat = new_chat("external-unloaded")
    script({"tool_calls": [{"name": "hello.greet", "arguments": {"name": "Ada"}}]}, {"text": "ok"})
    turn(chat, "Greet Ada")
    has(str(tool_results(chat)[0]["content"]), "no tool named", "the tool is gone")
    eq(api("/api/plugins/external/unload", HELLO)[0], 404, "it cannot be unloaded twice")

    status, loaded = api("/api/plugins/external/load", HELLO)
    eq(status, 200, "load")
    eq((loaded["state"], loaded["process"]["state"]), ("ok", "running"), "it runs again")
    eq([p["enabled"] for p in ok("/api/plugins") if p["plugin_subname"] == "hello"], [True], "switched on as it was")
    has(greet("external-reloaded", "Ada"), "Again, Ada!", "with the settings it had")
    eq(api("/api/plugins/external/load", HELLO)[0], 400, "it cannot be loaded twice")
    eq(api("/api/plugins/external/load", {"plugin_name": "example", "plugin_subname": "missing"})[0], 404, "a folder with no plugin in it")
    hello_on("Hello")


def plugin_file(version, answer, subname="pkg"):
    """A `.tulpa` file for a tiny plugin written straight against the protocol, with one tool that answers `answer`: a zip with a
    plugin.toml and a Python program, which needs no build."""
    import io, zipfile
    toml = ('name = "scenario"\nsubname = "%s"\nversion = "%s"\nprotocol = 1\nsummary = "A plugin made by a scenario"\n'
            'kind = "source"\nrun = "python3 {plugin_dir}/p.py"\n' % (subname, version))
    program = f"""import json, sys
def send(o): sys.stdout.write(json.dumps(o) + "\\n"); sys.stdout.flush()
send({{"jsonrpc": "2.0", "id": 1, "method": "hello", "params": {{"protocol": 1, "sdk": "scenario"}}}})
for line in sys.stdin:
    m = json.loads(line)
    if "method" not in m: continue
    i, method = m["id"], m["method"]
    if method == "tools":
        send({{"jsonrpc": "2.0", "id": i, "result": [{{"name": "scenario_{subname}.say", "description": "Says something. Use it when asked.", "parameters": []}}]}})
    elif method == "call_tool":
        send({{"jsonrpc": "2.0", "id": i, "result": {{"said": "{answer}"}}}})
    else:
        send({{"jsonrpc": "2.0", "id": i, "result": None}})
    if method == "shutdown": sys.exit(0)
"""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("scenario-pkg/plugin.toml", toml)
        z.writestr("scenario-pkg/p.py", program)
    return buf.getvalue()


def stage_plugin_file(data):
    boundary = "scenarioplugin"
    body = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.tulpa\"\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(BACKEND + "/api/plugins/external/stage/upload", data=body, method="POST",
                                 headers={"Authorization": "Bearer " + TOKEN, "Content-Type": "multipart/form-data; boundary=" + boundary})
    return json.load(urllib.request.urlopen(req, timeout=30))


def install_staged(staged):
    status, task = api("/api/plugins/external/install", {"id": staged["id"]})
    eq(status, 202, "the install starts")
    wait_for(lambda: ok("/api/plugins/external/tasks?id=" + staged["id"])["state"] != "running", limit=60, what="the install to end")
    return ok("/api/plugins/external/tasks?id=" + staged["id"])


@scenario
def a_plugin_is_installed_from_a_file_updated_and_uninstalled_without_restarting_the_backend():
    """The owner uploads a plugin file, looks at what is in it, installs it, switches it on, and the model uses its tool; a higher
    version replaces it with its settings and switch kept, and uninstalling removes it and its tool. The backend is not restarted."""
    hello_plugin()
    settings()
    key = {"plugin_name": "scenario", "plugin_subname": "pkg"}
    if any(p["name"] == "scenario" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})

    staged = stage_plugin_file(plugin_file("0.1.0", "first"))
    eq((staged["name"], staged["action"], staged["problems"]), ("scenario", "install", []), "staged as a new plugin, with nothing against it")
    has(staged["plugin_toml"], 'kind = "source"', "the owner can read the manifest")
    eq([f["path"] for f in staged["files"]], ["p.py", "plugin.toml"], "and the file list")
    eq([p for p in ok("/api/plugins") if p["plugin_name"] == "scenario"], [], "nothing is installed until the owner says so")
    task = install_staged(staged)
    eq((task["state"], task["error"]), ("done", None), "installed")
    row = [p for p in ok("/api/plugins/external") if p["name"] == "scenario"][0]
    eq((row["version"], row["source_type"], row["state"]), ("0.1.0", "file", "ok"), "recorded as installed from a file")
    eq(api("/api/plugins/enable", {**key, "enabled": True})[0], 204, "switch on")

    chat = new_chat("installed-plugin")
    script({"tool_calls": [{"name": "scenario_pkg.say", "arguments": {}}]}, {"text": "ok"})
    turn(chat, "say it")
    has(str(tool_results(chat)[0]["content"]), "first", "the installed plugin's tool works")

    newer = stage_plugin_file(plugin_file("0.2.0", "second"))
    eq((newer["action"], newer["installed_version"]), ("update", "0.1.0"), "a higher version is an update")
    same = stage_plugin_file(plugin_file("0.1.0", "again"))
    assert same["problems"], "the same version is refused"
    eq(api("/api/plugins/external/install", {"id": same["id"]})[0], 400, "and cannot be installed")
    eq(api("/api/plugins/external/stage/discard", {"id": same["id"]})[0], 204, "it can be thrown away")
    eq((install_staged(newer)["state"]), "done", "updated")
    eq([p["enabled"] for p in ok("/api/plugins") if p["plugin_name"] == "scenario"], [True], "still switched on")
    chat = new_chat("updated-plugin")
    script({"tool_calls": [{"name": "scenario_pkg.say", "arguments": {}}]}, {"text": "ok"})
    turn(chat, "say it")
    has(str(tool_results(chat)[0]["content"]), "second", "the new version answers")

    eq(api("/api/plugins/external/uninstall", {**key, "remove_data": True})[0], 204, "uninstalled")
    eq([p for p in ok("/api/plugins/external") if p["name"] == "scenario"], [], "it is gone from the list")
    chat = new_chat("uninstalled-plugin")
    script({"tool_calls": [{"name": "scenario_pkg.say", "arguments": {}}]}, {"text": "ok"})
    turn(chat, "say it")
    has(str(tool_results(chat)[0]["content"]), "no tool named", "and its tool is gone")


def python_example_file():
    """The hello-world-python example as a `.tulpa` file, packed the way an author would: the SDK copied next to `main.py` with
    `vendor.py`, since a plugin runs where the SDK is not."""
    import io, pathlib, shutil, subprocess, tempfile, zipfile
    sdk = pathlib.Path(__file__).resolve().parents[2] / "plugin-sdk"
    if not (sdk / "examples" / "hello-world-python").is_dir():
        raise Skip("plugin-sdk/examples/hello-world-python is not in this checkout")
    work = pathlib.Path(tempfile.mkdtemp(prefix="scenario-pyplugin-"))
    try:
        shutil.copytree(sdk / "examples" / "hello-world-python", work / "plugin", ignore=shutil.ignore_patterns("__pycache__"))
        subprocess.run([sys.executable, str(sdk / "python" / "vendor.py"), str(work / "plugin")], check=True, capture_output=True)
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            for path in sorted((work / "plugin").rglob("*")):
                if path.is_file() and "__pycache__" not in path.parts:
                    z.write(path, "hello-world-python/" + str(path.relative_to(work / "plugin")))
        return buf.getvalue()
    finally:
        shutil.rmtree(work, ignore_errors=True)


@scenario
def the_python_example_installs_from_a_file_without_a_build_and_works_like_a_built_in():
    """The Python hello plugin is packed with its SDK, staged, installed (there is nothing to build), given its settings and
    switched on: the model calls its tool, its tab and route answer, and a switch in the tab changes the tool's answer."""
    hello_plugin()
    key = {"plugin_name": "example", "plugin_subname": "hello_python"}
    if any(p["name"] == "example" and p["subname"] == "hello_python" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})
    settings()
    staged = stage_plugin_file(python_example_file())
    eq((staged["name"], staged["subname"], staged["action"], staged["problems"]), ("example", "hello_python", "install", []), "staged as new")
    assert any(f["path"] == "tulpa_plugin/__init__.py" for f in staged["files"]), "it carries its SDK"
    eq(staged["build"], None, "nothing to build")
    task = install_staged(staged)
    eq((task["state"], task["error"]), ("done", None), "installed")
    eq(api("/api/plugins/settings", {**key, "settings": {"greeting": "Moi"}})[0], 204, "settings")
    eq(api("/api/plugins/enable", {**key, "enabled": True})[0], 204, "switch on")

    chat = new_chat("python-plugin")
    script({"tool_calls": [{"name": "hello_python.greet", "arguments": {"name": "Ada"}}]}, {"text": "Greeted."})
    turn(chat, "Greet Ada with the python plugin")
    has(str(tool_results(chat)[0]["content"]), "Moi, Ada!", "the tool answers with the greeting from the settings")
    has(system_of_last_main_call(), "hello_python plugin is on", "the plugin's prompt section is in the prompt")
    tab = ok("/api/plugins/tab?plugin_name=example&plugin_subname=hello_python")
    has(tab["sections"][0]["blocks"][0]["text"], "Moi", "the tab shows the greeting")
    eq(ok("/api/plugins/example/hello_python/greeting"), {"greeting": "Moi", "asked_by": 1}, "its route")
    tab = ok("/api/plugins/tab/action", {**key, "action": "loud", "args": {"on": True}})
    has(tab["message"], "capitals", "a tab action")
    chat = new_chat("python-plugin-loud")
    script({"tool_calls": [{"name": "hello_python.greet", "arguments": {"name": "Ada"}}]}, {"text": "ok"})
    turn(chat, "Greet Ada again")
    has(str(tool_results(chat)[0]["content"]), "MOI, ADA!", "the switch changed the answer")
    status = api("/api/plugins/external/uninstall", {**key, "remove_data": True})[0]
    eq(status, 204, "uninstalled")


def plain_zip_of(folder, wrapper):
    """A folder as an author who does nothing but zip it would send it: no SDK copied in, no virtual environment or cache"""
    import io, pathlib, zipfile
    folder = pathlib.Path(folder)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for path in sorted(folder.rglob("*")):
            if path.is_file() and not {"__pycache__", ".venv"} & set(path.parts):
                z.write(path, wrapper + "/" + str(path.relative_to(folder)))
    return buf.getvalue()


@scenario
def a_python_plugin_of_several_files_gets_the_bundled_sdk_and_its_package_in_a_virtual_environment():
    """The python-deps example is zipped as it stands (no SDK in it): staging adds the backend's SDK, the install builds a virtual
    environment and installs humanize into it (this needs PyPI), and the tool, which imports a sibling module that imports
    humanize, answers from that environment."""
    import pathlib, urllib.request
    folder = pathlib.Path(__file__).resolve().parents[2] / "plugin-sdk" / "examples" / "python-deps"
    if not folder.is_dir():
        raise Skip("plugin-sdk/examples/python-deps is not in this checkout")
    try:
        urllib.request.urlopen("https://pypi.org/simple/humanize/", timeout=10).close()
    except OSError:
        raise Skip("PyPI cannot be reached from here")
    key = {"plugin_name": "example", "plugin_subname": "python_deps"}
    if any(p["name"] == "example" and p["subname"] == "python_deps" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})
    settings()
    staged = stage_plugin_file(plain_zip_of(folder, "python-deps"))
    eq((staged["subname"], staged["action"], staged["problems"]), ("python_deps", "install", []), "staged as new")
    paths = [f["path"] for f in staged["files"]]
    assert "tulpa_plugin/__init__.py" in paths and "tulpa_plugin/host.py" in paths, f"the backend added its SDK: {paths}"
    assert "sizes.py" in paths and "main.py" in paths, f"and the plugin's own modules are there: {paths}"
    has(staged["build"], "build.py", "the build is the plugin's script")
    assert "build.py" in paths, f"which is in the plugin: {paths}"
    task = install_staged(staged)
    eq((task["state"], task["error"]), ("done", None), "installed: " + str(task.get("log", ""))[-600:])
    has(str(task.get("log", "")), "humanize", "the package was installed")
    eq(api("/api/plugins/enable", {**key, "enabled": True})[0], 204, "switch on")
    chat = new_chat("python-deps")
    script({"tool_calls": [{"name": "python_deps.size", "arguments": {"count": 1500000}}]}, {"text": "Done."})
    turn(chat, "How big is 1500000 bytes?")
    has(str(tool_results(chat)[0]["content"]), "1.5 MB", "the tool answers through humanize")
    eq(api("/api/plugins/external/uninstall", {**key, "remove_data": True})[0], 204, "uninstalled")


class FakeTelegram:
    """Telegram's Bot API as far as the example plugin uses it, on a port of its own: updates are queued with `push`, and what the
    bot did (`sent`, `reactions`, `calls`) is read back. A token that starts with `bad` is refused, as Telegram refuses a wrong one."""

    PNG = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360f8cfc0f01f0005000101a5f645400000000049454e44ae426082")

    def __init__(self, port):
        import threading
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        outer = self
        self.lock = threading.Lock()
        self.updates, self.sent, self.reactions, self.calls, self.downloads = [], [], [], [], []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def reply(self, status, body, kind="application/json"):
                data = body if isinstance(body, bytes) else json.dumps(body).encode()
                self.send_response(status)
                self.send_header("Content-Type", kind)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                outer.downloads.append(self.path)
                self.reply(200, FakeTelegram.PNG, "image/png")

            def do_POST(self):
                _, token_part, method = self.path.split("/", 2)
                params = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
                if token_part.startswith("botbad"):
                    return self.reply(401, {"ok": False, "error_code": 401, "description": "Unauthorized"})
                with outer.lock:
                    outer.calls.append((method, params))
                if method == "getUpdates":
                    deadline = time.time() + 1
                    while time.time() < deadline:
                        with outer.lock:
                            found = [u for u in outer.updates if u["update_id"] >= params.get("offset", 0)] if params.get("offset", 0) >= 0 else outer.updates[-1:]
                        if found or params.get("timeout", 1) == 0:
                            return self.reply(200, {"ok": True, "result": found})
                        time.sleep(0.1)
                    return self.reply(200, {"ok": True, "result": []})
                if method == "sendMessage":
                    with outer.lock:
                        outer.sent.append(params)
                    return self.reply(200, {"ok": True, "result": {"message_id": 1000 + len(outer.sent)}})
                if method == "setMessageReaction":
                    with outer.lock:
                        outer.reactions.append(params)
                    return self.reply(200, {"ok": True, "result": True})
                if method == "getFile":
                    return self.reply(200, {"ok": True, "result": {"file_path": "photos/p.png"}})
                if method == "getMe":
                    return self.reply(200, {"ok": True, "result": {"id": 1, "username": "scenario_bot"}})
                return self.reply(200, {"ok": True, "result": True})

        self.server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f"http://127.0.0.1:{port}"

    def push(self, chat_id, text=None, message_id=1, photo=False, name="Ada", user_id=7):
        message = {"message_id": message_id, "date": int(time.time()), "chat": {"id": chat_id, "type": "private"}, "from": {"id": user_id, "first_name": name}}
        if photo:
            message.update({"photo": [{"file_id": "small", "width": 10, "height": 10, "file_size": 10}, {"file_id": "big", "width": 100, "height": 100, "file_size": 1000}]})
            if text:
                message["caption"] = text
        else:
            message["text"] = text
        with self.lock:
            self.updates.append({"update_id": len(self.updates) + 1, "message": message})

    def stop(self):
        self.server.shutdown()
        self.server.server_close()


def example_file(folder):
    """An example of plugin-sdk/examples as a `.tulpa` file, with the Python SDK copied next to its `main.py`."""
    import io, pathlib, shutil, subprocess, tempfile, zipfile
    sdk = pathlib.Path(__file__).resolve().parents[2] / "plugin-sdk"
    if not (sdk / "examples" / folder).is_dir():
        raise Skip(f"plugin-sdk/examples/{folder} is not in this checkout")
    work = pathlib.Path(tempfile.mkdtemp(prefix="scenario-example-"))
    try:
        shutil.copytree(sdk / "examples" / folder, work / "plugin", ignore=shutil.ignore_patterns("__pycache__"))
        subprocess.run([sys.executable, str(sdk / "python" / "vendor.py"), str(work / "plugin")], check=True, capture_output=True)
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            for path in sorted((work / "plugin").rglob("*")):
                if path.is_file() and "__pycache__" not in path.parts:
                    z.write(path, folder + "/" + str(path.relative_to(work / "plugin")))
        return buf.getvalue()
    finally:
        shutil.rmtree(work, ignore_errors=True)


@scenario
def the_telegram_example_answers_a_chat_through_the_agent_and_ignores_the_others():
    """The Telegram plugin of plugin-sdk/examples, installed from a file and pointed at a fake Bot API: a wrong token is refused when
    it is switched on; a message from an allowed chat is acknowledged, tagged with who sent it, answered by the model and sent back
    threaded to it, in pieces when it is long; a photo reaches the model; another chat gets no answer."""
    key = {"plugin_name": "example", "plugin_subname": "telegram"}
    if any(p["name"] == "example" and p["subname"] == "telegram" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})
    # A plugin's chats start on the owner's default model, which has to be the fake one for the model to answer
    settings(llm_provider="llama-cpp", active_model="fake.gguf", launch_profile_id=PROFILE)
    fake_telegram = FakeTelegram(18301)
    # A conversation keeps its chat, and with it the model it began on: a number of its own keeps one run from meeting the last one's
    chat_id = 40000 + os.getpid() % 10000
    other_id = chat_id + 1
    try:
        staged = stage_plugin_file(example_file("telegram"))
        eq((staged["name"], staged["subname"], staged["problems"]), ("example", "telegram", []), "staged")
        eq(install_staged(staged)["state"], "done", "installed")

        base = {"allowed_chat_ids": [str(chat_id)], "think": False, "name_hint": True, "timezone": "Europe/Moscow", "api_url": fake_telegram.url}
        eq(api("/api/plugins/settings", {**key, "settings": {**base, "token": "bad:token"}})[0], 204, "settings are kept")
        status, refused = api("/api/plugins/enable", {**key, "enabled": True})
        eq(status, 400, "a token Telegram refuses cannot be switched on")
        has(str(refused), "bot token", "and the owner is told why")
        eq(api("/api/plugins/settings", {**key, "settings": {**base, "token": "123:good"}})[0], 204, "the right token")
        eq(api("/api/plugins/enable", {**key, "enabled": True})[0], 204, "switched on")
        wait_for(lambda: any(m == "getUpdates" for m, _ in fake_telegram.calls), what="the plugin to start polling")

        # An allowed chat: acknowledged, tagged, answered, threaded
        script({"text": "Four."})
        fake_telegram.push(chat_id, "/chat What is two plus two?", message_id=11)
        wait_for(lambda: any(m["chat_id"] == str(chat_id) and m["text"] == "Four." for m in fake_telegram.sent), what="the answer to be sent")
        answer = [m for m in fake_telegram.sent if m["text"] == "Four."][0]
        eq(answer["reply_parameters"]["message_id"], 11, "the answer is threaded to the message")
        eq([r["message_id"] for r in fake_telegram.reactions], [11], "the message was acknowledged")
        user = [m for m in calls("main")[-1]["messages"] if m["role"] == "user"][-1]["content"]
        has(str(user), "[Message from user named Ada with userid 7, sent at", "the message is tagged with who sent it")
        has(str(user), "What is two plus two?", "and carries what was said")
        assert "/chat" not in str(user), "without the command"

        # A chat that is not allowed gets nothing
        before = len(calls("main"))
        fake_telegram.push(other_id, "hello?", message_id=5)
        time.sleep(2.5)
        eq([m for m in fake_telegram.sent if m["chat_id"] == str(other_id)], [], "nothing was sent to a chat that is not allowed")
        eq(len(calls("main")), before, "and the model was not asked")

        # A bare command is a prompt that asks for the message
        fake_telegram.push(chat_id, "/chat", message_id=12)
        wait_for(lambda: any(m["text"].startswith("What would you like to ask") for m in fake_telegram.sent), what="the prompt")
        prompt = [m for m in fake_telegram.sent if m["text"].startswith("What would you like")][0]
        eq(prompt["reply_markup"]["force_reply"], True, "the prompt opens the reply box")

        # A long answer goes in pieces
        script({"text": ("A paragraph of the long answer. " * 20 + "\n\n") * 9})
        fake_telegram.push(chat_id, "Tell me everything", message_id=13)
        wait_for(lambda: sum(1 for m in fake_telegram.sent if m["text"].startswith("A paragraph")) >= 2, what="the long answer in pieces")
        pieces = [m for m in fake_telegram.sent if m["text"].startswith("A paragraph")]
        assert all(len(m["text"]) <= 4000 for m in pieces), "every piece fits what Telegram takes"
        assert "reply_parameters" in pieces[0] and "reply_parameters" not in pieces[1], "only the first piece is threaded"

        # A photo is downloaded and reaches the model
        script({"text": "A small picture."})
        fake_telegram.push(chat_id, "What is this?", message_id=14, photo=True)
        wait_for(lambda: any(m["text"] == "A small picture." for m in fake_telegram.sent), what="the answer about the photo")
        assert fake_telegram.downloads, "the photo was downloaded"
        has(json.dumps(calls("main")[-1]["messages"]), "image", "and the model was sent an image")

        # The owner chooses a model for the plugin: its chat, which exists already, moves to it with the next message; with no
        # choice a chat is left on the model it has
        # Plugin chats are not in the chat list; this one is the newest chat of the sender's name
        found = {}
        def plugin_chat():
            if "id" not in found:
                for n in range(300, 0, -1):
                    status, chat = api(f"/api/chats?id={n}")
                    if status == 200 and chat["name"] == "Ada":
                        found["id"] = n
                        break
            return ok(f"/api/chats?id={found['id']}")
        def plugin_model():
            return [p for p in ok("/api/plugins") if p["plugin_subname"] == "telegram"][0]["model"]
        eq((plugin_chat()["model"], plugin_model()), ("fake.gguf", None), "the chat began on the default model and the plugin has no choice")
        eq(api("/api/plugins/model", {"plugin_name": "example", "plugin_subname": "nope", "model": "other.gguf"})[0], 404, "no such plugin")
        eq(api("/api/plugins/model", {**key, "profile_id": 999999})[0], 404, "no such launch profile")
        listed = ok("/api/plugins")
        assert [p for p in listed if p["plugin_subname"] == "telegram" and p["plugin_name"] == "example"][0]["uses_agent"], "a plugin that says agent = true has a model to choose"
        tool_only = [p for p in listed if not p["uses_agent"]]
        assert tool_only, "and a plugin that does not say so has none"
        eq(api("/api/plugins/model", {"plugin_name": tool_only[0]["plugin_name"], "plugin_subname": tool_only[0]["plugin_subname"], "model": "fake.gguf"})[0], 400, "no model for a plugin with no chats of its own")
        eq(api("/api/plugins/model", {**key, "model": "other.gguf"})[0], 400, "a model nobody has registered is refused")
        # The seeded Ollama model stands for "another model": nothing answers there, but the chat's model shows where it went
        eq(api("/api/plugins/model", {**key, "model": "verify-model:latest", "provider": "ollama"})[0], 204, "a model is chosen for the plugin")
        eq(plugin_model(), {"provider": "ollama", "model": "verify-model:latest", "launch_profile_id": None}, "the list shows it")
        eq(plugin_chat()["model"], "fake.gguf", "the chat moves with its next message, not before")
        fake_telegram.push(chat_id, "Which model is this?", message_id=15)
        wait_for(lambda: plugin_chat()["model"] == "verify-model:latest", what="the chat to move to the chosen model")
        eq(api("/api/plugins/model", {**key, "profile_id": PROFILE})[0], 204, "a launch profile is chosen instead")
        script({"text": "Back on the profile."})
        fake_telegram.push(chat_id, "And now?", message_id=16)
        wait_for(lambda: any(m["text"] == "Back on the profile." for m in fake_telegram.sent), what="the answer on the profile")
        eq((plugin_chat()["model"], plugin_chat()["launch_profile_id"]), ("fake.gguf", PROFILE), "the chat is on the profile and its model")
        eq(plugin_model()["launch_profile_id"], PROFILE, "the list shows the profile")
        eq(api("/api/plugins/model", {**key})[0], 204, "the choice is taken away")
        eq(plugin_model(), None, "and the list shows none")
    finally:
        fake_telegram.stop()
        api("/api/plugins/enable", {**key, "enabled": False})
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})


def staged_id(chat):
    """The id the last plugins.stage in a chat gave"""
    for message in reversed(tool_results(chat)):
        try:
            found = json.loads(message["content"])
        except (TypeError, ValueError):
            continue
        if isinstance(found, dict) and "id" in found:
            return found["id"]
    raise AssertionError("no plugins.stage result in the chat")


def write_plugin_folder(subname, version="0.1.0", answer="from a folder"):
    """A plugin written to a folder on this machine, the way the model would write one with the file tools"""
    import io, tempfile, zipfile
    folder = tempfile.mkdtemp(prefix="scenario-plugin-folder-")
    with zipfile.ZipFile(io.BytesIO(plugin_file(version, answer, subname))) as z:
        for name in z.namelist():
            target = os.path.join(folder, name.split("/", 1)[1])
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target, "wb") as f:
                f.write(z.read(name))
    return folder


@scenario
def the_model_stages_installs_and_uninstalls_a_plugin_only_with_the_owners_approval():
    """With approvals on, the model fetches a plugin from a folder (a read approval, like any read), reads its files, and the install
    waits for the owner with the plugin's name and what it will run; the approval covers that plugin only, and so does the removal's."""
    import shutil
    key = {"plugin_name": "scenario", "plugin_subname": "tooled"}
    if any(p["name"] == "scenario" and p["subname"] == "tooled" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})
    folder = write_plugin_folder("tooled")
    try:
        settings(auto_confirm=False)
        chat = new_chat("plugin-tools")
        script({"tool_calls": [{"name": "plugins.stage", "arguments": {"source": folder}}]}, {"text": "Staged."})
        s = turn(chat, "stage the plugin in " + folder)
        eq(s["status"], "waiting_for_permission", "reading a folder asks first, like every read")
        eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "permanent"}]})[0], 202, "approve the read")
        wait_idle(chat)
        review = json.loads(tool_results(chat)[0]["content"])
        eq((review["plugin"], review["action"], review["problems"]), ("scenario/tooled", "install", []), "the review says what it is")
        eq(review["from"]["type"], "folder", "and where it is from")
        has(json.dumps(review["commands"]), "python3", "and what it would run")
        eq(ok("/api/plugins/external"), [p for p in ok("/api/plugins/external") if p["name"] != "scenario" or p["subname"] != "tooled"], "nothing is installed by staging")

        script({"tool_calls": [{"name": "plugins.read_file", "arguments": {"id": review["id"], "path": "plugin.toml"}},
                               {"name": "plugins.install", "arguments": {"id": review["id"]}}]}, {"text": "Installed."})
        s = turn(chat, "install it")
        eq(s["status"], "waiting_for_permission", "installing waits for the owner")
        asked = s["pending"][0]["permission"]["escalation"]["ui_message"]
        has(asked, "scenario/tooled", "it names the plugin")
        has(asked, "python3", "and what will run")
        eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})[0], 202, "approve the install")
        wait_idle(chat)
        results = [str(m["content"]) for m in tool_results(chat)]
        has(results[1], "kind = ", "the model read the manifest")
        has(results[2], "scenario/tooled", "and the install says what it installed")
        row = [p for p in ok("/api/plugins/external") if p["name"] == "scenario" and p["subname"] == "tooled"][0]
        eq((row["state"], row["source_type"]), ("ok", "folder"), "recorded as installed from a folder")

        script({"tool_calls": [{"name": "plugins.list", "arguments": {}}, {"name": "plugins.uninstall", "arguments": {"plugin": "scenario/tooled"}}]}, {"text": "Gone."})
        s = turn(chat, "remove it")
        eq(s["status"], "waiting_for_permission", "removing waits for the owner too")
        has(s["pending"][0]["permission"]["escalation"]["ui_message"], "scenario/tooled", "it names the plugin")
        eq(api("/api/agent/answer", {"chat_id": chat, "decisions": [{"index": 0, "allowance": "only_now"}]})[0], 202, "approve the removal")
        wait_idle(chat)
        has(str(tool_results(chat)[-2]["content"]), "scenario/tooled", "the list showed it before")
        eq([p for p in ok("/api/plugins/external") if p["name"] == "scenario" and p["subname"] == "tooled"], [], "it is gone")
    finally:
        shutil.rmtree(folder, ignore_errors=True)
        settings()


@scenario
def with_auto_confirm_the_plugin_tools_run_without_a_prompt_and_a_tulpa_file_is_taken_by_its_path():
    """Auto-confirm is the owner's standing yes, as for every other tool: a plugin file on this machine is staged, installed, listed and
    removed (with its data) with no wait."""
    key = {"plugin_name": "scenario", "plugin_subname": "filed"}
    if any(p["name"] == "scenario" and p["subname"] == "filed" for p in ok("/api/plugins/external")):
        api("/api/plugins/external/uninstall", {**key, "remove_data": True})
    path = "/tmp/scenario-filed-%d.tulpa" % os.getpid()
    with open(path, "wb") as f:
        f.write(plugin_file("0.3.0", "from a file", "filed"))
    try:
        settings(auto_confirm=True)
        chat = new_chat("plugin-tools-auto")
        script({"tool_calls": [{"name": "plugins.stage", "arguments": {"source": path}}]}, {"text": "Staged."})
        eq(turn(chat, "stage " + path)["last_end"]["reason"], "answered", "no wait")
        review = json.loads(tool_results(chat)[0]["content"])
        eq((review["plugin"], review["version"], review["from"]["type"]), ("scenario/filed", "0.3.0", "file"), "a file is taken by its path")
        script({"tool_calls": [{"name": "plugins.install", "arguments": {"id": review["id"]}}, {"name": "plugins.list", "arguments": {}}]},
               {"tool_calls": [{"name": "plugins.uninstall", "arguments": {"plugin": "scenario/filed", "remove_data": True}}]}, {"text": "Done."})
        eq(turn(chat, "install it, look, and remove it with its data")["last_end"]["reason"], "answered", "no wait")
        results = [str(m["content"]) for m in tool_results(chat)]
        has(results[1], "scenario/filed", "installed")
        has(results[2], "0.3.0", "and listed")
        has(results[3], '"data_removed":true', "then removed with its data")
        eq([p for p in ok("/api/plugins/external") if p["subname"] == "filed"], [], "it is gone")
    finally:
        os.path.exists(path) and os.remove(path)
        settings()


@scenario
def another_user_cannot_use_the_plugin_tools():
    """A plugin runs as the owner, so only the owner's chats may use the plugin tools: for anyone else every one of them is refused."""
    before = ok("/api/plugins/external")
    made = ok("/api/users", {"username": "plugin-bob-%d" % os.getpid(), "password": "bob-pass-123"})
    BOB = token_for(made["id"], made["username"], "user")
    try:
        settings(auto_confirm=True)
        full = {"auto_confirm": True, "max_turn_steps": 0, "use_tools": True, "trim_old_thinking": False,
                "memory_enabled": True, "memory_max_entries": 300, "memory_slice_chars": 3000}
        eq(request(BACKEND, "/api/settings", full, token=BOB)[0] // 100, 2, "bob's settings")
        status, chat = request(BACKEND, "/api/chats", {"name": "plugins-bob", "launch_profile_id": PROFILE, "tools_enabled": True}, token=BOB)
        eq(status, 200, "bob's chat")
        script({"tool_calls": [{"name": "plugins.list", "arguments": {}},
                               {"name": "plugins.stage", "arguments": {"source": "https://github.com/octocat/Hello-World"}},
                               {"name": "plugins.install", "arguments": {"id": "0123456789abcdef"}},
                               {"name": "plugins.uninstall", "arguments": {"plugin": "example/hello"}}]}, {"text": "No."})
        eq(request(BACKEND, "/api/agent/turn", {"chat_id": chat["id"], "prompt": "list the plugins"}, token=BOB)[0], 202, "bob's turn")
        wait_for(lambda: request(BACKEND, f"/api/agent/turn?chat_id={chat['id']}", token=BOB)[1]["status"] != "running", what="bob's run")
        msgs = request(BACKEND, f"/api/chats/messages?chat_id={chat['id']}&limit=200", token=BOB)[1]["messages"]
        results = [str(m["content"]) for m in sorted(msgs, key=lambda m: m["id"]) if m["role"] == "tool"]
        eq(len(results), 4, "every call was answered")
        assert all("only the owner" in r for r in results), f"every plugin tool refuses a user who is not the owner: {results}"
        eq(ok("/api/plugins/external"), before, "and nothing was touched")
    finally:
        api(f"/api/users?id={made['id']}", None, "DELETE")
        settings()


def main():
    names = ONLY or list(SCENARIOS)
    failed = skipped = 0
    for name in names:
        # A run the last scenario left going (a wake-up after a sub-agent's result) would take the next one's scripted replies
        wait_for(lambda: ok("/api/agent/runs") == [], what="no run going on")
        fake("/__reset", {})
        t0 = time.time()
        try:
            SCENARIOS[name]()
            print(f"PASS  {name}  ({time.time() - t0:.1f}s)", flush=True)
        except Skip as e:
            skipped += 1
            print(f"SKIP  {name}: {e}", flush=True)
        except Exception as e:
            failed += 1
            print(f"FAIL  {name}: {e}", flush=True)
            if os.environ.get("TRACE"):
                traceback.print_exc()
    print(f"{len(names) - failed - skipped} of {len(names)} passed" + (f", {skipped} skipped" if skipped else ""), flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
