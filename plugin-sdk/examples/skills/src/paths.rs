//! A path inside a skill, as an upload or a tool gives it. Every such path is cleaned here before anything is written or
//! read, so what reaches the disk is always inside the skill's own folder: nothing absolute, nothing that climbs out with
//! `..`, no drive letters, no control characters.

/// How deep a skill's files may go, and how long a path or one of its parts may be.
const MAX_DEPTH: usize = 10;
const MAX_PATH_BYTES: usize = 250;
const MAX_PART_BYTES: usize = 100;

/// The path with `/` between its parts and nothing else in it, or why it cannot be used. Hidden parts (starting with a
/// dot) are fine here; `is_hidden` is how an upload leaves them out.
pub fn clean(path: &str) -> Result<String, String> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err(format!("'{path}' is an absolute path; a skill's files are named from the skill's own folder"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("'{path}' climbs out of the skill's folder")),
            _ if part.chars().any(char::is_control) => return Err(format!("'{path}' has a control character in it")),
            _ if part.len() > MAX_PART_BYTES => return Err(format!("'{path}' has a name that is too long")),
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err("a file needs a name".to_string());
    }
    if parts.len() > MAX_DEPTH {
        return Err(format!("'{path}' is nested too deep (at most {MAX_DEPTH} folders)"));
    }
    let joined = parts.join("/");
    if joined.len() > MAX_PATH_BYTES {
        return Err(format!("'{path}' is too long"));
    }
    Ok(joined)
}

/// Whether a cleaned path has a part that starts with a dot (`.git`, `.DS_Store`): what a folder carries around without
/// being part of the skill.
pub fn is_hidden(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_kept_and_tidied() {
        assert_eq!(clean("scripts/run.sh").unwrap(), "scripts/run.sh");
        assert_eq!(clean("./a//b/./c.md").unwrap(), "a/b/c.md");
        assert_eq!(clean("dir\\file.txt").unwrap(), "dir/file.txt");
        assert_eq!(clean("SKILL.md").unwrap(), "SKILL.md");
    }

    #[test]
    fn a_path_that_leaves_the_skills_folder_is_refused() {
        assert!(clean("../evil").is_err());
        assert!(clean("a/../../evil").is_err());
        assert!(clean("a/..").is_err());
        assert!(clean("/etc/passwd").is_err());
        assert!(clean("\\windows\\system").is_err());
        assert!(clean("C:/x").is_err());
        assert!(clean("").is_err());
        assert!(clean("./").is_err());
        assert!(clean("a\u{0}b").is_err());
        assert!(clean(&"a/".repeat(11)).is_err());
        assert!(clean(&"x".repeat(101)).is_err());
    }

    #[test]
    fn hidden_parts_are_found() {
        assert!(is_hidden(".git/config"));
        assert!(is_hidden("a/.DS_Store"));
        assert!(!is_hidden("a/b.md"));
        assert!(!is_hidden("a.b/c"));
    }
}
