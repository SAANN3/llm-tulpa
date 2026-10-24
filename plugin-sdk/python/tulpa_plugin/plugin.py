"""The plugin, as the backend sees it, and the `tool` decorator that makes a method a tool."""

from __future__ import annotations

import asyncio
import inspect
from typing import Any, Callable, Sequence

from .failure import BadArgs, NotFound
from .host import Host
from .spec import ToolSpec, params_of
from .tab import Tab
from .types import CallContext, HttpRequest, HttpResponse, UploadedFile

_TOOL = "_tulpa_tool"


def tool(name: str, description: str, *, params: dict[str, str] | None = None, confirm: bool = False,
         access: Sequence[dict[str, str]] = (), timeout_s: int | None = None) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """Makes a method of a `Plugin` a tool the model can call. `name` is `domain.action` in lowercase letters, digits and
    underscores; `description` is what the model reads to decide when to call it. The arguments come from the method's
    signature: each needs a type (str, int, float, bool, list[str], dict, or `X | None`), a description (in
    `Annotated[str, "who to greet"]`, or in `params={"name": "who to greet"}`), and is required unless it has a default or is
    `X | None`. A parameter called `ctx` is not an argument: it is given the `CallContext` of the call.

    `confirm=True` asks the user before the tool first runs in a chat. `access=[access_folder(...), access_path_arg(...)]`
    names the folders it works in, which the user is asked to approve. `timeout_s` is how long a call may take (30 s when
    not set). The method may be `async` or plain; a plain one runs on a thread of its own."""

    def mark(function: Callable[..., Any]) -> Callable[..., Any]:
        setattr(function, _TOOL, {"name": name, "description": description, "params": params or {}, "confirm": confirm, "access": tuple(access), "timeout_s": timeout_s})
        return function

    return mark


class Plugin:
    """Subclass it and override the hooks the plugin uses. What the plugin is (its name, version, summary, settings, whether it
    has a tab, whether it starts switched on) is in `plugin.toml`; this is what it does. Hooks are called from several tasks
    at once, so state that changes (the settings, say) is read and written without awaiting in between, or kept under a lock.

    Tools are the methods decorated with `@tool`; `tools` and `call_tool` need overriding only for tools that are made at
    run time."""

    async def configure(self, settings: dict[str, Any], host: Host) -> None:
        """The owner saved the plugin's settings, which the backend has already checked against `[[settings]]` in the
        manifest. It runs before the plugin is switched on, with a process that has just started. `host` is for logging and the
        database from the other hooks, which do not get one: keep it. Refuse settings only you can judge (a token the service
        rejects) by raising `BadArgs`; the owner sees the text."""

    def tools(self) -> list[ToolSpec]:
        """The tools the model can call. Asked for after every `configure`, so they may depend on the settings."""
        specs = []
        for _, function in inspect.getmembers(type(self), predicate=inspect.isfunction):
            meta = getattr(function, _TOOL, None)
            if meta:
                specs.append(ToolSpec(meta["name"], meta["description"], params_of(function, meta["params"]), meta["confirm"], meta["access"], meta["timeout_s"]))
        return sorted(specs, key=lambda spec: spec.name)

    async def call_tool(self, tool: str, args: dict[str, Any], ctx: CallContext) -> Any:
        """Runs one tool: `tool` is one of the names `tools` listed and `args` the arguments the model gave. The default
        finds the method decorated with that name and calls it."""
        for _, function in inspect.getmembers(type(self), predicate=inspect.isfunction):
            meta = getattr(function, _TOOL, None)
            if not meta or meta["name"] != tool:
                continue
            arguments = {}
            for param in params_of(function, meta["params"]):
                if param.name in args and args[param.name] is not None:
                    arguments[param.name] = args[param.name]
                elif param.required:
                    raise BadArgs(f"the argument '{param.name}' is missing")
            if "ctx" in inspect.signature(function).parameters:
                arguments["ctx"] = ctx
            if inspect.iscoroutinefunction(function):
                return await function(self, **arguments)
            return await asyncio.to_thread(function, self, **arguments)
        raise NotFound(f"no such tool: {tool}")

    async def enable(self) -> None:
        """The plugin was switched on: start the background work it does, if any."""

    async def disable(self) -> None:
        """The plugin was switched off, or its settings are about to change: stop what `enable` started. The process ends
        after this."""

    async def prompt_section(self, user_id: int, tools: bool) -> str | None:
        """A text added to the system prompt of `user_id`'s chats while the plugin is on, `None` for nothing. `tools` says
        whether the chat sends the model its tools. It is asked for every turn and has 5 s to answer, and it must be the same
        text each time while what it describes is the same: any change makes the model server read the whole chat again."""
        return None

    async def tab_layout(self, user_id: int) -> Tab | dict[str, Any]:
        """What the plugin's tab shows `user_id`. Needs `[tab]` in the manifest."""
        raise NotFound("this plugin has no tab")

    async def tab_action(self, user_id: int, action: str, args: dict[str, Any]) -> Tab | dict[str, Any]:
        """What `user_id` did in the tab: `action` is the word the control carries and `args` what it sent (`{"on": bool}` for a
        switch, `{"row": id}` for a row's button). Answer with the tab as it is after."""
        raise NotFound("this plugin has no tab")

    async def tab_upload(self, user_id: int, action: str, row: str | None, files: list[UploadedFile]) -> Tab | dict[str, Any]:
        """Files `user_id` dropped on a drop zone (`action` is the zone's), or chose with a row's upload button (then `row` is
        that row's id). Answer with the tab as it is after."""
        raise NotFound("this plugin has no tab")

    async def http_request(self, request: HttpRequest) -> HttpResponse:
        """A request to `/api/plugins/<name>/<subname>/...`"""
        return HttpResponse.empty(404)

    async def user_deleted(self, user_id: int) -> None:
        """`user_id` was deleted: remove what you keep for them outside the backend's database (files, say). The user is
        gone whatever happens here, so a failure is yours to log."""
