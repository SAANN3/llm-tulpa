/// The `coding` plugin family — everything giving the model structured insight into
/// code instead of raw file contents (today: `signatures`; more can be added
/// alongside it as their own submodule, each its own `Plugin`/`PluginBuilder` under the
/// same "coding" `plugin_name`).
pub mod signatures;
