"""Blueprint node declarations use the same Param schemas as tools."""
from dataclasses import dataclass, field
from typing import Any
from .spec import Param
from .host import Host


@dataclass
class NodeSpec:
    node_type: str
    version: int
    title: str
    description: str
    inputs: list[Param]
    outputs: list[Param]
    control_outputs: list[str]
    timeout_s: int = 30

    def to_json(self) -> dict[str, Any]:
        return {"node_type": self.node_type, "version": self.version, "title": self.title,
                "description": self.description, "inputs": [p.to_json() for p in self.inputs],
                "outputs": [p.to_json() for p in self.outputs],
                "control_outputs": self.control_outputs, "timeout_s": self.timeout_s}


@dataclass
class NodeContext:
    user_id: int
    blueprint_id: int
    run_id: int
    node_id: str
    host: Host


@dataclass
class NodeResult:
    port: str | None
    outputs: dict[str, Any] = field(default_factory=dict)

    def to_json(self) -> dict[str, Any]:
        return {"port": self.port, "outputs": self.outputs}
