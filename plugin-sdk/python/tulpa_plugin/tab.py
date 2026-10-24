"""What a plugin's tab in Settings shows, as data. The backend draws it with one component, so a plugin needs no UI code: it
says what is on the page for the user who opened it, and answers what the user does there with the page as it is after.
Each class turns into the JSON of `PROTOCOL.md` ("Tab layout") with `to_json`; a hook may also return a plain dict of that shape."""

from __future__ import annotations

from typing import Any


class Row:
    """A row of an `items` block, identified by `id` in what its controls send back"""

    def __init__(self, id: str, title: str):
        self._row: dict[str, Any] = {"id": id, "title": title, "buttons": []}

    def subtitle(self, text: str) -> "Row":
        self._row["subtitle"] = text
        return self

    def badge(self, text: str) -> "Row":
        """A short label beside the title"""
        self._row["badge"] = text
        return self

    def switch(self, action: str, on: bool) -> "Row":
        """A switch on the row; sends its `action` with `{"row": id, "on": bool}`"""
        self._row["switch"] = {"action": action, "on": on}
        return self

    def button(self, action: str, label: str) -> "Row":
        """A button; sends its `action` with `{"row": id}`"""
        self._row["buttons"].append({"action": action, "label": label, "upload": False})
        return self

    def confirm_button(self, action: str, label: str, question: str) -> "Row":
        """A button that asks `question` in a popup before it sends anything"""
        self._row["buttons"].append({"action": action, "label": label, "confirm": question, "upload": False})
        return self

    def upload_button(self, action: str, label: str) -> "Row":
        """A button that opens a file picker; the files arrive in `tab_upload` with this row's id"""
        self._row["buttons"].append({"action": action, "label": label, "upload": True})
        return self

    def to_json(self) -> dict[str, Any]:
        return self._row


class Block:
    """One thing in a section. Every control carries an `action`, the word it sends back in `tab_action` (or `tab_upload`)."""

    def __init__(self, data: dict[str, Any]):
        self._data = data

    @classmethod
    def text(cls, text: str) -> "Block":
        return cls({"type": "text", "text": text, "dim": False})

    @classmethod
    def hint(cls, text: str) -> "Block":
        """A line drawn as a hint"""
        return cls({"type": "text", "text": text, "dim": True})

    @classmethod
    def switch(cls, action: str, label: str, on: bool, help: str | None = None) -> "Block":
        """A labeled switch. Sends its `action` with `{"on": bool}`."""
        data: dict[str, Any] = {"type": "switch", "action": action, "label": label, "on": on}
        if help is not None:
            data["help"] = help
        return cls(data)

    @classmethod
    def items(cls, empty: str, rows: list[Row]) -> "Block":
        """A list of rows; `empty` is said when there are none"""
        return cls({"type": "items", "empty": empty, "rows": [row.to_json() for row in rows]})

    @classmethod
    def drop_zone(cls, action: str, label: str, accept: tuple[str, ...] | list[str] = (), help: str | None = None) -> "Block":
        """A drop zone for files and folders; `accept` lists the endings it takes (`.zip`), all when empty"""
        data: dict[str, Any] = {"type": "drop", "action": action, "label": label, "accept": list(accept)}
        if help is not None:
            data["help"] = help
        return cls(data)

    def to_json(self) -> dict[str, Any]:
        return self._data


class Section:
    """A titled group of blocks"""

    def __init__(self, title: str | None = None):
        self._section: dict[str, Any] = {"blocks": []}
        if title is not None:
            self._section["title"] = title

    def block(self, block: Block) -> "Section":
        self._section["blocks"].append(block.to_json())
        return self

    def to_json(self) -> dict[str, Any]:
        return self._section


class Tab:
    """Everything the tab shows one user"""

    def __init__(self) -> None:
        self._tab: dict[str, Any] = {"sections": []}

    def section(self, section: Section) -> "Tab":
        self._tab["sections"].append(section.to_json())
        return self

    def message(self, message: str) -> "Tab":
        """A line after an action: what was done, or why it was not"""
        self._tab["message"] = message
        return self

    def popup(self, title: str, text: str) -> "Tab":
        """A text shown in a popup"""
        self._tab["popup"] = {"title": title, "text": text}
        return self

    def to_json(self) -> dict[str, Any]:
        return self._tab
