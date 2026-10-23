//! A skill as it arrives: the files of an upload, a folder or a zip on disk, or a zip dropped in the browser, brought to one
//! shape, a list of files named from the skill's own folder with a `SKILL.md` among them. Nothing is written here; the
//! store does that with what this lets through.
//!
//! Everything that arrives is checked: every path is cleaned (`paths::clean`), a zip that holds a path leaving its folder
//! or a link is refused whole, hidden files (`.git`, `.DS_Store`) are left out, and the number of files and their sizes are
//! bounded, counted as they are read so a zip that lies about its sizes cannot fill the disk.

use std::io::Read;
use std::path::Path;

use super::format;
use super::paths;

pub const MAX_FILES: usize = 300;
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 50 * 1024 * 1024;

/// The file that holds a skill's instructions
pub const SKILL_FILE: &str = "SKILL.md";

/// One skill's files, named from its own folder, and what its folder or zip was called.
#[derive(Debug)]
pub struct Pack {
    pub name_hint: Option<String>,
    pub files: Vec<(String, Vec<u8>)>,
}

impl Pack {
    /// The text of the `SKILL.md`
    pub fn skill_text(&self) -> Result<String, String> {
        let bytes = self.files.iter().find(|(path, _)| path == SKILL_FILE).map(|(_, bytes)| bytes).ok_or("there is no SKILL.md")?;
        String::from_utf8(bytes.clone()).map_err(|_| "SKILL.md is not text".to_string())
    }
}

/// Counts what has been taken in and refuses what goes past the limits.
#[derive(Default)]
struct Budget {
    files: usize,
    bytes: usize,
}

impl Budget {
    fn take(&mut self, path: &str, size: usize) -> Result<(), String> {
        self.files += 1;
        self.bytes += size;
        if self.files > MAX_FILES {
            return Err(format!("more than {MAX_FILES} files: a skill is a folder of instructions, not a project"));
        }
        if size > MAX_FILE_BYTES {
            return Err(format!("'{path}' is larger than {} MB", MAX_FILE_BYTES / (1024 * 1024)));
        }
        if self.bytes > MAX_TOTAL_BYTES {
            return Err(format!("more than {} MB in all", MAX_TOTAL_BYTES / (1024 * 1024)));
        }
        Ok(())
    }
}

fn is_zip(name: &str) -> bool {
    name.to_lowercase().ends_with(".zip")
}

/// The last part of a path without its ending: what a file or a zip is called.
fn stem(name: &str) -> &str {
    let last = name.rsplit('/').next().unwrap_or(name);
    last.rsplit_once('.').map_or(last, |(stem, _)| stem)
}

/// The files of an upload, each with the path the browser gave it (a dropped folder's files keep their folder). One zip on
/// its own is unpacked; anything else is the skill's files as they are.
pub fn from_files(files: Vec<(String, Vec<u8>)>) -> Result<Pack, String> {
    if files.len() == 1 && is_zip(&files[0].0) {
        let (name, bytes) = files.into_iter().next().unwrap();
        return from_zip(&bytes, name_hint_of(&name));
    }
    let mut budget = Budget::default();
    let mut entries = Vec::new();
    for (path, bytes) in files {
        let path = paths::clean(&path)?;
        if paths::is_hidden(&path) {
            continue;
        }
        budget.take(&path, bytes.len())?;
        entries.push((path, bytes));
    }
    rooted(entries, None)
}

fn name_hint_of(file_name: &str) -> Option<String> {
    format::name_from(stem(file_name))
}

/// The files of a zip, unpacked in memory.
pub fn from_zip(bytes: &[u8], name_hint: Option<String>) -> Result<Pack, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("this is not a zip file that can be read: {e}"))?;
    let mut budget = Budget::default();
    let mut entries = Vec::new();
    for at in 0..archive.len() {
        let mut entry = archive.by_index(at).map_err(|e| format!("the zip could not be read: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        // Zip's own check of a name: one that is absolute or climbs out with `..` is none, and the whole zip is refused
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("the zip holds a path that leaves its folder ('{}'), so it was not opened", entry.name()));
        };
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(format!("the zip holds a link ('{}'); links are not taken", entry.name()));
        }
        let path = paths::clean(&name.to_string_lossy())?;
        if paths::is_hidden(&path) {
            continue;
        }
        // Read at most one byte over the limit: a declared size can lie, what is actually read cannot
        let mut data = Vec::new();
        entry.by_ref().take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut data).map_err(|e| format!("'{path}' could not be read from the zip: {e}"))?;
        budget.take(&path, data.len())?;
        entries.push((path, data));
    }
    rooted(entries, name_hint)
}

