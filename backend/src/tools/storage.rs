pub mod create_directory;
pub mod delete_directory;
pub mod delete_file;
pub mod detect_file_type;
pub mod find_files;
pub mod list_directory;
pub mod read_file;
pub mod replace_str;
pub mod write_file;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::base::{ResolvedScope, ScopeGrant, SharedBucket, Tool, ToolError, ToolPermission};

/// Every tool in the `storage` domain (function names prefixed `storage.`), for
/// `main.rs` to register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(read_file::ReadFileTool),
        Box::new(write_file::WriteFileTool),
        Box::new(replace_str::ReplaceStrTool),
        Box::new(delete_file::DeleteFileTool),
        Box::new(list_directory::ListDirectoryTool),
        Box::new(create_directory::CreateDirectoryTool),
        Box::new(delete_directory::DeleteDirectoryTool),
        Box::new(find_files::FindFilesTool),
        Box::new(detect_file_type::DetectFileTypeTool),
    ]
}

/// Lexically resolves `.`/`..` and makes `path` absolute against the server's current
/// working directory, after first expanding a leading `~`. Deliberately doesn't touch
/// the filesystem or follow symlinks — the path may not exist yet (`create_directory`'s
/// target, a new file for `write_file`), and every caller here just wants a canonical
/// string to compare/store, not a filesystem check.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let path = expand_tilde(path);

    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };

    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Expands a leading `~` to the server's home directory — the model reaches for `~`
/// following ordinary shell convention, but `std::path` never expands it (that's a
/// shell behavior, not a filesystem one), so left alone it was being read as a literal
/// path component instead. Only the bare `~` prefix is handled, not `~username` (a
/// rarer shell form for someone else's home directory, not worth chasing here). Falls
/// back to the path unchanged if the server's home directory can't be determined.
fn expand_tilde(path: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => dirs::home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| path.to_path_buf()),
        Err(_) => path.to_path_buf(),
    }
}

/// The map of every folder granted under one shared bucket — `{"<bucket.json_key()>":
/// {"<folder>": true, ...}}`. A folder is covered if it or any ancestor of it is a key
/// in the map, so approvals accumulate across separate calls instead of the most
/// recently granted folder silently replacing every earlier one.
fn granted_folders(bucket: SharedBucket, scope: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    scope?.get(bucket.json_key())?.as_object()
}

/// Where `path` (already `normalize`d) really is: the part of it that exists with its symlinks resolved, and the
/// rest — not created yet — appended as it is. A symlink that points nowhere yet (one a write would create the
/// target of) is followed by hand, so it can't pass for a plain new name inside a granted folder.
pub(crate) fn real_location(path: &Path) -> PathBuf {
    resolve_real(path, 0)
}

/// Symlinks followed by hand before giving up on a chain (a loop): the same limit Linux applies.
const MAX_SYMLINK_HOPS: usize = 40;

fn resolve_real(path: &Path, hops: usize) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(metadata) = std::fs::symlink_metadata(&existing) {
            let resolved = match std::fs::canonicalize(&existing) {
                Ok(real) => Some(real),
                // A dangling symlink: where it would lead once its target is created
                Err(_) if metadata.file_type().is_symlink() && hops < MAX_SYMLINK_HOPS => std::fs::read_link(&existing).ok().map(|target| {
                    let base = existing.parent().and_then(|parent| std::fs::canonicalize(parent).ok()).unwrap_or_default();
                    resolve_real(&normalize(&base.join(target)), hops + 1)
                }),
                Err(_) => None,
            };
            let Some(mut real) = resolved else { return path.to_path_buf() };
            real.extend(missing.iter().rev());
            return real;
        }
        match (existing.file_name().map(|name| name.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Whether `target` lies inside one of the granted `folders`, compared where both really are: a symlink inside a
/// granted folder that points out of it (at `/etc`, `~/.ssh`) doesn't take the grant along, and a folder granted
/// through a symlinked path still covers the place it names. Comparing the paths as written did the first.
pub(crate) fn is_within_granted<'a>(target: &Path, mut folders: impl Iterator<Item = &'a String>) -> bool {
    let real_target = real_location(target);
    folders.any(|folder| real_target.starts_with(real_location(&normalize(Path::new(folder)))))
}

fn check_scope(scope_root: &Path, bucket: SharedBucket, granted: Option<&Value>) -> ToolPermission {
    let folders = granted_folders(bucket, granted);

    if let Some(folders) = folders {
        if is_within_granted(scope_root, folders.keys()) {
            return ToolPermission::Allowed;
        }
    }

    // Just the one new entry, not the existing map re-sent alongside it — `Agent::
    // allow_scope` is what actually appends it to whatever's currently granted, read
    // fresh at persist time. Offering the whole accumulated map here instead would mean
    // two denied calls from the same reply (e.g. reading two different ungranted
    // folders at once) each build their escalation from the same pre-approval
    // snapshot — approving both would have the second overwrite the first's addition.
    //
    // The grant offered (and named in the question) is where the folder really is: a path through a symlink
    // would otherwise ask about `~/project/link` while granting `/etc`.
    let real_root = real_location(scope_root);
    let ui_message = if real_root == scope_root {
        format!("Allow access to everything under '{}' (including subfolders)?", scope_root.display())
    } else {
        format!(
            "Allow access to everything under '{}' (including subfolders)? The path asked for, '{}', leads there.",
            real_root.display(),
            scope_root.display()
        )
    };
    ToolPermission::Denied {
        reason: format!("no permission granted covering '{}'", scope_root.display()),
        escalation: Some(ScopeGrant {
            scope: ResolvedScope {
                own: None,
                shared: HashMap::from([(
                    bucket,
                    serde_json::json!({ bucket.json_key(): { real_root.to_string_lossy(): true } }),
                )]),
            },
            ui_message,
        }),
    }
}

/// For tools whose `path` argument names a file (`read_file`, `write_file`,
/// `delete_file`) — scopes to the file's containing folder, so a grant covers every
/// file in that folder, not just this one call's.
pub(super) fn check_file_scope(path: &str, bucket: SharedBucket, scope: Option<&Value>) -> ToolPermission {
    // The file itself is resolved before its folder is taken: a symlinked file in a granted folder (`notes` ->
    // `/etc/passwd`, or one whose target doesn't exist yet) belongs to the folder it leads to
    let target = real_location(&normalize(Path::new(path)));
    let root = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| target.clone());
    check_scope(&root, bucket, scope)
}

