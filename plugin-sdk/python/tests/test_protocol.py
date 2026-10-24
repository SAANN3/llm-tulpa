"""Runs a small plugin as a program and plays the backend's part, line by line, to check what is on the wire."""

import base64
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import threading
import unittest

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

PLUGIN = textwrap.dedent(
    """
    import sys
    sys.path.insert(0, %r)
    from tulpa_plugin import *

    class Probe(Plugin):
        settings = {}

        async def configure(self, settings, host):
            print("a stray print that must not reach the protocol")
            self.settings = settings
            self.host = host
            if settings.get("refuse"):
                raise BadArgs("the token was refused")

        @tool("probe.say", "Says what it is told", params={"text": "what to say"})
        async def say(self, text: str, ctx: CallContext) -> dict:
            await ctx.host.log("saying " + text)
            return {"said": text, "greeting": self.settings.get("greeting"), "user": ctx.user_id}

        @tool("probe.rows", "Asks the database")
        async def rows(self) -> list:
            return await self.host.db_query("SELECT $1::int AS n", [7])

        @tool("probe.key", "Raises a KeyError")
        async def key(self) -> None:
            raise KeyError("a bug of the plugin's")

        @tool("probe.bad", "Returns something that is not JSON")
        async def bad(self) -> object:
            return object()

        async def prompt_section(self, user_id, tools):
            return "PROBE SECTION" if tools else None

        async def tab_layout(self, user_id):
            return Tab().section(Section("Probe").block(Block.text(f"hello user {user_id}")))

        async def tab_upload(self, user_id, action, row, files):
            return Tab().message(" ".join(f"{f.path}={f.file.read_text()}" for f in files))

        async def http_request(self, request):
            return HttpResponse.json_body(200, {"path": request.path, "body": request.body.decode(), "user": request.user_id})

    run(Probe())
    """
) % HERE


class Host:
    """The backend's end of the conversation"""

    def __init__(self):
        self.dir = tempfile.mkdtemp()
        path = os.path.join(self.dir, "probe.py")
        with open(path, "w") as f:
            f.write(PLUGIN)
        self.process = subprocess.Popen([sys.executable, path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        # A plugin that hangs fails the test instead of stopping it
        self.watchdog = threading.Timer(30, self.process.kill)
        self.watchdog.start()
        self.next_id = 1
        self.log = []
        hello = self.read()
        assert hello["method"] == "hello", hello
        self.hello = hello
        self.answer(hello["id"], {"protocol": 1, "tulpa_version": "x", "data_dir": "/tmp/data", "plugin_dir": self.dir})

    def read(self):
        line = self.process.stdout.readline()
        assert line, "the plugin ended: " + self.process.stderr.read()
        return json.loads(line)  # every line the plugin writes must be JSON

    def answer(self, request_id, result):
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}) + "\n")

    def call(self, method, params, answers=None):
        """Sends a request; answers the plugin's own requests meanwhile (with `answers[method]`), and returns its answer"""
        request_id = self.next_id
        self.next_id += 1
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n")
        while True:
            message = self.read()
            if message.get("method"):
                self.log.append((message["method"], message["params"]))
                self.answer(message["id"], (answers or {}).get(message["method"]))
            elif message["id"] == request_id:
                return message

    def close(self):
        self.watchdog.cancel()
        self.process.stdin.close()
        self.process.wait(10)
        self.process.stdout.close()
        self.process.stderr.close()


class Protocol(unittest.TestCase):
    def setUp(self):
        self.host = Host()

    def tearDown(self):
        self.host.close()

    def test_hello_names_the_protocol_and_the_sdk(self):
        self.assertEqual(self.host.hello["params"], {"protocol": 1, "sdk": "python"})

    def test_settings_tools_and_a_tool_call(self):
        self.assertIsNone(self.host.call("configure", {"settings": {"greeting": "Hej"}})["result"])
        tools = self.host.call("tools", {})["result"]
        self.assertEqual([t["name"] for t in tools], ["probe.bad", "probe.key", "probe.rows", "probe.say"])
        say = next(t for t in tools if t["name"] == "probe.say")
        self.assertEqual(say["parameters"], [{"name": "text", "type": "string", "description": "what to say", "required": True}])
        result = self.host.call("call_tool", {"tool": "probe.say", "args": {"text": "hi"}, "ctx": {"user_id": 4, "chat_id": 9, "model": "m", "provider": "p"}})["result"]
        self.assertEqual(result, {"said": "hi", "greeting": "Hej", "user": 4})
        self.assertIn(("log", {"level": "info", "message": "saying hi"}), self.host.log)

    def test_settings_the_plugin_refuses_are_a_bad_args_error_with_its_text(self):
        error = self.host.call("configure", {"settings": {"refuse": True}})["error"]
        self.assertEqual((error["code"], error["message"]), (-32002, "the token was refused"))

    def test_the_database_is_asked_through_the_host(self):
        self.host.call("configure", {"settings": {}})
        result = self.host.call("call_tool", {"tool": "probe.rows", "args": {}, "ctx": {"user_id": 1}}, answers={"db.query": [{"n": 7}]})["result"]
        self.assertEqual(result, [{"n": 7}])
        self.assertEqual(self.host.log[-1], ("db.query", {"sql": "SELECT $1::int AS n", "params": [7]}))

    def test_a_bug_in_a_tool_is_an_error_answer_and_not_taken_for_an_unknown_method(self):
        self.host.call("configure", {"settings": {}})
        error = self.host.call("call_tool", {"tool": "probe.key", "args": {}, "ctx": {"user_id": 1}})["error"]
        self.assertEqual(error["code"], -32603)
        self.assertIn("KeyError", error["message"])
        not_json = self.host.call("call_tool", {"tool": "probe.bad", "args": {}, "ctx": {"user_id": 1}})["error"]
        self.assertEqual(not_json["code"], -32603)
        unknown = self.host.call("no_such_method", {})["error"]
        self.assertEqual(unknown["code"], -32601)
        # the plugin is still there
        self.assertEqual(self.host.call("prompt_section", {"user_id": 1, "tools": True})["result"], "PROBE SECTION")

    def test_the_other_hooks(self):
        self.assertIsNone(self.host.call("prompt_section", {"user_id": 1, "tools": False})["result"])
        layout = self.host.call("tab_layout", {"user_id": 3})["result"]
        self.assertEqual(layout["sections"][0]["blocks"][0]["text"], "hello user 3")
        path = os.path.join(self.host.dir, "up.txt")
        with open(path, "w") as f:
            f.write("contents")
        uploaded = self.host.call("tab_upload", {"user_id": 3, "action": "add", "row": None, "files": [{"path": "dir/up.txt", "file": path}]})["result"]
        self.assertEqual(uploaded["message"], "dir/up.txt=contents")
        response = self.host.call("http_request", {"user_id": 1, "method": "POST", "path": "/x", "query": "", "headers": [["a", "b"]], "body_b64": base64.b64encode(b"body").decode()})["result"]
        self.assertEqual((response["status"], json.loads(base64.b64decode(response["body_b64"]))), (200, {"path": "/x", "body": "body", "user": 1}))
        self.assertEqual(self.host.call("tab_action", {"user_id": 1, "action": "x", "args": {}})["error"]["code"], -32001)
        self.assertIsNone(self.host.call("user_deleted", {"user_id": 5})["result"])

    def test_shutdown_is_answered_and_the_program_exits(self):
        self.assertIsNone(self.host.call("shutdown", {})["result"])
        self.assertEqual(self.host.process.wait(10), 0)


if __name__ == "__main__":
    unittest.main()
