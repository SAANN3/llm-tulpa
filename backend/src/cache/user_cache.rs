use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use chrono::Utc;
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{
    facade::prompt::{GreetOut, PromptFacade},
    services::{error::ErrorService, settings_store::SettingsStore},
};

/// How long a cached value stays fresh before `greet` regenerates it on next access, and
/// how often the background loop calls `greet` on its own to keep that from ever being
/// visible on a real request's critical path.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// A cached value plus when it was generated, so a caller can decide for itself whether
/// it's still fresh. Generic so every cached thing (currently just the greeting) shares
/// the same shape rather than each defining its own `{value, generated_at}` pair.
struct Cached<T> {
    value: T,
    generated_at: Instant,
}

/// Holds the latest background-generated value for each thing worth caching. A plain
/// `tokio::sync::Mutex` per field, not `RwLock` — each cache-or-generate method always
/// takes its own mutex exclusively for the whole check-and-maybe-regenerate operation
/// (see `greet`'s docs for why), so there's no genuine read-only path to give a `RwLock`
/// an advantage over.
struct Cache {
    greet: Mutex<Option<Cached<GreetOut>>>,
    input_examples: Mutex<Option<Cached<Vec<String>>>>,
}

/// Keeps a background-refreshed, in-memory cache of LLM-generated content that would
/// otherwise sit on a request's critical path (a `greet` generation takes several
/// seconds). Composes `PromptFacade` to actually run a generation rather than
/// reimplementing prompt-building itself — this service only owns *when* to regenerate
/// and *where* the latest result lives, not *how* to produce it. Single-user-minded on
/// purpose: there's exactly one cached greeting, not one per user.
///
/// The loop can't run without a timezone, so it only starts once `SettingsStore` actually
/// has a row — `new` checks that on boot, and `start_loop` is re-triggered by the
/// settings route whenever that row is written for the first time or changed. Always
/// held as `Arc<UserCacheService>` (see `new`'s return type and `start_loop`'s `self`
/// type) — the background loop needs to hold a real `Self` reference across its `'static`
/// spawned task, not just cloned individual fields.
pub struct UserCacheService {
    settings: Arc<SettingsStore>,
    prompt: PromptFacade,
    cache: Cache,
    loop_handle: SyncMutex<Option<JoinHandle<()>>>,
}

impl UserCacheService {
    /// If settings are already persisted (every boot after the first), starts the
    /// refresh loop immediately. Otherwise leaves it stopped — there's nothing to
    /// generate a local time from yet — until the settings route calls `start_loop`
    /// once the frontend reports a timezone for the first time. Panics on a DB failure
    /// here rather than limping along half-initialized: if checking/starting fails now,
    /// it'll fail identically on the first real request anyway, just later and with a
    /// less obvious stack trace.
    pub async fn new(settings: Arc<SettingsStore>, prompt: PromptFacade) -> Arc<Self> {
        let this = Arc::new(Self {
            settings,
            prompt,
            cache: Cache { greet: Mutex::new(None), input_examples: Mutex::new(None) },
            loop_handle: SyncMutex::new(None),
        });

        if this
            .settings
            .has_settings()
            .await
            .expect("failed to check for settings on boot")
        {
            this.clone()
                .start_loop()
                .await
                .expect("failed to start user cache loop on boot despite settings existing");
        }

        this
    }

    /// (Re)starts the background refresh loop: stops any loop already running, drops
    /// the current cached values (they were generated under whatever settings were
    /// active before — possibly different ones — so they shouldn't outlive them even if
    /// not old enough to be time-stale yet), then spawns a fresh one that calls `greet`
    /// and `input_examples` every `REFRESH_INTERVAL`, starting immediately — so both are
    /// already warm by the time a real request needs them, not generated live on the
    /// frontend's first load. Errs without doing anything if no
    /// settings are persisted — this is the one precondition the loop can't work
    /// without. Takes `self` by owned `Arc` (not `&self`) — `Arc<Self>` is the one
    /// non-`&self`/`&mut self` receiver stable Rust allows without the
    /// `arbitrary_self_types` feature, and the spawned task needs an owned, cloned
    /// handle to call back into `greet` on. Callers pass `state.user_cache.clone()` (a
    /// cheap refcount bump, not a deep copy).
    pub async fn start_loop(self: Arc<Self>) -> Result<(), UserCacheErrors> {
        match self.settings.has_settings().await {
            Ok(true) => {}
            Ok(false) => return Err(UserCacheErrors::NoSettings),
            Err(_err) => {
                tracing::error!("user cache: failed to check for settings before starting loop");
                return Err(UserCacheErrors::NoSettings);
            }
        }

        if let Some(handle) = self.loop_handle.lock().unwrap().take() {
            handle.abort();
        }

        self.clear_cache().await;

        let this = self.clone();
        let handle = tokio::spawn(async move {
            loop {
                if let Err(err) = this.clone().greet().await {
                    tracing::error!("user cache: failed to refresh greet: {}", err.message.unwrap_or_default());
                }

                if let Err(err) = this.clone().input_examples().await {
                    tracing::error!(
                        "user cache: failed to refresh input_examples: {}",
                        err.message.unwrap_or_default()
                    );
                }

                tokio::time::sleep(REFRESH_INTERVAL).await;
            }
        });

        *self.loop_handle.lock().unwrap() = Some(handle);

        Ok(())
    }

