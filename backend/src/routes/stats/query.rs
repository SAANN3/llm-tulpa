use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Deserialize, IntoParams)]
pub(crate) struct StatsQuery {
    /// How many days to count, today included (default 30, at most 365)
    pub(super) days: Option<u32>,
    /// Instead of `days`: how many calendar months to count, the current one included — from the
    /// 1st of the earliest to today (at most 12)
    pub(super) months: Option<u32>,
}
