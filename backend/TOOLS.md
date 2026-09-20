# Tools
How the model gets to actually do things — read/write files, inspect the machine it's running on, reach the web, and whatever else gets added here.

## How it works
Every tool implements the `Tool` trait ([`src/tools/base.rs`](./src/tools/base.rs)):

```rust
trait Tool: Send + Sync {
    fn function_name(&self) -> &str;
    fn description(&self) -> &str;
    fn required_properties(&self) -> Vec<PropertyInfo>;
    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> { .. }
    fn shared_buckets(&self) -> &'static [SharedBucket] { &[] }
    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError>;
}
```

`function_name`/`description`/`required_properties` become the JSON schema Ollama sees — the model decides whether and when to call a tool based on that text alone, so a vague description directly causes missed or wrong calls. `call_untyped` gets the model's raw arguments and returns raw JSON back. `ctx` (`ToolContext`) is what a tool may reach beyond its own arguments — the file store, the Ollama client, the background job store, the event bus, and the id of the chat the call is happening in (so a tool never takes a chat id as a model-facing argument). Most tools ignore it; a field is added only when some tool actually needs it.

`is_dangerous` defaults to always-allowed. A tool that can actually change or expose something overrides it to return `Denied { reason, escalation }` unless the chat already has a matching grant — `escalation` (a `ScopeGrant`) is what lets the user approve a whole scope at once ("allow everything under `~/project`", "allow every request to `api.example.com`") instead of confirming every single call. A tool's scope is a `ResolvedScope`: its own grant (per `(chat_id, tool_name)`, for facts nothing else cares about — `web.request`'s per-host permission, say) plus whatever **shared buckets** it opted into via `shared_buckets()`. A shared bucket is cross-tool: every `storage.*` tool that reads files declares `SharedBucket::StorageRead`, so approving `storage.read_file` for a folder also covers `storage.list_directory`/`storage.find_files`/etc. for that same folder without a second prompt — granting write or delete access stays separate (`SharedBucket::StorageWrite`/`StorageDelete`), each is its own independent level. `SharedBucket::ShellCommands` works the same way across tools that run a command line: approving a command word for `os.execute_command` also covers it for `os.start_job`. Grants persist via [`PermissionStore`](./src/services/permission_store.rs), keyed by tool name for a tool's own bucket or by a fixed `GLOBAL.*` key per shared bucket (see `SharedBucket::db_key`).

## Adding a tool
[`src/tools/temperature.rs`](./src/tools/temperature.rs) is the smallest real example:

```rust
#[derive(Deserialize, ToolParams)]
struct TemperatureArgs {
    #[tool(description = "The city or place to get the temperature for.")]
    location: String,
}

#[async_trait]
impl Tool for TemperatureTool {
    fn function_name(&self) -> &str { "get_temperature" }
    fn description(&self) -> &str { "Get the current temperature for a given location." }
    fn required_properties(&self) -> Vec<PropertyInfo> { TemperatureArgs::tool_properties() }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: TemperatureArgs = serde_json::from_value(data)?;
        // .. do the thing, return JSON
    }
}
```

1. Write the args struct. `#[derive(ToolParams)]` (from the [`tool_derive`](./tool_derive) crate) reads each field's type plus its `#[tool(description = "...")]` attribute and generates the schema for you — `Option<T>` fields become optional automatically, everything else is required. There's no enum/oneOf support — a field with a fixed set of allowed values (e.g. an HTTP method) stays a plain `String`, with the allowed values spelled out in its description, and gets validated at call time instead.
2. Implement `Tool`. Only override `is_dangerous` if the tool can actually change or expose something that matters. If it shares a concern an existing shared bucket already covers (reading/writing/deleting under a folder, say), declare that bucket instead of managing its own separate grant — see `storage.rs`'s tools for the pattern, or `web/request.rs` for a tool combining its own bucket (per-host permission) with a shared one.
3. Register it. A one-off tool gets added straight to `tool_list` in [`main.rs`](./src/main.rs). A *domain* of related tools (see below) gets its own `collect()` instead.

### Domains
`os`, `storage`, `web`, `files`, `ui`, and `llm` are domains — a `tools/<domain>.rs` file (e.g. [`tools/storage.rs`](./src/tools/storage.rs)) next to a `tools/<domain>/` folder (e.g. [`tools/storage/`](./src/tools/storage/)) with one file per tool, each domain exposing `pub fn collect() -> Vec<Box<dyn Tool>>` (registered in `main.rs` via `tool_list.extend(tools::<domain>::collect())`). Function names are dot-namespaced: `storage.read_file`, not just `read_file`. Put a new tool in an existing domain if it shares that domain's concerns (e.g. another filesystem op belongs in `storage`, not standalone) — shared logic (like `storage`'s path-scoping helpers) lives in the domain's own `<domain>.rs`, not copy-pasted per tool.

