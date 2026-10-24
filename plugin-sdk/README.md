# Plugin SDK

Plugins that run as separate programs next to the backend, written against the contract in [`PROTOCOL.md`](./PROTOCOL.md).

```
plugin-sdk/
  PROTOCOL.md          the contract
  rust/                crate `tulpa-plugin`: the contract behind a trait
  examples/
    hello-world-rust/  one tool, a manifest, a check
```

## Try the example

```bash
cd plugin-sdk/examples/hello-world-rust
cargo build --release
mkdir -p ~/.llm-tulpa/external-plugins/example
cp -r . ~/.llm-tulpa/external-plugins/example/hello     # the folder is <name>/<subname>
```

Restart the backend; `example/hello` appears in Plugins. Save its settings there (the greeting may stay empty) and switch it on: the model gets a `hello.greet` tool, and Settings gets a Hello tab. The backend's log says when a plugin was left out and why. `external_plugins_dir` in `settings.json` moves the folder.

The example's `Cargo.toml` points at the SDK next to it (`path = "../../rust"`); a plugin of your own points at wherever the SDK is.

## Writing a plugin in Rust

Implement `tulpa_plugin::Plugin` (`call_tool` and the hooks you use: `configure`, `tools`, the tab, `prompt_section`, `http_request`...; what the plugin is, its settings and its tab go in `plugin.toml`) and call `tulpa_plugin::serve` from `main`. The crate documentation has the smallest complete plugin, and `examples/hello-world-rust/src/main.rs` uses most of the hooks: a setting, a tool that takes an argument and logs, a tab with a switch and a button, a prompt section and a route.
