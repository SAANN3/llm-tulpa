//! Read-only aggregations over a user's own chats for the stats page. Messages are read once per
//! request (only the columns the numbers need, never the text) and summed in Rust, which keeps
//! the day boundary — the user's own timezone — in one place instead of in every query.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Days, FixedOffset, Months, NaiveDate, NaiveTime, Offset, Timelike, Utc};
use sea_orm::{prelude::*, FromQueryResult, QueryOrder, QuerySelect, QueryTrait};

use super::entities::{chats, messages};
use super::{ChatStore, ChatStoreErrors};

/// A call this long (the whole round trip, as stored in `thought_duration_ms`) counts as slow:
/// on a prompt this size it nearly always means the server had to evaluate the prompt again
/// instead of finding it in its cache.
const SLOW_CALL_MS: i64 = 30_000;

/// How many of the largest chats `context_usage` lists
const LARGEST_CHATS: u64 = 8;

/// The span a stats query covers, in the user's own timezone (a whole-hour offset from UTC, as
/// their settings store it), ending today.
#[derive(Clone, Copy)]
pub struct StatsRange {
    offset: FixedOffset,
    first_day: NaiveDate,
    days: u32,
}

impl StatsRange {
    /// The last `days` calendar days, today included
    pub fn last_days(days: u32, offset_hours: i32) -> Self {
        let (offset, today) = Self::offset_and_today(offset_hours);
        Self { offset, first_day: today - Days::new(u64::from(days.saturating_sub(1))), days }
    }

    /// The last `months` calendar months, the current one included: from the 1st of the earliest
    /// of them to today. Whole months, so a view can lay them out as calendars.
    pub fn last_months(months: u32, offset_hours: i32) -> Self {
        let (offset, today) = Self::offset_and_today(offset_hours);
        let first_day = today
            .with_day(1)
            .and_then(|first| first.checked_sub_months(Months::new(months.saturating_sub(1))))
            .unwrap_or(today);
        let days = u32::try_from((today - first_day).num_days() + 1).unwrap_or(1);
        Self { offset, first_day, days }
    }

    fn offset_and_today(offset_hours: i32) -> (FixedOffset, NaiveDate) {
        // An out-of-range offset falls back to UTC rather than failing a stats request
        let offset = FixedOffset::east_opt(offset_hours.saturating_mul(3600)).unwrap_or(Utc.fix());
        (offset, Utc::now().with_timezone(&offset).date_naive())
    }

    /// The instant the first day starts at, in the user's timezone
    pub fn since(&self) -> DateTimeUtc {
        let local_midnight = self.first_day.and_time(NaiveTime::MIN);
        (local_midnight - chrono::Duration::seconds(i64::from(self.offset.local_minus_utc()))).and_utc()
    }

    fn day_of(&self, at: DateTimeUtc) -> NaiveDate {
        at.with_timezone(&self.offset).date_naive()
    }

    /// Every day of the range, oldest first
    pub fn each_day(&self) -> Vec<NaiveDate> {
        (0..self.days).map(|i| self.first_day + Days::new(u64::from(i))).collect()
    }
}

/// One day of replies and what they cost. The `timed_*`/`processed_*` sums cover only the replies
/// that carry the matching timing, so a speed (`tokens / ms`) never divides by a total that
/// includes replies recorded before timings existed, or from a backend that doesn't report them.
pub struct DailyUsage {
    pub day: NaiveDate,
    pub replies: u64,
    pub prompt_tokens: i64,
    pub eval_tokens: i64,
    pub timed_replies: u64,
    pub timed_eval_tokens: i64,
    pub eval_ms: i64,
    pub processed_calls: u64,
    pub processed_tokens: i64,
    pub processed_ms: i64,
    pub load_ms: i64,
    pub call_ms_median: Option<i64>,
    pub call_ms_p95: Option<i64>,
    pub slow_calls: u64,
}

pub struct ModelUsage {
    pub provider: String,
    pub model: String,
    pub chats: u64,
    pub replies: u64,
    pub eval_tokens: i64,
    pub timed_eval_tokens: i64,
    pub eval_ms: i64,
    pub call_ms_median: Option<i64>,
}

