//! A plugin's folder as a `.tulpa` file: a zip of what is needed to install it elsewhere. What a build or a run leaves behind
//! (`target/`, virtual environments, caches) is not part of it.

use std::io::Write;
use std::path::Path;

/// Folders that are made by building or running a plugin, never shipped
const LEFT_OUT: [&str; 7] = ["target", ".venv", "venv", "__pycache__", "node_modules", ".git", ".staging"];
const MAX_EXPORT_BYTES: u64 = 512 * 1024 * 1024;

/// The files of `dir` as a zip, with `plugin.toml` at its top. Blocking. Links are left out.
pub fn zip_folder(dir: &Path) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut total = 0u64;
    let mut pending = vec![dir.to_path_buf()];
    let mut files = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() || LEFT_OUT.contains(&name.as_str()) || name.ends_with(".pyc") {
                continue;
            }
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    for path in files {
        let relative = path.strip_prefix(dir).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {relative}: {e}"))?;
        total += bytes.len() as u64;
        if total > MAX_EXPORT_BYTES {
            return Err(format!("the plugin is more than {} MB; it is not exported", MAX_EXPORT_BYTES / (1024 * 1024)));
        }
        #[allow(unused_mut)]
        let mut options = zip::write::SimpleFileOptions::default();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            options = options.unix_permissions(std::fs::metadata(&path).map_err(|e| e.to_string())?.permissions().mode() & 0o777);
        }
        writer.start_file(relative.as_str(), options).map_err(|e| e.to_string())?;
        writer.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    Ok(writer.finish().map_err(|e| e.to_string())?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_build_leaves_is_not_exported_and_what_is_needed_is() {
        let dir = std::env::temp_dir().join(format!("tulpa-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in ["plugin.toml", "src/main.py", "target/release/big", "__pycache__/x.pyc", ".venv/bin/python", "migrations/0001_a.sql"] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let bytes = zip_folder(&dir).unwrap();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut names: Vec<&str> = archive.file_names().collect();
        names.sort();
        assert_eq!(names, ["migrations/0001_a.sql", "plugin.toml", "src/main.py"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
