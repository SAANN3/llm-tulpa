# tulpa_plugin: the Python SDK

Write an llm-tulpa plugin in Python 3.10 or newer with nothing to install: the package uses the standard library only, and a plugin
carries a copy of it. The wire format is [`../PROTOCOL.md`](../PROTOCOL.md); this package is a convenience over it, so a plugin is a
class and one call.

```python
from typing import Annotated
from tulpa_plugin import Plugin, run, tool

class Hello(Plugin):
    @tool("hello.greet", "Greets someone by name. Use it when the user asks for a greeting.")
    async def greet(self, name: Annotated[str, "who to greet"]) -> dict:
        return {"greeting": f"Hello, {name}!"}

if __name__ == "__main__":
    run(Hello())
```

Next to it a `plugin.toml` says what the plugin is (see `PROTOCOL.md`: its name, version, summary, settings, tab, `run = "python3 {plugin_dir}/main.py"`),
and `examples/hello-world-python` is a complete one: a setting, a tool, a tab with a switch and a button, a prompt section, a route.

## The hooks

Subclass `Plugin` and override what the plugin uses; every hook has a default. They are the methods of the Rust SDK's `Plugin` trait:
`configure(settings, host)`, `enable`, `disable`, `prompt_section(user_id, tools)`, `tab_layout(user_id)`,
`tab_action(user_id, action, args)`, `tab_upload(user_id, action, row, files)`, `http_request(request)` and `user_deleted(user_id)`. A tab
hook answers with a `Tab` (built with `Tab`, `Section`, `Block` and `Row`) or a dict of the shape in `PROTOCOL.md`.

**Tools** are methods decorated with `@tool(name, description, ...)`. Their arguments come from the method's signature, so the model is told
exactly what the function takes: each argument needs a type (`str`, `int`, `float`, `bool`, `list[str]`, `dict`, or `X | None`) and a
description (`Annotated[str, "who to greet"]`, or `params={"name": "who to greet"}` on the decorator); it is required unless it has a default or is
`X | None`. A parameter called `ctx` is not an argument: it is given the `CallContext` (`user_id`, `chat_id`, `model`, `provider` and `host`). A tool
works on `ctx.user_id`'s data only. `confirm=True`, `timeout_s=...` and `access=[access_folder(...), access_path_arg(...)]` are the tool spec's
`confirm`, `timeout_s` and `access`. A method may be `async` or plain (a plain one runs on a thread). What a tool returns must be JSON.

**Failures** are exceptions: `BadArgs("...")` (a refusal of the arguments or of the settings, which the caller sees), `NotFound("...")`, or `Failure("...")`.
Any other exception is reported as an internal error with its type and message, and the plugin carries on.

**The host** (`Host`, given to `configure` and in `ctx.host`) has `await host.log(message, level="info")`, `host.data_dir`, `host.plugin_dir`, and the database
calls `await host.db_query(sql, params)`, `db_execute` and `db_batch` (see "Database" in `PROTOCOL.md` for how params are typed).

**Printing:** `run` keeps the real standard output for the protocol and points the program's standard output at its standard error, so a `print` lands in the
log the backend shows instead of corrupting the conversation.

## Shipping a plugin

A plugin is installed where this package is not, so it carries a copy: `python3 vendor.py <the plugin's folder>` puts `tulpa_plugin/` next to its `main.py`.
Then `python3 pack.py <the plugin's folder>` makes the `.tulpa` file (a zip with `plugin.toml` at its top, named from the manifest; it copies the SDK in by itself when the folder does not carry it), or push the folder to a git repository; see "Installing a plugin" in `PROTOCOL.md`.

## Tests

`python3 -m unittest discover -s tests` from this folder: the tool specs, the tab shape, and the protocol itself, by running a small plugin as a program and playing the
backend's part.
