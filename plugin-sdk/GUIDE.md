# Write your first plugin

A plugin is a program the backend starts when the plugin is switched on. This guide makes one in Python, which needs nothing installed beyond Python 3.10; the Rust SDK works the same way (see `examples/hello-world-rust`). The whole contract is in [`PROTOCOL.md`](./PROTOCOL.md).

## 1. A folder with a manifest

The folder is `<name>/<subname>`. Make `notes/shout` with a `plugin.toml` in it:

```toml
name     = "notes"
subname  = "shout"
version  = "0.1.0"
protocol = 1
summary  = "Shouts what it is told"
kind     = "source"
run      = "python3 {plugin_dir}/main.py"
```

The manifest is the one place that says what the plugin is. Everything the owner sees before the program has ever run (its name, summary, settings form, tab) comes from it.

## 2. A program with a tool

`main.py`, next to it:

```python
from typing import Annotated
from tulpa_plugin import Plugin, run, tool

class Shout(Plugin):
    @tool("shout.say", "Says a text in capitals. Use it when the user asks to have something shouted.")
    async def say(self, text: Annotated[str, "what to shout"]) -> dict:
        return {"shouted": text.upper()}

if __name__ == "__main__":
    run(Shout())
```

The model is told the tool's name and description and each argument's type and description, all read from the method, so write the description for the model: it decides when to call the tool from it.

## 3. Give it its SDK

A plugin runs wherever it is installed, where the SDK is not, so it carries a copy:

```bash
python3 plugin-sdk/python/vendor.py notes/shout
```

## 4. Try it

Put the folder where the backend looks for plugins and load it, without restarting the backend:

```bash
mkdir -p ~/.llm-tulpa/external-plugins/notes
cp -r notes/shout ~/.llm-tulpa/external-plugins/notes/
```

It is already where plugins live, so restart the backend, or call `POST /api/plugins/external/load` with `{"plugin_name": "notes", "plugin_subname": "shout"}`. The plugin is on the Plugins page; switch it on and ask the model to shout something. If it does not start, the page shows why and the end of its log; a `print` in the program lands in that log.

## 5. Settings, a tab, a prompt section

Each is a few lines. A setting is a `[[settings]]` table in the manifest (the owner gets the form) and arrives in `configure(self, settings, host)`; a tab is a `[tab]` table and `tab_layout`/`tab_action` that answer with a `Tab`; a prompt section is `prompt_section(user_id, tools)`. `examples/hello-world-python/main.py` has all three, and `python/README.md` lists every hook.

## 6. Share it

Pack the folder into a `.tulpa` file with `python3 plugin-sdk/python/pack.py notes/shout` (it names the file from the manifest, leaves out `target/`, `.venv/` and caches, and copies the Python SDK in if the plugin does not carry it), or put it in a git repository. On the Plugins page's Install tab the owner drops the file or gives the address, reads what is in it (the manifest, the commands that will run, the files) and installs it. A plugin of kind `source` that needs building names the command in `build`, and a plugin that needs something on the machine (a program, a library) says how to check for it in `check`, so the owner is told what is missing instead of a failed start.

## Where to look next

- `examples/hello-world-python`: a setting, a tool, a tab, a prompt section and a route.
- `examples/telegram`: a plugin with a loop that runs while it is on, which answers messages with the plugin's agent (`host.agent_reply`).
- `examples/skills`: a plugin with a database, uploads and folder approvals (in Rust).
- `scripts/verify-examples` runs them all against a disposable backend, and is how you test a plugin of your own the same way.
