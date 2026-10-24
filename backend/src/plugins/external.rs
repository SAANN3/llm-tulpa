//! Plugins that live outside the backend's code: each is a separate program the backend starts and talks to over its
//! stdin and stdout (`plugin-sdk/PROTOCOL.md` is the contract). To the registry an external plugin is a plugin like any
//! other: `ExternalPluginBuilder` and `ExternalPlugin` implement the same two traits and answer them by asking the process.

pub mod agent;
pub mod builder;
pub mod db;
pub mod export;
pub mod http;
pub mod install;
pub mod installer;
pub mod loader;
pub mod manager;
pub mod manifest;
pub mod process;
pub mod rpc;
pub mod services;
pub mod spec;
pub mod stage;
pub mod supervisor;
