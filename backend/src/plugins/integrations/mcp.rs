//! MCP client: each user keeps their own servers. Connecting one publishes that server's tools for that user.
pub mod builder;
mod config;
mod plugin;
mod runtime;
mod store;
mod tab;
mod tools;