## Current tools

### `get_temperature`
Fake/hardcoded — the original proof-of-concept tool, kept around as the minimal example above. Not dangerous.

### `os` — info about, and some control over, the machine running the backend
Mostly read-only and ungated. The tools that run commands (`os.execute_command`, `os.start_job`) and `os.env_write` genuinely change or expose something and are scoped narrowly — see the table.

| Tool | What it does | Permission |
|---|---|---|
| `os.get_date` | The actual current date/time (UTC, plus this backend's own local clock) — mostly a fallback for a precise/machine-readable timestamp, since the current date is already appended to the newest message on every turn (see `Agent::advance`, and `SYSTEM_PROMPT`'s matching rule, in `facade/agent.rs`) so the model doesn't assume a stale one from training. | none |
| `os.get_hardware` | CPU model, GPU model + VRAM, system RAM, OS name. GPU fields come back `null` if nothing supported was detected. | none |
| `os.get_disk_space` | Every mounted disk/volume, `df`-style: filesystem, mount point, size, used, available. | none |
| `os.get_process_list` | Running processes, sorted by CPU usage (highest first): PID, name, CPU%, memory, status. Can filter to one PID or cap the result count. | none |
| `os.get_network_info` | Network interfaces and their IPv4/IPv6/MAC addresses, and whether each is up or down. | none |
| `os.cpu_usage` | Current CPU usage %, logical CPU count, and 1/5/15-minute load averages. | none |
| `os.get_user_info` | Username, home directory, and process executable path of the user running the backend. | none |
| `os.env_read` | Reads this process's environment variables — all of them, or one by key. | none |
| `os.env_write` | Sets an environment variable on this running backend process. Refuses a short blocklist of obviously sensitive names (`PATH`, `*_TOKEN`, `*_KEY`, etc.) outright. | approve once per exact variable name, reused for any future value written to it — a different variable still needs its own approval |
| `os.execute_command` | Runs a shell command and returns stdout/stderr/exit code (each capped independently at 40,000 characters, truncated with a marker past that — same convention as `storage.read_file`). Output goes to scratch files rather than pipes and only the shell's own exit is waited on, so a command that backgrounds something (`cmd &`, e.g. a dev server) returns immediately and the background process keeps running undisturbed; a command still running in the foreground after 10 minutes is killed along with everything it started (its process group), and whatever it printed by then is returned. Anything long-running belongs in `os.start_job` instead, which the description says — inside this backend's own container if running under Docker (see the repo root's `compose.yaml`), directly on the host otherwise; `description()` reflects whichever is actually true at runtime. Under Docker the container permanently ships python3 (+pip/venv), nodejs (+npm), go, rustc (+cargo), build-essential, git, jq, unzip/zip, curl, wget, poppler-utils, ripgrep, fd, tree, sqlite3, docx2txt, gnumeric, and Playwright + a real headless Chromium (`DISPLAY` is set too, so a non-headless launch shows up as a real window on the host's own screen) — the model is told this list directly and is free to `apt-get`/`pip`/`npm`/`cargo`/`go get` install anything else on top (a scoped, passwordless `sudo` covers just `apt-get`/`apt`/`dpkg`, spliced in transparently via shell aliases so it applies wherever those are actually invoked in a command, not just as its first word); anything installed this way only lasts for that container instance, unlike the baked-in list. Blocks `dd`/`mkfs` as an actual leading word of a shell statement, and `rm -rf /`/`> /dev/sd*` as a substring, outright regardless of approval. | approve once per command word (the first word of the command, e.g. `python`), reused for that word with any arguments — a different command word still needs its own approval. Shares `SharedBucket::ShellCommands` with `os.start_job` |
| `os.start_job` | Starts a shell command as a background job and returns right away with a job id and the path of its log. Waits up to `wait_seconds` (default 3, at most 30) for it to finish by itself, so a command that fails immediately (a missing program, a port already in use) shows its error in the same result. The job keeps running on its own, and a message appears in the chat when it ends (see *Background jobs* below). Same access, blocklist and per-command approval as `os.execute_command`. | approve once per command word, shared with `os.execute_command` via `SharedBucket::ShellCommands` |
| `os.job_output` | A job's status (`running`, `exited` with its exit code, `killed`, or `lost` if the backend restarted while it ran) plus the end of its output — stdout and stderr combined, in order — at most `tail_lines` lines (default 100) and 40,000 characters. Works the same while it's running and after it finishes. Only jobs started in this chat. | none — reads the output of a command that was already approved |
| `os.job_kill` | Stops a running job and everything it started. Fails, saying what state it's in, if it already finished. Only jobs started in this chat. | none — never broader than starting it was |
| `os.list_jobs` | Every job started in this chat, oldest first: id, command, status, exit code, log path. For finding a job's id again or seeing what's still running. | none |