/// For tools whose `path` argument names a directory itself (`list_directory`,
/// `create_directory`, `delete_directory`, `find_files`) — scopes to that directory
/// directly, not its parent.
pub(crate) fn check_directory_scope(path: &str, bucket: SharedBucket, scope: Option<&Value>) -> ToolPermission {
    check_scope(&normalize(Path::new(path)), bucket, scope)
}

/// Refuses a file larger than `max_bytes` before anything reads it: these tools read a file whole, and a
/// model that points one at a disk image or a model file would otherwise pull gigabytes into the backend's
/// memory. `what` and `instead` finish the message ("too large to read: ...").
pub(crate) async fn refuse_if_larger(path: &Path, max_bytes: u64, what: &str, instead: &str) -> Result<(), ToolError> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?
        .len();
    if size > max_bytes {
        return Err(ToolError::FailedUnknown(format!(
            "'{}' is {}, too large {what} (the limit is {}): {instead}",
            path.display(),
            human_size(size),
            human_size(max_bytes)
        )));
    }
    Ok(())
}

/// Adaptive human-readable byte count — individual files span a much wider range than
/// the whole-disk figures `os`'s `format_gb` is built for, so this steps up through
/// b/kb/mb/gb instead of always reporting gb.
pub(crate) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["kb", "mb", "gb"];

    let mut size = bytes as f64;
    let mut unit = None;
    for name in UNITS {
        if size < 1024.0 {
            break;
        }
        size /= 1024.0;
        unit = Some(name);
    }

    match unit {
        Some(name) => format!("{size:.1}{name}"),
        None => format!("{bytes}b"),
    }
}

/// Shared by `write_file`'s `replace` mode and `replace_str` — swaps the one, unique
/// occurrence of `needle` in the file at `path` for `replacement`. Errors rather than
/// guessing if `needle` is missing or ambiguous, and rejects a `replacement` that echoes
/// verbatim chunks of the file's untouched surrounding text (see `overlapping_context`)
/// — a real, repeated model failure mode where the whole file gets passed as the
/// replacement instead of just the part that's actually changing.
pub(super) async fn replace_in_file(
    path: &Path,
    needle: &str,
    replacement: &str,
) -> Result<usize, ToolError> {
    let existing = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

    let count = existing.matches(needle).count();
    if count == 0 {
        return Err(ToolError::FailedUnknown(format!(
            "'{}' doesn't contain the text given to replace",
            path.display()
        )));
    }
    if count > 1 {
        return Err(ToolError::FailedUnknown(format!(
            "the text to replace appears {count} times in '{}' — include more surrounding \
             context so it matches exactly once",
            path.display()
        )));
    }

    if let Some(overlap) = overlapping_context(&existing, needle, replacement) {
        return Err(ToolError::FailedUnknown(format!(
            "the replacement text contains '{overlap}' — text that belongs right next to what's \
             being replaced in '{}' but is outside it. This usually means the whole file (or a \
             large chunk of it) was passed as the replacement instead of just the new text that \
             takes its place — pass only that.",
            path.display()
        )));
    }

    let updated = existing.replacen(needle, replacement, 1);
    let bytes_written = updated.len();

    tokio::fs::write(path, updated)
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't write '{}': {e}", path.display())))?;

    Ok(bytes_written)
}

const CONTEXT_PROBE_CHARS: usize = 24;
const MIN_PROBE_SIGNAL_CHARS: usize = 12;

