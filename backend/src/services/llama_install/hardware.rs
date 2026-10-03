//! What this machine has, and which llama.cpp build suits it. Linux is read from sysfs and the
//! vendor tools; other systems get what `sysinfo` and `gfxinfo` can say. `llama-server
//! --list-devices` stays the authority on what is really usable.

use std::path::Path;

use serde::Serialize;
use sysinfo::System;
use utoipa::ToSchema;

#[derive(Clone, Serialize, ToSchema)]
pub struct Gpu {
    /// `amd`, `nvidia`, `intel` or `other`
    pub vendor: String,
    pub name: String,
    pub vram_mib: Option<u64>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct MissingLib {
    pub name: String,
    /// What to run to get it, for the detected distribution; package names vary, so it is a hint
    pub hint: String,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct Hardware {
    pub os: String,
    pub arch: String,
    pub cpu: String,
    pub cores: usize,
    pub ram_mib: u64,
    pub gpus: Vec<Gpu>,
    /// The build id to start from: `cpu`, `vulkan`, `rocm`, `cuda12`, `cuda13`
    pub recommended: String,
    pub reason: String,
    /// Every build id that exists for this system
    pub builds: Vec<String>,
    /// `tested` for Linux, `untested` elsewhere
    pub support: String,
    /// Libraries the recommended build needs that aren't installed
    pub missing: Vec<MissingLib>,
}

pub fn detect() -> Hardware {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let gpus = detect_gpus();
    let builds = builds_for(&os, &arch);
    let (recommended, reason) = recommend(&os, &gpus, &builds);
    let missing = missing_libs(&os, &recommended);
    Hardware {
        support: if os == "linux" { "tested" } else { "untested" }.into(),
        cpu: sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default(),
        cores: sys.cpus().len(),
        ram_mib: sys.total_memory() / (1024 * 1024),
        os,
        arch,
        gpus,
        recommended,
        reason,
        builds,
        missing,
    }
}

/// The build ids the release has for a system.
pub fn builds_for(os: &str, arch: &str) -> Vec<String> {
    let ids: &[&str] = match (os, arch) {
        ("linux", "x86_64") => &["cpu", "vulkan", "rocm", "cuda12", "cuda13"],
        ("linux", "aarch64") => &["cpu", "vulkan"],
        ("windows", "x86_64") => &["cpu", "vulkan", "rocm", "cuda12", "cuda13"],
        ("windows", "aarch64") => &["cpu"],
        ("macos", _) => &["cpu"],
        _ => &[],
    };
    ids.iter().map(|s| s.to_string()).collect()
}

fn recommend(os: &str, gpus: &[Gpu], builds: &[String]) -> (String, String) {
    let has = |id: &str| builds.iter().any(|b| b == id);
    if os == "macos" {
        return ("cpu".into(), "the macOS build uses Metal for the GPU".into());
    }
    if let Some(gpu) = gpus.iter().find(|g| g.vendor == "nvidia") {
        if has("cuda13") {
            let new_driver = nvidia_driver_major().is_some_and(|major| major >= 580);
            let id = if new_driver { "cuda13" } else { "cuda12" };
            return (id.into(), format!("{} is an NVIDIA GPU; {id} matches its driver", gpu.name));
        }
    }
    if let Some(gpu) = gpus.iter().find(|g| g.vendor == "amd") {
        // The ROCm release is built on a newer distribution than the others and needs glibc 2.38
        let glibc_ok = glibc_version().is_none_or(|(major, minor)| (major, minor) >= (2, 38));
        if has("rocm") && lib_present("libhipblas.so") && glibc_ok {
            return ("rocm".into(), format!("{} is an AMD GPU and the ROCm libraries are installed", gpu.name));
        }
        if has("vulkan") {
            return ("vulkan".into(), format!("{} is an AMD GPU; Vulkan needs no extra libraries (ROCm is faster once hipblas is installed)", gpu.name));
        }
    }
    if !gpus.is_empty() && has("vulkan") {
        return ("vulkan".into(), "a GPU was found; Vulkan works with most of them".into());
    }
    ("cpu".into(), "no GPU was found, so the model runs on the CPU".into())
}

/// The system's glibc version, from `ldd --version` (`None` where there is none, or it can't be read).
fn glibc_version() -> Option<(u32, u32)> {
    let out = std::process::Command::new("ldd").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    parse_glibc(text.lines().next()?)
}

/// `ldd (Debian GLIBC 2.36-9) 2.36` or `ldd (GNU libc) 2.42` -> (2, 36) / (2, 42)
fn parse_glibc(line: &str) -> Option<(u32, u32)> {
    let version = line.split_whitespace().last()?;
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn detect_gpus() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    if cfg!(target_os = "linux") {
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else { return gpus };
        let mut cards: Vec<_> = entries
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit())
            })
            .collect();
        cards.sort_by_key(|e| e.file_name());
        for card in cards {
            let dev = card.path().join("device");
            let vendor_id = read_trim(&dev.join("vendor")).unwrap_or_default();
            let vendor = match vendor_id.as_str() {
                "0x1002" => "amd",
                "0x10de" => "nvidia",
                "0x8086" => "intel",
                _ => "other",
            };
            let vram = read_trim(&dev.join("mem_info_vram_total")).and_then(|b| b.parse::<u64>().ok()).map(|b| b / (1024 * 1024));
            let device_id = read_trim(&dev.join("device")).unwrap_or_default();
            gpus.push(Gpu { vendor: vendor.into(), name: format!("{} GPU ({device_id})", vendor.to_uppercase()), vram_mib: vram });
        }
        // Real names where a library can give them
        if let Ok(active) = gfxinfo::active_gpu() {
            let model = active.model().to_string();
            let vram = active.info().total_vram() / (1024 * 1024);
            if let Some(first) = gpus.iter_mut().find(|g| g.vendor == "amd" || g.vendor == "nvidia") {
                first.name = model;
                first.vram_mib = first.vram_mib.or(Some(vram));
            }
        }
        if gpus.iter().any(|g| g.vendor == "nvidia") {
            if let Some((name, mib)) = nvidia_smi() {
                if let Some(g) = gpus.iter_mut().find(|g| g.vendor == "nvidia") {
                    g.name = name;
                    g.vram_mib = Some(mib);
                }
            }
        }
    } else if let Ok(active) = gfxinfo::active_gpu() {
        let model = active.model().to_string();
        let lower = active.vendor().to_lowercase();
        let vendor = if lower.contains("nvidia") { "nvidia" } else if lower.contains("amd") { "amd" } else if lower.contains("intel") { "intel" } else { "other" };
        gpus.push(Gpu { vendor: vendor.into(), name: model, vram_mib: Some(active.info().total_vram() / (1024 * 1024)) });
    }
    gpus
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn nvidia_smi() -> Option<(String, u64)> {
    let out = std::process::Command::new("nvidia-smi").args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let line = text.lines().next()?;
    let (name, mib) = line.rsplit_once(',')?;
    Some((name.trim().to_string(), mib.trim().parse().ok()?))
}

