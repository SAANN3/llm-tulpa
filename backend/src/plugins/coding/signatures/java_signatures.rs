use std::path::Path;
use std::process::Command;

/// Resolves a Maven dependency's `.jar` via Maven's own `dependency:build-classpath`
/// (never a hardcoded `~/.m2` path — respects wherever the local repository actually
/// is), then reads its real public API straight from the compiled bytecode with `javap`
/// — the JVM's own decompiler-to-signatures tool, so what comes back is exactly what the
/// class file actually exports, generics and all, not a guess from source. Also what
/// `kotlin_signatures` reuses wholesale: a `.jar`'s bytecode looks the same to `javap`
/// regardless of whether `javac` or `kotlinc` produced it.
///
/// Gradle projects aren't supported yet — resolving a Gradle-built classpath without a
/// project-specific task defined for it isn't something a generic tool invocation can do
/// the way `mvn dependency:build-classpath` can for Maven, so this errors clearly instead
/// of guessing at a `~/.gradle/caches` layout.
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    if !repo_path.join("pom.xml").is_file() {
        return Err(format!(
            "'{}' has no pom.xml — java support needs a Maven project for now (Gradle isn't \
             supported yet)",
            repo_path.display()
        ));
    }

    let classpath = build_classpath(repo_path)?;
    let jar_path = classpath
        .iter()
        .find(|p| {
            let stem = Path::new(p).file_stem().and_then(|s| s.to_str()).unwrap_or("");
            stem == dependency || stem.starts_with(&format!("{dependency}-"))
        })
        .ok_or_else(|| {
            format!(
                "'{dependency}' isn't on the resolved Maven classpath for '{}' — check it's \
                 actually a dependency (its artifactId, exactly) and that `mvn` has resolved it \
                 at least once",
                repo_path.display()
            )
        })?;

    let classes = list_top_level_classes(jar_path)?;
    let mut out = format!("# {dependency} (jar: {jar_path})\n\n");

    const MAX_CLASSES: usize = 150;
    for (i, class_name) in classes.iter().enumerate() {
        if i >= MAX_CLASSES {
            out.push_str(&format!("\n[... stopped after {MAX_CLASSES} classes, the jar has more ...]\n"));
            break;
        }
        if let Ok(text) = javap(jar_path, class_name) {
            out.push_str(&text);
            out.push('\n');
        }
        // A class javap refuses (package-private, synthetic, anonymous) is just skipped —
        // not every `.class` file in a jar is meant to be part of the public API.
    }

    Ok(out)
}

fn build_classpath(repo_path: &Path) -> Result<Vec<String>, String> {
    let out_file = std::env::temp_dir().join(format!("code-instruments-cp-{}.txt", std::process::id()));

    let output = Command::new("mvn")
        .args(["-q", "dependency:build-classpath"])
        .arg(format!("-Dmdep.outputFile={}", out_file.display()))
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("couldn't run `mvn` in '{}': {e}", repo_path.display()))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&out_file);
        return Err(format!(
            "`mvn dependency:build-classpath` failed in '{}': {}",
            repo_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let content = std::fs::read_to_string(&out_file).map_err(|e| format!("couldn't read maven's classpath output: {e}"));
    let _ = std::fs::remove_file(&out_file);
    let content = content?;

    let sep = if cfg!(windows) { ';' } else { ':' };
    Ok(content.trim().split(sep).map(str::to_string).filter(|s| !s.is_empty()).collect())
}

/// Every top-level (no `$`, i.e. not an inner/anonymous/synthetic class) `.class` entry
/// in the jar, as a fully qualified name — via `unzip -l` rather than a zip-reading
/// dependency, since nothing else in this codebase needs one yet.
fn list_top_level_classes(jar_path: &str) -> Result<Vec<String>, String> {
    let output = Command::new("unzip").args(["-l", jar_path]).output().map_err(|e| format!("couldn't run `unzip`: {e}"))?;
    if !output.status.success() {
        return Err(format!("couldn't list '{jar_path}': {}", String::from_utf8_lossy(&output.stderr).trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut classes: Vec<String> = text
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .filter(|path| path.ends_with(".class") && !path.contains('$'))
        .map(|path| path.trim_end_matches(".class").replace('/', "."))
        // `package-info.class` is a compiler-generated placeholder for package-level
        // Javadoc/annotations, not a real type — `javap` renders it as a nonsensical
        // `interface ...package-info {}`, pure noise for this purpose.
        .filter(|name| !name.ends_with(".package-info"))
        .collect();
    classes.sort();
    classes.dedup();
    Ok(classes)
}

fn javap(jar_path: &str, class_name: &str) -> Result<String, String> {
    let output = Command::new("javap")
        .args(["-public", "-classpath", jar_path, class_name])
        .output()
        .map_err(|e| format!("couldn't run `javap`: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
