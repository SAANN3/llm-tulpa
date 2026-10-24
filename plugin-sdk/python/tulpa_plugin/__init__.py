"""SDK for writing an llm-tulpa plugin as a separate program in Python (3.10 or newer, standard library only). The wire format is
`plugin-sdk/PROTOCOL.md`; this package is a convenience over it, so a plugin is one class and a `run` call and never writes
protocol code.

    from tulpa_plugin import Plugin, run, tool

    class Hello(Plugin):
        @tool("hello.greet", "Greets someone by name", params={"name": "who to greet"})
        async def greet(self, name: str) -> dict:
            return {"greeting": f"Hello, {name}!"}

    run(Hello())

A plugin talks to the backend over its stdin and stdout, so a stray `print` would corrupt the conversation. `run` points the
program's standard output at its standard error and keeps the real one for the protocol; whatever the plugin prints then lands in
the log the backend shows.

To ship a plugin, put this folder (`tulpa_plugin/`) next to its `main.py`: `python3 vendor.py <the plugin's folder>` does it."""

from .failure import BadArgs, Failure, NotFound
from .host import Host
from .plugin import Plugin, tool
from .run import PROTOCOL, run
from .spec import DELETE, READ, WRITE, Param, ToolSpec, access_folder, access_path_arg
from .tab import Block, Row, Section, Tab
from .types import CallContext, HttpRequest, HttpResponse, UploadedFile

__all__ = [
    "BadArgs", "Block", "CallContext", "DELETE", "Failure", "Host", "HttpRequest", "HttpResponse", "NotFound", "PROTOCOL", "Param",
    "Plugin", "READ", "Row", "Section", "Tab", "ToolSpec", "UploadedFile", "WRITE", "access_folder", "access_path_arg", "run", "tool",
]
