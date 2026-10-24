"""The plugin's end of the conversation: JSON-RPC, one object per line, over the plugin's stdin and stdout. Reading and writing
are done on threads of their own, so a hook that blocks the event loop cannot stop the plugin from answering `shutdown`, and a
slow reader on the other end cannot stop it from thinking."""

from __future__ import annotations

import asyncio
import json
import os
import queue
import sys
import threading
from typing import Any, Callable, Awaitable

from .failure import INTERNAL


class ProtocolError(Exception):
    """The backend answered a call of the plugin's with an error, or went away"""


class Connection:
    def __init__(self, read_fd: int, write_fd: int):
        self._read_fd = read_fd
        self._write_fd = write_fd
        self._out: queue.SimpleQueue[bytes | None] = queue.SimpleQueue()
        self._pending: dict[int, asyncio.Future[Any]] = {}
        self._next_id = 1
        self._loop: asyncio.AbstractEventLoop | None = None
        #: What the backend answered to `hello`: its version, the plugin's data folder and its own folder
        self.info: dict[str, Any] = {}

    def start(self, loop: asyncio.AbstractEventLoop, on_request: Callable[[dict[str, Any]], Awaitable[None]], on_end: Callable[[], None]) -> None:
        """Starts the two threads. `on_request` is run on the loop for every request the backend sends; `on_end` when its end
        of the pipe closes."""
        self._loop = loop

        def read() -> None:
            with os.fdopen(self._read_fd, "rb", closefd=False) as stream:
                for line in stream:
                    try:
                        message = json.loads(line)
                    except ValueError:
                        print("tulpa-plugin: a line from the backend is not JSON, ignored", file=sys.stderr)
                        continue
                    loop.call_soon_threadsafe(self._handle, message, on_request)
            loop.call_soon_threadsafe(self._closed, on_end)

        def write() -> None:
            while (data := self._out.get()) is not None:
                view = memoryview(data)
                try:
                    while view:
                        view = view[os.write(self._write_fd, view):]
                except OSError:
                    return

        threading.Thread(target=read, name="tulpa-plugin-read", daemon=True).start()
        threading.Thread(target=write, name="tulpa-plugin-write", daemon=True).start()

    def _handle(self, message: dict[str, Any], on_request: Callable[[dict[str, Any]], Awaitable[None]]) -> None:
        if "method" in message:
            assert self._loop is not None
            self._loop.create_task(on_request(message))
            return
        future = self._pending.pop(message.get("id"), None)
        if future is None or future.done():
            return
        if "error" in message:
            future.set_exception(ProtocolError(str(message["error"].get("message", "the backend reported an error"))))
        else:
            future.set_result(message.get("result"))

    def _closed(self, on_end: Callable[[], None]) -> None:
        # Anything still waiting for an answer never gets one
        for future in self._pending.values():
            if not future.done():
                future.set_exception(ProtocolError("the backend closed the connection"))
        self._pending.clear()
        on_end()

    def send(self, message: dict[str, Any]) -> None:
        """Queues a message; a message that is not JSON raises here, before anything of it is written"""
        self._out.put((json.dumps(message, separators=(",", ":")) + "\n").encode())

    def answer(self, request_id: Any, result: Any = None, error: tuple[int, str] | None = None) -> None:
        if error is None:
            self.send({"jsonrpc": "2.0", "id": request_id, "result": result})
        else:
            self.send({"jsonrpc": "2.0", "id": request_id, "error": {"code": error[0], "message": error[1]}})

    async def call(self, method: str, params: Any) -> Any:
        """Asks the backend and waits for its answer"""
        assert self._loop is not None
        request_id = self._next_id
        self._next_id += 1
        future: asyncio.Future[Any] = self._loop.create_future()
        self._pending[request_id] = future
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        return await future

    def finish(self) -> None:
        """Lets the writer send what is queued and stop"""
        self._out.put(None)


def internal_error(error: BaseException) -> tuple[int, str]:
    return INTERNAL, f"{type(error).__name__}: {error}"
