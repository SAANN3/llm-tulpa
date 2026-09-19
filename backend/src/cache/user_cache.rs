use std::collections::HashMap;
use std::sync::Arc;
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

/// Per-user, on-demand cache of the LLM-generated landing-page content (greeting +
/// input-example placeholders) that would otherwise sit on a request's critical path.
/// Keyed by user id, so each user sees content generated from their own name/timezone.
/// No background pre-warm loop — generation happens lazily the first time each user's
/// content is requested (and after it goes stale), which decouples this from the old
/// global settings singleton.
pub struct UserCacheService {
    settings: Arc<SettingsStore>,
    prompt: PromptFacade,
    greet: Mutex<HashMap<i64, Cached<GreetOut>>>,
    input_examples: Mutex<HashMap<i64, Cached<Vec<String>>>>,
}

impl UserCacheService {
    pub fn new(settings: Arc<SettingsStore>, prompt: PromptFacade) -> Arc<Self> {
        Arc::new(Self {
            settings,
            prompt,
            greet: Mutex::new(HashMap::new()),
            input_examples: Mutex::new(HashMap::new()),
        })
    }

    /// A greeting for `user_id`, from cache when fresh, otherwise generated from that
    /// user's persisted name/timezone and cached.
    pub async fn greet(&self, user_id: i64) -> Result<GreetOut, ErrorService> {
        {
            let cache = self.greet.lock().await;
            if let Some(cached) = cache.get(&user_id) {
                if cached.generated_at.elapsed() < REFRESH_INTERVAL {
                    return Ok(cached.value.clone());
                }
            }
        }

        let settings = self.settings.settings(user_id).await?;
        let name = settings.name.unwrap_or_default();
        let model = self.settings.effective_model(user_id).await?;
        let greet = self
            .prompt
            .greet(Self::local_time_text(settings.timezone.unwrap_or(0)), name, &model)
            .await?;

        self.greet
            .lock()
            .await
            .insert(user_id, Cached { value: greet.clone(), generated_at: Instant::now() });
        Ok(greet)
    }

    /// One randomly-picked placeholder string for `user_id`, regenerating the cached batch
    /// when stale.
    pub async fn input_examples(&self, user_id: i64) -> Result<String, ErrorService> {
        {
            let cache = self.input_examples.lock().await;
            if let Some(cached) = cache.get(&user_id) {
                if cached.generated_at.elapsed() < REFRESH_INTERVAL {
                    return Ok(Self::random_pick(&cached.value));
                }
            }
        }

        let settings = self.settings.settings(user_id).await?;
        let model = self.settings.effective_model(user_id).await?;
        let examples = self
            .prompt
            .input_examples(Self::local_time_text(settings.timezone.unwrap_or(0)), &model)
            .await?;
        let chosen = Self::random_pick(&examples);

        self.input_examples
            .lock()
            .await
            .insert(user_id, Cached { value: examples, generated_at: Instant::now() });
        Ok(chosen)
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
