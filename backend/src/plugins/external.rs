//! Plugins that live outside the backend's code: each is a separate program the backend starts and talks to over its
//! stdin and stdout (`plugin-sdk/PROTOCOL.md` is the contract). To the registry an external plugin is a plugin like any
//! other: `ExternalPluginBuilder` and `ExternalPlugin` implement the same two traits and answer them by asking the process.

pub mod builder;
pub mod loader;
pub mod manifest;
pub mod process;
pub mod rpc;
pub mod spec;
