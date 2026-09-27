use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use syn::{Fields, Item, Visibility};

/// Renders `dependency`'s whole public-ish surface (real definitions plus raw `pub use`
/// re-export lines) as trimmed, typed Rust signatures — real parameter/return types and
/// doc summaries, no function bodies. `repo_path` only has to be *some* crate that
/// depends on it (directly or transitively); `cargo metadata` run there is what resolves
/// where cargo actually put that dependency's source, wherever `CARGO_HOME`/the registry
/// cache happens to live, so nothing here hardcodes a path like `~/.cargo`.
///
/// Best-effort, not exhaustive — see the doc comments below on the two spots this can
/// miss real content: a macro invocation whose body isn't a plain item list, and `#[cfg]`
/// gates, which aren't evaluated at all (so mutually exclusive feature branches can both
/// show up, never neither).
pub fn get_dependency_signatures(repo_path: &Path, dependency: &str) -> Result<String, String> {
    let manifest_path = repo_path.join("Cargo.toml");

    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&manifest_path)
        .output()
        .map_err(|e| format!("couldn't run `cargo metadata` in '{}': {e}", repo_path.display()))?;

    if !output.status.success() {
        return Err(format!(
            "`cargo metadata` failed for '{}': {}",
            repo_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let metadata: Metadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("couldn't parse `cargo metadata` output: {e}"))?;

    let package = select_package(&metadata, dependency, repo_path)?;

    let lib_target = package
        .targets
        .iter()
        .find(|t| t.kind.iter().any(|k| k == "lib" || k == "proc-macro"))
        .ok_or_else(|| format!("'{dependency}' has no library target to read signatures from"))?;

    let entry_path = PathBuf::from(&lib_target.src_path);
    let mut out = format!("# {dependency} {} (source: {})\n\n", package.version, entry_path.display());
    out.push_str(&walk_crate(&entry_path));
    Ok(out)
}

/// A crate can resolve to more than one version at once (a direct `reqwest = "0.13"` in
/// `Cargo.toml` alongside an older `0.12` pulled in transitively by some other
/// dependency) — picking `packages` order's first match is arbitrary and was observed to
/// silently return the wrong one. Prefers the version actually listed as a direct
/// dependency of the root package (what "the project's own reqwest" means in practice);
/// falls back to the highest version among every match when it's only ever transitive,
/// rather than guessing at whichever the cache happened to list first.
fn select_package<'a>(metadata: &'a Metadata, dependency: &str, repo_path: &Path) -> Result<&'a Package, String> {
    let matches: Vec<&Package> = metadata.packages.iter().filter(|p| p.name == dependency).collect();

    if matches.is_empty() {
        return Err(format!(
            "'{dependency}' isn't in the resolved dependency graph for '{}' — check it's actually \
             a (possibly transitive) dependency of that project and the name is spelled exactly as \
             in Cargo.toml",
            repo_path.display()
        ));
    }

    if let Some(resolve) = &metadata.resolve {
        if let Some(root_id) = &resolve.root {
            if let Some(root_node) = resolve.nodes.iter().find(|n| &n.id == root_id) {
                let direct = root_node
                    .deps
                    .iter()
                    .find_map(|d| matches.iter().find(|p| p.id == d.pkg).copied());
                if let Some(direct) = direct {
                    return Ok(direct);
                }
            }
        }
    }

    // Transitive-only, or `resolve` wasn't present for some reason — highest version
    // wins, since that's the one most likely to reflect what the dependency graph
    // actually settled on using in practice.
    matches
        .into_iter()
        .max_by(|a, b| compare_versions(&a.version, &b.version))
        .ok_or_else(|| format!("'{dependency}' resolved to no usable version"))
}

/// Best-effort numeric `major.minor.patch` comparison — good enough to pick "the newer
/// one" between two real crate versions without pulling in a full semver dependency just
/// for this.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct Resolve {
    root: Option<String>,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    kind: Vec<String>,
    src_path: String,
}

/// Walks the crate's whole module tree starting from its lib entry point, following
/// every `mod foo;` declaration to its file (`foo.rs` or `foo/mod.rs`) regardless of
/// that mod's own visibility — a lot of real crates (reqwest, tokio) keep their actual
/// definitions in private modules and expose them only via `pub use` re-exports at a
/// shallower level, so skipping private mods would silently drop most of the public API.
/// Doesn't yet handle `#[path = "..."]` overrides.
fn walk_crate(entry_path: &Path) -> String {
    let mut out = String::new();
    let mut queue: VecDeque<(PathBuf, String)> = VecDeque::new();
    queue.push_back((entry_path.to_path_buf(), String::new()));

    while let Some((file_path, mod_prefix)) = queue.pop_front() {
        let Ok(src) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };

        print_module(&mod_prefix, &parsed.items, &file_path, &mut queue, &mut out);
    }

    out
}

