"""The tool descriptions a plugin sends in `tools`, and how they are made from a function's own signature."""

from __future__ import annotations

import inspect
import types
import typing
from dataclasses import dataclass
from typing import Any, Callable

#: Which of the user's folder approvals a tool draws on (the same three every file tool of the backend uses)
READ, WRITE, DELETE = "read", "write", "delete"


def access_folder(bucket: str, folder: str) -> dict[str, str]:
    """The tool works in a folder: the user is asked to approve `bucket` access to it before a call runs, as for any file
    tool. `folder` may use `{data_dir}` (the plugin's data folder) and `{user_id}` (the user whose chat the call is in)."""
    return {"bucket": bucket, "folder": folder}


def access_path_arg(bucket: str, arg: str) -> dict[str, str]:
    """The tool works on the path that its string argument `arg` holds (a file, or a folder): the user is asked to approve
    `bucket` access to it, for a file to the folder it is in."""
    return {"bucket": bucket, "path_arg": arg}


@dataclass
class Param:
    name: str
    type: str
    description: str
    required: bool = True
    items: str | None = None

    def to_json(self) -> dict[str, Any]:
        data: dict[str, Any] = {"name": self.name, "type": self.type, "description": self.description, "required": self.required}
        if self.items is not None:
            data["items"] = self.items
        return data


@dataclass
class ToolSpec:
    """One tool the model can call. The description is what the model reads to decide when to call it. `name` is
    `domain.action`, in lowercase letters, digits and underscores."""

    name: str
    description: str
    parameters: list[Param]
    confirm: bool = False
    access: tuple[dict[str, str], ...] = ()
    timeout_s: int | None = None

    def to_json(self) -> dict[str, Any]:
        data: dict[str, Any] = {"name": self.name, "description": self.description, "parameters": [p.to_json() for p in self.parameters], "confirm": self.confirm}
        if self.access:
            data["access"] = list(self.access)
        if self.timeout_s is not None:
            data["timeout_s"] = self.timeout_s
        return data


_SCALARS = {str: "string", bool: "boolean", int: "integer", float: "number"}


def _json_type(annotation: Any, where: str) -> tuple[str, str | None, bool]:
    """The protocol's type of a Python annotation: `(type, element type of an array, optional)`"""
    optional = False
    origin = typing.get_origin(annotation)
    if origin is typing.Annotated:
        annotation = typing.get_args(annotation)[0]
        origin = typing.get_origin(annotation)
    if origin in (typing.Union, types.UnionType):
        arms = [arm for arm in typing.get_args(annotation) if arm is not type(None)]
        optional = len(arms) < len(typing.get_args(annotation))
        if len(arms) != 1:
            raise TypeError(f"{where}: a union of several types cannot be described to the model; use one type, or `X | None`")
        annotation = arms[0]
        origin = typing.get_origin(annotation)
    if annotation in _SCALARS:
        return _SCALARS[annotation], None, optional
    if annotation is dict or origin is dict:
        return "object", None, optional
    if annotation is list or origin is list:
        args = typing.get_args(annotation)
        if len(args) != 1 or args[0] not in _SCALARS:
            raise TypeError(f"{where}: a list needs its element type (list[str], list[int], ...)")
        return "array", _SCALARS[args[0]], optional
    raise TypeError(f"{where}: {annotation!r} is not a type a tool argument can have (str, int, float, bool, list[...], dict)")


def params_of(function: Callable[..., Any], descriptions: dict[str, str], skip: tuple[str, ...] = ("self", "ctx")) -> list[Param]:
    """The arguments of a tool from its function's signature: a parameter without a default is required, `X | None` is
    optional; the description is the string in `Annotated[X, "..."]`, else the one given in `params={...}`."""
    hints = typing.get_type_hints(function, include_extras=True)
    result = []
    for name, parameter in inspect.signature(function).parameters.items():
        if name in skip:
            continue
        where = f"{function.__qualname__}, argument '{name}'"
        if name not in hints:
            raise TypeError(f"{where}: give it a type, so that the model is told what it takes")
        kind, items, optional = _json_type(hints[name], where)
        description = descriptions.get(name)
        if description is None and typing.get_origin(hints[name]) is typing.Annotated:
            description = next((m for m in typing.get_args(hints[name])[1:] if isinstance(m, str)), None)
        if not description:
            raise TypeError(f"{where}: describe it, with Annotated[type, \"what it is\"] or params={{'{name}': \"what it is\"}}; the model reads this")
        result.append(Param(name, kind, description, parameter.default is inspect.Parameter.empty and not optional, items))
    return result