**Background jobs.** A job is a row in `jobs` ([`JobStore`](./src/services/job_store.rs)) plus a log file (`jobs_dir` in `settings.json`, default `~/.llm-tulpa/jobs/<id>.log`, stdout and stderr interleaved) — output goes to a file rather than a pipe, so nothing waits on the process and a job that keeps running keeps writing undisturbed. A tokio task watches each process and records how it ended. Everything platform-specific about running one (`sh -c` vs `cmd /C`, detaching into its own process group, killing a whole process tree) lives in [`services/process.rs`](./src/services/process.rs). A job whose backend restarted while it ran is marked `lost` on the next start. The log of a finished job is kept for `job_log_retention_days` (default 7, `0` = forever) and removed by a sweep at startup — not when the job ends, since the model and the user need to read it afterwards; `os.job_output` then says the log was cleaned up.

When a job ends on its own, the model has to find out without polling and without depending on it remembering to check:
- the watcher publishes a `job_finished` event on the [`EventBus`](./src/services/event_bus.rs), which `GET /api/events` streams to every connected frontend as Server-Sent Events (one-way, server to client — each event is only a hint, the data is always read back through the normal API). Every event is an unnamed SSE message carrying its `type`, so adding an event is adding a `ServerEvent` variant (routes publish through `AppState::events`, tools through `ToolContext::events`, services through the bus handed to their constructor) plus one line in the frontend's `ServerEvent` union; a page holds one shared connection however many components subscribe, opened only while something is listening;
- at the start of the chat's next turn, `Agent` claims every finished-but-unreported job (atomically, so two callers can't both report one) and persists a `notice` message per job — after the newest message already stored and before the model's reply, so it can never land between a tool call and its result. A `notice` reaches the model as a `user`-role message, the UI renders it as a muted marker, and it comes back in the same place after a reload. `ChatOut.notices` carries them to a client that's showing the turn live, so the UI and the database agree;
- if the chat is idle when the event arrives, the frontend calls `POST /api/agent/job_notices`, which starts that turn (or returns `null` without calling the model if there's nothing to report — e.g. the notice already went out with an in-progress turn — or if tool calls are still waiting to run). An event that arrives mid-turn is deferred until the turn ends, not dropped.

A job whose outcome the model has already seen through a tool result (`os.start_job` returning it finished, `os.job_output` on a finished job, `os.job_kill`) is not reported again.

### `storage` — reading and writing files
Every tool here is scoped per-folder, and read/write/delete are three independent shared permission levels — approving `storage.read_file` under a folder also covers every other read tool there (`list_directory`, `find_files`, `detect_file_type`), but implies nothing about write or delete access to the same folder. File-targeting tools (`read_file`, `write_file`, `replace_str`, `delete_file`, `detect_file_type`) scope to the file's *containing folder*; directory-targeting tools scope to the directory itself.

| Tool | What it does | Level |
|---|---|---|
| `storage.read_file` | Reads a file as text. At most 40,000 characters come back per call (the file itself is untouched); the result says whether more follows (`truncated`), how long the file is (`total_chars`) and where to pick up (`next_offset`), which is passed back as `offset` to read the next part. An `offset` past the end says so instead of returning nothing. | read |
| `storage.list_directory` | Lists a directory's immediate contents — name, type, size, modified time, read-only — like `ls -lsh`. Returns at most 200 entries per call, sorted by name; an `offset` argument pages through the rest, and a `note` field says how many are left when truncated. | read |
| `storage.find_files` | Searches a directory tree by filename substring, file-content substring, or both. | read |
| `storage.detect_file_type` | Identifies what a file actually is by sniffing its first bytes (magic numbers) rather than trusting its name — the right first call on something of unknown format, e.g. right after `web.download_file`. Returns a general kind (image/video/audio/archive/doc/font/text) plus a MIME type when recognized. | read |
| `storage.write_file` | Overwrites a file's entire contents, or appends to the end. Creates the file if needed. | write |
| `storage.replace_str` | Swaps one exact, unique snippet of a file's current text for new text — a precise edit, preferred over `write_file` for changing part of an existing file. | write |
| `storage.create_directory` | Creates a directory, including missing parents (`mkdir -p`). | write |
| `storage.delete_file` | Deletes a single file. | delete |
| `storage.delete_directory` | Deletes a directory and everything in it, recursively (`rm -rf`). | delete |

### `web` — reaching outside this machine
Every tool here is scoped per-host (the exact hostname, no wildcard/subdomain matching — `example.com` and `api.example.com` need separate approvals) rather than per-folder. Runs with whatever network access this backend's own environment has (e.g. `network_mode: host` under the repo's Docker setup), so approving a host is a real decision, not a formality.