/// Looks for a chunk of the file's text immediately before/after `needle` (untouched context
/// that the replacement isn't supposed to cover) showing up verbatim inside `replacement`.
/// That's the signature of the whole file (or a large slice of it) being passed as the
/// replacement instead of just the new text — a real, repeated model failure mode, not a
/// hypothetical.
fn overlapping_context(existing: &str, needle: &str, replacement: &str) -> Option<String> {
    let split = existing.find(needle)?;
    let before = &existing[..split];
    let after = &existing[split + needle.len()..];

    let before_probe: String = before
        .trim_end()
        .chars()
        .rev()
        .take(CONTEXT_PROBE_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if is_meaningful_probe(&before_probe) && replacement.contains(&before_probe) {
        return Some(before_probe);
    }

    let after_probe: String = after.trim_start().chars().take(CONTEXT_PROBE_CHARS).collect();
    if is_meaningful_probe(&after_probe) && replacement.contains(&after_probe) {
        return Some(after_probe);
    }

    None
}

fn is_meaningful_probe(probe: &str) -> bool {
    probe.chars().filter(|c| !c.is_whitespace()).count() >= MIN_PROBE_SIGNAL_CHARS
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[tokio::test]
    async fn a_file_over_the_limit_is_refused_before_it_is_read() {
        let path = std::env::temp_dir().join(format!("tulpa-size-test-{}", std::process::id()));
        std::fs::File::create(&path).unwrap().set_len(3 * 1024 * 1024).unwrap();
        assert!(refuse_if_larger(&path, 4 * 1024 * 1024, "to read", "x").await.is_ok());
        let Err(ToolError::FailedUnknown(message)) = refuse_if_larger(&path, 1024 * 1024, "to read", "use head").await else {
            panic!("a file over the limit must be refused");
        };
        assert!(message.contains("3.0mb") && message.contains("1.0mb") && message.contains("use head"), "{message}");
        std::fs::remove_file(&path).unwrap();
    }
}

#[cfg(all(test, unix))]
mod symlink_tests {
    use std::os::unix::fs::symlink;

    use super::*;

    fn grant(folder: &Path) -> Value {
        serde_json::json!({ SharedBucket::StorageRead.json_key(): { folder.to_string_lossy(): true } })
    }

    fn allowed(path: &Path, granted: &Value) -> bool {
        matches!(check_file_scope(&path.to_string_lossy(), SharedBucket::StorageRead, Some(granted)), ToolPermission::Allowed)
    }

    #[test]
    fn a_grant_covers_where_things_really_are() {
        let root = std::env::temp_dir().join(format!("tulpa-symlink-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let granted = root.join("granted");
        let secret = root.join("secret");
        std::fs::create_dir_all(granted.join("sub")).unwrap();
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::write(secret.join("key"), "x").unwrap();
        symlink(&secret, granted.join("link")).unwrap();
        // A link whose target doesn't exist yet: a write through it would create the file outside
        symlink(secret.join("not-yet"), granted.join("dangling")).unwrap();
        symlink(&granted, root.join("alias")).unwrap();
        let in_granted = grant(&granted);

        assert!(allowed(&granted.join("sub/file.txt"), &in_granted));
        assert!(allowed(&granted.join("new/deeper/file.txt"), &in_granted), "a path that doesn't exist yet");
        assert!(allowed(&granted.join("sub/../sub/file.txt"), &in_granted));
        assert!(!allowed(&granted.join("link/key"), &in_granted), "a symlink out of the granted folder");
        assert!(!allowed(&granted.join("dangling/x"), &in_granted), "a dangling symlink out of it");
        // A symlinked file belongs to the folder it leads to
        symlink(secret.join("key"), granted.join("file-link")).unwrap();
        assert!(!allowed(&granted.join("file-link"), &in_granted), "a symlinked file out of the folder");
        assert!(!allowed(&granted.join("dangling"), &in_granted), "a dangling symlinked file out of the folder");
        symlink(granted.join("sub/inside.txt"), granted.join("inner-link")).unwrap();
        assert!(allowed(&granted.join("inner-link"), &in_granted), "a symlink that stays inside is fine");
        assert!(!allowed(&secret.join("key"), &in_granted));

        // A folder granted through a symlinked path covers the place it names
        let through_alias = grant(&root.join("alias"));
        assert!(allowed(&granted.join("sub/file.txt"), &through_alias));
        assert!(allowed(&root.join("alias/sub/file.txt"), &in_granted));

        // The question and the grant name the real place
        let ToolPermission::Denied { escalation: Some(offer), .. } =
            check_directory_scope(&granted.join("link").to_string_lossy(), SharedBucket::StorageRead, Some(&in_granted))
        else {
            panic!("a path out of the grant must ask");
        };
        let real_secret = std::fs::canonicalize(&secret).unwrap();
        assert!(offer.ui_message.contains(&*real_secret.to_string_lossy()), "{}", offer.ui_message);
        let offered = &offer.scope.shared[&SharedBucket::StorageRead][SharedBucket::StorageRead.json_key()];
        assert!(offered.get(&*real_secret.to_string_lossy()).is_some(), "{offered}");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
