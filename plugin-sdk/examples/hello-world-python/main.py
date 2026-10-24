import sys
from pathlib import Path
from typing import Annotated

# A plugin that is shipped carries its SDK in `tulpa_plugin/` next to this file (`python3 ../../python/vendor.py .` puts it
# there). In a checkout of llm-tulpa the SDK is two folders up, and is found there when no copy is next to this file.
sys.path.append(str(Path(__file__).resolve().parent.parent.parent / "python"))

from tulpa_plugin import Block, CallContext, HttpRequest, HttpResponse, Host, NotFound, BadArgs, Plugin, Row, Section, Tab, run, tool  # noqa: E402

DEFAULT_GREETING = "Hello"


class Hello(Plugin):
    def __init__(self) -> None:
        self.greeting = DEFAULT_GREETING
        # The users who chose greetings in capitals. Kept in memory: a restart forgets it, which is fine for an example; a plugin
        # that must remember writes to its data folder (`host.data_dir`).
        self.loud: set[int] = set()
        self.host: Host | None = None

    async def configure(self, settings: dict, host: Host) -> None:
        self.greeting = str(settings.get("greeting") or "").strip() or DEFAULT_GREETING
        self.host = host

    @tool("hello_python.greet", "Greets a person by name. Use it when the user asks for a greeting from the hello_python plugin.")
    async def greet(self, ctx: CallContext, name: Annotated[str | None, "Who to greet"] = None) -> dict:
        name = (name or "").strip() or "there"
        await ctx.host.log(f"greeting {name} for user {ctx.user_id}")
        greeting = f"{self.greeting}, {name}! (from the external hello plugin in Python, for user {ctx.user_id})"
        return {"greeting": greeting.upper() if ctx.user_id in self.loud else greeting}

    async def enable(self) -> None:
        if self.host:
            await self.host.log("switched on")

    # The same text every turn while nothing it describes changes, so the model server keeps its cache of the chat
    async def prompt_section(self, user_id: int, tools: bool) -> str | None:
        return "The hello_python plugin is on: when the user asks to be greeted by it, call hello_python.greet." if tools else None

    def tab(self, user_id: int) -> Tab:
        return Tab().section(
            Section("Greetings")
            .block(Block.text(f'The greeting is "{self.greeting}".'))
            .block(Block.switch("loud", "Greet in capitals", user_id in self.loud, help="Applies to the greetings the model makes for you."))
            .block(Block.items("Nobody to greet.", [Row("world", "The world").subtitle("Tries the greeting out").button("greet", "Say hello")]))
        )

    async def tab_layout(self, user_id: int) -> Tab:
        return self.tab(user_id)

    async def tab_action(self, user_id: int, action: str, args: dict) -> Tab:
        if action == "loud":
            if not isinstance(args.get("on"), bool):
                raise BadArgs("the switch sent no value")
            (self.loud.add if args["on"] else self.loud.discard)(user_id)
            return self.tab(user_id).message("Greetings are in capitals now." if args["on"] else "Greetings are as they are.")
        if action == "greet":
            return self.tab(user_id).message(f"{self.greeting}, world!")
        raise NotFound(f"no such action: {action}")

    async def http_request(self, request: HttpRequest) -> HttpResponse:
        if (request.method, request.path) == ("GET", "/greeting"):
            return HttpResponse.json_body(200, {"greeting": self.greeting, "asked_by": request.user_id})
        return HttpResponse.empty(404)

    async def user_deleted(self, user_id: int) -> None:
        self.loud.discard(user_id)


if __name__ == "__main__":
    run(Hello())
