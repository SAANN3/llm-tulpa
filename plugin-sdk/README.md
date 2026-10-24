# Plugin SDK

Plugins that run as separate programs next to the backend, written against the contract in [`PROTOCOL.md`](./PROTOCOL.md).

```
plugin-sdk/
  PROTOCOL.md          the contract
  rust/                crate `tulpa-plugin`: the contract behind a trait
  examples/
    hello-world-rust/  a manifest, a setting, a tool, a tab, a prompt section and a route
    skills/            the built-in skills plugin as an external one: database, uploads, folder approvals
```

## Try the example

```bash
cd plugin-sdk/examples/hello-world-rust
cargo build --release
mkdir -p ~/.llm-tulpa/external-plugins/example
cp -r . ~/.llm-tulpa/external-plugins/example/hello     # the folder is <name>/<subname>
```

Restart the backend; `example/hello` appears in Plugins. Save its settings there (the greeting may stay empty) and switch it on: the model gets a `hello.greet` tool, and Settings gets a Hello tab. The backend's log says when a plugin was left out and why. `external_plugins_dir` in `settings.json` moves the folder.

The `skills` example is installed the same way (`example/skills`). It has tools of its own (`example_skills.*`) and its own tab and tables, so it can be on next to the built-in skills plugin, and the same scenarios in `scripts/fake-llama/run_scenarios.py` run against both. The first start applies its `migrations/`, which needs a database user that can create roles (the bundled one can).

The examples' `Cargo.toml` points at the SDK next to it (`path = "../../rust"`); a plugin of your own points at wherever the SDK is.

## Sharing a plugin

A plugin is shared as a `.tulpa` file (a zip of its folder, `plugin.toml` at the top, without `target/` or other build output) or as a git repository. The owner installs either through the backend (`POST /api/plugins/external/stage/upload` or `.../stage/git`, look at what is in it, then `POST /api/plugins/external/install`; see "Installing a plugin" in [`PROTOCOL.md`](./PROTOCOL.md)), and `GET /api/plugins/external/export` makes the file from an installed plugin. A plugin that is built on install has to find its SDK from inside its own folder: the examples point at the SDK next to them (`path = "../../rust"`), which only works in a checkout, so a plugin you share carries the SDK in a folder of its own or depends on it by a git address.

## Writing a plugin in Rust

Implement `tulpa_plugin::Plugin` (`call_tool` and the hooks you use: `configure`, `tools`, the tab, `prompt_section`, `http_request`...; what the plugin is, its settings and its tab go in `plugin.toml`) and call `tulpa_plugin::serve` from `main`. The crate documentation has the smallest complete plugin, and `examples/hello-world-rust/src/main.rs` uses most of the hooks: a setting, a tool that takes an argument and logs, a tab with a switch and a button, a prompt section and a route. A plugin that keeps data in the database lists `[database]` in its manifest and keeps its `.sql` migrations in a folder; `examples/skills` shows that, with files in the data folder and uploads.
