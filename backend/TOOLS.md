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
    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError>;
}
```

`function_name`/`description`/`required_properties` become the JSON schema Ollama sees — the model decides whether and when to call a tool based on that text alone, so a vague description directly causes missed or wrong calls. `call_untyped` gets the model's raw arguments and returns raw JSON back.

`is_dangerous` defaults to always-allowed. A tool that can actually change or expose something overrides it to return `Denied { reason, escalation }` unless the chat already has a matching grant — `escalation` (a `ScopeGrant`) is what lets the user approve a whole scope at once ("allow everything under `~/project`", "allow every request to `api.example.com`") instead of confirming every single call. A tool's scope is a `ResolvedScope`: its own grant (per `(chat_id, tool_name)`, for facts nothing else cares about — `web.request`'s per-host permission, say) plus whatever **shared buckets** it opted into via `shared_buckets()`. A shared bucket is cross-tool: every `storage.*` tool that reads files declares `SharedBucket::StorageRead`, so approving `storage.read_file` for a folder also covers `storage.list_directory`/`storage.find_files`/etc. for that same folder without a second prompt — granting write or delete access stays separate (`SharedBucket::StorageWrite`/`StorageDelete`), each is its own independent level. Grants persist via [`PermissionStore`](./src/services/permission_store.rs), keyed by tool name for a tool's own bucket or by a fixed `GLOBAL.*` key per shared bucket (see `SharedBucket::db_key`).

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
`os`, `storage`, and `web` are domains — a `tools/<domain>.rs` file (e.g. [`tools/storage.rs`](./src/tools/storage.rs)) next to a `tools/<domain>/` folder (e.g. [`tools/storage/`](./src/tools/storage/)) with one file per tool, each domain exposing `pub fn collect() -> Vec<Box<dyn Tool>>` (registered in `main.rs` via `tool_list.extend(tools::<domain>::collect())`). Function names are dot-namespaced: `storage.read_file`, not just `read_file`. Put a new tool in an existing domain if it shares that domain's concerns (e.g. another filesystem op belongs in `storage`, not standalone) — shared logic (like `storage`'s path-scoping helpers) lives in the domain's own `<domain>.rs`, not copy-pasted per tool.

## Current tools

### `get_temperature`
Fake/hardcoded — the original proof-of-concept tool, kept around as the minimal example above. Not dangerous.

### `os` — info about, and some control over, the machine running the backend
Mostly read-only and ungated. Two tools genuinely change or expose something and are scoped narrowly — see the table.

| Tool | What it does | Permission |
|---|---|---|
| `os.get_date` | The actual current date/time (UTC, plus this backend's own local clock) — mostly a fallback for a precise/machine-readable timestamp, since the current date is already given directly in the system prompt on every turn (see `SYSTEM_PROMPT` in `facade/agent.rs`) so the model doesn't assume a stale one from training. | none |
| `os.get_hardware` | CPU model, GPU model + VRAM, system RAM, OS name. GPU fields come back `null` if nothing supported was detected. | none |
| `os.get_disk_space` | Every mounted disk/volume, `df`-style: filesystem, mount point, size, used, available. | none |
| `os.get_process_list` | Running processes, sorted by CPU usage (highest first): PID, name, CPU%, memory, status. Can filter to one PID or cap the result count. | none |
| `os.get_network_info` | Network interfaces and their IPv4/IPv6/MAC addresses, and whether each is up or down. | none |
| `os.cpu_usage` | Current CPU usage %, logical CPU count, and 1/5/15-minute load averages. | none |
| `os.get_user_info` | Username, home directory, and process executable path of the user running the backend. | none |
| `os.env_read` | Reads this process's environment variables — all of them, or one by key. | none |
| `os.env_write` | Sets an environment variable on this running backend process. Refuses a short blocklist of obviously sensitive names (`PATH`, `*_TOKEN`, `*_KEY`, etc.) outright. | approve once per exact variable name, reused for any future value written to it — a different variable still needs its own approval |
| `os.execute_command` | Runs a shell command and returns stdout/stderr/exit code — inside this backend's own container if running under Docker (see the repo root's `compose.yaml`), directly on the host otherwise; `description()` reflects whichever is actually true at runtime. Blocks a short list of obviously destructive patterns (`rm -rf /`, `mkfs`, ...) outright, regardless of approval. | approve once per command word (the first word of the command, e.g. `python`), reused for that word with any arguments — a different command word still needs its own approval |

### `storage` — reading and writing files
Every tool here is scoped per-folder, and read/write/delete are three independent shared permission levels — approving `storage.read_file` under a folder also covers every other read tool there (`list_directory`, `find_files`, `detect_file_type`), but implies nothing about write or delete access to the same folder. File-targeting tools (`read_file`, `write_file`, `replace_str`, `delete_file`, `detect_file_type`) scope to the file's *containing folder*; directory-targeting tools scope to the directory itself.

| Tool | What it does | Level |
|---|---|---|
| `storage.read_file` | Reads a file as text. Truncated past 40,000 characters (the file itself is untouched). | read |
| `storage.list_directory` | Lists a directory's immediate contents — name, type, size, modified time, read-only — like `ls -lsh`. | read |
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
| `web.request` | Makes an HTTP request (GET/HEAD/POST/PUT/PATCH/DELETE) and returns status/content-type/body inline (UTF-8, lossily decoded, capped at `max_response_bytes` — default 50,000, but just 4,000 for an HTML response specifically since raw markup is mostly noise; hard-capped at 500,000 either way, and an explicit `max_response_bytes` always overrides the default). For reading an API response or small page *now*; use `download_file` for anything binary or too large to read inline. Two independent per-host levels: approving GET/HEAD covers GET/HEAD there from then on; approving any one of POST/PUT/PATCH/DELETE covers all four there from then on, *and* GET/HEAD too (write implies read, not the reverse). | host approved once per level, reused |
| `web.search_query` | Runs a web search against a local SearXNG instance (see the repo root's `searxng/`) and returns `{title, url, snippet}` results. For anything the model wouldn't know from training alone — current events, a specific library/product, recent docs. | none |

`web.search_query` needs the `searxng`/`searxng-nginx` services from the repo root's `compose.yaml` running (`SEARXNG_URL` env var, default `http://localhost:8090` — the rate-limiting sidecar's port, not searxng's own 8080) — see `searxng/settings.yml` for which search engines are enabled and `searxng/nginx.conf` for the rate limit in front of them (calls are throttled at the container level, not in this backend's own code, so a burst of searches can't hammer the underlying engines). Both services run `network_mode: host`, same as `backend` itself — a bridge-networked SearXNG can't reach the outside internet at all when outbound traffic depends on host-level VPN/proxy routing.