pub struct ToolUsage {
    pub tool: String,
    pub calls: u64,
    /// Ran and failed
    pub failed: u64,
    /// Never ran: the permission system refused the call
    pub denied: u64,
}

pub struct Activity {
    /// The user's own messages per hour of each day of the range, oldest day first
    pub hourly: Vec<(NaiveDate, [u32; 24])>,
    /// Chats started per day, for every day of the range
    pub chats_per_day: Vec<(NaiveDate, u32)>,
}

pub struct ContextEntry {
    pub chat_id: i64,
    pub name: String,
    pub prompt_tokens: i64,
}

pub struct ContextUsage {
    pub largest: Vec<ContextEntry>,
    pub compacted_chats: u64,
    pub total_chats: u64,
}

/// The columns of `messages` the aggregations read
#[derive(FromQueryResult)]
struct StatRow {
    chat_id: i64,
    role: String,
    tool_name: Option<String>,
    tool_success: Option<bool>,
    tool_denied: bool,
    created_at: DateTimeUtc,
    prompt_tokens: Option<i64>,
    eval_tokens: Option<i64>,
    thought_duration_ms: Option<i64>,
    eval_duration_ms: Option<i64>,
    prompt_eval_duration_ms: Option<i64>,
    load_duration_ms: Option<i64>,
    prompt_processed_tokens: Option<i64>,
}

impl StatRow {
    /// An assistant message that came from a model call (the others carry no token counts)
    fn is_reply(&self) -> bool {
        self.role == "assistant" && (self.prompt_tokens.is_some() || self.eval_tokens.is_some())
    }
}

/// The value `percent` of the way up `sorted` (nearest rank); `None` when empty
fn percentile(sorted: &[i64], percent: usize) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    Some(sorted[rank - 1])
}

fn sorted(mut values: Vec<i64>) -> Vec<i64> {
    values.sort_unstable();
    values
}

impl ChatStore {
    /// Every message of the user's chats since the range began — sub-agent and deleted chats
    /// included, since what they cost was spent either way.
    async fn stat_rows(&self, user_id: i64, range: &StatsRange) -> Result<Vec<StatRow>, ChatStoreErrors> {
        let rows = messages::Entity::find()
            .select_only()
            .columns([
                messages::Column::ChatId,
                messages::Column::Role,
                messages::Column::ToolName,
                messages::Column::ToolSuccess,
                messages::Column::ToolDenied,
                messages::Column::CreatedAt,
                messages::Column::PromptTokens,
                messages::Column::EvalTokens,
                messages::Column::ThoughtDurationMs,
                messages::Column::EvalDurationMs,
                messages::Column::PromptEvalDurationMs,
                messages::Column::LoadDurationMs,
                messages::Column::PromptProcessedTokens,
            ])
            .filter(messages::Column::CreatedAt.gte(range.since()))
            .filter(messages::Column::ChatId.in_subquery(
                chats::Entity::find()
                    .select_only()
                    .column(chats::Column::Id)
                    .filter(chats::Column::UserId.eq(user_id))
                    .into_query(),
            ))
            .into_model::<StatRow>()
            .all(&self.db)
            .await?;
        Ok(rows)
    }

