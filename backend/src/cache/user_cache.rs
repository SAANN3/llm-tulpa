use std::collections::HashMap;
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::Mutex;

use crate::{
    facade::prompt::{GreetOut, PromptFacade},
    services::{error::ErrorService, settings_store::SettingsStore},
};

/// How long a per-user cached value stays fresh before it's regenerated on next access.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

struct Cached<T> {
    value: T,
    generated_at: Instant,
}

/// One user's cached value. The tokio mutex is held for the whole check-and-maybe-generate,
/// so two requests for the same user (or a request racing a `warm`) produce one generation
/// between them rather than two; different users have different slots and never wait on each
/// other's generation.
type Slot<T> = Arc<Mutex<Option<Cached<T>>>>;

/// Per-user map of slots. The std mutex only guards the map itself (never held across an
/// `.await`); the slot's own mutex guards the generation.
struct PerUser<T> {
    slots: SyncMutex<HashMap<i64, Slot<T>>>,
}

impl<T> PerUser<T> {
    fn new() -> Self {
        Self { slots: SyncMutex::new(HashMap::new()) }
    }

    fn slot(&self, user_id: i64) -> Slot<T> {
        self.slots.lock().unwrap().entry(user_id).or_default().clone()
    }

    /// Forgets the user's cached value. A generation already in flight finishes into the
    /// discarded slot, so a stale result can never land in the fresh one.
    fn invalidate(&self, user_id: i64) {
        self.slots.lock().unwrap().remove(&user_id);
    }
}

/// Per-user, on-demand cache of the LLM-generated landing-page content (greeting +
/// input-example placeholders) that would otherwise sit on a request's critical path.
/// Keyed by user id, so each user sees content generated from their own name/timezone/model.
///
/// There is deliberately no background refresh loop: a loop per user would generate every
/// user's content every hour whether or not they ever open the app, on a GPU shared with real
/// chat turns. Content is generated the first time it's asked for and again once it's an hour
/// old; `warm` (called at sign-in) hides that first wait, and `invalidate` (called when a user
/// changes name, timezone or model) drops content that no longer reflects them.
pub struct UserCacheService {
    settings: Arc<SettingsStore>,
    prompt: PromptFacade,
    greet: PerUser<GreetOut>,
    input_examples: PerUser<Vec<String>>,
}

impl UserCacheService {
    pub fn new(settings: Arc<SettingsStore>, prompt: PromptFacade) -> Arc<Self> {
        Arc::new(Self {
            settings,
            prompt,
            greet: PerUser::new(),
            input_examples: PerUser::new(),
        })
    }

    /// A greeting for `user_id`, from cache when fresh, otherwise generated from that
    /// user's persisted name/timezone and cached.
    pub async fn greet(&self, user_id: i64) -> Result<GreetOut, ErrorService> {
        let slot = self.greet.slot(user_id);
        let mut cached = slot.lock().await;
        if let Some(fresh) = cached.as_ref().filter(|c| c.generated_at.elapsed() < REFRESH_INTERVAL) {
            return Ok(fresh.value.clone());
        }

        let settings = self.settings.settings(user_id).await?;
        let name = settings.name.unwrap_or_default();
        let model = self.settings.effective_model(user_id).await?;
        let greet = self
            .prompt
            .greet(Self::local_time_text(settings.timezone.unwrap_or(0)), name, &model)
            .await?;

        *cached = Some(Cached { value: greet.clone(), generated_at: Instant::now() });
        Ok(greet)
    }

    /// One randomly-picked placeholder string for `user_id`, regenerating the cached batch
    /// when stale.
    pub async fn input_examples(&self, user_id: i64) -> Result<String, ErrorService> {
        let slot = self.input_examples.slot(user_id);
        let mut cached = slot.lock().await;
        if let Some(fresh) = cached.as_ref().filter(|c| c.generated_at.elapsed() < REFRESH_INTERVAL) {
            return Ok(Self::random_pick(&fresh.value));
        }

        let settings = self.settings.settings(user_id).await?;
        let model = self.settings.effective_model(user_id).await?;
        let examples = self
            .prompt
            .input_examples(Self::local_time_text(settings.timezone.unwrap_or(0)), &model)
            .await?;
        let chosen = Self::random_pick(&examples);

        *cached = Some(Cached { value: examples, generated_at: Instant::now() });
        Ok(chosen)
    }

    /// Drops everything cached for `user_id`; the next request regenerates it. Called when the
    /// user changes something the content is generated from.
    pub fn invalidate(&self, user_id: i64) {
        self.greet.invalidate(user_id);
        self.input_examples.invalidate(user_id);
    }

    /// Generates `user_id`'s content in the background, so the landing page finds it ready.
    /// Best effort: a failure (no model picked yet, Ollama busy or down) is only logged, and
    /// the request that actually needs the content simply generates it then.
    pub fn warm(self: &Arc<Self>, user_id: i64) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.greet(user_id).await {
                tracing::debug!("warming the greeting for user {user_id} skipped: {}", e.message.unwrap_or_default());
            }
            if let Err(e) = this.input_examples(user_id).await {
                tracing::debug!("warming the input examples for user {user_id} skipped: {}", e.message.unwrap_or_default());
            }
        });
    }

    fn random_pick(items: &[String]) -> String {
        if items.is_empty() {
            return String::new();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        items[nanos as usize % items.len()].clone()
    }

    /// Renders "now" at the given UTC offset (whole hours) the same way the frontend does.
    fn local_time_text(offset_hours: i32) -> String {
        let local = Utc::now() + chrono::Duration::hours(offset_hours as i64);
        format!(
            "{} {} of {} {}, {}",
            local.format("%A"),
            local.format("%-d"),
            local.format("%B").to_string().to_lowercase(),
            local.format("%Y"),
            local.format("%H:%M:%S"),
        )
    }
}
