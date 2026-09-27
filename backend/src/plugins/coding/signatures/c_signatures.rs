use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

/// C/C++ has no single accepted dependency manager the way Rust/Node/Go do, and a
/// header *is* the public API by convention (no separate signature-only format like
/// `.d.ts` exists) — so `dependency` here is the header's own name (e.g. `"curl/curl.h"`,
/// or a bare name like `"sqlite3"` to try as `sqlite3.h`/`sqlite3/sqlite3.h`). Finding it
/// tries, in order: `pkg-config`'s own `--cflags` (asks a real tool rather than guessing
/// a path, for the common case where the library registered one), a handful of
/// conventional local dependency directories under `repo_path`, and the compiler's own
/// system include search path (asked via `cc -E -v`, not hardcoded either).
///
/// Best-effort, not exhaustive: no preprocessing happens (no macro expansion, no
/// `#ifdef` evaluation — mirrors the same "cfg gates aren't evaluated" caveat as the
/// Rust extractor), a local `#include "..."` is followed but a system `#include <...>`
/// is not (same "don't cross into a different dependency's own source" rule as the
/// others), and this is a textual heuristic, not a real C/C++ parser — an unusual macro-
/// heavy header (calling-convention/visibility macros wrapping every declaration, common
/// in older C libraries) can come through looking noisier than a hand-written summary
/// would.
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    let header = find_header(repo_path, dependency)?;
    let mut out = format!("# {dependency} (header: {})\n\n", header.display());
    out.push_str(&walk_header(&header));
    Ok(out)
}

fn find_header(repo_path: &Path, dependency: &str) -> Result<PathBuf, String> {
    let candidate_names: Vec<String> = if dependency.ends_with(".h") || dependency.ends_with(".hpp") {
        vec![dependency.to_string()]
    } else {
        vec![format!("{dependency}.h"), format!("{dependency}/{dependency}.h"), format!("{dependency}.hpp")]
    };

    // `pkg-config` is itself a resolver, same spirit as `cargo metadata`/`require.resolve`
    // — asked first since it's the most reliable when the library registered with it.
    if let Ok(output) = Command::new("pkg-config").args(["--cflags", dependency]).output() {
        if output.status.success() {
            for dir in include_dirs_from_flags(&String::from_utf8_lossy(&output.stdout)) {
                for name in &candidate_names {
                    let path = Path::new(&dir).join(name);
                    if path.is_file() {
                        return Ok(path);
                    }
                }
            }
        }
    }

    let mut search_dirs = vec![repo_path.to_path_buf()];
    for sub in ["include", "third_party", "vendor", "deps"] {
        search_dirs.push(repo_path.join(sub));
    }
    search_dirs.extend(compiler_system_include_dirs());

    for dir in &search_dirs {
        for name in &candidate_names {
            let path = dir.join(name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }

    Err(format!(
        "couldn't find a header for '{dependency}' — tried pkg-config, '{}' and its common \
         subdirectories, and the compiler's system include paths; pass the header's own \
         relative path (e.g. \"somelib/somelib.h\") if it lives somewhere else",
        repo_path.display()
    ))
}

fn include_dirs_from_flags(flags: &str) -> Vec<String> {
    flags
        .split_whitespace()
        .filter_map(|tok| tok.strip_prefix("-I"))
        .map(str::to_string)
        .collect()
}

/// Asks the compiler itself where it looks for `<...>` includes (`cc -E -v -xc /dev/null`
/// prints them between two marker lines) — never a hardcoded `/usr/include` guess, since
/// that differs by platform and by compiler.
fn compiler_system_include_dirs() -> Vec<PathBuf> {
    let Ok(output) = Command::new("cc").args(["-E", "-v", "-xc", "-"]).arg("/dev/null").output() else {
        return vec![];
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut dirs = vec![];
    let mut in_list = false;
    for line in stderr.lines() {
        if line.contains("#include <...> search starts here") {
            in_list = true;
            continue;
        }
        if line.starts_with("End of search list") {
            break;
        }
        if in_list {
            dirs.push(PathBuf::from(line.trim()));
        }
    }
    dirs
}

/// Follows local (`"..."`, not `<...>`) includes from `entry`, concatenating each
/// header's cleaned-up content.
fn walk_header(entry: &Path) -> String {
    let mut out = String::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(entry.to_path_buf());

    while let Some(path) = queue.pop_front() {
        let Ok(canonical) = path.canonicalize() else { continue };
        if !visited.insert(canonical) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else { continue };

        out.push_str(&format!("## {}\n\n", path.display()));
        out.push_str(&clean_header_source(&content));
        out.push('\n');

        for local in local_includes(&content) {
            if let Some(dir) = path.parent() {
                let resolved = dir.join(&local);
                if resolved.is_file() {
                    queue.push_back(resolved);
                }
            }
        }
    }

    out
}

fn local_includes(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("#include")?.trim();
            let inner = rest.strip_prefix('"')?;
            let end = inner.find('"')?;
            Some(inner[..end].to_string())
        })
        .collect()
}

/// Drops preprocessor directives (except the local `#include`s already followed above —
/// keeping them here too would just duplicate the walked content) and comments (block
/// comments trimmed to their first line, matching the same convention the Rust/JS
/// extractors use), leaving what's left — typedefs, struct/enum/union bodies, function
/// prototypes — essentially untouched, since a header's declarations already *are* its
/// signatures.
fn clean_header_source(content: &str) -> String {
    let mut out = String::new();
    let mut in_block_comment_trimmed = false;
    // A `#define`/`#if`/... directive can span several lines via a trailing `\` —
    // dropping only the line that starts with `#` and not its continuations left their
    // raw macro-body text leaking straight into the output as garbage.
    let mut in_directive_continuation = false;

    for line in content.lines() {
        let trimmed = line.trim_start();

        if in_directive_continuation {
            in_directive_continuation = trimmed.ends_with('\\');
            continue;
        }

        if in_block_comment_trimmed {
            if trimmed.contains("*/") {
                in_block_comment_trimmed = false;
            }
            continue;
        }

        if trimmed.starts_with("#") {
            in_directive_continuation = trimmed.ends_with('\\');
            continue;
        }

        if trimmed.starts_with("/*") {
            if let Some(end) = trimmed.find("*/") {
                out.push_str(&trimmed[..end + 2]);
                out.push('\n');
            } else {
                out.push_str(trimmed);
                out.push('\n');
                in_block_comment_trimmed = true;
            }
            continue;
        }

        if let Some(idx) = trimmed.find("//") {
            let code = trimmed[..idx].trim_end();
            if !code.is_empty() {
                out.push_str(code);
                out.push('\n');
            }
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    out
}
