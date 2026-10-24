from typing import Annotated

from tulpa_plugin import CallContext, Plugin, run, tool

# A module beside main.py is imported like any other: the folder of the script is on the import path
from sizes import readable


class PythonDeps(Plugin):
    @tool("python_deps.size", "Writes a number of bytes the way people read it, such as 1.5 MB. Use it when the user asks what a byte count means.")
    async def size(self, ctx: CallContext, count: Annotated[int, "The number of bytes"]) -> dict:
        return {"size": readable(count)}


if __name__ == "__main__":
    run(PythonDeps())
