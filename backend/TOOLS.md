# Tools
How the model gets to actually do things — read/write files, inspect the machine it's running on, and whatever else gets added here.

## How it works
Every tool implements the `Tool` trait ([`src/tools/base.rs`](./src/tools/base.rs)):

```rust
trait Tool: Send + Sync {
    fn function_name(&self) -> &str;
    fn description(&self) -> &str;
    fn required_properties(&self) -> Vec<PropertyInfo>;
    fn is_dangerous(&self, data: Value, scope: Option<Value>) -> Result<ToolPermission, ToolSerializationError> { .. }
    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError>;
}
```

`function_name`/`description`/`required_properties` become the JSON schema Ollama sees — the model decides whether and when to call a tool based on that text alone, so a vague description directly causes missed or wrong calls. `call_untyped` gets the model's raw arguments and returns raw JSON back.

`is_dangerous` defaults to always-allowed. A tool that can actually change something overrides it to return `Denied { reason, escalation }` unless the chat already has a matching grant — `escalation` (a `ScopeGrant`) is what lets the user approve a whole scope at once ("allow everything under `~/project`") instead of confirming every single call. Scope grants are opaque `serde_json::Value`s that only the tool which produced them interprets, and persist per `(chat_id, tool_name)` via [`PermissionStore`](./src/services/permission_store.rs) — granting one tool never implies anything about another.

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

1. Write the args struct. `#[derive(ToolParams)]` (from the [`tool_derive`](./tool_derive) crate) reads each field's type plus its `#[tool(description = "...")]` attribute and generates the schema for you — `Option<T>` fields become optional automatically, everything else is required.
2. Implement `Tool`. Only override `is_dangerous` if the tool can actually change or expose something that matters.
3. Register it. A one-off tool gets added straight to `tool_list` in [`main.rs`](./src/main.rs). A *domain* of related tools (see below) gets its own `collect()` instead.

### Domains
`os` and `storage` are domains — a `tools/<domain>.rs` file (e.g. [`tools/storage.rs`](./src/tools/storage.rs)) next to a `tools/<domain>/` folder (e.g. [`tools/storage/`](./src/tools/storage/)) with one file per tool, each domain exposing `pub fn collect() -> Vec<Box<dyn Tool>>` (registered in `main.rs` via `tool_list.extend(tools::<domain>::collect())`). Function names are dot-namespaced: `storage.read_file`, not just `read_file`. Put a new tool in an existing domain if it shares that domain's concerns (e.g. another filesystem op belongs in `storage`, not standalone) — shared logic (like `storage`'s path-scoping helpers) lives in the domain's own `<domain>.rs`, not copy-pasted per tool.

## Current tools

### `get_temperature`
Fake/hardcoded — the original proof-of-concept tool, kept around as the minimal example above. Not dangerous.

### `os` — read-only info about the machine running the backend
Never needs permission — nothing here can change anything.

| Tool | What it does |
|---|---|
| `os.get_hardware` | CPU model, GPU model + VRAM, system RAM, OS name. GPU fields come back `null` if nothing supported was detected. |
| `os.get_disk_space` | Every mounted disk/volume, `df`-style: filesystem, mount point, size, used, available. |

### `storage` — reading and writing files
Every tool here is scoped per-folder: the first call against a new path asks for approval (once, or "always for this tool in this chat"), and file-targeting tools (`read_file`, `write_file`, `replace_str`, `delete_file`) scope to the file's *containing folder*, so one grant covers everything else in it too. Directory-targeting tools scope to the directory itself.

| Tool | What it does |
|---|---|
| `storage.read_file` | Reads a file as text. Truncated past 40,000 characters (the file itself is untouched). |
| `storage.write_file` | Overwrites a file's entire contents, or appends to the end. Creates the file if needed. |
| `storage.replace_str` | Swaps one exact, unique snippet of a file's current text for new text — a precise edit, preferred over `write_file` for changing part of an existing file. |
| `storage.delete_file` | Deletes a single file. |
| `storage.list_directory` | Lists a directory's immediate contents — name, type, size, modified time, read-only — like `ls -lsh`. |
| `storage.create_directory` | Creates a directory, including missing parents (`mkdir -p`). |
| `storage.delete_directory` | Deletes a directory and everything in it, recursively (`rm -rf`). |
| `storage.find_files` | Searches a directory tree by filename substring, file-content substring, or both. |
