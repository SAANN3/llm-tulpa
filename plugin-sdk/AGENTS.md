# For an agent whose job is to write one llm-tulpa plugin

This file is the whole brief for a plugin-writing agent: read it, then write the plugin the user asked for. It is not about working on llm-tulpa itself (that is the repository's `AGENTS.md`). The contract is [`PROTOCOL.md`](./PROTOCOL.md), a slower first read is [`GUIDE.md`](./GUIDE.md), and every claim below can be checked in [`examples/`](./examples).

## What you are making

A plugin is a folder with a `plugin.toml` and a program. The backend starts the program when the owner switches the plugin on and talks to it over stdin and stdout; the program gives the model **tools**, and may also have settings, a tab in Settings, a section in the system prompt, HTTP routes, a database schema, and the plugin's agent (`agent.reply`). Nothing else about llm-tulpa needs to be known to write one.

**Write it in Python unless the user said otherwise**: nothing to build, nothing to install, Python 3.10 or newer. Rust is the other SDK (`examples/hello-world-rust`).

## The shape of the folder

```
my-plugin/              any name for the folder you write; installing files it as <name>/<subname> from the manifest
  plugin.toml           what the plugin is (the only place its name, version, summary, settings and tab are stated)
  main.py               the entry point; other modules and packages beside it are imported as usual
  requirements.txt      only if it needs packages from PyPI (see "Packages")
  migrations/0001_x.sql only if it keeps data in the database
```

Do **not** copy the SDK into the folder: when `plugin.toml` says `sdk = "python"` and the folder has no `tulpa_plugin/`, the backend puts its own copy there when the plugin is staged. (A folder that carries its own `tulpa_plugin/` keeps it, which is how a plugin pins a version.)

### `plugin.toml`, the smallest useful one

```toml
name     = "notes"            # lowercase letters, digits, underscores; never "external"; not a built-in plugin's name
subname  = "shout"            # same rule
version  = "0.1.0"            # three numbers; an update must be higher
protocol = 1
summary  = "Shouts what it is told"          # one line
help_message = "A longer text for the plugin's page"   # optional
sdk      = "python"
agent    = false          # true if the plugin calls host.agent_reply: the owner can then choose the model of its chats
kind     = "source"
run      = "python3 {plugin_dir}/main.py"
check    = "python3 -c \"import sys; sys.exit(0 if sys.version_info >= (3, 10) else 'needs Python 3.10 or newer')\""
```

`run`, `build` and `check` are command lines split like a shell would split them but run **without a shell**: so no `a && b`, no pipes, no `$VAR` (a build of two steps is a script). `{plugin_dir}` and `{data_dir}` are replaced in each word. A `[[settings]]` table makes a field the owner fills in before the plugin can run (`name`, `type` of `string|number|integer|boolean|array|object`, `description`, `required`); a `[tab]` table (`id`, `title`) gives the plugin a tab; `[database]` gives it a schema. The full list is in `PROTOCOL.md`, "plugin.toml".

## The program

```python
from typing import Annotated
from tulpa_plugin import CallContext, Plugin, run, tool

class Shout(Plugin):
    @tool("shout.say", "Says a text in capitals. Use it when the user asks to have something shouted.")
    async def say(self, ctx: CallContext, text: Annotated[str, "what to shout"]) -> dict:
        return {"shouted": text.upper()}

if __name__ == "__main__":
    run(Shout())
```

Rules that decide whether it works:

- **Tool names** are `domain.action`, lowercase letters, digits and underscores, and unique across the whole backend. Start every name with something only your plugin uses (`notes_shout.say`); a name another tool has stops the plugin from switching on.
- **The description is for the model**, which decides from it when to call the tool. Say what the tool does and when to use it; for anything with side effects, say when *not* to.
- **Every argument** needs a type (`str`, `int`, `float`, `bool`, `list[str]`, `dict`, `X | None`) and a description (`Annotated[str, "..."]`). It is required unless it has a default or is `X | None`. `ctx` is not an argument: it carries `user_id`, `chat_id`, `host`.
- **A tool returns JSON** (a dict is best) and raises `BadArgs("...")`, `NotFound("...")` or `Failure("...")` with a message the model can act on. Any other exception is reported as an internal error.
- **A tool works on `ctx.user_id`'s data only.** This is a multi-user backend; never keep one global state for everybody.
- **Never print to stdout** for yourself: the SDK points `print` at the log, which is what the Plugins page shows. Use `await ctx.host.log("...")` or `print` for diagnostics.
- **Persistent files** go in `host.data_dir`; the plugin's own folder is replaced on an update.
- **Dangerous tools** declare it: `@tool(..., confirm=True)` asks the user once per chat; `access=[...]` makes the usual read/write/delete folder approval for the folders the tool touches (`PROTOCOL.md`, "Tool spec"). A tool that deletes, sends, pays or otherwise cannot be undone is `confirm=True`.
- **Secrets** the owner must enter (a token) are a `[[settings]]` entry; the plugin reads them in `configure(self, settings, host)`. Never write a secret into a file of the plugin.

Hooks beyond tools (`configure`, `enable`, `disable`, `prompt_section`, `tab_layout`, `tab_action`, `tab_upload`, `http_request`, `user_deleted`) are listed in [`python/README.md`](./python/README.md); `examples/hello-world-python/main.py` uses most of them. A plugin that answers messages from a chat app uses `host.agent_reply` (`examples/telegram`). A plugin that stores data uses `[database]`, `migrations/*.sql` (append-only, new file per change) and `host.db_query/db_execute/db_batch` (`examples/skills`, `PROTOCOL.md`, "Database").

## Several files and packages

Split freely. `python3 main.py` puts the folder of `main.py` first on the import path, so `from sizes import readable` finds `sizes.py` beside it, and a sub-package is a sub-folder with an `__init__.py`. `examples/python-deps` is a complete example.

### Packages from PyPI

Install them into a virtual environment inside the plugin, in `build` (it runs once, on install), and start the program with that environment's Python:

```toml
build = "python3 {plugin_dir}/build.py"
run   = "{plugin_dir}/.venv/bin/python {plugin_dir}/main.py"
```

`build.py` (copy `examples/python-deps/build.py`) runs `python -m venv .venv` and then `.venv/bin/python -m pip install -r requirements.txt`; it is a script because `build` has no shell, so `a && b` does not work there.

Pin versions in `requirements.txt` (`humanize==4.12.3`). Start the environment as `.venv/bin/python` (on Windows `.venv\Scripts\python.exe`), never through an installed script such as `.venv/bin/pip` at run time: the plugin is built where it is unpacked and moved into place afterwards, and the environment's own scripts break when moved while its Python does not. The machine needs `python3 -m venv` (on Debian, `python3-venv`); say so in `check` if the plugin depends on it. Do not use `pip install` on the machine itself or `sudo`.

## Writing and trying it

1. Write the folder somewhere under a path the owner approved for you; start from `examples/hello-world-python` or `examples/python-deps`.
2. Read your own `plugin.toml` against the rules above: names, `run` command, one description per tool, every argument typed.
3. If you are a model inside llm-tulpa with the `plugins` tools: `plugins.stage` with `source` set to the folder shows what would be installed (the manifest, the commands that would run, problems such as a taken name); fix every problem it lists; `plugins.read_file` lets you read back what was staged. **Install only when the user asked you to**: `plugins.install` asks the owner, and the plugin is installed **switched off**; tell the user to switch it on in Plugins (they fill in the settings there), then try a tool in a chat. Never uninstall anything the user did not ask you to.
4. If you cannot install it: run the program's logic on its own with a small `unittest` (import the modules, call the functions the tools use), and say plainly that the protocol part was not run.
5. When it fails to start, the Plugins page shows the reason and the end of the plugin's log. Whatever was written to stderr, including a traceback, is there.

## Sharing it

`python3 plugin-sdk/python/pack.py my-plugin` makes `<name>-<subname>-<version>.tulpa` (a zip with `plugin.toml` at its top; it leaves out `.venv/`, `target/` and caches). A git repository with `plugin.toml` at its top works the same way (`plugins.stage` takes the address). Raise `version` for each release: an installed plugin is replaced only by a higher one, and the old version is put back if the new one does not start.

## Do not

- Do not add `tulpa_plugin/` to the folder yourself unless the user wants a pinned copy.
- Do not write to the backend's files (`settings.json`, its database) or reach for anything outside the plugin's data folder: the plugin runs as the owner without a sandbox, which is why an install is the owner's decision.
- Do not install a plugin, uninstall one or switch one on without the user having asked for exactly that.
- Do not guess at the protocol. If something here or in `PROTOCOL.md` does not cover what you need, say so rather than invent a field.