    /// Per day of the range (every day, empty ones included): replies, tokens, timings and how
    /// long the calls took.
    pub async fn usage_daily(&self, user_id: i64, range: StatsRange) -> Result<Vec<DailyUsage>, ChatStoreErrors> {
        let rows = self.stat_rows(user_id, &range).await?;

        let mut call_times: HashMap<NaiveDate, Vec<i64>> = HashMap::new();
        let mut days: BTreeMap<NaiveDate, DailyUsage> = range
            .each_day()
            .into_iter()
            .map(|day| {
                let empty = DailyUsage {
                    day,
                    replies: 0,
                    prompt_tokens: 0,
                    eval_tokens: 0,
                    timed_replies: 0,
                    timed_eval_tokens: 0,
                    eval_ms: 0,
                    processed_calls: 0,
                    processed_tokens: 0,
                    processed_ms: 0,
                    load_ms: 0,
                    call_ms_median: None,
                    call_ms_p95: None,
                    slow_calls: 0,
                };
                (day, empty)
            })
            .collect();

        for row in rows.iter().filter(|row| row.is_reply()) {
            let day = range.day_of(row.created_at);
            let Some(usage) = days.get_mut(&day) else { continue };

            usage.replies += 1;
            usage.prompt_tokens += row.prompt_tokens.unwrap_or(0);
            usage.eval_tokens += row.eval_tokens.unwrap_or(0);
            if let (Some(eval_ms), Some(tokens)) = (row.eval_duration_ms, row.eval_tokens) {
                usage.timed_replies += 1;
                usage.timed_eval_tokens += tokens;
                usage.eval_ms += eval_ms;
            }
            if let (Some(prompt_ms), Some(processed)) = (row.prompt_eval_duration_ms, row.prompt_processed_tokens) {
                usage.processed_calls += 1;
                usage.processed_tokens += processed;
                usage.processed_ms += prompt_ms;
            }
            usage.load_ms += row.load_duration_ms.unwrap_or(0);
            if let Some(call_ms) = row.thought_duration_ms {
                call_times.entry(day).or_default().push(call_ms);
                if call_ms >= SLOW_CALL_MS {
                    usage.slow_calls += 1;
                }
            }
        }

        for (day, times) in call_times {
            let times = sorted(times);
            if let Some(usage) = days.get_mut(&day) {
                usage.call_ms_median = percentile(&times, 50);
                usage.call_ms_p95 = percentile(&times, 95);
            }
        }

        Ok(days.into_values().collect())
    }

    /// Replies per model the user's chats were bound to, busiest first
    pub async fn usage_by_model(&self, user_id: i64, range: StatsRange) -> Result<Vec<ModelUsage>, ChatStoreErrors> {
        let rows = self.stat_rows(user_id, &range).await?;

        let chat_models: HashMap<i64, i64> = chats::Entity::find()
            .select_only()
            .columns([chats::Column::Id, chats::Column::ModelId])
            .filter(chats::Column::UserId.eq(user_id))
            .into_tuple()
            .all(&self.db)
            .await?
            .into_iter()
            .collect();
        let model_ids: Vec<i64> = chat_models.values().copied().collect::<std::collections::HashSet<_>>().into_iter().collect();
        let refs = self.models.get_many(&model_ids).await?;

        #[derive(Default)]
        struct Sums {
            chats: std::collections::HashSet<i64>,
            replies: u64,
            eval_tokens: i64,
            timed_eval_tokens: i64,
            eval_ms: i64,
            call_times: Vec<i64>,
        }
        let mut by_model: HashMap<i64, Sums> = HashMap::new();
        for row in rows.iter().filter(|row| row.is_reply()) {
            let Some(model_id) = chat_models.get(&row.chat_id) else { continue };
            let sums = by_model.entry(*model_id).or_default();
            sums.chats.insert(row.chat_id);
            sums.replies += 1;
            sums.eval_tokens += row.eval_tokens.unwrap_or(0);
            if let (Some(eval_ms), Some(tokens)) = (row.eval_duration_ms, row.eval_tokens) {
                sums.timed_eval_tokens += tokens;
                sums.eval_ms += eval_ms;
            }
            if let Some(call_ms) = row.thought_duration_ms {
                sums.call_times.push(call_ms);
            }
        }

        let mut out: Vec<ModelUsage> = by_model
            .into_iter()
            .filter_map(|(model_id, sums)| {
                // A model that was since removed has nothing to name it by
                let model = refs.get(&model_id)?;
                Some(ModelUsage {
                    provider: model.provider.clone(),
                    model: model.name.clone(),
                    chats: sums.chats.len() as u64,
                    replies: sums.replies,
                    eval_tokens: sums.eval_tokens,
                    timed_eval_tokens: sums.timed_eval_tokens,
                    eval_ms: sums.eval_ms,
                    call_ms_median: percentile(&sorted(sums.call_times), 50),
                })
            })
            .collect();
        out.sort_by(|a, b| b.replies.cmp(&a.replies).then_with(|| a.model.cmp(&b.model)));
        Ok(out)
    }

