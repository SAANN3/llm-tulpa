use std::path::Path;
use std::process::Command;

/// `go doc -all <import path>` already does exactly what this whole tool family is for —
/// resolves the dependency via the project's own `go.mod`/module cache (wherever
/// `GOPATH`/`GOMODCACHE` actually is, nothing hardcoded) and prints its whole exported
/// API, signatures and doc comments included, with implementation bodies already
/// omitted. Nothing to parse or re-render here, unlike Rust/JS.
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    ensure_go_available()?;

    let output = Command::new("go")
        .args(["doc", "-all", dependency])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("couldn't run `go doc`: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "`go doc -all {dependency}` failed in '{}': {} — check `dependency` is the \
             package's full import path (e.g. \"github.com/pkg/errors\", not just \"errors\" \
             unless it really is the standard library one) and that it's actually a \
             dependency of that project",
            repo_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ensure_go_available() -> Result<(), String> {
    Command::new("go")
        .arg("version")
        .output()
        .map(|_| ())
        .map_err(|_| "`go` isn't available — this tool needs it to resolve and document Go module dependencies".to_string())
}
