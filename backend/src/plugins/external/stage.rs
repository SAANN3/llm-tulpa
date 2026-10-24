//! A plugin on its way in: a `.tulpa` file or a git repository unpacked into a staging folder, checked, and described to the
//! owner so they can see what they would be installing before anything of it runs. Nothing here starts a program, except `git`
//! to fetch a repository.
//!
//! A staging folder is `<external_plugins_dir>/.staging/<id>/` with the plugin's files in `src/` and where they came from in
//! `source.json`. Everything that arrives is held to limits and refused when it tries to leave its folder or to carry a link.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use utoipa::ToSchema;

use super::manifest::Manifest;

/// A staging folder nobody confirmed is removed after this long
const STAGING_LIFETIME: Duration = Duration::from_secs(60 * 60);
const MAX_FILES: usize = 10_000;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The file list sent to the owner stops here; the count and size above cover all of it
const MAX_LISTED_FILES: usize = 500;
const MAX_README_BYTES: usize = 20 * 1024;
const MAX_VIEWED_FILE_BYTES: u64 = 256 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SOURCE_FILE: &str = "source.json";
const TREE: &str = "src";

/// Where a staged plugin came from
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct StagedSource {
    /// `file` or `git`
    pub source_type: String,
    pub url: Option<String>,
    /// For git: the branch or tag asked for
    pub git_ref: Option<String>,
    /// For git: the commit that was fetched
    pub commit: Option<String>,
    /// For a file: its sha256
    pub sha256: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StagedFile {
    pub path: String,
    pub size: u64,
}

pub fn staging_root(root: &Path) -> PathBuf {
    root.join(".staging")
}

/// The folder of a staged plugin, for an `id` made by `new_staging`: sixteen hex digits, so it cannot name anything else
pub fn staging_dir(root: &Path, id: &str) -> Option<PathBuf> {
    (id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())).then(|| staging_root(root).join(id))
}

pub fn tree_dir(staging: &Path) -> PathBuf {
    staging.join(TREE)
}

/// A new, empty staging folder
pub fn new_staging(root: &Path) -> Result<(String, PathBuf), String> {
    let id = format!("{:016x}", rand::random::<u64>());
    let dir = staging_dir(root, &id).ok_or("no id")?;
    std::fs::create_dir_all(tree_dir(&dir)).map_err(|e| format!("cannot make the staging folder {}: {e}", dir.display()))?;
    Ok((id, dir))
}

/// Removes the staging folders that nobody confirmed in time (and any left by a backend that stopped half-way)
pub fn prune(root: &Path) {
    let Ok(entries) = std::fs::read_dir(staging_root(root)) else { return };
    for entry in entries.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age > STAGING_LIFETIME);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn save_source(staging: &Path, source: &StagedSource) -> Result<(), String> {
    let text = serde_json::to_string(source).map_err(|e| e.to_string())?;
    std::fs::write(staging.join(SOURCE_FILE), text).map_err(|e| format!("cannot write the staging record: {e}"))
}

pub fn load_source(staging: &Path) -> StagedSource {
    std::fs::read_to_string(staging.join(SOURCE_FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Unpacks a `.tulpa` file (a zip with `plugin.toml` at its top, or inside the one folder that holds everything) into `dest`.
/// Blocking. A path that leaves the folder, a link, too many files or too many bytes refuses the whole file; the bytes are
/// counted as they are read, so a zip that lies about its sizes cannot fill the disk.
pub fn unpack_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("this is not a zip file that can be read: {e}"))?;
    if archive.len() > MAX_FILES {
        return Err(format!("the file holds more than {MAX_FILES} files"));
    }
    let mut total = 0u64;
    for at in 0..archive.len() {
        let mut entry = archive.by_index(at).map_err(|e| format!("the file could not be read: {e}"))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("the file holds a path that leaves its folder ('{}'), so it was not opened", entry.name()));
        };
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(format!("the file holds a link ('{}'); links are not taken", entry.name()));
        }
        let target = dest.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("cannot unpack '{}': {e}", name.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot unpack '{}': {e}", name.display()))?;
        }
        let mut file = std::fs::File::create(&target).map_err(|e| format!("cannot unpack '{}': {e}", name.display()))?;
        // At most one byte over the limit is read: a declared size can lie, what is actually read cannot
        let mut limited = entry.by_ref().take(MAX_FILE_BYTES + 1);
        let written = std::io::copy(&mut limited, &mut file).map_err(|e| format!("'{}' could not be unpacked: {e}", name.display()))?;
        file.flush().map_err(|e| e.to_string())?;
        if written > MAX_FILE_BYTES {
            return Err(format!("'{}' is larger than {} MB", name.display(), MAX_FILE_BYTES / (1024 * 1024)));
        }
        total += written;
        if total > MAX_TOTAL_BYTES {
            return Err(format!("more than {} MB in all", MAX_TOTAL_BYTES / (1024 * 1024)));
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            // The owner's bits only: a file is the backend user's, whatever the zip's maker had in mind for others
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode & 0o700 | 0o600));
        }
    }
    hoist(dest)
}

