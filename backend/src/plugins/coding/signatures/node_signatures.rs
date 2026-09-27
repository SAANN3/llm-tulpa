use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Renders `dependency`'s type declarations (`.d.ts`) more or less as-is, following
/// local `import`/`export ... from`/triple-slash-reference specifiers within the
/// package — a `.d.ts` file is already just signatures (no implementation bodies), so
/// unlike the Rust extractor this doesn't need to parse and re-render anything, just
/// find the right files and concatenate them. `repo_path` only has to be *some* project
/// that depends on it; Node's own `require.resolve` (run via a real `node` process) is
/// what finds where npm/yarn/pnpm actually put that dependency, so nothing here
/// hardcodes a `node_modules` layout.
///
/// Best-effort, not exhaustive: a package with no shipped types and no matching
/// `@types/*` package has nothing to show; Yarn PnP (no real `node_modules` directory)
/// isn't resolved by plain `require.resolve` and isn't handled here; and a package's
/// `exports` map is only read for the simple/common shapes, not every possible
/// condition combination.
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    ensure_node_available()?;

    let (pkg_dir, entry) = resolve_entry_dts(repo_path, dependency)?;
    let mut out = format!("# {dependency} (source: {})\n\n", entry.display());
    out.push_str(&walk_declarations(&pkg_dir, &entry));
    Ok(out)
}

fn ensure_node_available() -> Result<(), String> {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|_| ())
        .map_err(|_| "`node` isn't available — this tool needs it to resolve npm package locations".to_string())
}

/// Resolves `dependency`'s `package.json` via Node's own module resolution (started
/// from `repo_path`), then reads that package's declared entry point — its own
/// `types`/`typings` field, a same-named file next to `main`, a handful of common
/// default paths, or (for a JS-only package) a matching `@types/<dependency>` package.
fn resolve_entry_dts(repo_path: &Path, dependency: &str) -> Result<(PathBuf, PathBuf), String> {
    if let Ok(pkg_json_path) = resolve_package_json(repo_path, dependency) {
        let pkg_dir = pkg_json_path.parent().unwrap_or(repo_path).to_path_buf();
        if let Some(entry) = types_entry_from_package_json(&pkg_dir, &pkg_json_path)? {
            return Ok((pkg_dir, entry));
        }
    }

    // No types of its own (or not resolvable at all) — a JS-only package's types often
    // live in a separate DefinitelyTyped package instead.
    let types_pkg = format!("@types/{}", dependency.trim_start_matches('@').replace('/', "__"));
    let pkg_json_path = resolve_package_json(repo_path, &types_pkg).map_err(|_| {
        format!(
            "'{dependency}' isn't resolvable from '{}', has no shipped type declarations, or has \
             neither and no matching '{types_pkg}' package either — check it's actually a \
             dependency of that project and the name is spelled exactly as in package.json",
            repo_path.display()
        )
    })?;
    let pkg_dir = pkg_json_path.parent().unwrap_or(repo_path).to_path_buf();
    let entry = types_entry_from_package_json(&pkg_dir, &pkg_json_path)?
        .ok_or_else(|| format!("found '{types_pkg}' but couldn't find its entry declaration file"))?;
    Ok((pkg_dir, entry))
}

/// Runs Node's `require.resolve(<package>/package.json, { paths: [repo_path] })` in a
/// throwaway `node -e` process — arguments are passed via environment variables, not
/// interpolated into the script text, so a package name can't break out of the script.
fn resolve_package_json(repo_path: &Path, package: &str) -> Result<PathBuf, String> {
    const SCRIPT: &str = r#"
        const dep = process.env.CI_DEP;
        const repo = process.env.CI_REPO;
        try {
            console.log(require.resolve(dep + "/package.json", { paths: [repo] }));
        } catch (e) {
            process.exit(1);
        }
    "#;

    let output = Command::new("node")
        .arg("-e")
        .arg(SCRIPT)
        .env("CI_DEP", package)
        .env("CI_REPO", repo_path)
        .output()
        .map_err(|e| format!("couldn't run node: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "'{package}' isn't resolvable from '{}' — check it's actually a dependency of that project \
             (directly or transitively) and the name is spelled exactly as in package.json",
            repo_path.display()
        ));
    }

    Ok(PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
}