fn print_module(mod_prefix: &str, items: &[Item], file_path: &Path, queue: &mut VecDeque<(PathBuf, String)>, out: &mut String) {
    let mut body = String::new();

    for item in items {
        match item {
            Item::Mod(m) => {
                let child_prefix = if mod_prefix.is_empty() { m.ident.to_string() } else { format!("{mod_prefix}::{}", m.ident) };
                match &m.content {
                    // `mod foo { ... }` inline — recurse right here, no file to resolve.
                    Some((_, inline_items)) => print_module(&child_prefix, inline_items, file_path, queue, out),
                    // `mod foo;` — resolve to its file and handle it in the main queue loop.
                    None => {
                        if let Some(child_path) = resolve_submodule(file_path, &m.ident.to_string()) {
                            queue.push_back((child_path, child_prefix));
                        }
                    }
                }
            }
            Item::Use(u) if is_pub(&u.vis) => {
                let mut u = u.clone();
                u.attrs.clear();
                body.push_str(&unparse_item(Item::Use(u)));
            }
            // Best-effort: some crates (reqwest's `if_hyper!{ mod client; pub use ...; }`,
            // cfg_if-style patterns) put their real `mod`/`pub use`/item declarations inside
            // a macro invocation instead of writing them directly — `syn` doesn't expand
            // macros, but plenty of these are just "paste these items verbatim" wrappers, so
            // reparsing the macro's own token stream as a plain item list recovers them when
            // that's the case. Anything that isn't gets silently skipped, same as before.
            Item::Macro(m) => {
                if let Ok(inner) = syn::parse2::<syn::File>(m.mac.tokens.clone()) {
                    print_module(mod_prefix, &inner.items, file_path, queue, out);
                }
            }
            _ => {
                if let Some(rendered) = render_item(item) {
                    body.push_str(&rendered);
                    body.push('\n');
                }
            }
        }
    }

    if !body.is_empty() {
        if mod_prefix.is_empty() {
            out.push_str(&body);
        } else {
            out.push_str(&format!("## {mod_prefix}\n\n{body}"));
        }
    }
}

/// `foo.rs`'s submodule `bar` lives at either `foo/bar.rs` or `foo/bar/mod.rs` — except
/// for `lib.rs`/`mod.rs` themselves, whose submodules are siblings in the same directory.
fn resolve_submodule(parent_file: &Path, name: &str) -> Option<PathBuf> {
    let parent_dir = parent_file.parent()?;
    let file_stem = parent_file.file_stem()?.to_str()?;
    let base_dir = if file_stem == "lib" || file_stem == "mod" || file_stem == "main" {
        parent_dir.to_path_buf()
    } else {
        parent_dir.join(file_stem)
    };

    let flat = base_dir.join(format!("{name}.rs"));
    if flat.exists() {
        return Some(flat);
    }
    let nested = base_dir.join(name).join("mod.rs");
    if nested.exists() {
        return Some(nested);
    }
    None
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    return Some(s.value().trim().to_string());
                }
            }
        }
    }
    None
}

/// Pretty-prints a single item as real, rustfmt-shaped Rust source (via `prettyplease`)
/// rather than hand-assembling a one-line stub — this is what gets full parameter/return
/// types, generics and where-clauses onto the page for free, instead of a bare name.
fn unparse_item(item: Item) -> String {
    let file = syn::File { shebang: None, attrs: vec![], items: vec![item] };
    prettyplease::unparse(&file)
}

/// Renders a bare type the same properly formatted way `unparse_item` renders a whole
/// item (`Cookie<'a>`, not `to_token_stream()`'s raw `Cookie < 'a >`) — done by wrapping
/// it in a throwaway type alias and unparsing that, then trimming the wrapper back off,
/// since `prettyplease` only knows how to format whole items, not a bare `syn::Type`.
fn render_type(ty: &syn::Type) -> String {
    let alias: Item = syn::parse_quote!(type __Rendered = #ty;);
    let text = unparse_item(alias);
    text.trim().strip_prefix("type __Rendered = ").and_then(|s| s.strip_suffix(';')).unwrap_or(&text).to_string()
}

/// Doc comments are `#[doc = "..."]` attributes under the hood — `push_doc` already
/// prints a condensed one-line summary, so the full block gets stripped here before
/// pretty-printing to avoid showing the same documentation twice.
fn strip_doc_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|a| !a.path().is_ident("doc"));
}

