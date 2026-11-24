use super::{BlueprintFacade, definition::Schedule};
use crate::services::{blueprint_store::ScheduledTick, error::ErrorService};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use std::{sync::Arc, time::Duration as Wait};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const CLOCK_RECHECK: Wait = Wait::from_secs(30);
const ERROR_RETRY: Wait = Wait::from_secs(5);

pub(super) struct Scheduler {
    changed: Notify,
    shutdown: CancellationToken,
}
impl Scheduler {
    pub fn new(shutdown: CancellationToken) -> Self {
        Self {
            changed: Notify::new(),
            shutdown,
        }
    }
    pub fn changed(&self) {
        self.changed.notify_one();
    }

    pub fn start(facade: Arc<BlueprintFacade>) {
        tokio::spawn(async move {
            let scheduler = &facade.scheduler;
            loop {
                if scheduler.shutdown.is_cancelled() {
                    break;
                }
                // Register before reading: an edit during the query must interrupt the resulting sleep.
                let notified = scheduler.changed.notified();
                let result = tokio::select! {
                    biased;
                    _ = scheduler.shutdown.cancelled() => break,
                    result = scheduler.tick(&facade, Utc::now()) => result,
                };
                let wait = match result {
                    Ok(wait) => wait,
                    Err(error) => {
                        // Database failures leave the occurrence intact for a later retry.
                        tracing::warn!(detail = ?error.message, "blueprint scheduling failed");
                        ERROR_RETRY
                    }
                };
                tokio::select! {
                    _ = scheduler.shutdown.cancelled() => break,
                    _ = notified => {},
                    _ = tokio::time::sleep(wait) => {},
                }
            }
        });
    }
    async fn tick(
        &self,
        facade: &BlueprintFacade,
        now: DateTime<Utc>,
    ) -> Result<Wait, ErrorService> {
        let Some(record) = facade.store.next_scheduled().await? else {
            return Ok(CLOCK_RECHECK);
        };
        let due = record
            .next_due_at
            .ok_or_else(|| ErrorService::internal("scheduled blueprint has no due time"))?;
        if due > now {
            return Ok((due - now).to_std().unwrap_or_default().min(CLOCK_RECHECK));
        }
        let next = record
            .schedule
            .after_tick(due, now)
            .map_err(ErrorService::internal)?
            .ok_or_else(|| ErrorService::internal("manual blueprint has a due time"))?;
        let result = facade
            .enqueue(
                record.user_id,
                record.id,
                record.revision,
                Some(ScheduledTick {
                    due_at: due,
                    next_due_at: next,
                }),
            )
            .await;
        if let Err(error) = result {
            // An edit/delete may win while readiness is checked. Re-read the current schedule.
            if !matches!(
                error.http_code,
                StatusCode::CONFLICT | StatusCode::NOT_FOUND
            ) {
                return Err(error);
            }
        }
        Ok(Wait::ZERO)
    }
}

impl Schedule {
    pub(crate) fn next_after(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Manual => Ok(None),
            Self::Interval { seconds } => {
                if *seconds == 0 {
                    return Err("the interval must be at least one second".into());
                }
                after
                    .checked_add_signed(Duration::seconds(*seconds as i64))
                    .map(Some)
                    .ok_or_else(|| "schedule time overflow".into())
            }
            Self::Daily { time, timezone } => {
                let time = NaiveTime::parse_from_str(time, "%H:%M")
                    .map_err(|_| "daily time must be HH:MM")?;
                let tz = timezone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| "unknown IANA timezone")?;
                let mut date = after.with_timezone(&tz).date_naive();
                loop {
                    // A gap has no occurrence; an overlap has only its first occurrence.
                    let candidate = match tz.from_local_datetime(&date.and_time(time)) {
                        LocalResult::Single(t) => Some(t),
                        LocalResult::Ambiguous(first, second) => Some(first.min(second)),
                        LocalResult::None => None,
                    };
                    if let Some(t) = candidate.filter(|t| t.with_timezone(&Utc) > after) {
                        return Ok(Some(t.with_timezone(&Utc)));
                    }
                    date = date.succ_opt().ok_or("schedule time overflow")?;
                }
            }
        }
    }
    pub(crate) fn after_tick(
        &self,
        due: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Interval { seconds } if *seconds > 0 => {
                // Keep the original cadence, coalescing missed occurrences rather than drifting from completion.
                let seconds = *seconds as i64;
                let steps = (now - due).num_seconds().max(0) / seconds + 1;
                let elapsed = steps.checked_mul(seconds).ok_or("schedule time overflow")?;
                due.checked_add_signed(Duration::seconds(elapsed))
                    .map(Some)
                    .ok_or_else(|| "schedule time overflow".into())
            }
            _ => self.next_after(now),
        }
    }
}
