use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::tools::base::{PropertyInfo, Tool, ToolError};

pub struct GetDateTool;

#[derive(Serialize)]
struct DateOut {
    /// Unambiguous machine-readable form, always UTC — the one field safe to rely on
    /// regardless of what timezone this backend happens to be running in.
    utc_iso8601: String,
    /// The same instant, spelled out for a reader: weekday, full month name, day,
    /// year, 24h time, "UTC" — e.g. "Saturday, September 5, 2026 01:55:00 UTC".
    utc_formatted: String,
    /// This backend's own local clock, same format as `utc_formatted` plus its UTC
    /// offset — only actually different from `utc_formatted` when this process is
    /// running somewhere with a real timezone set (most Docker deployments default to
    /// UTC, so expect this to usually match).
    server_local_formatted: String,
}

#[async_trait]
impl Tool for GetDateTool {
    fn function_name(&self) -> &str {
        "os.get_date"
    }

    fn description(&self) -> &str {
        "Returns the actual current date and time (UTC, plus this backend's own local clock). \
         The current date is already given to you directly at the top of every conversation, so \
         you shouldn't need this for the basic 'what year is it' case — reach for this instead \
         when you specifically need a precise, freshly-computed timestamp: an exact machine-\
         readable ISO 8601 string, this backend's local time and UTC offset rather than UTC \
         alone, or the exact instant a long-running turn actually finishes rather than when it \
         started. Takes no arguments."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let now_utc = Utc::now();
        let now_local = chrono::Local::now();

        Ok(serde_json::to_value(DateOut {
            utc_iso8601: now_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
            utc_formatted: now_utc.format("%A, %B %-d, %Y %H:%M:%S UTC").to_string(),
            server_local_formatted: now_local.format("%A, %B %-d, %Y %H:%M:%S (UTC%:z)").to_string(),
        })?)
    }
}
