"""What the backend tells a hook about a call, and the shapes of a route's request and response."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .host import Host


@dataclass
class CallContext:
    """Where a tool call comes from"""

    #: The user whose chat the call is in. A tool works on this user's data only.
    user_id: int
    chat_id: int
    model: str
    provider: str
    host: "Host"


@dataclass
class UploadedFile:
    """A file the user gave a tab, already written to disk. The folder holding it is removed when `tab_upload` returns, so
    copy or move what you keep."""

    #: The path the file had in the user's browser (a dropped folder keeps its shape)
    path: str
    #: Where it is now
    file: Path


@dataclass
class HttpRequest:
    """A request to `/api/plugins/<name>/<subname>/...`. Only the owner reaches these routes; `user_id` says who. The caller's
    `Authorization` and `Cookie` headers are not passed on."""

    user_id: int | None
    method: str
    #: The path after `/api/plugins/<name>/<subname>`, starting with `/`
    path: str
    #: The query string without the `?`, empty when there is none
    query: str
    headers: list[tuple[str, str]]
    body: bytes

    def header(self, name: str) -> str | None:
        """The first header called `name`, whatever its case"""
        wanted = name.lower()
        return next((value for key, value in self.headers if key.lower() == wanted), None)

    def json(self) -> Any:
        """The body read as JSON"""
        return json.loads(self.body)


@dataclass
class HttpResponse:
    """What the caller gets back"""

    status: int = 200
    headers: list[tuple[str, str]] = field(default_factory=list)
    body: bytes = b""

    @classmethod
    def empty(cls, status: int) -> "HttpResponse":
        return cls(status)

    @classmethod
    def text(cls, status: int, text: str) -> "HttpResponse":
        return cls(status, [("content-type", "text/plain; charset=utf-8")], text.encode())

    @classmethod
    def json_body(cls, status: int, value: Any) -> "HttpResponse":
        return cls(status, [("content-type", "application/json")], json.dumps(value).encode())
