use std::path::{Path, PathBuf};
use std::process::Command;

/// Real Python introspection (`import` the package, then `inspect`), not source
/// parsing — this is how `pydoc`/Sphinx's `autodoc` do it too, and it's what makes
/// signatures accurate even when the real callable is behind a decorator or otherwise
/// not what a naive read of the `.py` source would show. `repo_path`'s own virtualenv
/// (or Poetry's, asked for rather than guessed at) is used when there is one, so the
/// resolved package is the one that project actually depends on.
///
/// Best-effort, not exhaustive: importing a package runs its module-level code, exactly
/// like any other Python import would — this is not safe against a genuinely malicious
/// package, same caveat as running `pip install` on one in the first place. A submodule
/// that fails to import (an optional extra not installed) is skipped rather than failing
/// the whole call, and the walk is capped at a few hundred modules so a huge package
/// can't run away.
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    let python = resolve_interpreter(repo_path);

    let output = Command::new(&python)
        .arg("-c")
        .arg(INTROSPECT_SCRIPT)
        .env("CI_DEP", dependency)
        .output()
        .map_err(|e| format!("couldn't run '{}': {e}", python.display()))?;

    if !output.status.success() {
        return Err(format!(
            "couldn't introspect '{dependency}' with '{}': {}",
            python.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Prefers `repo_path`'s own virtualenv over the system `python3` — this is what makes
/// the resolved package the project's actual dependency rather than whatever (if
/// anything) happens to be installed globally. Tries the common local venv directory
/// names first, then asks Poetry for its own managed venv (which lives in a per-project
/// cache directory elsewhere on disk, not under `repo_path` at all, so it can't be found
/// by just looking in a fixed subfolder) before giving up and falling back to whatever
/// `python3` is on `PATH`.
fn resolve_interpreter(repo_path: &Path) -> PathBuf {
    for venv_dir in [".venv", "venv", "env"] {
        let candidate = repo_path.join(venv_dir).join("bin").join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }

    if repo_path.join("pyproject.toml").is_file() {
        if let Ok(output) = Command::new("poetry").args(["env", "info", "--path"]).current_dir(repo_path).output() {
            if output.status.success() {
                let venv_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !venv_path.is_empty() {
                    return PathBuf::from(venv_path).join("bin").join("python3");
                }
            }
        }
    }

    PathBuf::from("python3")
}

/// Arguments come in only via the `CI_DEP` environment variable, never interpolated into
/// this script's text, so a package name can't break out of it.
const INTROSPECT_SCRIPT: &str = r###"
import importlib, inspect, os, pkgutil, sys

dep = os.environ["CI_DEP"]
MAX_MODULES = 300

def doc_summary(obj):
    doc = inspect.getdoc(obj)
    if not doc:
        return None
    return doc.strip().split("\n\n")[0].split("\n")[0].strip()

def render_signature(obj):
    try:
        return str(inspect.signature(obj))
    except (ValueError, TypeError):
        return "(...)"

def is_public(name):
    return not name.startswith("_")

def render_module(mod, seen_ids):
    lines = []
    names = getattr(mod, "__all__", None)
    if names is None:
        names = [n for n in vars(mod) if is_public(n)]

    for name in sorted(names):
        try:
            obj = getattr(mod, name)
        except AttributeError:
            continue
        if id(obj) in seen_ids:
            continue

        if inspect.isclass(obj):
            seen_ids.add(id(obj))
            bases = ", ".join(b.__name__ for b in obj.__bases__ if b is not object)
            summary = doc_summary(obj)
            if summary:
                lines.append(f"# {summary}")
            lines.append(f"class {name}({bases}):" if bases else f"class {name}:")
            members = inspect.getmembers(obj, predicate=lambda m: inspect.isfunction(m) or inspect.ismethod(m))
            wrote_any = False
            for mname, member in members:
                if not is_public(mname):
                    continue
                wrote_any = True
                lines.append(f"    def {mname}{render_signature(member)}")
            if not wrote_any:
                lines.append("    pass")
            lines.append("")
        elif inspect.isfunction(obj) or inspect.isbuiltin(obj):
            seen_ids.add(id(obj))
            summary = doc_summary(obj)
            if summary:
                lines.append(f"# {summary}")
            lines.append(f"def {name}{render_signature(obj)}")
            lines.append("")
        elif inspect.ismodule(obj):
            continue  # submodules are walked separately below
        else:
            type_name = type(obj).__name__
            value = repr(obj) if isinstance(obj, (int, float, bool, str, tuple)) and len(repr(obj)) < 80 else f"<{type_name}>"
            lines.append(f"{name}: {type_name} = {value}")

    return "\n".join(lines)

try:
    root = importlib.import_module(dep)
except Exception as e:
    print(f"couldn't import '{dep}': {e}", file=sys.stderr)
    sys.exit(1)

seen_ids = set()
print(f"# {dep} {getattr(root, '__version__', '')}".rstrip())
print()
print(render_module(root, seen_ids))

if hasattr(root, "__path__"):
    count = 0
    for finder, mod_name, is_pkg in pkgutil.walk_packages(root.__path__, prefix=dep + "."):
        if count >= MAX_MODULES:
            print(f"\n[... stopped after {MAX_MODULES} submodules, the package has more ...]")
            break
        # Skip private/internal submodules (leading underscore in any path segment) —
        # never part of the public API by Python convention.
        if any(part.startswith("_") for part in mod_name.split(".")):
            continue
        try:
            sub = importlib.import_module(mod_name)
        except Exception:
            continue
        count += 1
        body = render_module(sub, seen_ids)
        if body.strip():
            print(f"## {mod_name}\n")
            print(body)
"###;
