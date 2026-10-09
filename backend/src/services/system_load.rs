//! What this machine is using right now, for the Hardware view: CPU, memory, the model server process's own
//! memory, and the GPU's memory, load, temperature and clock. CPU, memory and processes come from `sysinfo`
//! (Linux and Windows alike); the GPU's live figures come from the AMD driver's sysfs files on Linux, and
//! are absent elsewhere. Detection of what the machine *has* (for choosing a build) is `llama_install::hardware`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct SystemSnapshot {
    /// All cores together, 0 to 100, since the previous snapshot (0 on the first)
    pub cpu_percent: f32,
    pub cpu_threads: usize,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    /// The model server process's resident memory, when one runs
    pub server_memory_bytes: Option<u64>,
    /// The first AMD GPU, when the driver says
    pub gpu: Option<GpuLoad>,
}

#[derive(Serialize, ToSchema)]
pub struct GpuLoad {
    pub vram_used_bytes: u64,
    pub vram_total_bytes: u64,
    /// How busy the GPU is, 0 to 100
    pub busy_percent: Option<u32>,
    pub temperature_c: Option<f32>,
    pub clock_mhz: Option<u32>,
}

/// Keeps one `sysinfo` reader, because CPU use is the change between two reads.
pub struct SystemLoad {
    system: Mutex<System>,
}

impl SystemLoad {
    pub fn new() -> Self {
        Self { system: Mutex::new(System::new()) }
    }

    /// A snapshot; `server_pid` is the model server's process, when one runs. Blocking (reads `/proc` and
    /// sysfs): call it off the async runtime.
    pub fn snapshot(&self, server_pid: Option<u32>) -> SystemSnapshot {
        let mut system = self.system.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        system.refresh_cpu_usage();
        system.refresh_memory();
        let server_memory_bytes = server_pid.and_then(|pid| {
            let pid = Pid::from_u32(pid);
            system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
            system.process(pid).map(|process| process.memory())
        });
        SystemSnapshot {
            cpu_percent: system.global_cpu_usage(),
            cpu_threads: system.cpus().len(),
            memory_used_bytes: system.used_memory(),
            memory_total_bytes: system.total_memory(),
            server_memory_bytes,
            gpu: amd_gpu(),
        }
    }
}

impl Default for SystemLoad {
    fn default() -> Self {
        Self::new()
    }
}

/// The first card the AMD driver reports VRAM for (`/sys/class/drm/cardN/device`), Linux only.
fn amd_gpu() -> Option<GpuLoad> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())
        })
        .map(|e| e.path().join("device"))
        .collect();
    cards.sort();
    let device = cards.into_iter().find(|d| d.join("mem_info_vram_total").is_file())?;
    Some(GpuLoad {
        vram_used_bytes: read_number(&device.join("mem_info_vram_used"))?,
        vram_total_bytes: read_number(&device.join("mem_info_vram_total"))?,
        busy_percent: read_number(&device.join("gpu_busy_percent")).map(|n| n as u32),
        temperature_c: hwmon_temperature(&device),
        clock_mhz: std::fs::read_to_string(device.join("pp_dpm_sclk")).ok().and_then(|text| current_clock(&text)),
    })
}

fn read_number(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The edge temperature the driver's hwmon reports, in millidegrees
fn hwmon_temperature(device: &Path) -> Option<f32> {
    let hwmon = std::fs::read_dir(device.join("hwmon")).ok()?.flatten().next()?.path();
    read_number(&hwmon.join("temp1_input")).map(|milli| milli as f32 / 1000.0)
}

/// The clock level marked current in `pp_dpm_sclk` (`1: 2475Mhz *`)
fn current_clock(text: &str) -> Option<u32> {
    let line = text.lines().find(|line| line.trim_end().ends_with('*'))?;
    let mhz = line.split_whitespace().find(|part| part.to_ascii_lowercase().ends_with("mhz"))?;
    mhz[..mhz.len() - 3].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_clock_level_is_the_starred_one() {
        assert_eq!(current_clock("0: 500Mhz *\n1: 2475Mhz \n"), Some(500));
        assert_eq!(current_clock("0: 500Mhz \n1: 2475Mhz *\n"), Some(2475));
        assert_eq!(current_clock("0: 500Mhz \n"), None);
    }
}