/// Reads `package.json` and works out which file actually holds this package's type
/// declarations, trying (in order): `types`/`typings`, the `types` condition under
/// `exports["."]`, a `.d.ts` sibling of `main`, then a few conventional default paths.
fn types_entry_from_package_json(pkg_dir: &Path, pkg_json_path: &Path) -> Result<Option<PathBuf>, String> {
    let raw = std::fs::read_to_string(pkg_json_path).map_err(|e| format!("couldn't read '{}': {e}", pkg_json_path.display()))?;
    let pkg: Value = serde_json::from_str(&raw).map_err(|e| format!("couldn't parse '{}': {e}", pkg_json_path.display()))?;

    let candidates = [
        pkg.get("types").and_then(Value::as_str),
        pkg.get("typings").and_then(Value::as_str),
        pkg.get("exports").and_then(|e| e.get(".")).and_then(|e| e.get("types")).and_then(Value::as_str),
        pkg.get("exports").and_then(|e| e.get("types")).and_then(Value::as_str),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = pkg_dir.join(candidate);
        if path.exists() {
            return Ok(Some(path));
        }
    }

    if let Some(main) = pkg.get("main").and_then(Value::as_str) {
        let as_dts = pkg_dir.join(main).with_extension("d.ts");
        if as_dts.exists() {
            return Ok(Some(as_dts));
        }
    }

    for default in ["index.d.ts", "dist/index.d.ts", "lib/index.d.ts", "types/index.d.ts"] {
        let path = pkg_dir.join(default);
        if path.exists() {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

/// Walks every `.d.ts` file reachable from `entry` via a local (relative) `import`,
/// `export ... from`, or `/// <reference path="...">`, concatenating their contents.
/// Doesn't follow a specifier into another package (`from "some-other-pkg"`) — that's a
/// separate `get_dependency_signatures` call, same as the Rust extractor not resolving a
/// re-export into a different crate's own source.
fn walk_declarations(pkg_dir: &Path, entry: &Path) -> String {
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

        let rel = path.strip_prefix(pkg_dir).unwrap_or(&path);
        out.push_str(&format!("## {}\n\n", rel.display()));
        out.push_str(&clean_declaration_source(&content));
        out.push('\n');

        for spec in local_specifiers(&content) {
            if let Some(resolved) = resolve_local_specifier(path.parent().unwrap_or(pkg_dir), &spec) {
                queue.push_back(resolved);
            }
        }
    }

    out
}

/// Every relative (`./...`/`../...`) module specifier a `.d.ts` file points at, from
/// `import`/`export ... from "..."`, bare `import("...")` type references, and
/// triple-slash `<reference path="...">` directives — the handful of ways a type
/// declaration file links to another one.
fn local_specifiers(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        for marker in ["from \"", "from '", "import(\"", "import('", "path=\""] {
            if let Some(start) = line.find(marker) {
                let rest = &line[start + marker.len()..];
                let quote = if marker.ends_with('\'') { '\'' } else { '"' };
                if let Some(end) = rest.find(quote) {
                    let spec = &rest[..end];
                    if spec.starts_with('.') {
                        specs.push(spec.to_string());
                    }
                }
            }
        }
    }
    specs
}

/// Resolves a relative specifier the same handful of ways Node's own resolver would for
/// a declaration file: as-is, with `.d.ts` appended, or as `<dir>/index.d.ts`.
fn resolve_local_specifier(from_dir: &Path, spec: &str) -> Option<PathBuf> {
    let base = from_dir.join(spec);
    for candidate in [base.clone(), base.with_extension("d.ts"), base.join("index.d.ts")] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Light cleanup, not a real parser: drops the `//# sourceMappingURL=` trailer generated
/// declaration files often end with, and collapses runs of blank lines — nothing here
/// touches the actual declarations, which are already signature-only by construction.
fn clean_declaration_source(content: &str) -> String {
    let mut out = String::new();
    let mut blank_run = 0;
    // `None` = not inside a `/** ... */` block. `Some(false)` = inside one, still in its
    // free-text description. `Some(true)` = inside one, already cut short at its first
    // `@tag` line (a real-world `/** */` block on a widely-used API is dominated by
    // `@example`/`@param`/`@see` bulk that repeats what the signature already says —
    // cutting there is what keeps e.g. React's actually-useful hooks from being pushed
    // out of the output-size cap by their own doc comments' example code).
    let mut in_doc: Option<bool> = None;

    for line in content.lines() {
        if let Some(already_cut) = in_doc {
            if line.contains("*/") {
                in_doc = None;
                if !already_cut {
                    out.push_str(line);
                    out.push('\n');
                }
                // Already emitted our own early `*/` when the cut happened — the real
                // closing line is just discarded here.
                continue;
            }
            if already_cut {
                continue;
            }
            let body = line.trim_start().trim_start_matches('*').trim();
            if body.starts_with('@') {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(indent);
                out.push_str("*/\n");
                in_doc = Some(true);
                continue;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if line.trim_start().starts_with("//# sourceMappingURL=") {
            continue;
        }
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }

        if line.contains("/**") && !line.contains("*/") {
            in_doc = Some(false);
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
