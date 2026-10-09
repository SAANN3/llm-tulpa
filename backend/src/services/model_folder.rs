//! The folder the model files live in, as one shared handle: the owner can change it while the
//! backend runs (from the setup wizard or the Models page), and everything that reads model files —
//! the model library, Hugging Face downloads, the llama.cpp provider and the launch builder — sees
//! the change at once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use super::error::ErrorService;

pub struct ModelFolder {
    path: RwLock<Option<PathBuf>>,
}

impl ModelFolder {
    pub fn new(path: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self { path: RwLock::new(path) })
    }

    pub fn get(&self) -> Option<PathBuf> {
        self.path.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set(&self, path: PathBuf) {
        *self.path.write().unwrap_or_else(|p| p.into_inner()) = Some(path);
    }
}

/// A folder that can be used as the model folder, as far as can be told without using it.
pub struct CheckedFolder {
    pub path: PathBuf,
    /// Whether files can be created in it (downloads need that; running the models in it doesn't)
    pub writable: bool,
}

fn bad(why: impl Into<String>) -> ErrorService {
    ErrorService::new(StatusCode::BAD_REQUEST, why)
}

/// Checks a folder the client named: absolute, existing, a directory, readable. The canonical path
/// is what gets stored, so a symlink or `..` can't make it mean something else later.
pub fn check(path: &str) -> Result<CheckedFolder, ErrorService> {
    let path = Path::new(path.trim());
    if !path.is_absolute() {
        return Err(bad("the folder must be an absolute path"));
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| bad("that folder doesn't exist"))?;
    if !canonical.is_dir() {
        return Err(bad("that is not a folder"));
    }
    std::fs::read_dir(&canonical).map_err(|e| bad(format!("that folder can't be read: {e}")))?;

    let probe = canonical.join(format!(".llm-tulpa-write-test-{}", std::process::id()));
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    Ok(CheckedFolder { path: canonical, writable })
}

/// The mount whose path is the longest prefix of `path`: the disk the path lives on.
fn mount_of<'a, T>(path: &Path, mounts: &'a [(PathBuf, T)]) -> Option<&'a (PathBuf, T)> {
    mounts.iter().filter(|(mount, _)| path.starts_with(mount)).max_by_key(|(mount, _)| mount.as_os_str().len())
}

/// How big the disk a path is on is, and how much of it can still be written, in bytes
#[derive(Clone, Copy)]
pub struct DiskSpace {
    pub free: u64,
    pub total: u64,
}

/// How many bytes can still be written on the disk `path` is on, when the system says (see `disk_space`).
pub fn free_bytes(path: &Path) -> Option<u64> {
    disk_space(path).map(|space| space.free)
}

/// The disk `path` is on, when the system says. A path that does not exist yet counts as the nearest folder
/// above it that does.
pub fn disk_space(path: &Path) -> Option<DiskSpace> {
    let mut existing = path;
    while !existing.exists() {
        existing = existing.parent()?;
    }
    let canonical = std::fs::canonicalize(existing).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mounts: Vec<(PathBuf, DiskSpace)> = disks
        .list()
        .iter()
        .map(|d| (d.mount_point().to_path_buf(), DiskSpace { free: d.available_space(), total: d.total_space() }))
        .collect();
    let (mount, space) = mount_of(&canonical, &mounts)?;
    // `Disks` leaves some filesystems out (tmpfs, say), so the deepest mount it lists may be a different
    // disk than the one the path is on: then nothing is said, rather than the wrong disk's space
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if std::fs::metadata(&canonical).ok()?.dev() != std::fs::metadata(mount).ok()?.dev() {
            return None;
        }
    }
    Some(*space)
}

#[derive(Serialize, ToSchema)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct Browsed {
    /// The folder listed (canonical)
    pub path: String,
    /// Its parent, `None` at the top
    pub parent: Option<String>,
    /// The folders inside it, hidden ones left out, by name
    pub folders: Vec<FolderEntry>,
}

/// The sub-folders of `path`, for a folder picker. At most 500, hidden ones skipped.
pub fn browse(path: &str) -> Result<Browsed, ErrorService> {
    let canonical = std::fs::canonicalize(path.trim()).map_err(|_| bad("that folder doesn't exist"))?;
    let entries = std::fs::read_dir(&canonical).map_err(|e| bad(format!("that folder can't be read: {e}")))?;
    let mut folders: Vec<FolderEntry> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) || e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.')).then(|| FolderEntry { path: e.path().display().to_string(), name })
        })
        .collect();
    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    folders.truncate(500);
    Ok(Browsed {
        parent: canonical.parent().map(|p| p.display().to_string()),
        path: canonical.display().to_string(),
        folders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_has_to_be_an_existing_absolute_directory() {
        let base = std::env::temp_dir().join(format!("llm-tulpa-folder-test-{}", std::process::id()));
        std::fs::create_dir_all(base.join("a")).unwrap();
        std::fs::create_dir_all(base.join(".hidden")).unwrap();
        std::fs::write(base.join("file.txt"), "x").unwrap();

        assert!(check("relative/path").is_err());
        assert!(check(&base.join("missing").display().to_string()).is_err());
        assert!(check(&base.join("file.txt").display().to_string()).is_err());
        let ok = check(&base.display().to_string()).ok().unwrap();
        assert!(ok.writable);

        let listed = browse(&base.display().to_string()).ok().unwrap();
        assert_eq!(listed.folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["a"]);
        assert!(listed.parent.is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_handle_follows_a_change() {
        let folder = ModelFolder::new(None);
        assert!(folder.get().is_none());
        folder.set(PathBuf::from("/x"));
        assert_eq!(folder.get(), Some(PathBuf::from("/x")));
    }

    #[test]
    fn a_path_belongs_to_the_deepest_mount_that_holds_it() {
        let mounts = vec![(PathBuf::from("/"), 10), (PathBuf::from("/games"), 20), (PathBuf::from("/games/llm"), 30), (PathBuf::from("/gamesx"), 99)];
        assert_eq!(mount_of(Path::new("/games/llm/Qwen/x.gguf"), &mounts).map(|m| m.1), Some(30));
        assert_eq!(mount_of(Path::new("/games/other"), &mounts).map(|m| m.1), Some(20));
        assert_eq!(mount_of(Path::new("/home/me"), &mounts).map(|m| m.1), Some(10));
    }
}