    /// Stops the background loop, if running, and drops whatever's cached. Used when
    /// settings are deleted — otherwise the loop would keep ticking and erring against a
    /// settings row that's gone, and a stale greeting generated under settings that no
    /// longer exist could still be served. Unlike `start_loop`, just `&self`: nothing
    /// gets spawned here, only an existing task aborted, so there's no need for an owned
    /// `Arc<Self>`.
    pub async fn stop_loop(&self) {
        if let Some(handle) = self.loop_handle.lock().unwrap().take() {
            handle.abort();
        }

        self.clear_cache().await;
    }

    /// Clears every cached value. The one place that happens, so a future third cached
    /// field only needs one more line added here, not a matching line hunted down at
    /// every call site that currently clears `greet`/`input_examples`.
    async fn clear_cache(&self) {
        *self.cache.greet.lock().await = None;
        *self.cache.input_examples.lock().await = None;
    }

    /// Returns the cached greeting if one exists and is younger than `REFRESH_INTERVAL`;
    /// otherwise generates a fresh one from the currently persisted settings, caches it,
    /// and returns that. The single place this logic lives — called both by the
    /// periodic loop and directly by the greet route, so there's no separate "real"
    /// generation path to drift out of sync with.
    ///
    /// The actual work runs inside a detached `tokio::spawn`, which this only ever
    /// *joins* (`.await`s the handle) — never `.abort()`s. If the caller's own future
    /// gets dropped (an HTTP client disconnecting mid-request, say), only this join
    /// drops; the spawned task keeps running to completion regardless, still holding
    /// `cache.greet`'s lock for its whole check-and-maybe-regenerate span, and still
    /// lands its result in the cache once Ollama responds. That's deliberate: a
    /// cancelled request no longer wastes an in-flight generation, and repeated rapid
    /// disconnects (a page getting refreshed several times in a row, say) can't prevent
    /// the cache from ever catching up, the way aborting the work on every cancel would.
    /// Concurrent callers stay safe the same way as before — whichever caller's spawned
    /// task gets the lock first runs the real regeneration, everyone else's task just
    /// blocks on the same lock and then reads the now-fresh result instead of racing to
    /// independently regenerate. Takes `self` by owned `Arc` for the same reason
    /// `start_loop` does — the spawned task must outlive this call's own stack frame.
    pub async fn greet(self: Arc<Self>) -> Result<GreetOut, ErrorService> {
        let handle = tokio::spawn(async move {
            let mut cache = self.cache.greet.lock().await;

            if let Some(cached) = cache.as_ref() {
                if cached.generated_at.elapsed() < REFRESH_INTERVAL {
                    return Ok(cached.value.clone());
                }
            }

            let current = self.settings.settings().await?;
            let greet = self.prompt.greet(Self::local_time_text(current.timezone), current.name).await?;

            *cache = Some(Cached { value: greet.clone(), generated_at: Instant::now() });

            Ok(greet)
        });

        handle.await.map_err(Self::join_panic_error)?
    }

    /// Returns one randomly-picked placeholder string, regenerating the cached batch of
    /// them from scratch once it's older than `REFRESH_INTERVAL` — same shape, same
    /// spawn-and-join-not-abort behavior, and the same whole-operation locking as
    /// `greet` (see its docs), just with a random pick off the end instead of handing
    /// back the cached value directly, so repeat callers within one `REFRESH_INTERVAL`
    /// window still see variety instead of the same string every time.
    pub async fn input_examples(self: Arc<Self>) -> Result<String, ErrorService> {
        let handle = tokio::spawn(async move {
            let mut cache = self.cache.input_examples.lock().await;

            if let Some(cached) = cache.as_ref() {
                if cached.generated_at.elapsed() < REFRESH_INTERVAL {
                    return Ok(Self::random_pick(&cached.value));
                }
            }

            let current = self.settings.settings().await?;
            let examples = self.prompt.input_examples(Self::local_time_text(current.timezone)).await?;

            let chosen = Self::random_pick(&examples);
            *cache = Some(Cached { value: examples, generated_at: Instant::now() });

            Ok(chosen)
        });

        handle.await.map_err(Self::join_panic_error)?
    }

    /// Maps a spawned task's `JoinError` (only ever a panic here — see `greet`'s docs on
    /// why this is joined, never aborted) to the one error type route handlers return.
    fn join_panic_error(_: tokio::task::JoinError) -> ErrorService {
        ErrorService::internal("background generation task panicked")
    }

    /// Picks one element at a pseudo-random index — a nanosecond timestamp modulo the
    /// slice length is plenty for "vary the placeholder text shown," which has no
    /// correctness or security stake in true randomness, so this skips pulling in a
    /// dedicated RNG crate for it. Empty input is a caller bug (a generation that
    /// produced zero usable lines), not something to paper over, so it panics rather
    /// than silently returning an empty string.
    fn random_pick(items: &[String]) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();

        items[nanos as usize % items.len()].clone()
    }

    /// Renders "now" at the given UTC offset (whole hours) the same way the frontend's
    /// `localTimeText` does for the live-request path — fixed English weekday/month
    /// names and 24-hour time, so the model gets a consistently formatted value
    /// regardless of where it was computed.
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

#[derive(Debug)]
pub enum UserCacheErrors {
    NoSettings,
}

impl From<UserCacheErrors> for ErrorService {
    fn from(err: UserCacheErrors) -> Self {
        match err {
            UserCacheErrors::NoSettings => ErrorService::new(
                StatusCode::CONFLICT,
                "user settings have not been configured yet",
            ),
        }
    }
}
