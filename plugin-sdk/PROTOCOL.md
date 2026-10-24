# External plugin protocol, version 1

An external plugin is a separate program that llm-tulpa's backend starts and talks to. This file is the contract between the two; the SDKs (`rust/`) are conveniences over it, so a plugin in any language can follow it directly.

To the rest of the backend an external plugin is a plugin like any other: it is listed on the Plugins page, switched on and off by the owner, its tools are called by the model through the normal permission system, and its prompt and settings behave as a built-in plugin's. Only the owner installs plugins, and an installed plugin runs with the owner's full trust: there is no sandbox, and the process inherits the backend's environment.

## A plugin on disk

Each plugin lives in its own folder, `<external_plugins_dir>/<name>/<subname>/` (`external_plugins_dir` in the backend's `settings.json`, default `~/.llm-tulpa/external-plugins`). The folder holds a `plugin.toml` and whatever the plugin runs from. What a plugin stores for itself goes in its data folder, `<plugins_dir>/<name>/<subname>`, which is the same folder a built-in plugin gets; the backend does not create it.

`name` and `subname` are lowercase letters, digits and underscores (the same rule the plugin's database tables follow); `external` is not available as a `name`, since `/api/plugins/external/...` is the backend's own. A plugin whose `(name, subname)` is already taken by a built-in plugin or by another folder is refused.

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

# optional: what the owner fills in on the Plugins page before the plugin can be switched on
[[settings]]
name        = "token"
type        = "string"          # string | number | integer | boolean | array | object; an array also has items = "string"
description = "Shown under the field"
required    = true

# optional: a tab of the plugin's own in Settings, shown to every user while the plugin is on
[tab]
id    = "hello"                 # /settings/hello: lowercase letters, digits and hyphens, not one of the Settings page's own
title = "Hello"

# optional: the plugin keeps data in the database (see "Database" below)
[database]
migrations = "migrations"       # optional: the folder of .sql files, this is the default
```

`run`, `check` and `build` are command lines, split into words the way a shell would split them (spaces, single and double quotes) but **without a shell**: nothing in them is expanded or piped. `{plugin_dir}` (the plugin's folder) and `{data_dir}` (its data folder) are replaced in each word after splitting, so a folder with a space in its name stays one word. Commands run with the plugin's folder as the working directory, and a program written as a relative path with a slash (`.venv/bin/python`) is looked for in that folder.

The folder must be named `<name>/<subname>` exactly as the manifest says. A plugin whose `check` fails, whose manifest is invalid, or that cannot be started is left out when the backend starts, with the reason in the backend's log and in `GET /api/plugins/external` (see "Plugins while the backend runs"); the rest start regardless. `check` runs again when the owner switches the plugin on, and the plugin stays off with its output as the reason when it fails.

The manifest is the only place a plugin's name, subname and version are stated; the program never repeats them and `hello` does not carry them. The same holds for what is static about the plugin: its summary, help text, settings and tab are read from the manifest, so the backend can show them before the program has run. `default_enabled` is refused together with `[[settings]]`: a plugin that needs settings has nothing to run with until they are filled in.

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
3. The backend calls `configure` with the settings (when the owner has saved them, or `{}` for a plugin without any), then `tools`, then `enable` if the plugin is switched on. A change of the settings runs `disable` (if it was on), `configure`, `tools` and `enable` again; the process is not restarted. Switching the plugin off runs `disable`, and on runs `enable`.
4. The plugin runs until the backend ends it: `shutdown` is a request the plugin answers (with `null`) before exiting, and the backend then closes stdin and kills the process if it is still there after 5 s. The backend does this when it stops.

## Crashes and restarts

The backend watches the plugin's process. When it ends without having been asked to (it crashed, or was killed), a new one is started after a pause: 1 s after the first crash, 5 s after the second, 30 s after the third and later ones. The new process goes through the opening again (`hello`), then gets `configure` with the settings the old one had and `enable` if the plugin was switched on, so it is where the old one was. Meanwhile the plugin's tools stay registered, and a call to them fails at once with a message saying the plugin is being restarted.

A plugin that fails to answer three calls in a row is taken to be stuck and is restarted the same way. A plugin that crashes five times within ten minutes is given up on: it is switched off (as if the owner had done it), and `GET /api/plugins/external` shows it as `failing` with the last crash. Switching it on again starts a fresh process.

The last 200 lines the plugin wrote to stderr or sent with `log` are kept across restarts, with a marker line where a new process begins.

## Plugins while the backend runs

All owner-only, under `/api/plugins/external`:

| Request | What it does |
|---|---|
| `GET /api/plugins/external` | every external plugin the backend has seen: `state` (`ok`, `unavailable`, `failing`, `unloaded`), `last_error`, version, where it came from, and, for one that has a process, `process` with its state (`running`, `restarting`, `failed`), pid, number of restarts and the end of its log |
| `POST /api/plugins/external/load` `{plugin_name, plugin_subname}` | loads the plugin in `<external_plugins_dir>/<name>/<subname>` as if it had been there at start-up: check, process, migrations, registration. A plugin that cannot start answers 400 with the reason and is recorded as `unavailable`. |
| `POST /api/plugins/external/unload` `{plugin_name, plugin_subname}` | switches the plugin off if it is on, takes it out of the registry and ends its process. Its files, settings, tables and data folder stay. |

A plugin whose folder is gone from the plugins folder is shown as `unavailable` at the next start. Switching a plugin on or off is the same call as for any plugin (`POST /api/plugins/enable`).

## Backend to plugin

| Method | Params | Result |
|---|---|---|
| `configure` | `{settings}` | `null` |
| `tools` | none | a list of tool specs (below) |
| `call_tool` | `{tool, args, ctx}` | the tool's result, any JSON |
| `enable` / `disable` | none | `null` |
| `prompt_section` | `{user_id, tools}` | a string, or `null` for nothing |
| `tab_layout` | `{user_id}` | a tab layout (below) |
| `tab_action` | `{user_id, action, args}` | the tab layout after |
| `tab_upload` | `{user_id, action, row, files}` | the tab layout after |
| `http_request` | `{user_id, method, path, query, headers, body_b64}` | `{status, headers, body_b64}` |
| `user_deleted` | `{user_id}` | `null` |
| `shutdown` | none | `null` |

A plugin may leave out any of these but `shutdown`: answering `-32601` (method not found) to `configure`, `tools`, `enable`, `disable`, `prompt_section` or `user_deleted` means there is nothing to do, and to a tab method means the plugin has no tab. The SDKs answer all of them.

**`configure`**: `settings` is an object that already passed the manifest's `[[settings]]` (every required one present, each of the type it names; a setting the manifest does not list is passed through). The plugin refuses settings only it can judge, such as a token the service rejects, with the error `-32002` and a message the owner reads; the settings are then not saved. `tools` is asked for after each `configure`, so the tools may depend on the settings.

**`disable`**: a plugin that fails, hangs or has died is switched off all the same; the failure is logged.

**`prompt_section`**: text added to the system prompt of that user's chats while the plugin is on. `tools` says whether the chat sends the model its tools. It is asked for at the start of every turn and has 5 s: a plugin that does not answer in time adds nothing that turn. It must be the same text every time while what it describes has not changed, because any change makes the model server read the whole chat again.

**Tab**: `tab_layout` is what the tab shows `user_id`; `tab_action` is what the user did (`action` is the word the control carries, `args` what it sent: `{"on": bool}` for a switch, `{"row": id}` for a row's button, `{"row": id, "on": bool}` for a row's switch); both answer with the layout as it is after. `tab_upload` gets files the user dropped on a drop zone (`action` is the zone's) or chose with a row's upload button (`row` is that row's id), as `files: [{path, file}]`: `path` is the path the file had in the user's browser, `file` where the backend wrote it, under `<data_dir>/.staging/`. The backend removes that folder when the call ends, so the plugin copies or moves what it keeps.

**`http_request`**: everything under `/api/plugins/<name>/<subname>/` goes to the plugin while it is on. Only the owner reaches these routes, and `user_id` is theirs. `path` starts with `/` and is relative to the plugin; `query` has no `?`; `headers` is a list of `[name, value]` pairs without `Authorization` and `Cookie`. A body of more than 8 MiB is refused with 413. The answer's `headers` and `body_b64` may be left out; `content-length`, `transfer-encoding` and `connection` in it are ignored.

**`user_deleted`**: `user_id` was deleted: the plugin removes what it keeps for them outside the backend's database. It is sent to every plugin that has been started, switched on or not.

`ctx` in `call_tool` carries `user_id` (the user whose chat the call is in), `chat_id`, `model` and `provider`. A tool works on `user_id`'s data only.

Every call has a time limit, and a call that is not answered within it fails with an error the caller sees: 30 s for most (`call_tool` uses the tool's `timeout_s` when it has one), 120 s for `tab_upload`, 10 s for `disable` and `user_deleted`, 5 s for `prompt_section`.

### Tool spec

```json
{
  "name": "hello.greet",
  "description": "What it does and when to call it. The model reads this to decide.",
  "parameters": [
    {"name": "name", "type": "string", "description": "Who to greet", "required": false}
  ],
  "confirm": false,
  "access": [
    {"bucket": "write", "folder": "{data_dir}/{user_id}"},
    {"bucket": "read", "path_arg": "path"}
  ],
  "timeout_s": 30
}
```

- `name` is `domain.action` in lowercase letters, digits and underscores. A name another plugin or a built-in tool already uses stops the plugin from being switched on.
- `type` is `string`, `number`, `integer`, `boolean`, `array` or `object`. An `array` also names its element type in `items`.
- `confirm`: when true, the user is asked before the tool first runs in a chat, and their approval holds for that chat. Default false.
- `access`: the folders the tool reads, writes or deletes in. Each entry is asked about the way the backend's own file tools ask about a folder, and the three `bucket`s (`read`, `write`, `delete`) are the same approvals those tools use, so a folder the user has already approved for one is approved for the others. The entries are asked one at a time, in the order listed, and the call runs once all are approved (or when the user has auto-confirm on). An entry names either a `folder` (`{data_dir}` is the plugin's data folder and `{user_id}` the user whose chat the call is in, so a plugin that keeps a folder per user asks about that user's) or a `path_arg`, the name of a string argument that holds the path the call works on: for a file the folder it is in is asked about, for a folder that folder. This is the user's say over the model, not a limit on the plugin: the plugin is trusted and can touch any file itself.
- `timeout_s` is optional.

The list must have no repeated name, a description on every tool, and every argument named once.

### Tab layout

```json
{
  "sections": [
    {"title": "Greetings", "blocks": [
      {"type": "text", "text": "A line", "dim": false},
      {"type": "switch", "action": "loud", "label": "Greet in capitals", "help": "optional", "on": true},
      {"type": "items", "empty": "Nothing here.", "rows": [
        {"id": "r1", "title": "A row", "subtitle": "optional", "badge": "optional",
         "switch": {"action": "toggle", "on": true},
         "buttons": [{"action": "go", "label": "Go", "confirm": "optional question asked first", "upload": false}]}
      ]},
      {"type": "drop", "action": "add", "label": "Drop files here", "help": "optional", "accept": [".zip"]}
    ]}
  ],
  "message": "optional: a line after an action",
  "popup": {"title": "optional", "text": "a text shown in a popup"}
}
```

Everything that is empty may be left out. A button with `"upload": true` opens a file picker and sends the files to `tab_upload` with the row's id.

## Plugin to backend

| Method | Params | Result |
|---|---|---|
| `hello` | see above | host info |
| `log` | `{level, message}` with level `error`, `warn`, `info` or `debug` | `null` |
| `db.query` | `{sql, params?}` | a list of rows, each an object keyed by column name |
| `db.execute` | `{sql, params?}` | the number of rows changed |
| `db.batch` | `{statements: [{sql, params?}, ...]}` | a list with the number of rows each statement changed |

## Database

A plugin whose manifest has `[database]` gets a schema and a role of its own, both named `plugin_<name>_<subname>`. The role cannot log in and owns the schema; the only thing it may do with the core tables is reference the user ids, so `user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE` works and a user's rows go when the user does. Creating the role needs a database user with `CREATEROLE` (or a superuser, which the bundled setup has); without it the plugin is left out at start-up, with the reason in the log. The name must fit in 63 bytes.

**Migrations** are the `.sql` files of the migrations folder, applied in the order of their names (`0001_items.sql`, `0002_flag.sql`), one step each, before the plugin starts. Each step runs as the plugin's role with its schema first on the search path, so `CREATE TABLE items (...)` is the plugin's own `items`. The backend counts the steps a database has applied and runs only the ones after them, so a step that has been released is never edited: a change is a new file. A step only adds: `CREATE TABLE`, `CREATE INDEX`, `ALTER TABLE`, `INSERT` and `UPDATE`, no drops or deletes, and no quoted names. A step that fails is rolled back and the plugin is left out.

**Queries** go through the backend: `db.query`, `db.execute` and `db.batch` run on the backend's connection pool inside a transaction that has switched to the plugin's role and its search path, so a plain table name is the plugin's own and a core table is refused for want of a privilege (`permission denied for table users`). They are available from `configure` on.

- `sql` is one statement; `$1`, `$2`... stand for the `params`. A statement that returns rows (a `SELECT`, or `INSERT`/`UPDATE`/`DELETE` with `RETURNING`) goes to `db.query`; anything else to `db.execute`. `db.batch` runs several in one transaction, all or none.
- A param goes to Postgres as the type its JSON form has: text, `bigint`, `double precision`, `boolean`, or `jsonb` for an array or object. A column of another type needs a cast in the SQL (`$1::timestamptz`, `$1::uuid`), and so does a `null`, which arrives as text. An integer beyond `bigint` is refused.
- A row's columns come back in their JSON form: numbers, text, booleans, `null`, a timestamp as an ISO 8601 string, `jsonb` as itself.
- A statement is cancelled after 25 s, a result of more than 8 MiB is refused (add a `LIMIT`), and a batch takes at most 1000 statements.
- A failed statement answers with an error whose message is Postgres's, ending in its SQLSTATE: `duplicate key value violates unique constraint "items_user_id_name_key" (SQLSTATE 23505)`.

The role is a guard against mistakes, not against a plugin that means harm. The plugin is a program the owner installed and trusts: it runs as the owner, can read the backend's settings file, and could leave its role with a statement of its own. What the role does is keep a plugin's mistake (a query that reaches for the wrong table) from touching the core data, and keep its tables in one place that can be removed.

## Errors

A failed call answers with a JSON-RPC `error`: `{code, message}`. `message` is what the model (for a tool), the owner (for settings) or the user (for a tab) reads, so it says what went wrong. Codes: the standard JSON-RPC ones (`-32601` method not found, `-32602` invalid params, `-32603` internal error) and `-32001` not found, `-32002` bad arguments. Outside a tool call the backend turns `-32002` into a refusal of the request (HTTP 400), `-32001` into 404 and anything else into a failure on the plugin's side (500). A process that exits, or closes its stdout, fails every call waiting on it and every later one.

## Versioning

`protocol = 1`. Adding an optional field is not a version change. Removing a field or changing what one means is, and a plugin that speaks a protocol the backend does not is refused with that said.
