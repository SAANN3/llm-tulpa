# External plugin protocol, version 1

An external plugin is a separate program that llm-tulpa's backend starts and talks to. This file is the contract between the two; the SDKs (`rust/`) are conveniences over it, so a plugin in any language can follow it directly.

To the rest of the backend an external plugin is a plugin like any other: it is listed on the Plugins page, switched on and off by the owner, its tools are called by the model through the normal permission system, and its prompt and settings behave as a built-in plugin's. Only the owner installs plugins, and an installed plugin runs with the owner's full trust: there is no sandbox, and the process inherits the backend's environment.

## A plugin on disk

Each plugin lives in its own folder, `<external_plugins_dir>/<name>/<subname>/` (`external_plugins_dir` in the backend's `settings.json`, default `~/.llm-tulpa/external-plugins`). The folder holds a `plugin.toml` and whatever the plugin runs from. What a plugin stores for itself goes in its data folder, `<plugins_dir>/<name>/<subname>`, which is the same folder a built-in plugin gets; the backend does not create it.

`name` and `subname` are lowercase letters, digits and underscores (the same rule the plugin's database tables follow). A plugin whose `(name, subname)` is already taken by a built-in plugin or by another folder is refused.

### `plugin.toml`

```toml
name     = "example"
subname  = "hello"
version  = "0.1.0"             # three numbers, 1.2.3
protocol = 1                   # the protocol version the plugin speaks
summary  = "Greets people"     # one line, shown in the Plugins list
help_message = "..."           # optional: a longer text for the plugin's page
default_enabled = false        # optional: switched on the first time the backend sees it (a plugin with no settings only; the owner's later choice wins)
sdk      = "rust"              # informational
kind     = "source"            # source | prebuilt

# kind = "source"
build = "cargo build --release"              # run when the plugin is installed
run   = "{plugin_dir}/target/release/tulpa-example-hello"

# kind = "prebuilt": one binary per machine, <arch>-<os>
# [prebuilt.x86_64-linux]
# path   = "bin/x86_64-linux/tulpa-example-hello"
# sha256 = "<64 hex digits>"

check     = "cargo --version"  # optional: exit 0 = what the plugin needs is there; its output says what is missing
min_tulpa = "0.3.0"            # optional: the oldest backend it works with
```

`run`, `check` and `build` are command lines, split into words the way a shell would split them (spaces, single and double quotes) but **without a shell**: nothing in them is expanded or piped. `{plugin_dir}` (the plugin's folder) and `{data_dir}` (its data folder) are replaced in each word after splitting, so a folder with a space in its name stays one word. Commands run with the plugin's folder as the working directory, and a program written as a relative path with a slash (`.venv/bin/python`) is looked for in that folder.

The folder must be named `<name>/<subname>` exactly as the manifest says. A plugin whose `check` fails, whose manifest is invalid, or that cannot be started is left out when the backend starts, with the reason in the backend's log; the rest start regardless.

The manifest is the only place a plugin's name, subname and version are stated; the program never repeats them and `hello` does not carry them.

## Transport

JSON-RPC 2.0, one JSON object per line, UTF-8, over the child's stdin (backend to plugin) and stdout (plugin to backend). There are no ports.

- Both sides send requests with an `id` and answer the other's. A message without an `id` is a notification and gets no answer.
- The backend's request ids are integers. The plugin's may be integers or strings; the answer carries the id unchanged.
- A line is at most 16 MiB. Files and other large payloads are never sent inline; they go through paths in the plugin's data folder.
- stderr is the plugin's log. The backend keeps its last 200 lines for the Plugins page.
- A line on stdout that is not JSON is logged as a protocol error and skipped. The SDKs keep stray prints off stdout (see their documentation); a plugin written without one must keep its own output off stdout.
- When the backend closes the plugin's stdin it is finished with the plugin. A plugin exits when its stdin ends.

## Starting up

1. The backend starts the process with the environment it has itself plus `TULPA_PLUGIN_PROTOCOL` (the protocol version), `TULPA_DATA_DIR` and `TULPA_PLUGIN_DIR`.
2. The plugin sends the request `hello` with `{protocol, sdk}`. The backend answers `{protocol, tulpa_version, data_dir, plugin_dir}`. If `hello` does not arrive within 5 s, or names a protocol the backend does not speak, the backend kills the process and records why.
3. The backend calls `tools`.
4. The plugin runs until the backend ends it: `shutdown` is a request the plugin answers (with `null`) before exiting, and the backend then closes stdin and kills the process if it is still there after 5 s. The backend does this when it stops.

## Backend to plugin

| Method | Params | Result |
|---|---|---|
| `tools` | none | a list of tool specs (below) |
| `call_tool` | `{tool, args, ctx}` | the tool's result, any JSON |
| `shutdown` | none | `null` |

`ctx` in `call_tool` carries `user_id` (the user whose chat the call is in), `chat_id`, `model` and `provider`. A tool works on `user_id`'s data only.

A call that the backend does not get an answer to within 30 s (or the tool's `timeout_s`) fails with an error the model sees.

### Tool spec

```json
{
  "name": "hello.greet",
  "description": "What it does and when to call it. The model reads this to decide.",
  "parameters": [
    {"name": "name", "type": "string", "description": "Who to greet", "required": false}
  ],
  "confirm": false,
  "timeout_s": 30
}
```

- `name` is `domain.action` in lowercase letters, digits and underscores. A name another plugin or a built-in tool already uses stops the plugin from being switched on.
- `type` is `string`, `number`, `integer`, `boolean`, `array` or `object`. An `array` also names its element type in `items`.
- `confirm`: when true, the user is asked before the tool first runs in a chat, and their approval holds for that chat. Default false.
- `timeout_s` is optional.

The list must have no repeated name, a description on every tool, and every argument named once.

## Plugin to backend

| Method | Params | Result |
|---|---|---|
| `hello` | see above | host info |
| `log` | `{level, message}` with level `error`, `warn`, `info` or `debug` | `null` |

## Errors

A failed call answers with a JSON-RPC `error`: `{code, message}`. `message` is what the model (for a tool) or the owner reads, so it says what went wrong. Codes: the standard JSON-RPC ones (`-32601` method not found, `-32602` invalid params, `-32603` internal error) and `-32001` not found, `-32002` bad arguments. A process that exits, or closes its stdout, fails every call waiting on it and every later one.

## Versioning

`protocol = 1`. Adding an optional field is not a version change. Removing a field or changing what one means is, and a plugin that speaks a protocol the backend does not is refused with that said.