/// Copies the plugin folder `source` into `dest` (the staging tree) as an install would take it: what a build or a run leaves
/// behind (`target/`, virtual environments, caches, `.git`) is left out, a link is refused, and the limits above hold. Blocking.
pub fn copy_folder(source: &Path, dest: &Path) -> Result<(), String> {
    let mut count = 0usize;
    let mut total = 0u64;
    let mut pending = vec![(source.to_path_buf(), dest.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        std::fs::create_dir_all(&to).map_err(|e| format!("cannot copy into {}: {e}", to.display()))?;
        for entry in std::fs::read_dir(&from).map_err(|e| format!("cannot read {}: {e}", from.display()))?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if super::export::LEFT_OUT.contains(&name.as_str()) || name.ends_with(".pyc") || name == ".DS_Store" {
                continue;
            }
            if meta.file_type().is_symlink() {
                return Err(format!("{} is a link; links are not taken", entry.path().display()));
            }
            if meta.is_dir() {
                pending.push((entry.path(), to.join(&name)));
            } else if meta.is_file() {
                count += 1;
                total += meta.len();
                if count > MAX_FILES || meta.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
                    return Err("the folder is too large to be a plugin".to_string());
                }
                std::fs::copy(entry.path(), to.join(&name)).map_err(|e| format!("cannot copy {}: {e}", entry.path().display()))?;
            }
        }
    }
    hoist(dest)
}

/// A zip of a folder has everything inside that one folder: its contents are moved up so that `plugin.toml` is at the top
fn hoist(dest: &Path) -> Result<(), String> {
    if dest.join("plugin.toml").is_file() {
        return Ok(());
    }
    // What a Mac's Archive Utility adds (`__MACOSX`, `.DS_Store`) is not part of the plugin
    let entries: Vec<_> = std::fs::read_dir(dest)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.') && e.file_name() != "__MACOSX")
        .collect();
    let [only] = entries.as_slice() else {
        return Err("there is no plugin.toml at the top of the file".to_string());
    };
    let inner = only.path();
    if !inner.is_dir() || !inner.join("plugin.toml").is_file() {
        return Err("there is no plugin.toml at the top of the file".to_string());
    }
    for child in std::fs::read_dir(&inner).map_err(|e| e.to_string())?.flatten() {
        std::fs::rename(child.path(), dest.join(child.file_name())).map_err(|e| format!("cannot unpack: {e}"))?;
    }
    std::fs::remove_dir(&inner).map_err(|e| e.to_string())
}

/// Whether `url` is an address `git clone` may be given: https, http or ssh, or the `user@host:path` form. Not a local path or
/// `file:`, which would read the machine's own files, and not anything that starts like an option.
pub fn valid_git_url(url: &str) -> bool {
    if url.is_empty() || url.len() > 500 || url.starts_with('-') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = url.to_lowercase();
    if ["https://", "http://", "ssh://"].iter().any(|scheme| lower.starts_with(scheme)) {
        return true;
    }
    // user@host:path
    let Some((user_host, path)) = url.split_once(':') else { return false };
    let Some((user, host)) = user_host.split_once('@') else { return false };
    let word = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    word(user) && word(host) && !path.is_empty() && !path.starts_with('-')
}

