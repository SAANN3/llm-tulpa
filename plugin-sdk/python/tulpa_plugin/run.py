"""Runs a plugin: `run(MyPlugin())` is the whole of a plugin's `main`."""

from __future__ import annotations

import asyncio
import base64
import os
import sys
from pathlib import Path
from typing import Any

from .connection import Connection, ProtocolError, internal_error
from .failure import Failure
from .host import Host
from .plugin import Plugin
from .tab import Tab
from .types import CallContext, HttpRequest, HttpResponse, UploadedFile

#: Which protocol this SDK speaks
PROTOCOL = 1

METHOD_NOT_FOUND = -32601


class _UnknownMethod(Exception):
    """The backend asked for a method the protocol does not have; not a `LookupError`, which a plugin's own bug may raise"""


def _claim_stdio() -> tuple[int, int]:
    """Keeps the real stdin and stdout for the protocol and points file descriptor 1 at stderr, so that a `print` (or a C library
    that writes to stdout) cannot corrupt the conversation: whatever the plugin prints lands in the log the backend shows."""
    read_fd = os.dup(0)
    write_fd = os.dup(1)
    os.dup2(2, 1)
    return read_fd, write_fd


def _tab(result: Tab | dict[str, Any]) -> dict[str, Any]:
    return result.to_json() if isinstance(result, Tab) else result


async def _dispatch(plugin: Plugin, host: Host, method: str, params: dict[str, Any]) -> Any:
    def user_id() -> int:
        value = params.get("user_id")
        if not isinstance(value, int):
            raise Failure("user_id is missing")
        return value

    if method == "configure":
        await plugin.configure(params.get("settings") or {}, host)
        return None
    if method == "tools":
        return [spec.to_json() for spec in plugin.tools()]
    if method == "call_tool":
        context = params.get("ctx") or {}
        ctx = CallContext(int(context.get("user_id", 0)), int(context.get("chat_id", 0)), str(context.get("model", "")), str(context.get("provider", "")), host)
        return await plugin.call_tool(str(params.get("tool", "")), params.get("args") or {}, ctx)
    if method == "enable":
        await plugin.enable()
        return None
    if method == "disable":
        await plugin.disable()
        return None
    if method == "prompt_section":
        return await plugin.prompt_section(user_id(), bool(params.get("tools", True)))
    if method == "tab_layout":
        return _tab(await plugin.tab_layout(user_id()))
    if method == "tab_action":
        return _tab(await plugin.tab_action(user_id(), str(params.get("action", "")), params.get("args") or {}))
    if method == "tab_upload":
        files = [UploadedFile(f["path"], Path(f["file"])) for f in params.get("files") or []]
        return _tab(await plugin.tab_upload(user_id(), str(params.get("action", "")), params.get("row"), files))
    if method == "http_request":
        request = HttpRequest(params.get("user_id"), params.get("method", "GET"), params.get("path", "/"), params.get("query", ""),
                              [(k, v) for k, v in params.get("headers") or []], base64.b64decode(params.get("body_b64", "")))
        response = await plugin.http_request(request)
        return {"status": response.status, "headers": [list(h) for h in response.headers], "body_b64": base64.b64encode(response.body).decode()}
    if method == "user_deleted":
        await plugin.user_deleted(user_id())
        return None
    if method == "shutdown":
        return None
    raise _UnknownMethod(method)


async def _main(plugin: Plugin) -> None:
    read_fd, write_fd = _claim_stdio()
    connection = Connection(read_fd, write_fd)
    host = Host(connection)
    loop = asyncio.get_running_loop()
    ended = asyncio.Event()

    async def on_request(message: dict[str, Any]) -> None:
        request_id, method = message.get("id"), str(message.get("method"))
        try:
            result = await _dispatch(plugin, host, method, message.get("params") or {})
        except _UnknownMethod:
            connection.answer(request_id, error=(METHOD_NOT_FOUND, f"the plugin has no method {method}"))
            return
        except Failure as failure:
            connection.answer(request_id, error=(failure.code, str(failure)))
            return
        except Exception as error:  # noqa: BLE001 -- a hook's bug is an error answer, not the end of the plugin
            print(f"tulpa-plugin: {method} failed", file=sys.stderr)
            import traceback

            traceback.print_exc()
            connection.answer(request_id, error=internal_error(error))
            return
        if request_id is None:
            return
        try:
            connection.answer(request_id, result)
        except (TypeError, ValueError) as error:
            # A result that is not JSON is the plugin's mistake, told to the caller instead of left unanswered
            connection.answer(request_id, error=internal_error(error))
            return
        if method == "shutdown":
            # The answer has to leave before the process does
            connection.finish()
            await asyncio.sleep(0.05)
            os._exit(0)

    connection.start(loop, on_request, ended.set)
    try:
        connection.info = await connection.call("hello", {"protocol": PROTOCOL, "sdk": "python"}) or {}
    except ProtocolError as error:
        print(f"tulpa-plugin: the backend refused hello: {error}", file=sys.stderr)
        os._exit(1)
    # Until the backend closes our stdin or asks us to shut down
    await ended.wait()


def run(plugin: Plugin) -> None:
    """Runs the plugin until the backend ends it: the entry point of a plugin's `main`."""
    asyncio.run(_main(plugin))