fn nvidia_driver_major() -> Option<u32> {
    let out = std::process::Command::new("nvidia-smi").args(["--query-gpu=driver_version", "--format=csv,noheader"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().split('.').next()?.parse().ok()
}

/// Whether a shared library whose file name starts with `prefix` sits in a usual library folder.
fn lib_present(prefix: &str) -> bool {
    let mut dirs: Vec<String> = ["/usr/lib", "/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/usr/local/lib", "/lib64"].iter().map(|s| s.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir("/opt") {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("rocm") {
                dirs.push(format!("{}/lib", e.path().display()));
            }
        }
    }
    dirs.iter().any(|d| {
        std::fs::read_dir(d).is_ok_and(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with(prefix)))
    })
}

fn distro_id() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("ID=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_default()
}

/// The install command for the package holding `lib`, on this distribution.
pub fn install_hint(lib: &str) -> String {
    let id = distro_id();
    let group = if lib.contains("hip") || lib.contains("rocblas") || lib.contains("amdhip") { "rocm" } else if lib.contains("vulkan") { "vulkan" } else if lib.contains("cuda") { "cuda" } else { "" };
    match (id.as_str(), group) {
        ("arch" | "manjaro" | "endeavouros" | "cachyos", "rocm") => "sudo pacman -S hipblas rocblas".into(),
        ("arch" | "manjaro" | "endeavouros" | "cachyos", "vulkan") => "sudo pacman -S vulkan-icd-loader vulkan-radeon (or vulkan-intel / nvidia-utils)".into(),
        ("debian" | "ubuntu" | "linuxmint" | "pop", "vulkan") => "sudo apt install libvulkan1 mesa-vulkan-drivers".into(),
        ("fedora", "vulkan") => "sudo dnf install vulkan-loader mesa-vulkan-drivers".into(),
        (_, "rocm") => "install ROCm's hipblas and rocblas packages (see AMD's ROCm install guide for your distribution)".into(),
        (_, "cuda") => "install the NVIDIA driver (it provides libcuda.so.1)".into(),
        _ => format!("install the package that provides {lib}"),
    }
}

fn missing_libs(os: &str, build: &str) -> Vec<MissingLib> {
    if os != "linux" {
        return Vec::new();
    }
    let needed: &[&str] = match build {
        "rocm" => &["libhipblas.so", "libamdhip64.so", "librocblas.so"],
        "vulkan" => &["libvulkan.so"],
        "cuda12" | "cuda13" => &["libcuda.so"],
        _ => &[],
    };
    needed.iter().filter(|lib| !lib_present(lib)).map(|lib| MissingLib { name: lib.to_string(), hint: install_hint(lib) }).collect()
}

/// The library named in a dynamic loader failure ("error while loading shared libraries: X: ...").
pub fn missing_from_output(output: &str) -> Option<MissingLib> {
    let rest = output.split("error while loading shared libraries: ").nth(1)?;
    let name = rest.split(':').next()?.trim().to_string();
    Some(MissingLib { hint: install_hint(&name), name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_failures_name_the_library() {
        let m = missing_from_output("llama-server: error while loading shared libraries: libhipblas.so.3: cannot open shared object file").unwrap();
        assert_eq!(m.name, "libhipblas.so.3");
        assert!(missing_from_output("Available devices:").is_none());
    }

    #[test]
    fn glibc_versions_are_read_from_ldd() {
        assert_eq!(parse_glibc("ldd (Debian GLIBC 2.36-9+deb12u10) 2.36"), Some((2, 36)));
        assert_eq!(parse_glibc("ldd (GNU libc) 2.42"), Some((2, 42)));
        assert_eq!(parse_glibc("nonsense"), None);
    }

    #[test]
    fn builds_exist_per_system() {
        assert!(builds_for("linux", "x86_64").contains(&"rocm".to_string()));
        assert_eq!(builds_for("macos", "aarch64"), vec!["cpu"]);
    }
}