/// Whether `git_ref` is a branch or tag name that cannot be taken for an option or a path trick
pub fn valid_git_ref(git_ref: &str) -> bool {
    !git_ref.is_empty()
        && git_ref.len() <= 100
        && git_ref.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && !git_ref.contains("..")
        && git_ref.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

/// Fetches `url` (at `git_ref`, else its default branch) into `dest` with its history left behind, and says which commit it is.
pub async fn git_clone(url: &str, git_ref: Option<&str>, dest: &Path) -> Result<String, String> {
    let mut clone = Command::new("git");
    clone
        .args(["-c", "protocol.file.allow=never", "clone", "--depth", "1", "--single-branch", "--no-tags"]);
    if let Some(git_ref) = git_ref {
        clone.args(["--branch", git_ref]);
    }
    // `--` keeps an address from being read as an option; the protocols git itself accepts are narrowed as well
    clone
        .args(["--", url])
        .arg(dest)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "https:http:ssh")
        .kill_on_drop(true);
    let output = tokio::time::timeout(GIT_TIMEOUT, clone.output())
        .await
        .map_err(|_| format!("git did not finish within {} minutes", GIT_TIMEOUT.as_secs() / 60))?
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "git is not installed on the machine the backend runs on".to_string() } else { format!("git could not run: {e}") })?;
    if !output.status.success() {
        // git's progress line names the staging folder, which means nothing to the owner
        let stderr = String::from_utf8_lossy(&output.stderr);
        let said: Vec<&str> = stderr.lines().filter(|l| !l.starts_with("Cloning into")).map(str::trim).filter(|l| !l.is_empty()).collect();
        let said: String = said.join("; ").chars().take(500).collect();
        return Err(format!("git could not fetch it: {said}"));
    }
    let head = Command::new("git").arg("-C").arg(dest).args(["rev-parse", "HEAD"]).output().await.map_err(|e| format!("git could not run: {e}"))?;
    let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();
    // The history is not part of the plugin and would be exported with it
    let _ = std::fs::remove_dir_all(dest.join(".git"));
    Ok(commit)
}

/// What is in the tree, or why it cannot be taken: no link anywhere, and the limits above. Blocking.
pub fn survey(dir: &Path) -> Result<(Vec<StagedFile>, u64), String> {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?.flatten() {
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            let relative = entry.path().strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if meta.file_type().is_symlink() {
                return Err(format!("it holds a link ('{relative}'); links are not taken"));
            }
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                if files.len() >= MAX_FILES {
                    return Err(format!("it holds more than {MAX_FILES} files"));
                }
                if meta.len() > MAX_FILE_BYTES {
                    return Err(format!("'{relative}' is larger than {} MB", MAX_FILE_BYTES / (1024 * 1024)));
                }
                total += meta.len();
                if total > MAX_TOTAL_BYTES {
                    return Err(format!("it is more than {} MB in all", MAX_TOTAL_BYTES / (1024 * 1024)));
                }
                files.push(StagedFile { path: relative, size: meta.len() });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, total))
}

/// The sha256 of a file, as lowercase hex. Blocking.
pub fn sha256_of(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn sha256_of_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The text of a staged file for the owner to read: a plain path inside the tree, a text file, not too large
pub fn read_staged_file(tree: &Path, path: &str) -> Result<String, String> {
    let relative = Path::new(path);
    if path.is_empty() || !relative.is_relative() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("'{path}' is not a path inside the plugin"));
    }
    let file = tree.join(relative);
    let meta = std::fs::symlink_metadata(&file).map_err(|_| format!("there is no file '{path}'"))?;
    if !meta.is_file() {
        return Err(format!("there is no file '{path}'"));
    }
    if meta.len() > MAX_VIEWED_FILE_BYTES {
        return Err(format!("'{path}' is larger than {} KB, too large to show", MAX_VIEWED_FILE_BYTES / 1024));
    }
    let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| format!("'{path}' is not text"))
}

/// The plugin's README, if it has one, cut to a size that is read at a glance
pub fn readme(tree: &Path) -> Option<String> {
    for name in ["README.md", "readme.md", "README.txt", "README"] {
        if let Ok(text) = std::fs::read_to_string(tree.join(name)) {
            let mut text: String = text.chars().take(MAX_README_BYTES).collect();
            if text.len() >= MAX_README_BYTES {
                text.push_str("\n...");
            }
            return Some(text);
        }
    }
    None
}

