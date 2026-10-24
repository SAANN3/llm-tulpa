"""What the plugin asks of the backend."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Sequence

from .connection import Connection, ProtocolError

LEVELS = ("error", "warn", "info", "debug")


class Host:
    """A handle for what the plugin asks of the backend: its log, its database, its folders."""

    def __init__(self, connection: Connection):
        self._connection = connection

    @property
    def data_dir(self) -> Path | None:
        """The folder that is the plugin's own for files; the backend does not create it"""
        path = self._connection.info.get("data_dir")
        return Path(path) if path else None

    @property
    def plugin_dir(self) -> Path | None:
        """The folder the plugin's own files (its code, its manifest) are in"""
        path = self._connection.info.get("plugin_dir")
        return Path(path) if path else None

    async def log(self, message: str, level: str = "info") -> None:
        """A line in the backend's log and on the plugin's page"""
        try:
            await self._connection.call("log", {"level": level if level in LEVELS else "info", "message": message})
        except ProtocolError:
            # A log line that cannot be delivered is not worth failing the call for
            pass

    async def agent_reply(self, conversation: str, text: str, *, title: str | None = None, images: Sequence[str] = (), think: bool = False) -> str:
        """Has the plugin's own agent answer a message: a model with no tools and a chat of its own for each `conversation`, where
        the same conversation always has the same chat, which is how the model remembers what was said before. `title` names a
        conversation's chat the first time. `images` are base64, without a data-URL prefix. It waits as long as the model takes."""
        answer = await self._connection.call("agent.reply", {"conversation": conversation, "title": title, "text": text, "images": list(images), "think": think})
        if not isinstance(answer, dict) or not isinstance(answer.get("content"), str):
            raise ProtocolError(f"agent.reply answered {answer!r}, not a reply")
        return answer["content"]

    async def db_query(self, sql: str, params: Sequence[Any] = ()) -> list[dict[str, Any]]:
        """Runs a statement that returns rows (a SELECT, or an INSERT/UPDATE/DELETE with RETURNING) in the plugin's database
        and gives them as a list of dicts keyed by column name. `params` fill `$1`, `$2`...: each goes to Postgres as text,
        bigint, double precision, boolean, or jsonb for a list or dict, so a column of another type needs a cast in the SQL
        (`$1::timestamptz`), and so does `None`. Needs `[database]` in the manifest. The error carries Postgres's message
        and SQLSTATE."""
        rows = await self._connection.call("db.query", {"sql": sql, "params": list(params)})
        if not isinstance(rows, list):
            raise ProtocolError(f"db.query answered {rows!r}, not a list of rows")
        return rows

    async def db_execute(self, sql: str, params: Sequence[Any] = ()) -> int:
        """Runs a statement and gives the number of rows it changed"""
        return int(await self._connection.call("db.execute", {"sql": sql, "params": list(params)}))

    async def db_batch(self, statements: Sequence[tuple[str, Sequence[Any]]]) -> list[int]:
        """Runs the statements in one transaction, all or none, and gives how many rows each changed"""
        done = await self._connection.call("db.batch", {"statements": [{"sql": sql, "params": list(params)} for sql, params in statements]})
        return [int(n) for n in done]
