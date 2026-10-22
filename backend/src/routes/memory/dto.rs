use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::memory_store::MemoryEntry;

/// One thing remembered about the caller.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryEntryOut {
    pub(crate) id: i64,
    /// `about_you`, `preferences` or `misc`
    pub(crate) category: String,
    pub(crate) text: String,
    /// The caller's own words an entry the model learned rests on; `null` for one they wrote or edited
    pub(crate) quote: Option<String>,
    /// Written by the model, which may correct it and drops the oldest of these for the caller's limit
    pub(crate) learned: bool,
    pub(crate) updated_at: String,
}

impl From<MemoryEntry> for MemoryEntryOut {
    fn from(entry: MemoryEntry) -> Self {
        Self {
            id: entry.id,
            category: entry.category.as_str().to_string(),
            text: entry.text,
            quote: entry.quote,
            learned: entry.learned,
            updated_at: entry.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryOut {
    /// The most recently changed first
    pub(crate) entries: Vec<MemoryEntryOut>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct MemoryEntryIn {
    /// `about_you`, `preferences` or `misc`
    pub(crate) category: String,
    pub(crate) text: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateMemoryRequest {
    pub(crate) id: i64,
    pub(crate) category: String,
    pub(crate) text: String,
}

/// How many entries a request removed.
#[derive(Serialize, ToSchema)]
pub(crate) struct ClearedOut {
    pub(crate) removed: u64,
}