pub fn listed(files: Vec<StagedFile>) -> (Vec<StagedFile>, bool) {
    let truncated = files.len() > MAX_LISTED_FILES;
    (files.into_iter().take(MAX_LISTED_FILES).collect(), truncated)
}

/// The manifest of a staged tree
pub fn manifest_of(tree: &Path) -> Result<(Manifest, String), String> {
    let text = std::fs::read_to_string(tree.join("plugin.toml")).map_err(|_| "there is no plugin.toml at the top of it".to_string())?;
    let manifest = Manifest::parse(&text).map_err(|e| e.to_string())?;
    Ok((manifest, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in files {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tulpa-stage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_zip_is_unpacked_and_a_wrapping_folder_is_taken_off() {
        let dir = scratch("wrap");
        unpack_zip(&zip_of(&[("my-plugin/plugin.toml", b"x"), ("my-plugin/src/main.py", b"y")]), &dir).unwrap();
        assert!(dir.join("plugin.toml").is_file() && dir.join("src/main.py").is_file());
        assert!(!dir.join("my-plugin").exists());
        let flat = scratch("flat");
        unpack_zip(&zip_of(&[("plugin.toml", b"x")]), &flat).unwrap();
        assert!(flat.join("plugin.toml").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&flat).unwrap();
    }

    #[test]
    fn a_zip_that_leaves_its_folder_or_has_no_manifest_is_refused() {
        let dir = scratch("bad");
        assert!(unpack_zip(&zip_of(&[("plugin.toml", b"x"), ("../evil", b"y")]), &dir).unwrap_err().contains("leaves its folder"));
        assert!(unpack_zip(&zip_of(&[("a/x", b"x"), ("b/y", b"y")]), &scratch("two")).unwrap_err().contains("no plugin.toml"));
        assert!(unpack_zip(b"not a zip", &scratch("junk")).unwrap_err().contains("not a zip"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_git_address_is_a_remote_one_and_a_ref_is_a_plain_name() {
        for good in ["https://github.com/a/b", "http://host/a.git", "ssh://git@host/a/b", "git@github.com:a/b.git"] {
            assert!(valid_git_url(good), "{good}");
        }
        for bad in ["", "-oProxyCommand=x", "/etc", "file:///etc", "../x", "ext::sh -c x", "https://a b", "git@host", "host:path"] {
            assert!(!valid_git_url(bad), "{bad}");
        }
        for good in ["main", "v1.2.3", "release/1.0"] {
            assert!(valid_git_ref(good), "{good}");
        }
        for bad in ["", "-x", "a..b", "a b", "a;b", "$(x)"] {
            assert!(!valid_git_ref(bad), "{bad}");
        }
    }

    #[test]
    fn a_staging_folder_nobody_confirmed_is_removed_and_a_new_one_is_not() {
        let root = scratch("prune");
        let old = staging_root(&root).join("0000000000000001");
        let new = staging_root(&root).join("0000000000000002");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::File::open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(2 * 60 * 60)).unwrap();
        prune(&root);
        assert!(!old.exists() && new.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_staged_file_is_a_plain_path_inside_the_tree() {
        let dir = scratch("view");
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        assert_eq!(read_staged_file(&dir, "a.txt").unwrap(), "hello");
        for bad in ["", "../a.txt", "/etc/passwd", "missing.txt"] {
            assert!(read_staged_file(&dir, bad).is_err(), "{bad}");
        }
        assert_eq!(staging_dir(Path::new("/r"), "0123456789abcdef"), Some(PathBuf::from("/r/.staging/0123456789abcdef")));
        assert_eq!(staging_dir(Path::new("/r"), "../etc"), None);
        assert_eq!(staging_dir(Path::new("/r"), "0123456789ABCDEF"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_a_tree_is_refused() {
        let dir = scratch("link");
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.join("l")).unwrap();
        assert!(survey(&dir).unwrap_err().contains("link"));
        std::fs::remove_file(dir.join("l")).unwrap();
        assert_eq!(survey(&dir).unwrap().0.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
