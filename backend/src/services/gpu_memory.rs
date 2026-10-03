//! How much GPU memory a process holds, read from the kernel's per-process DRM accounting
//! (`/proc/<pid>/fdinfo`, the `drm-resident-*` keys the amdgpu, i915 and xe drivers publish). The model
//! server's own log doesn't say where the model went unless it runs with debug verbosity, which
//! writes about eighteen lines per generated token, so the kernel is asked instead.

use std::collections::HashSet;

use serde::Serialize;
use utoipa::ToSchema;

/// What one process holds on the GPUs
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, ToSchema)]
pub struct GpuMemory {
    /// Memory on the card itself
    pub vram_mib: u64,
    /// System memory the GPU uses: what a full card spills into (GTT)
    pub system_mib: u64,
}

/// `"12 KiB"`, `"2 MiB"`, `"1 GiB"` or a bare byte count, as MiB
fn parse_size(text: &str) -> Option<f64> {
    let mut parts = text.split_whitespace();
    let number: f64 = parts.next()?.parse().ok()?;
    let mib = match parts.next() {
        Some("KiB") => number / 1024.0,
        Some("MiB") => number,
        Some("GiB") => number * 1024.0,
        None => number / (1024.0 * 1024.0),
        Some(_) => return None,
    };
    Some(mib)
}

/// One `fdinfo` file: the DRM client it belongs to and what it holds, or `None` for a file that isn't a
/// DRM client's.
fn parse_fdinfo(text: &str) -> Option<(String, f64, f64)> {
    let (mut client, mut vram, mut gtt) = (None, 0.0, 0.0);
    let mut drm = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key {
            "drm-driver" => drm = true,
            "drm-client-id" => client = Some(value.to_string()),
            "drm-resident-vram" => vram = parse_size(value).unwrap_or(0.0),
            "drm-resident-gtt" => gtt = parse_size(value).unwrap_or(0.0),
            _ => {}
        }
    }
    // The same client shows up under every duplicated descriptor; the PCI address tells GPUs apart
    let pdev = text.lines().find_map(|l| l.strip_prefix("drm-pdev:")).map(|v| v.trim().to_string()).unwrap_or_default();
    drm.then(|| (format!("{pdev}/{}", client.unwrap_or_default()), vram, gtt))
}

/// Adds up the DRM clients' memory; each client counts once.
fn total(files: impl IntoIterator<Item = String>) -> Option<GpuMemory> {
    let mut seen = HashSet::new();
    let (mut vram, mut gtt, mut any) = (0.0, 0.0, false);
    for text in files {
        if let Some((client, v, g)) = parse_fdinfo(&text) {
            any = true;
            if seen.insert(client) {
                vram += v;
                gtt += g;
            }
        }
    }
    any.then(|| GpuMemory { vram_mib: vram.round() as u64, system_mib: gtt.round() as u64 })
}

/// What `pid` holds on the GPUs; `None` where the system doesn't say (not Linux, a driver without
/// this accounting such as NVIDIA's, or a process that has no GPU open at all).
#[cfg(target_os = "linux")]
pub fn of_process(pid: u32) -> Option<GpuMemory> {
    let fds = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    let texts = fds.filter_map(Result::ok).filter_map(|entry| {
        let target = std::fs::read_link(entry.path()).ok()?;
        if !target.starts_with("/dev/dri") {
            return None;
        }
        std::fs::read_to_string(format!("/proc/{pid}/fdinfo/{}", entry.file_name().to_string_lossy())).ok()
    });
    total(texts.collect::<Vec<_>>())
}

#[cfg(not(target_os = "linux"))]
pub fn of_process(_pid: u32) -> Option<GpuMemory> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "pos:\t0\ndrm-driver:\tamdgpu\ndrm-client-id:\t23\ndrm-pdev:\t0000:2b:00.0\ndrm-total-gtt:\t26668 KiB\ndrm-resident-gtt:\t240004 KiB\ndrm-total-vram:\t583444 KiB\ndrm-resident-vram:\t583444 KiB\n";

    #[test]
    fn reads_the_resident_memory_of_a_client() {
        let (_, vram, gtt) = parse_fdinfo(FIREFOX).unwrap();
        assert_eq!(vram.round() as u64, 570);
        assert_eq!(gtt.round() as u64, 234);
    }

    #[test]
    fn a_client_behind_two_descriptors_counts_once() {
        let other = FIREFOX.replace("23", "24").replace("583444", "1048576");
        let both = total(vec![FIREFOX.to_string(), FIREFOX.to_string(), other]).unwrap();
        assert_eq!(both, GpuMemory { vram_mib: 570 + 1024, system_mib: 469 });
    }

    #[test]
    fn a_file_that_is_not_a_drm_client_is_skipped() {
        assert_eq!(total(vec!["pos:\t0\nflags:\t02\n".to_string()]), None);
    }

    #[test]
    fn units_are_understood() {
        assert_eq!(parse_size("2 GiB"), Some(2048.0));
        assert_eq!(parse_size("1048576"), Some(1.0));
        assert_eq!(parse_size("nonsense"), None);
    }
}