    /// How often each tool was called and how those calls ended, most used first
    pub async fn usage_by_tool(&self, user_id: i64, range: StatsRange) -> Result<Vec<ToolUsage>, ChatStoreErrors> {
        let rows = self.stat_rows(user_id, &range).await?;

        let mut by_tool: BTreeMap<String, ToolUsage> = BTreeMap::new();
        for row in rows.iter().filter(|row| row.role == "tool") {
            let Some(tool) = &row.tool_name else { continue };
            let usage = by_tool.entry(tool.clone()).or_insert_with(|| ToolUsage {
                tool: tool.clone(),
                calls: 0,
                failed: 0,
                denied: 0,
            });
            usage.calls += 1;
            if row.tool_denied {
                usage.denied += 1;
            } else if row.tool_success == Some(false) {
                usage.failed += 1;
            }
        }

        let mut out: Vec<ToolUsage> = by_tool.into_values().collect();
        out.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.tool.cmp(&b.tool)));
        Ok(out)
    }

    /// When the user is active (their messages per hour of each day), and how many chats they started
    pub async fn activity(&self, user_id: i64, range: StatsRange) -> Result<Activity, ChatStoreErrors> {
        let rows = self.stat_rows(user_id, &range).await?;

        let mut hourly: BTreeMap<NaiveDate, [u32; 24]> =
            range.each_day().into_iter().map(|day| (day, [0u32; 24])).collect();
        for row in rows.iter().filter(|row| row.role == "user") {
            let local = row.created_at.with_timezone(&range.offset);
            if let Some(hours) = hourly.get_mut(&local.date_naive()) {
                hours[local.hour() as usize] += 1;
            }
        }

        let started: Vec<DateTimeUtc> = chats::Entity::find()
            .select_only()
            .column(chats::Column::CreatedAt)
            .filter(chats::Column::UserId.eq(user_id))
            .filter(chats::Column::ParentChatId.is_null())
            .filter(chats::Column::CreatedAt.gte(range.since()))
            .into_tuple()
            .all(&self.db)
            .await?;
        let mut per_day: BTreeMap<NaiveDate, u32> = range.each_day().into_iter().map(|day| (day, 0)).collect();
        for created_at in started {
            if let Some(count) = per_day.get_mut(&range.day_of(created_at)) {
                *count += 1;
            }
        }

        Ok(Activity { hourly: hourly.into_iter().collect(), chats_per_day: per_day.into_iter().collect() })
    }

    /// The user's biggest chats by what the model last evaluated for them, and how many chats
    /// have been compacted
    pub async fn context_usage(&self, user_id: i64) -> Result<ContextUsage, ChatStoreErrors> {
        let live = || {
            chats::Entity::find()
                .filter(chats::Column::UserId.eq(user_id))
                .filter(chats::Column::IsDeleted.eq(false))
                .filter(chats::Column::ParentChatId.is_null())
        };

        let total_chats = live().count(&self.db).await?;
        let compacted_chats = live().filter(chats::Column::Summary.is_not_null()).count(&self.db).await?;
        let largest = live()
            .filter(chats::Column::LastPromptTokens.is_not_null())
            .order_by_desc(chats::Column::LastPromptTokens)
            .limit(LARGEST_CHATS)
            .all(&self.db)
            .await?
            .into_iter()
            .filter_map(|chat| {
                Some(ContextEntry { chat_id: chat.id, name: chat.name, prompt_tokens: chat.last_prompt_tokens? })
            })
            .collect();

        Ok(ContextUsage { largest, compacted_chats, total_chats })
    }

    /// The ids of every chat the user owns, sub-agent and deleted ones included — what the job
    /// store scopes its own queries by, since a job belongs to a chat and not to a user.
    pub async fn chat_ids_of(&self, user_id: i64) -> Result<Vec<i64>, ChatStoreErrors> {
        Ok(chats::Entity::find()
            .select_only()
            .column(chats::Column::Id)
            .filter(chats::Column::UserId.eq(user_id))
            .into_tuple()
            .all(&self.db)
            .await?)
    }
}