| Tool | What it does | Permission |
|---|---|---|
| `web.download_file` | Downloads a URL straight to a file on disk and returns status code/content type/bytes written — the body itself is never returned inline, so this is the tool for anything binary or large. Follow up with `storage.detect_file_type` and `storage.read_file`. Needs both a host grant and a folder grant (shares `storage.write_file`'s `SharedBucket::StorageWrite` for the destination). | host approved once, reused |
| `web.request` | Makes an HTTP request (GET/HEAD/POST/PUT/PATCH/DELETE) and returns status/content-type/body inline (UTF-8, lossily decoded, capped at `max_response_bytes` — default 50,000, hard-capped at 500,000). An HTML response is converted to its extracted readable text (via `html2text`) before the cap applies, not raw markup — but this is a plain GET, nothing here runs the page's JavaScript, so a JS-rendered page comes back as a near-empty shell; use the baked-in Playwright (`os.execute_command`) for that, or for anything needing interaction or a screenshot. For reading an API response or small page *now*; use `download_file` for anything binary or too large to read inline. Two independent per-host levels: approving GET/HEAD covers GET/HEAD there from then on; approving any one of POST/PUT/PATCH/DELETE covers all four there from then on, *and* GET/HEAD too (write implies read, not the reverse). | host approved once per level, reused |
| `web.search_query` | Runs a web search against a local SearXNG instance (see the repo root's `searxng/`) and returns `{title, url, snippet}` results. For anything the model wouldn't know from training alone — current events, a specific library/product, recent docs. | none |

`web.search_query` needs the `searxng`/`searxng-nginx` services from the repo root's `compose.yaml` running (`searxng_url` in `data/settings.json`, default `http://localhost:8090` — the rate-limiting sidecar's port, not searxng's own 8080) — see `searxng/settings.yml` for which search engines are enabled (currently Google, Yandex, and DuckDuckGo — Bing was dropped after its SearXNG engine started returning results with nothing to do with the actual query) and `searxng/nginx.conf` for the rate limit in front of them (calls are throttled at the container level, not in this backend's own code, so a burst of searches can't hammer the underlying engines). Both services run `network_mode: host`, same as `backend` itself — a bridge-networked SearXNG can't reach the outside internet at all when outbound traffic depends on host-level VPN/proxy routing.

### `files` — files already attached to this chat, referenced by id
As opposed to `storage.*`, which takes arbitrary real filesystem paths the model names itself.

| Tool | What it does | Permission |
|---|---|---|
| `files.get_attached_file` | Given a file id from an attached-files note on a message, makes a snapshot copy and returns its path and name — read the path with `storage.read_file` (or `storage.detect_file_type` first) to actually see its content. Refuses (same error as "doesn't exist") for a file id not attached to this chat. | shares `storage.read_file`'s `SharedBucket::StorageRead` |

### `ui` — making the frontend show the user something
| Tool | What it does | Permission |
|---|---|---|
| `ui.attach_file` | Snapshots a file from a real path and attaches it to the model's own reply, the same way a user-attached file would show up — not to *this* tool's own (usually content-empty) tool-calling message, but to whichever message turns out to be the turn's actual final, non-tool-calling reply (`Agent::advance` resolves this by scanning back through the turn's own tool calls for `ui.attach_file` results once it knows a reply won't call anything else). | shares `storage.read_file`'s `SharedBucket::StorageRead` |

### `llm` — a tool that makes its own separate call to the model
| Tool | What it does | Permission |
|---|---|---|
| `llm.read_image` | Looks at an image file by path and answers a given prompt about it (or gives a general description if none is given) — a one-shot, tool-free call straight to Ollama, not a full agent turn. For an image found or pointed to via a path; an image already visible inline in the conversation needs no tool at all. | shares `storage.read_file`'s `SharedBucket::StorageRead` |
