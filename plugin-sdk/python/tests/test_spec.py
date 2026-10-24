import asyncio
import os
import sys
import unittest
from typing import Annotated, Optional

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from tulpa_plugin import BadArgs, CallContext, NotFound, Plugin, access_folder, access_path_arg, tool, WRITE, READ  # noqa: E402


class Sample(Plugin):
    @tool("sample.greet", "Greets someone", params={"name": "who to greet", "shout": "in capitals"})
    async def greet(self, name: str, shout: bool = False) -> dict:
        return {"greeting": name.upper() if shout else name}

    @tool("sample.sum", "Adds numbers", confirm=True, timeout_s=60, access=[access_folder(WRITE, "{data_dir}/{user_id}"), access_path_arg(READ, "path")])
    def total(self, numbers: Annotated[list[int], "the numbers"], path: Annotated[str, "a file"], note: Annotated[Optional[str], "a note"] = None, ctx: CallContext = None) -> dict:
        return {"sum": sum(numbers), "user": ctx.user_id if ctx else None}

    @tool("sample.fail", "Raises a KeyError")
    async def fail(self) -> None:
        raise KeyError("nope")


def spec_of(plugin, name):
    return next(spec for spec in plugin.tools() if spec.name == name).to_json()


class ToolSpecs(unittest.TestCase):
    def test_a_spec_is_made_from_the_signature(self):
        greet = spec_of(Sample(), "sample.greet")
        self.assertEqual(greet["description"], "Greets someone")
        self.assertEqual(greet["parameters"], [
            {"name": "name", "type": "string", "description": "who to greet", "required": True},
            {"name": "shout", "type": "boolean", "description": "in capitals", "required": False},
        ])

    def test_a_default_or_an_optional_type_makes_an_argument_optional_and_a_list_names_its_elements(self):
        total = spec_of(Sample(), "sample.sum")
        params = {p["name"]: p for p in total["parameters"]}
        self.assertEqual((params["numbers"]["type"], params["numbers"]["items"], params["numbers"]["required"]), ("array", "integer", True))
        self.assertFalse(params["note"]["required"])
        self.assertNotIn("ctx", params)
        self.assertEqual((total["confirm"], total["timeout_s"]), (True, 60))
        self.assertEqual(total["access"], [{"bucket": "write", "folder": "{data_dir}/{user_id}"}, {"bucket": "read", "path_arg": "path"}])

    def test_an_argument_the_model_could_not_be_told_about_is_refused_when_the_spec_is_made(self):
        class NoDescription(Plugin):
            @tool("a.b", "d")
            async def go(self, name: str): ...

        class NoType(Plugin):
            @tool("a.b", "d", params={"name": "x"})
            async def go(self, name): ...

        class BareList(Plugin):
            @tool("a.b", "d", params={"items": "x"})
            async def go(self, items: list): ...

        for plugin, expected in ((NoDescription(), "describe it"), (NoType(), "give it a type"), (BareList(), "element type")):
            with self.assertRaises(TypeError) as caught:
                plugin.tools()
            self.assertIn(expected, str(caught.exception))


class Calls(unittest.TestCase):
    ctx = CallContext(7, 1, "m", "p", None)

    def call(self, name, args):
        return asyncio.run(Sample().call_tool(name, args, self.ctx))

    def test_a_tool_is_called_with_its_arguments_and_the_context(self):
        self.assertEqual(self.call("sample.greet", {"name": "ada", "shout": True}), {"greeting": "ADA"})
        self.assertEqual(self.call("sample.sum", {"numbers": [1, 2, 3], "path": "/x"}), {"sum": 6, "user": 7})

    def test_a_missing_argument_and_an_unknown_tool_are_refused_with_their_own_failures(self):
        with self.assertRaises(BadArgs):
            self.call("sample.greet", {})
        with self.assertRaises(NotFound):
            self.call("sample.nothing", {})


if __name__ == "__main__":
    unittest.main()
