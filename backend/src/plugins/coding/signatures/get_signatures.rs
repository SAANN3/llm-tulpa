use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams,
    ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{check_directory_scope, normalize};

use super::{c_signatures, csharp_signatures, go_signatures, java_signatures, node_signatures, python_signatures, rust_signatures};

/// Hard ceiling on how much signature text comes back in one call — the same cap
/// `storage.read_file` uses, for the same reason: a big crate (a couple hundred files)
/// can produce far more text than is worth spending the model's context budget on in one
/// go. Cut short with a note rather than silently sent in full.
const MAX_OUTPUT_CHARS: usize = 40_000;

#[derive(Deserialize, ToolParams)]
struct GetSignaturesArgs {
    #[tool(description = "Which language/ecosystem the dependency belongs to. One of \"rust\", \"javascript\"/\"typescript\" (treated identically — both resolve through npm/yarn/pnpm and read the package's .d.ts type declarations), \"python\", \"go\", \"c\"/\"c++\" (treated identically — see `dependency`'s own description), \"java\", \"kotlin\" (Maven projects only, reuses the same javap-based path as java), or \"csharp\"/\"c#\". Any other value fails with a clear error instead of guessing.")]
    language: String,
    #[tool(description = "Path to a local project that has `dependency` as one of its own (possibly transitive) dependencies — its manifest is what's used to resolve exactly where that dependency's source/types/bytecode actually live on disk: Cargo.toml (rust), package.json (javascript/typescript), any project with dependencies already installed by pip/poetry into a virtualenv (python), go.mod (go), pom.xml (java/kotlin — Gradle isn't supported yet). For c/c++, or for csharp (which resolves via the global NuGet cache instead), this can be any directory — it's used as one of several places searched, not the sole source of truth.")]
    repo_path: String,
    #[tool(description = "The dependency's package name, exactly as it appears in the project's manifest — e.g. \"reqwest\"/\"tokio\" (rust), \"react\"/\"@types/node\" (javascript/typescript), an importable module name (python), a full import path like \"github.com/pkg/errors\" (go), a Maven artifactId (java/kotlin), a NuGet package id (csharp). For c/c++ this is the header's own name instead — a bare name tried as \"<name>.h\"/\"<name>/<name>.h\", or a relative path like \"curl/curl.h\" when that doesn't match.")]
    dependency: String,
    #[tool(description = "How many characters to skip from the start of the signature output before reading — to read past where an earlier call was cut off, pass the `next_offset` it returned. Defaults to 0 (start of the output). Note this doesn't re-run the extraction with a narrower scope, just paginates through the same full output.")]
    offset: Option<usize>,
}

pub struct GetSignaturesTool;

#[async_trait]
impl Tool for GetSignaturesTool {
    fn function_name(&self) -> &str {
        "coding.get_signatures"
    }