/// A file, a zip or a folder on disk, read whole. Blocking. A folder's links are left out.
pub fn from_path(path: &Path) -> Result<Pack, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("'{}' could not be read: {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if meta.is_dir() {
        let mut budget = Budget::default();
        let mut entries = Vec::new();
        walk(path, "", &mut budget, &mut entries)?;
        return rooted(entries, format::name_from(&name));
    }
    if !meta.is_file() {
        return Err(format!("'{}' is neither a file nor a folder", path.display()));
    }
    if meta.len() as usize > MAX_TOTAL_BYTES {
        return Err(format!("'{}' is larger than {} MB", path.display(), MAX_TOTAL_BYTES / (1024 * 1024)));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("'{}' could not be read: {e}", path.display()))?;
    if is_zip(&name) {
        return from_zip(&bytes, name_hint_of(&name));
    }
    from_files(vec![(name, bytes)])
}

fn walk(dir: &Path, prefix: &str, budget: &mut Budget, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let mut children: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("'{}' could not be read: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    children.sort_by_key(|child| child.file_name());
    for child in children {
        let name = child.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let meta = std::fs::symlink_metadata(child.path()).map_err(|e| e.to_string())?;
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if meta.file_type().is_symlink() {
            continue;
        } else if meta.is_dir() {
            walk(&child.path(), &path, budget, out)?;
        } else if meta.is_file() {
            budget.take(&path, meta.len() as usize)?;
            let bytes = std::fs::read(child.path()).map_err(|e| format!("'{path}' could not be read: {e}"))?;
            out.push((paths::clean(&path)?, bytes));
        }
    }
    Ok(())
}

/// Finds the skill among the files: the `SKILL.md` closest to the top (or, for one markdown file on its own, that file), the
/// folder it is in becomes the skill's own, and the files outside it are left out.
fn rooted(entries: Vec<(String, Vec<u8>)>, name_hint: Option<String>) -> Result<Pack, String> {
    let depth = |path: &str| path.matches('/').count();
    let skill_files: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).filter(|p| p.rsplit('/').next() == Some(SKILL_FILE)).collect();
    let Some(shallowest) = skill_files.iter().map(|p| depth(p)).min() else {
        // One markdown file on its own is a SKILL.md under another name
        if let [(path, bytes)] = entries.as_slice() {
            let lower = path.to_lowercase();
            if lower.ends_with(".md") || lower.ends_with(".markdown") {
                return Ok(Pack { name_hint: format::name_from(stem(path)).or(name_hint), files: vec![(SKILL_FILE.to_string(), bytes.clone())] });
            }
        }
        return Err("there is no SKILL.md in what was sent: a skill is a folder with a SKILL.md in it".to_string());
    };
    let top: Vec<&str> = skill_files.iter().copied().filter(|p| depth(p) == shallowest).collect();
    if top.len() > 1 {
        return Err(format!("more than one skill was sent ({}): send one skill at a time", top.join(", ")));
    }
    // Owned, so that the entries can be moved below
    let root: String = top[0].strip_suffix(SKILL_FILE).unwrap_or("").to_string();
    let folder_name = root.trim_end_matches('/').rsplit('/').next().filter(|n| !n.is_empty()).and_then(format::name_from);
    let name_hint = folder_name.or(name_hint);
    let files = entries
        .into_iter()
        .filter_map(|(path, bytes)| path.strip_prefix(root.as_str()).filter(|rest| !rest.is_empty()).map(|rest| (rest.to_string(), bytes)))
        .collect();
    Ok(Pack { name_hint, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in files {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn named(pack: &Pack) -> Vec<&str> {
        let mut names: Vec<&str> = pack.files.iter().map(|(p, _)| p.as_str()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_folder_of_files_is_rooted_at_its_skill_md() {
        let pack = from_files(vec![
            ("my-skill/SKILL.md".into(), b"x".to_vec()),
            ("my-skill/scripts/run.sh".into(), b"y".to_vec()),
            ("my-skill/.git/config".into(), b"z".to_vec()),
            ("other/readme.txt".into(), b"w".to_vec()),
        ])
        .unwrap();
        assert_eq!(named(&pack), ["SKILL.md", "scripts/run.sh"], "the hidden file and what is outside the skill are left out");
        assert_eq!(pack.name_hint.as_deref(), Some("my-skill"));
    }

    #[test]
    fn a_skill_md_at_the_top_and_a_single_markdown_file_are_taken() {
        let pack = from_files(vec![("SKILL.md".into(), b"x".to_vec()), ("notes.md".into(), b"y".to_vec())]).unwrap();
        assert_eq!(named(&pack), ["SKILL.md", "notes.md"]);
        assert_eq!(pack.name_hint, None);
        let single = from_files(vec![("Lab Report.md".into(), b"---\n".to_vec())]).unwrap();
        assert_eq!(named(&single), ["SKILL.md"]);
        assert_eq!(single.name_hint.as_deref(), Some("lab-report"));
    }

    #[test]
    fn nothing_to_go_by_or_several_skills_is_refused_with_a_reason() {
        assert!(from_files(vec![("a.txt".into(), b"x".to_vec())]).unwrap_err().contains("no SKILL.md"));
        assert!(from_files(vec![("a.txt".into(), b"x".to_vec()), ("b.txt".into(), b"y".to_vec())]).is_err());
        let two = from_files(vec![("one/SKILL.md".into(), b"x".to_vec()), ("two/SKILL.md".into(), b"y".to_vec())]).unwrap_err();
        assert!(two.contains("more than one skill"), "{two}");
    }

    #[test]
    fn a_zip_is_unpacked_and_rooted_like_a_folder() {
        let bytes = zip_of(&[("pack/SKILL.md", b"x"), ("pack/ref/a.md", b"y")]);
        let pack = from_files(vec![("My Pack.zip".into(), bytes)]).unwrap();
        assert_eq!(named(&pack), ["SKILL.md", "ref/a.md"]);
        assert_eq!(pack.name_hint.as_deref(), Some("pack"), "the folder in the zip names it before the zip does");
        let flat = from_zip(&zip_of(&[("SKILL.md", b"x")]), Some("from-zip".into())).unwrap();
        assert_eq!(flat.name_hint.as_deref(), Some("from-zip"));
    }

    #[test]
    fn a_zip_that_tries_to_leave_its_folder_is_refused_whole() {
        let bytes = zip_of(&[("SKILL.md", b"x"), ("../evil.txt", b"y")]);
        let err = from_zip(&bytes, None).unwrap_err();
        assert!(err.contains("leaves its folder"), "{err}");
        assert!(from_zip(&zip_of(&[("/abs/evil.txt", b"y")]), None).is_err());
        assert!(from_zip(b"not a zip at all", None).unwrap_err().contains("not a zip"));
    }

    #[test]
    fn the_limits_are_kept() {
        let many: Vec<(String, Vec<u8>)> = (0..=MAX_FILES).map(|i| (format!("f{i}.txt"), vec![b'x'])).collect();
        assert!(from_files(many).unwrap_err().contains("more than"));
        let big = from_files(vec![("SKILL.md".into(), vec![b'x'; MAX_FILE_BYTES + 1])]).unwrap_err();
        assert!(big.contains("larger than"), "{big}");
        // A zip is read up to the limit however large it says its files are
        let bomb = zip_of(&[("SKILL.md", &vec![b'a'; MAX_FILE_BYTES + 10])]);
        assert!(from_zip(&bomb, None).unwrap_err().contains("larger than"));
    }

    #[test]
    fn a_folder_on_disk_is_read_without_links_or_hidden_files() {
        let dir = std::env::temp_dir().join(format!("skills-pack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("skill/scripts")).unwrap();
        std::fs::write(dir.join("skill/SKILL.md"), "x").unwrap();
        std::fs::write(dir.join("skill/scripts/run.sh"), "y").unwrap();
        std::fs::write(dir.join("skill/.hidden"), "z").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", dir.join("skill/link")).unwrap();
        let pack = from_path(&dir.join("skill")).unwrap();
        assert_eq!(named(&pack), ["SKILL.md", "scripts/run.sh"]);
        assert_eq!(pack.name_hint.as_deref(), Some("skill"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(from_path(&dir.join("missing")).is_err());
    }
}
