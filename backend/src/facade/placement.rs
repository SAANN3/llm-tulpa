//! Whether the loaded model is really on the GPU. The server's own log doesn't say at its normal
//! verbosity, so the answer is put together from what the profile asked for and what the kernel says
//! the process holds on the GPUs (`services/gpu_memory.rs`).

use serde::Serialize;
use utoipa::ToSchema;

use crate::services::{
    gguf::read_gguf_info,
    gpu_memory::{self, GpuMemory},
    llama_runtime::LlamaRuntime,
};

const MIB: u64 = 1024 * 1024;

/// A model's weights below this don't tell a GPU from a CPU by memory use (a test model, say)
const MEASURABLE_MODEL_MIB: u64 = 256;

/// System memory the GPU maps below this is ordinary: drivers keep host-visible buffers there
const ORDINARY_SYSTEM_MIB: u64 = 256;

/// A card with less than this free (or under 5% of its memory) is full, and what no longer fits on it
/// goes to system memory
const FULL_CARD_FREE_MIB: u64 = 512;

#[derive(Clone, Serialize, ToSchema)]
pub struct Placement {
    /// `gpu`, `partial` (some of the model runs on the CPU or in system memory), `cpu`, or `unknown`
    /// (all layers asked for the GPU, but this system can't say what the process holds there)
    pub verdict: String,
    /// The answer in a sentence
    pub summary: String,
    /// The layers the launch profile puts on the GPU, at most the model's own count
    pub layers_requested: Option<u32>,
    pub layers_total: Option<u32>,
    pub model_mib: u64,
    /// What the server process holds on the GPU itself and in system memory the GPU uses, when the system reports it
    pub memory: Option<GpuMemory>,
}

/// Judges the loaded launch, or `None` when nothing is loaded.
pub async fn describe(runtime: &LlamaRuntime) -> Option<Placement> {
    let (request, pid) = runtime.loaded_launch()?;
    let file = request.model_file.clone();
    let gpu_layers = request.profile.gpu_layers.max(0) as u32;
    let (info, size) = tokio::task::spawn_blocking(move || {
        (read_gguf_info(&file).ok(), std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0))
    })
    .await
    .ok()?;
    let layers_total = info.and_then(|i| i.block_count).map(|n| n as u32);
    let model_mib = size / MIB;
    let memory = tokio::task::spawn_blocking(move || gpu_memory::of_process(pid)).await.ok().flatten();
    Some(judge(gpu_layers, layers_total, model_mib, memory))
}

fn judge(gpu_layers: u32, layers_total: Option<u32>, model_mib: u64, memory: Option<GpuMemory>) -> Placement {
    // 99 is the profile's "everything"; a model with more layers than the number asked for is partial
    let all_asked = layers_total.map_or(gpu_layers >= 99, |total| gpu_layers >= total);
    let layers_requested = layers_total.map_or(Some(gpu_layers), |total| Some(gpu_layers.min(total)));
    let make = |verdict: &str, summary: String| Placement {
        verdict: verdict.into(),
        summary,
        layers_requested,
        layers_total,
        model_mib,
        memory,
    };

    if gpu_layers == 0 {
        return make("cpu", "Runs on the CPU: the launch profile puts no layers on the GPU.".into());
    }
    if !all_asked {
        let total = layers_total.map(|t| t.to_string()).unwrap_or_else(|| "all".into());
        return make(
            "partial",
            format!("Spilled to the CPU: the launch profile puts {gpu_layers} of {total} layers on the GPU, the rest run on the CPU."),
        );
    }
    let Some(held) = memory else {
        return make(
            "unknown",
            "Every layer is asked for the GPU, but this system doesn't report what the server holds there, so it can't be confirmed.".into(),
        );
    };
    if model_mib >= MEASURABLE_MODEL_MIB && held.vram_mib + held.system_mib < model_mib / 10 {
        return make(
            "cpu",
            format!(
                "Runs on the CPU: the server holds only {} MiB on the GPU for a {model_mib} MiB model (a build without GPU support, or no usable GPU).",
                held.vram_mib + held.system_mib
            ),
        );
    }
    // System memory used by the GPU is only a spill when the card has no room left: with room, it is
    // host-visible buffers the driver keeps there anyway (Vulkan does, for a few hundred MiB)
    let card_is_full = match (held.card_free_mib, held.card_total_mib) {
        (Some(free), Some(total)) => Some(free < (total / 20).max(FULL_CARD_FREE_MIB)),
        _ => None,
    };
    if model_mib >= MEASURABLE_MODEL_MIB && held.system_mib > ORDINARY_SYSTEM_MIB && card_is_full == Some(true) {
        return make(
            "partial",
            format!(
                "Spilled into system memory: {} MiB of what the GPU uses is in RAM, because the card is full ({} MiB free of {}). It works but is slower; a smaller context or KV cache keeps it on the card.",
                held.system_mib,
                held.card_free_mib.unwrap_or_default(),
                held.card_total_mib.unwrap_or_default()
            ),
        );
    }
    let mut summary = format!("Fully on the GPU: {} MiB of GPU memory in use", held.vram_mib);
    if held.system_mib > ORDINARY_SYSTEM_MIB {
        summary.push_str(&match held.card_free_mib {
            Some(free) => format!(", plus {} MiB of system memory the GPU maps (host-visible buffers; the card still has {free} MiB free)", held.system_mib),
            None => format!(", plus {} MiB of system memory the GPU maps (this system can't say whether the card is full)", held.system_mib),
        });
    }
    summary.push('.');
    make("gpu", summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(vram: u64, system: u64) -> Option<GpuMemory> {
        card(vram, system, Some(2_000))
    }

    fn card(vram: u64, system: u64, free: Option<u64>) -> Option<GpuMemory> {
        Some(GpuMemory { vram_mib: vram, system_mib: system, card_total_mib: free.map(|_| 16_368), card_free_mib: free })
    }

    #[test]
    fn the_profile_alone_can_say_the_model_is_split() {
        assert_eq!(judge(20, Some(40), 12_000, None).verdict, "partial");
        assert_eq!(judge(0, Some(40), 12_000, None).verdict, "cpu");
    }

    #[test]
    fn all_layers_on_a_gpu_that_cannot_be_measured_is_unconfirmed() {
        assert_eq!(judge(99, Some(40), 12_000, None).verdict, "unknown");
    }

    #[test]
    fn memory_tells_a_full_card_from_a_spilled_one_and_a_cpu_run() {
        assert_eq!(judge(99, Some(40), 12_000, mem(14_000, 80)).verdict, "gpu");
        assert_eq!(judge(99, Some(40), 12_000, card(11_000, 3_000, Some(100))).verdict, "partial");
        assert_eq!(judge(99, Some(40), 12_000, mem(40, 0)).verdict, "cpu");
    }

    #[test]
    fn system_memory_on_a_card_with_room_is_not_a_spill() {
        // A 1.3 GB model on a 16 GB card with 13 GB free: Vulkan keeps ~600 MiB of host-visible buffers in system memory
        let placement = judge(99, Some(36), 1_313, card(1_641, 612, Some(13_322)));
        assert_eq!(placement.verdict, "gpu");
        assert!(placement.summary.contains("host-visible"), "{}", placement.summary);
        // Without a way to read the card's free memory nothing is claimed either
        assert_eq!(judge(99, Some(40), 12_000, card(11_000, 3_000, None)).verdict, "gpu");
    }

    #[test]
    fn a_tiny_model_is_not_judged_by_its_memory() {
        assert_eq!(judge(99, Some(5), 1, mem(0, 0)).verdict, "gpu");
    }
}
