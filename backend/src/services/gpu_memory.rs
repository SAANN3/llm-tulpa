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
    /// System memory the GPU maps (GTT): host-visible buffers a driver keeps there whatever the card has
    /// free, and also what a full card spills into. The size alone doesn't say which.
    pub system_mib: u64,
    /// The memory of the card the process mainly uses, and how much of it is still free: what tells a
    /// spill from ordinary host-visible buffers. Known for amdgpu only.
    pub card_total_mib: Option<u64>,
    pub card_free_mib: Option<u64>,
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
fn parse_fdinfo(text: &str) -> Option<(String, String, f64, f64)> {
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
    drm.then(|| (format!("{pdev}/{}", client.unwrap_or_default()), pdev, vram, gtt))
}

/// Adds up the DRM clients' memory; each client counts once. Also names the card (its PCI address) the
/// process holds the most on.
fn total(files: impl IntoIterator<Item = String>) -> Option<(GpuMemory, String)> {
    let mut seen = HashSet::new();
    let (mut vram, mut gtt, mut any) = (0.0, 0.0, false);
    let mut by_card: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for text in files {
        if let Some((client, pdev, v, g)) = parse_fdinfo(&text) {
            any = true;
            if seen.insert(client) {
                vram += v;
                gtt += g;
                *by_card.entry(pdev).or_default() += v;
            }
        }
    }
    let main_card = by_card.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(pdev, _)| pdev).unwrap_or_default();
    any.then(|| {
        (GpuMemory { vram_mib: vram.round() as u64, system_mib: gtt.round() as u64, ..GpuMemory::default() }, main_card)
    })
}

/// The memory of a card and how much of it is free, in MiB, from what amdgpu publishes for its PCI device.
#[cfg(target_os = "linux")]
fn card_memory(pdev: &str) -> Option<(u64, u64)> {
    // The address comes from the kernel, but it ends up in a path, so only what an address is made of passes
    if pdev.is_empty() || !pdev.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
        return None;
    }
    let read = |name: &str| -> Option<u64> {
        std::fs::read_to_string(format!("/sys/bus/pci/devices/{pdev}/{name}")).ok()?.trim().parse::<u64>().ok()
    };
    let (total, used) = (read("mem_info_vram_total")?, read("mem_info_vram_used")?);
    Some((total / (1024 * 1024), total.saturating_sub(used) / (1024 * 1024)))
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
    let (mut memory, card) = total(texts.collect::<Vec<_>>())?;
    if let Some((total, free)) = card_memory(&card) {
        memory.card_total_mib = Some(total);
        memory.card_free_mib = Some(free);
    }
    Some(memory)
}

#[cfg(not(target_os = "linux"))]
pub fn of_process(_pid: u32) -> Option<GpuMemory> {
    None
}

/// The memory of the biggest card the system describes (total and free, MiB), for when no process is
/// there to ask about: a model that just failed to load. amdgpu only, like `card_memory`.
#[cfg(target_os = "linux")]
pub fn biggest_card() -> Option<(u64, u64)> {
    std::fs::read_dir("/sys/bus/pci/devices")
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| card_memory(&entry.file_name().to_string_lossy()))
        .max_by_key(|(total, _)| *total)
}

#[cfg(not(target_os = "linux"))]
pub fn biggest_card() -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "pos:\t0\ndrm-driver:\tamdgpu\ndrm-client-id:\t23\ndrm-pdev:\t0000:2b:00.0\ndrm-total-gtt:\t26668 KiB\ndrm-resident-gtt:\t240004 KiB\ndrm-total-vram:\t583444 KiB\ndrm-resident-vram:\t583444 KiB\n";

    #[test]
    fn reads_the_resident_memory_of_a_client() {
        let (_, pdev, vram, gtt) = parse_fdinfo(FIREFOX).unwrap();
        assert_eq!(pdev, "0000:2b:00.0");
        assert_eq!(vram.round() as u64, 570);
        assert_eq!(gtt.round() as u64, 234);
    }

    #[test]
    fn a_client_behind_two_descriptors_counts_once() {
        let other = FIREFOX.replace("23", "24").replace("583444", "1048576");
        let both = total(vec![FIREFOX.to_string(), FIREFOX.to_string(), other]).unwrap();
        assert_eq!(both.0, GpuMemory { vram_mib: 570 + 1024, system_mib: 469, ..GpuMemory::default() });
        assert_eq!(both.1, "0000:2b:00.0");
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
