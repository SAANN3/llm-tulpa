//! How fast the installed speech models read on this machine at each thread count, measured with a recording the owner
//! made. A model's speed depends on the voice and the words only a little, and a tone or noise would make a model write
//! nothing, which is quicker than reading speech: so the measurement is made on a real voice, kept in memory for as long
//! as it takes and dropped after. It runs as a background task, since a model at a low thread count takes a while.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use super::catalog::MODELS;
use super::install::is_installed;
use super::session::SAMPLE_RATE;
use super::SttService;
use crate::services::error::ErrorService;

/// A recording shorter than this says too little to time, and a longer one makes a model at two threads take minutes
const MIN_SECONDS: usize = 3;
const MAX_SECONDS: usize = 15;
/// A thread count within this share of the fastest is as good as it: the rest of the cores are better left to the language model
const GOOD_ENOUGH: f32 = 0.9;
/// The thread counts worth trying, up to the machine's cores; the cores themselves are tried too. One thread is
/// left out: it only makes a model take longest.
const COUNTS: [u32; 6] = [2, 4, 6, 8, 12, 16];

#[derive(Clone, Serialize, ToSchema)]
pub struct SttBenchRun {
    pub threads: u32,
    /// How long the recording took to read
    pub seconds: f32,
    /// How many times faster than real time: 2 reads a second of speech in half a second
    pub speed: f32,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct SttBenchModel {
    pub model: String,
    pub label: String,
    pub runs: Vec<SttBenchRun>,
    /// The fewest threads that come within a tenth of the fastest run
    pub best_threads: Option<u32>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct SttBenchmark {
    /// `running`, `done` or `failed`
    pub state: String,
    pub audio_seconds: f32,
    /// How many of the `total` readings are done
    pub done: usize,
    pub total: usize,
    /// What is being read now, while it is
    pub note: Option<String>,
    pub models: Vec<SttBenchModel>,
    pub error: Option<String>,
}

/// How many cores this machine has for the models to use
pub fn cores() -> u32 {
    std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4)
}

/// The thread counts to measure on a machine of `cores` cores
fn thread_counts(cores: u32) -> Vec<u32> {
    let mut counts: Vec<u32> = COUNTS.iter().copied().filter(|&n| n <= cores).collect();
    if counts.last() != Some(&cores) {
        counts.push(cores);
    }
    counts.dedup();
    counts
}

fn best_of(runs: &[SttBenchRun]) -> Option<u32> {
    let fastest = runs.iter().map(|r| r.speed).fold(0.0_f32, f32::max);
    runs.iter().find(|r| r.speed >= fastest * GOOD_ENOUGH).map(|r| r.threads)
}

impl SttService {
    pub fn benchmark(&self) -> Option<SttBenchmark> {
        self.benchmark.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn update_benchmark(slot: &Mutex<Option<SttBenchmark>>, change: impl FnOnce(&mut SttBenchmark)) {
        if let Some(bench) = slot.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            change(bench);
        }
    }

    /// Starts reading `audio` (16 kHz mono, a voice of a few seconds) with every installed model at each thread count,
    /// in the background; `language` is what a model that needs one is told. Watch it with `benchmark`.
    pub fn start_benchmark(self: &Arc<Self>, audio: Vec<f32>, language: String) -> Result<SttBenchmark, ErrorService> {
        if audio.len() < MIN_SECONDS * SAMPLE_RATE || audio.len() > MAX_SECONDS * SAMPLE_RATE {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("the recording must be {MIN_SECONDS} to {MAX_SECONDS} seconds long")));
        }
        let models: Vec<_> = MODELS.iter().filter(|m| is_installed(&self.dir, m.id)).collect();
        if models.is_empty() {
            return Err(ErrorService::new(StatusCode::CONFLICT, "no speech model is installed"));
        }
        if !self.sessions().is_empty() {
            return Err(ErrorService::new(StatusCode::CONFLICT, "a voice input is open: measuring takes all the cores, so it waits until nobody is speaking"));
        }
        let counts = thread_counts(cores());
        let started = SttBenchmark {
            state: "running".into(),
            audio_seconds: audio.len() as f32 / SAMPLE_RATE as f32,
            done: 0,
            total: models.len() * counts.len(),
            note: None,
            models: models.iter().map(|m| SttBenchModel { model: m.id.to_string(), label: m.label.to_string(), runs: Vec::new(), best_threads: None }).collect(),
            error: None,
        };
        {
            let mut slot = self.benchmark.lock().unwrap_or_else(|p| p.into_inner());
            if slot.as_ref().is_some_and(|b| b.state == "running") {
                return Err(ErrorService::new(StatusCode::CONFLICT, "a measurement is already running"));
            }
            *slot = Some(started.clone());
        }
        let service = self.clone();
        tokio::spawn(async move {
            let audio = Arc::new(audio);
            // Its models take the memory the loaded one has
            service.engine.unload_all();
            let mut failure = None;
            'models: for (index, spec) in models.iter().enumerate() {
                for &threads in &counts {
                    Self::update_benchmark(&service.benchmark, |b| b.note = Some(format!("{} with {threads} threads", spec.label)));
                    let (engine, dir, language, clip) = (service.engine.clone(), service.dir.join(spec.id), language.clone(), audio.clone());
                    let spec = *spec;
                    let timed = tokio::task::spawn_blocking(move || engine.time_read(spec, dir, &language, threads as i32, &clip)).await;
                    let elapsed = match timed {
                        Ok(Ok(elapsed)) => elapsed,
                        Ok(Err(e)) => {
                            failure = Some(e);
                            break 'models;
                        }
                        Err(e) => {
                            failure = Some(format!("the measurement stopped: {e}"));
                            break 'models;
                        }
                    };
                    let seconds = elapsed.as_secs_f32();
                    let speed = if seconds > 0.0 { audio.len() as f32 / SAMPLE_RATE as f32 / seconds } else { 0.0 };
                    Self::update_benchmark(&service.benchmark, |b| {
                        b.done += 1;
                        let model = &mut b.models[index];
                        model.runs.push(SttBenchRun { threads, seconds, speed });
                        model.best_threads = best_of(&model.runs);
                    });
                }
            }
            Self::update_benchmark(&service.benchmark, |b| {
                b.note = None;
                match failure {
                    Some(e) => {
                        tracing::warn!("the speech speed measurement failed: {e}");
                        b.state = "failed".into();
                        b.error = Some(e);
                    }
                    None => b.state = "done".into(),
                }
            });
        });
        Ok(started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_follow_the_cores_and_end_with_them() {
        assert_eq!(thread_counts(12), vec![2, 4, 6, 8, 12]);
        assert_eq!(thread_counts(16), vec![2, 4, 6, 8, 12, 16]);
        assert_eq!(thread_counts(10), vec![2, 4, 6, 8, 10]);
        assert_eq!(thread_counts(4), vec![2, 4]);
        assert_eq!(thread_counts(2), vec![2]);
        assert_eq!(thread_counts(1), vec![1]);
    }

    #[test]
    fn the_best_is_the_fewest_threads_that_come_close_to_the_fastest() {
        let run = |threads, speed| SttBenchRun { threads, seconds: 1.0, speed };
        assert_eq!(best_of(&[run(2, 1.0), run(4, 1.8), run(8, 2.0), run(12, 2.05)]), Some(8));
        assert_eq!(best_of(&[run(2, 1.0)]), Some(2));
        assert_eq!(best_of(&[]), None);
    }
}