    fn description(&self) -> &str {
        "Gives a trimmed, typed view of a dependency's public API — struct/enum/trait/\
         interface definitions, function and method signatures with real parameter and \
         return types, and doc summaries — without pulling its full source into context. \
         Prefer this over storage.read_file/find_files on a dependency's own source tree \
         (e.g. under a package cache or node_modules directory) when the goal is just to \
         see what it exposes: this is far smaller and already trimmed to what's callable. \
         For rust the output's first line names the exact resolved version (the project's \
         own dependency graph can pin more than one version of the same crate at once). \
         Best-effort, not exhaustive: \
         for rust — declarations hidden inside a code-generating macro (rather than a \
         plain `mod`/`pub use`) can be missed, `#[cfg(...)]` feature gates aren't \
         evaluated (so mutually exclusive branches can both show up), and a type \
         re-exported from a different crate only shows the re-export line, not that \
         other crate's definition; \
         for javascript/typescript — a package with no shipped `.d.ts` and no matching \
         `@types/*` package has nothing to show, Yarn PnP setups (no real node_modules \
         directory) aren't resolved, and only the common shapes of a package's `exports` \
         map are read, not every condition combination; \
         for python — importing the dependency runs its module-level code, exactly like \
         any other Python import would, and a submodule that fails to import (an optional \
         extra not installed) is silently skipped; \
         for go — none known, `go doc -all` is the language's own tool doing the real work; \
         for c/c++ — no preprocessing happens at all (no macro expansion, no `#ifdef` \
         evaluation), a header found via a heuristic search (pkg-config, common local \
         dependency folders, then the compiler's own system include path) rather than a \
         real build system, and only local `#include \"...\"` is followed, never a system \
         `#include <...>`; \
         for java — Gradle projects aren't supported yet, only Maven (a pom.xml); \
         for kotlin — resolved through the exact same Maven+javap path as java (a compiled \
         Kotlin class reads identically to a Java one), so it's Maven-only too, and \
         Kotlin's compiler-generated synthetic members/classes (`access$...`, internal \
         debug markers) can show up alongside the real public API; \
         for csharp — needs the package already restored at least once by the real \
         project, a property's accessors are collapsed into `Type Name { get; set; }` but \
         other compiler-generated members aren't filtered, and generic constraints/\
         nullable-reference annotations aren't reproduced, only parameter/return types. \
         Verified hands-on against a real dependency for every language above except \
         kotlin (verified only that a Kotlin-compiled jar reads correctly through the same \
         java/javap path, not an actual Kotlin-toolchain-built one end to end). \
         Output longer than 40,000 characters comes back truncated (see `truncated` in \
         the response) — pass the response's `next_offset` as `offset` to keep reading, \
         same as storage.read_file; there's no way to narrow the extraction itself to \
         just one export yet, only to page through all of it."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetSignaturesArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: GetSignaturesArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(&args.repo_path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: GetSignaturesArgs = serde_json::from_value(data)?;
        let repo_path = normalize(std::path::Path::new(&args.repo_path));

        // Shells out to the language's own tooling (`cargo metadata`, `node`) and does
        // real parsing/file-walking work — kept off the async runtime the same way any
        // other blocking work here is.
        let language = args.language.to_lowercase();
        let result = tokio::task::spawn_blocking(move || match language.as_str() {
            "rust" => rust_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            "javascript" | "typescript" | "js" | "ts" | "node" => {
                node_signatures::get_dependency_signatures(&repo_path, &args.dependency)
            }
            "python" | "py" => python_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            "go" | "golang" => go_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            "c" | "c++" | "cpp" | "cxx" => c_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            // Kotlin compiles to the same JVM bytecode `javap` reads regardless of source
            // language — a Kotlin dependency on Maven resolves through the identical path
            // as a Java one, nothing Kotlin-specific needed.
            "java" | "kotlin" | "kt" => java_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            "csharp" | "c#" | "cs" => csharp_signatures::get_dependency_signatures(&repo_path, &args.dependency),
            other => Err(format!(
                "language '{other}' isn't supported yet — supported: rust, javascript/typescript, python, go, c/c++, \
                 java/kotlin, csharp"
            )),
        })
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("signature extraction task panicked: {e}")))?;

        let content = result.map_err(ToolError::FailedUnknown)?;
        Ok(paginate(&content, args.offset.unwrap_or(0)))
    }
}

/// Pages through `content` `MAX_OUTPUT_CHARS` at a time — same convention as
/// `storage.read_file`'s own `offset`/`next_offset` pair, factored out here so it can be
/// exercised directly without a full `ToolContext`.
fn paginate(content: &str, offset: usize) -> Value {
    let total_chars = content.chars().count();

    if offset >= total_chars && total_chars > 0 {
        return serde_json::json!({
            "content": format!("[offset {offset} is past the end of the output, which is {total_chars} characters long]"),
            "truncated": false,
            "total_chars": total_chars,
            "next_offset": null,
        });
    }

    let window: String = content.chars().skip(offset).take(MAX_OUTPUT_CHARS).collect();
    let end = offset + window.chars().count();
    let truncated = end < total_chars;

    let content = if truncated {
        format!(
            "{window}\n\n[... output truncated: showing characters {offset}-{end} of {total_chars}; \
             call again with offset={end} to read on ...]"
        )
    } else {
        window
    };

    serde_json::json!({
        "content": content,
        "truncated": truncated,
        "total_chars": total_chars,
        "next_offset": truncated.then_some(end),
    })
}