/// A function body isn't part of its signature and can be arbitrarily large, so it's
/// blanked out before pretty-printing and the resulting `{}` is swapped for a `;` —
/// simplest way to reuse `prettyplease`'s real formatter without teaching it to omit
/// bodies itself.
fn signature_only(mut item: Item) -> String {
    if let Item::Fn(f) = &mut item {
        strip_doc_attrs(&mut f.attrs);
        f.block = Box::new(syn::parse_quote!({}));
    }
    let text = unparse_item(item);
    match text.trim_end().strip_suffix("{}") {
        Some(head) => format!("{};\n", head.trim_end()),
        None => text,
    }
}

fn render_item(item: &Item) -> Option<String> {
    let mut out = String::new();
    match item {
        Item::Fn(f) if is_pub(&f.vis) => {
            push_doc(&mut out, &f.attrs);
            out.push_str(&signature_only(item.clone()));
        }
        Item::Struct(s) if is_pub(&s.vis) => {
            push_doc(&mut out, &s.attrs);
            let mut s = s.clone();
            strip_doc_attrs(&mut s.attrs);
            filter_pub_fields(&mut s.fields);
            out.push_str(&unparse_item(Item::Struct(s)));
        }
        Item::Enum(e) if is_pub(&e.vis) => {
            push_doc(&mut out, &e.attrs);
            let mut e = e.clone();
            strip_doc_attrs(&mut e.attrs);
            out.push_str(&unparse_item(Item::Enum(e)));
        }
        Item::Trait(t) if is_pub(&t.vis) => {
            push_doc(&mut out, &t.attrs);
            let mut t = t.clone();
            strip_doc_attrs(&mut t.attrs);
            for ti in &mut t.items {
                if let syn::TraitItem::Fn(f) = ti {
                    f.default = None;
                    strip_doc_attrs(&mut f.attrs);
                }
            }
            out.push_str(&unparse_item(Item::Trait(t)));
        }
        Item::Const(c) if is_pub(&c.vis) => {
            push_doc(&mut out, &c.attrs);
            let mut c = c.clone();
            strip_doc_attrs(&mut c.attrs);
            out.push_str(&unparse_item(Item::Const(c)));
        }
        Item::Type(t) if is_pub(&t.vis) => {
            push_doc(&mut out, &t.attrs);
            let mut t = t.clone();
            strip_doc_attrs(&mut t.attrs);
            out.push_str(&unparse_item(Item::Type(t)));
        }
        Item::Impl(im) => {
            let self_ty = render_type(&im.self_ty);
            let mut methods = String::new();
            for ii in &im.items {
                let syn::ImplItem::Fn(f) = ii else { continue };
                if !is_pub(&f.vis) {
                    continue;
                }
                let synthetic = Item::Fn(syn::ItemFn {
                    attrs: vec![],
                    vis: f.vis.clone(),
                    sig: f.sig.clone(),
                    block: Box::new(syn::parse_quote!({})),
                });
                let rendered = signature_only(synthetic);
                for line in rendered.lines() {
                    methods.push_str("    ");
                    methods.push_str(line);
                    methods.push('\n');
                }
            }
            if !methods.is_empty() {
                out.push_str(&format!("impl {self_ty} {{\n{methods}}}\n"));
            }
        }
        _ => return None,
    }
    if out.is_empty() { None } else { Some(out) }
}

fn push_doc(out: &mut String, attrs: &[syn::Attribute]) {
    if let Some(doc) = doc_comment(attrs) {
        out.push_str("// ");
        out.push_str(&doc);
        out.push('\n');
    }
}

/// Keeps only publicly visible fields — a struct's private fields aren't part of what a
/// dependent crate can actually use, so they're just noise for this purpose.
fn filter_pub_fields(fields: &mut Fields) {
    match fields {
        Fields::Named(named) => {
            named.named = named.named.iter().filter(|f| is_pub(&f.vis)).cloned().collect();
        }
        Fields::Unnamed(unnamed) => {
            unnamed.unnamed = unnamed.unnamed.iter().filter(|f| is_pub(&f.vis)).cloned().collect();
        }
        Fields::Unit => {}
    }
}
