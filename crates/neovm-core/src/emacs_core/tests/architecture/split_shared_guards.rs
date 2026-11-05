//! Shared prep guards owned by memq's Prep E guard set (peer PC10/R6/R7).
//!
//! Four guards, each with an in-file mutation proof:
//!
//! 1. **N13 carried pairs**: every `(placeholder, swap)` pair named by
//!    `CARRIERS` may be referenced only from `eval/green/carried.rs`, its own
//!    definition line, a resolved `use` re-export, or a same-named defining
//!    function's qualified delegation into its actual child module.  The pair
//!    list is derived from the table, so a later pair joins automatically.
//! 2. **PC1 fenced hot-path methods**: rows in `pc1_hot_path_methods.tsv`
//!    list `(type, method, protected caller)`.  No workspace crate other than
//!    neovm-core and neovm-value may call a listed method.
//! 3. **PC2 hook installers**: rows in `pc2_hook_seams.tsv` list
//!    `(kind, static, installer, adapter, exact seams)`; callback installers
//!    and adapters form exact signed chains, including SQLite handle creation.
//!    Test-only callers remain typed; DATA installation has one exact caller.
//!    Completeness follows writes of statics owned by the sealed lower crates.
//! 4. **TA2 authority fence**: `test_support::authority` may be named only
//!    from `crates/neovm-core/core_tests/**`, never from `src/**`, including
//!    aliases and glob imports, across every workspace member and target.
//!    Only the actual value-layer authority definition is exempt.
//!
//! All scans are fail-closed: unparseable sources, missing tables, malformed
//! rows and any occurrence not accounted for by an allowed site fail.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::workspace_root as shared_workspace_root;
use syn::{Expr, Item, UseTree};

const PC1_TABLE: &str = include_str!("pc1_hot_path_methods.tsv");
const PC2_TABLE: &str = include_str!("pc2_hook_seams.tsv");

fn workspace_root() -> PathBuf {
    shared_workspace_root()
}

// ---------------------------------------------------------------------------
// Source text: blank comments and literal contents, preserving line numbers.
// ---------------------------------------------------------------------------

fn skip_normal_string(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_raw_string(bytes: &[u8], prefix_len: usize) -> Option<usize> {
    let mut i = prefix_len;
    let mut hashes = 0;
    while i < bytes.len() && bytes[i] == b'#' {
        hashes += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut seen = 0;
            while j < bytes.len() && bytes[j] == b'#' && seen < hashes {
                seen += 1;
                j += 1;
            }
            if seen == hashes {
                return Some(j);
            }
        }
        i += 1;
    }
    Some(bytes.len())
}

fn skip_char_or_lifetime(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    if j >= bytes.len() {
        return i + 1;
    }
    if bytes[j] == b'\\' {
        j += 2;
        while j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        return if j < bytes.len() && bytes[j] == b'\'' {
            j + 1
        } else {
            i + 1
        };
    }
    j += 1;
    while j < bytes.len() && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'\'' {
        j + 1
    } else {
        i + 1
    }
}

fn skip_literal(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let raw_prefix = match b {
        b'r' => Some(1),
        b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
        _ => None,
    };
    if let Some(plen) = raw_prefix {
        if let Some(end) = skip_raw_string(bytes, i + plen) {
            return end;
        }
    }
    if b == b'"' {
        return skip_normal_string(bytes, i);
    }
    if b == b'\'' {
        return skip_char_or_lifetime(bytes, i);
    }
    i + 1
}

/// Comments and literal contents become spaces; newlines stay so every later
/// line number matches the original file.
fn cleaned_code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let push_blank = |out: &mut String, start: usize, end: usize| {
        for &b in &bytes[start..end] {
            out.push(if b == b'\n' { '\n' } else { ' ' });
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            push_blank(&mut out, start, i);
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start = i;
            let mut depth = 1usize;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            push_blank(&mut out, start, i);
            continue;
        }
        let raw_prefix = match b {
            b'r' => Some(1),
            b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
            _ => None,
        };
        if let Some(plen) = raw_prefix {
            if let Some(end) = skip_raw_string(bytes, i + plen) {
                push_blank(&mut out, i, end);
                i = end;
                continue;
            }
        }
        if b == b'"' {
            let end = skip_normal_string(bytes, i);
            push_blank(&mut out, i, end);
            i = end;
            continue;
        }
        if b == b'\'' {
            let end = skip_char_or_lifetime(bytes, i);
            push_blank(&mut out, i, end);
            i = end;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn line_of(span: Span) -> usize {
    span.start().line
}

fn parse_file(path: &Path) -> Result<(String, syn::File), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let file = syn::parse_file(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((text, file))
}

// One workspace/target discovery path shared by every split source fence.
#[derive(Clone, Debug)]
struct GuardSource {
    package: String,
    member_root: PathBuf,
    path: PathBuf,
}

fn guard_read_manifest(path: &Path) -> Result<toml::Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("SPLIT_DISCOVERY_READ {}: {error}", path.display()))?;
    toml::from_str(&text)
        .map_err(|error| format!("SPLIT_DISCOVERY_PARSE {}: {error}", path.display()))
}

fn guard_optional_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "SPLIT_DISCOVERY_METADATA {}: {error}",
            path.display()
        )),
    }
}

fn guard_walk_sources(
    path: &Path,
    ancestors: &mut BTreeSet<PathBuf>,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("SPLIT_DISCOVERY_METADATA {}: {error}", path.display()))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("SPLIT_DISCOVERY_CANONICAL {}: {error}", path.display()))?;
    if metadata.is_dir() {
        if !ancestors.insert(canonical.clone()) {
            return Err(format!(
                "SPLIT_DISCOVERY_DIRECTORY_CYCLE {}",
                path.display()
            ));
        }
        let entries = std::fs::read_dir(path)
            .map_err(|error| format!("SPLIT_DISCOVERY_DIRECTORY {}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("SPLIT_DISCOVERY_DIRECTORY {}: {error}", path.display())
            })?;
            guard_walk_sources(&entry.path(), ancestors, files)?;
        }
        ancestors.remove(&canonical);
    } else if metadata.is_file() {
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.insert(canonical);
        }
    } else {
        return Err(format!("SPLIT_DISCOVERY_SOURCE_KIND {}", path.display()));
    }
    Ok(())
}

fn discover_sources(workspace: &Path) -> Result<Vec<GuardSource>, String> {
    let root = guard_read_manifest(&workspace.join("Cargo.toml"))?;
    let members = root
        .get("workspace")
        .and_then(|table| table.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "SPLIT_DISCOVERY_MEMBERS: missing workspace member array".to_owned())?;
    if members.is_empty() {
        return Err("SPLIT_DISCOVERY_MEMBERS: empty workspace".to_owned());
    }
    let mut packages = BTreeSet::new();
    let mut result = Vec::new();
    for member in members {
        let member = member
            .as_str()
            .ok_or_else(|| "SPLIT_DISCOVERY_MEMBERS: member is not a string".to_owned())?;
        if member.is_empty() || member.chars().any(|c| matches!(c, '*' | '?' | '[' | ']')) {
            return Err(format!("SPLIT_DISCOVERY_UNKNOWN_MEMBER_PATTERN {member:?}"));
        }
        let member_root = workspace
            .join(member)
            .canonicalize()
            .map_err(|error| format!("SPLIT_DISCOVERY_MEMBER {member}: {error}"))?;
        let manifest = guard_read_manifest(&member_root.join("Cargo.toml"))?;
        let package = manifest
            .get("package")
            .and_then(|table| table.get("name"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("SPLIT_DISCOVERY_PACKAGE {member}"))?
            .to_owned();
        if !packages.insert(package.clone()) {
            return Err(format!("SPLIT_DISCOVERY_DUPLICATE_PACKAGE {package}"));
        }
        let mut files = BTreeSet::new();
        for directory in ["src", "tests", "benches", "examples"] {
            let path = member_root.join(directory);
            if guard_optional_metadata(&path)?.is_some() {
                guard_walk_sources(&path, &mut BTreeSet::new(), &mut files)?;
            }
        }
        // Prep D owns these sources even though declarations stay at their
        // old module paths. They cannot disappear from N13/TA2 discovery.
        if package == "neovm-core" {
            for directory in ["core_tests", "core_test_facades"] {
                let path = member_root.join(directory);
                if guard_optional_metadata(&path)?.is_some() {
                    guard_walk_sources(&path, &mut BTreeSet::new(), &mut files)?;
                }
            }
        }
        let build = member_root.join("build.rs");
        if guard_optional_metadata(&build)?.is_some() {
            guard_walk_sources(&build, &mut BTreeSet::new(), &mut files)?;
        }
        let mut explicit = Vec::new();
        if let Some(lib) = manifest.get("lib") {
            explicit
                .push(lib.as_table().ok_or_else(|| {
                    format!("SPLIT_DISCOVERY_TARGET {package}: lib is not table")
                })?);
        }
        for kind in ["bin", "test", "bench", "example"] {
            if let Some(targets) = manifest.get(kind) {
                let targets = targets.as_array().ok_or_else(|| {
                    format!("SPLIT_DISCOVERY_TARGET {package}: {kind} is not array")
                })?;
                for target in targets {
                    explicit.push(target.as_table().ok_or_else(|| {
                        format!("SPLIT_DISCOVERY_TARGET {package}: {kind} is not table")
                    })?);
                }
            }
        }
        for target in explicit {
            if let Some(path) = target.get("path") {
                let path = path
                    .as_str()
                    .ok_or_else(|| format!("SPLIT_DISCOVERY_TARGET {package}: non-string path"))?;
                guard_walk_sources(&member_root.join(path), &mut BTreeSet::new(), &mut files)?;
            }
        }
        if let Some(path) = manifest.get("package").and_then(|p| p.get("build")) {
            match path {
                toml::Value::String(path) => {
                    guard_walk_sources(&member_root.join(path), &mut BTreeSet::new(), &mut files)?
                }
                toml::Value::Boolean(_) => {}
                _ => return Err(format!("SPLIT_DISCOVERY_BUILD_PATH {package}")),
            }
        }
        if matches!(package.as_str(), "neovm-core" | "neovm-value") {
            let src = member_root
                .join("src")
                .canonicalize()
                .map_err(|error| format!("SPLIT_DISCOVERY_SANITY {package}: {error}"))?;
            if !files.iter().any(|file| file.starts_with(&src)) {
                return Err(format!("SPLIT_DISCOVERY_EMPTY_SOURCE {package}"));
            }
        }
        for path in files {
            result.push(GuardSource {
                package: package.clone(),
                member_root: member_root.clone(),
                path,
            });
        }
    }
    if !packages.contains("neovm-core") || !packages.contains("neovm-value") {
        return Err("SPLIT_DISCOVERY_SANITY: core/value missing".to_owned());
    }
    result.sort_by(|a, b| (&a.package, &a.path).cmp(&(&b.package, &b.path)));
    Ok(result)
}

// Mutation fixture setup, never a second source-discovery implementation.
fn guard_fixture_workspace(root: &Path, members: &[&str]) {
    let entries = members
        .iter()
        .map(|name| format!("\"crates/{name}\""))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers=[{entries}]\n"),
    )
    .expect("workspace fixture");
    for name in members {
        let member = root.join("crates").join(name);
        std::fs::create_dir_all(member.join("src")).expect("fixture source directory");
        std::fs::write(
            member.join("Cargo.toml"),
            format!("[package]\nname=\"{name}\"\nversion=\"0.0.0\"\n"),
        )
        .expect("member fixture");
        if !member.join("src/lib.rs").exists() {
            std::fs::write(member.join("src/lib.rs"), "").expect("fixture lib");
        }
    }
}

#[test]
fn architecture_split_discovery_missing_member_red_then_restored() {
    let dir = tempfile::tempdir().expect("fixture");
    guard_fixture_workspace(dir.path(), &["neovm-core", "neovm-value"]);
    let green = discover_sources(dir.path()).expect("positive discovery");
    assert!(!green.is_empty());
    let member = dir.path().join("crates/neovm-value/Cargo.toml");
    let saved = std::fs::read(&member).expect("save manifest");
    std::fs::remove_file(&member).expect("mutation");
    assert!(
        discover_sources(dir.path())
            .expect_err("missing member must fail")
            .contains("SPLIT_DISCOVERY_READ")
    );
    std::fs::write(member, saved).expect("restore");
    assert_eq!(
        discover_sources(dir.path()).expect("restored").len(),
        green.len()
    );
}

#[test]
fn architecture_split_discovery_includes_explicit_target_and_core_tests() {
    let dir = tempfile::tempdir().expect("fixture");
    guard_fixture_workspace(dir.path(), &["neovm-core", "neovm-value"]);
    let core = dir.path().join("crates/neovm-core");
    std::fs::create_dir_all(core.join("core_tests")).expect("fixture core tests");
    std::fs::write(core.join("core_tests/moved.rs"), "fn moved() {} ").expect("moved test");
    std::fs::write(core.join("custom.rs"), "fn outside_autodiscovery() {} ").expect("target");
    let manifest = core.join("Cargo.toml");
    let mut text = std::fs::read_to_string(&manifest).expect("manifest");
    text.push_str("\n[[test]]\nname='custom'\npath='custom.rs'\n");
    std::fs::write(&manifest, text).expect("target declaration");
    let files = discover_sources(dir.path()).expect("discovery");
    assert!(files.iter().any(|file| file.path.ends_with("custom.rs")));
    assert!(
        files
            .iter()
            .any(|file| file.path.ends_with("core_tests/moved.rs"))
    );
    std::fs::remove_file(core.join("custom.rs")).expect("mutation");
    assert!(
        discover_sources(dir.path())
            .expect_err("declared target missing must fail")
            .contains("SPLIT_DISCOVERY_METADATA")
    );
    std::fs::write(core.join("custom.rs"), "fn outside_autodiscovery() {} ").expect("restore");
    assert!(discover_sources(dir.path()).is_ok());
}

// ---------------------------------------------------------------------------
// N13: source-resolved CARRIERS-derived reference guard (tests included).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CarriedPair {
    members: [(Vec<String>, String); 2],
}

type N13ModuleKey = Vec<String>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct N13FunctionIdentity {
    module: N13ModuleKey,
    name: String,
}

#[derive(Clone)]
struct N13Use {
    path: Vec<String>,
    alias: Option<String>, // None means a glob, never an authority grant.
}

#[derive(Default)]
struct N13Module {
    imports: Vec<N13Use>,
    children: BTreeSet<String>,
    functions: BTreeSet<String>,
}

#[derive(Default)]
struct N13Modules {
    modules: BTreeMap<N13ModuleKey, N13Module>,
    files: BTreeMap<PathBuf, BTreeSet<N13ModuleKey>>,
}

fn path_names(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

fn n13_use_leaves(tree: &UseTree, prefix: &[String], out: &mut Vec<N13Use>) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            n13_use_leaves(&p.tree, &next, out);
        }
        UseTree::Name(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            let alias = path.last().cloned();
            out.push(N13Use { path, alias });
        }
        UseTree::Rename(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            out.push(N13Use {
                path,
                alias: Some(n.rename.to_string()),
            });
        }
        UseTree::Group(g) => {
            for item in &g.items {
                n13_use_leaves(item, prefix, out);
            }
        }
        UseTree::Glob(_) => out.push(N13Use {
            path: prefix.to_vec(),
            alias: None,
        }),
    }
}

fn n13_macro_items(stream: proc_macro2::TokenStream) -> Vec<Item> {
    // cfg_select! branches are groups containing ordinary Rust items. Only
    // source-defined declarations are indexed; arbitrary macro expansion is
    // never guessed. Unmapped relevant references fail later with UNRESOLVED.
    if let Ok(file) = syn::parse2::<syn::File>(stream.clone()) {
        return file.items;
    }
    stream
        .into_iter()
        .flat_map(|token| match token {
            proc_macro2::TokenTree::Group(g) => n13_macro_items(g.stream()),
            _ => Vec::new(),
        })
        .collect()
}

impl N13Modules {
    fn add_file(&mut self, key: N13ModuleKey, path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("N13_UNRESOLVED {}: {e}", path.display()))?;
        if !self
            .files
            .entry(path.clone())
            .or_default()
            .insert(key.clone())
        {
            return Ok(());
        }
        let (_, file) = parse_file(&path)?;
        let dir = if key.len() == 1
            || matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("lib.rs" | "main.rs" | "mod.rs")
            ) {
            path.parent()
                .ok_or("N13_UNRESOLVED module parent")?
                .to_path_buf()
        } else {
            path.with_extension("")
        };
        self.add_items(
            key,
            &path,
            &dir,
            path.parent().ok_or("N13_UNRESOLVED source parent")?,
            &file.items,
        )
    }

    fn add_items(
        &mut self,
        key: N13ModuleKey,
        path: &Path,
        dir: &Path,
        path_base: &Path,
        items: &[Item],
    ) -> Result<(), String> {
        self.modules.entry(key.clone()).or_default();
        for item in items {
            match item {
                Item::Fn(f) => {
                    self.modules
                        .entry(key.clone())
                        .or_default()
                        .functions
                        .insert(f.sig.ident.to_string());
                }
                Item::Use(u) => {
                    let mut leaves = Vec::new();
                    n13_use_leaves(&u.tree, &[], &mut leaves);
                    self.modules
                        .entry(key.clone())
                        .or_default()
                        .imports
                        .extend(leaves);
                }
                Item::Mod(m) => {
                    let name = m.ident.to_string();
                    self.modules
                        .entry(key.clone())
                        .or_default()
                        .children
                        .insert(name.clone());
                    let mut child = key.clone();
                    child.push(name.clone());
                    if let Some((_, nested)) = &m.content {
                        self.add_items(child, path, &dir.join(&name), &dir.join(&name), nested)?;
                    } else {
                        let mut explicit = Vec::new();
                        for attr in &m.attrs {
                            if attr.path().is_ident("cfg_attr") {
                                let args = attr.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
                                    .map_err(|e| format!("N13_UNRESOLVED cfg_attr: {e}"))?;
                                if args.iter().skip(1).any(|meta| {
                                    meta.path().is_ident("path") || meta.path().is_ident("cfg_attr")
                                }) {
                                    return Err(format!(
                                        "N13_UNRESOLVED conditional module path in {}::{name}",
                                        key.join("::")
                                    ));
                                }
                            }
                            if attr.path().is_ident("path") {
                                let syn::Meta::NameValue(value) = &attr.meta else {
                                    return Err("N13_UNRESOLVED malformed #[path]".into());
                                };
                                let Expr::Lit(lit) = &value.value else {
                                    return Err("N13_UNRESOLVED nonliteral #[path]".into());
                                };
                                let syn::Lit::Str(s) = &lit.lit else {
                                    return Err("N13_UNRESOLVED nonstring #[path]".into());
                                };
                                explicit.push(s.value());
                            }
                        }
                        if explicit.len() > 1 {
                            return Err(format!("N13_AMBIGUOUS_MODULE {}::{name}", key.join("::")));
                        }
                        let candidates = if let Some(explicit) = explicit.first() {
                            // #[path] in a file module is relative to its source
                            // directory; in inline modules it is relative to the
                            // inline module directory.
                            vec![path_base.join(explicit)]
                        } else {
                            [
                                dir.join(format!("{name}.rs")),
                                dir.join(&name).join("mod.rs"),
                            ]
                            .into_iter()
                            .filter(|p| p.is_file())
                            .collect()
                        };
                        if candidates.len() != 1 || !candidates[0].is_file() {
                            return Err(format!(
                                "N13_UNRESOLVED module {}::{name} declared in {}",
                                key.join("::"),
                                path.display()
                            ));
                        }
                        self.add_file(child, &candidates[0])?;
                    }
                }
                Item::Macro(m) => self.add_items(
                    key.clone(),
                    path,
                    dir,
                    path_base,
                    &n13_macro_items(m.mac.tokens.clone()),
                )?,
                _ => {}
            }
        }
        Ok(())
    }

    fn anchored(&self, module: &[String], path: &[String]) -> Vec<N13ModuleKey> {
        let Some(first) = path.first() else {
            return vec![module.to_vec()];
        };
        match first.as_str() {
            "crate" => {
                let mut key = module[..1].to_vec();
                key.extend_from_slice(&path[1..]);
                vec![key]
            }
            "self" => {
                let mut key = module.to_vec();
                key.extend_from_slice(&path[1..]);
                vec![key]
            }
            "super" => {
                let supers = path.iter().take_while(|s| s.as_str() == "super").count();
                if supers >= module.len() {
                    return Vec::new();
                }
                let mut key = module[..module.len() - supers].to_vec();
                key.extend_from_slice(&path[supers..]);
                vec![key]
            }
            _ => {
                let mut local = module.to_vec();
                local.extend_from_slice(path);
                let mut keys = vec![local];
                if self.modules.contains_key(&vec![first.clone()]) {
                    keys.push(path.to_vec());
                }
                keys
            }
        }
    }

    fn resolve_absolute(
        &self,
        path: &[String],
        seen: &mut BTreeSet<N13ModuleKey>,
    ) -> BTreeSet<N13ModuleKey> {
        if !seen.insert(path.to_vec()) {
            return BTreeSet::new();
        }
        let mut result = BTreeSet::new();
        if self.modules.contains_key(path) {
            result.insert(path.to_vec());
        }
        for split in (1..path.len()).rev() {
            let prefix = &path[..split];
            let Some(module) = self.modules.get(prefix) else {
                continue;
            };
            for import in &module.imports {
                let tail = match &import.alias {
                    Some(alias) if alias == &path[split] => &path[split + 1..],
                    Some(_) => continue,
                    None => &path[split..],
                };
                for mut target in self.anchored(prefix, &import.path) {
                    target.extend_from_slice(tail);
                    result.extend(self.resolve_absolute(&target, &mut seen.clone()));
                }
            }
        }
        result
    }

    fn member_identities(
        &self,
        module: &[String],
        name: &str,
        seen: &mut BTreeSet<(N13ModuleKey, String)>,
    ) -> BTreeSet<N13FunctionIdentity> {
        if !seen.insert((module.to_vec(), name.to_string())) {
            return BTreeSet::new();
        }
        let Some(info) = self.modules.get(module) else {
            return BTreeSet::new();
        };
        let mut identities = BTreeSet::new();
        if info.functions.contains(name) {
            identities.insert(N13FunctionIdentity {
                module: module.to_vec(),
                name: name.to_string(),
            });
        }
        for import in &info.imports {
            let (prefix, original) = match &import.alias {
                Some(alias) if alias == name => {
                    let Some(original) = import.path.last() else {
                        continue;
                    };
                    (&import.path[..import.path.len() - 1], original.as_str())
                }
                Some(_) => continue,
                None => (import.path.as_slice(), name),
            };
            for anchored in self.anchored(module, prefix) {
                for target in self.resolve_absolute(&anchored, &mut BTreeSet::new()) {
                    identities.extend(self.member_identities(&target, original, &mut seen.clone()));
                }
            }
        }
        identities
    }

    fn function_identities(
        &self,
        module: &[String],
        path: &[String],
        name: &str,
    ) -> BTreeSet<N13FunctionIdentity> {
        let mut identities = BTreeSet::new();
        for anchored in self.anchored(module, path) {
            for target in self.resolve_absolute(&anchored, &mut BTreeSet::new()) {
                identities.extend(self.member_identities(&target, name, &mut BTreeSet::new()));
            }
        }
        identities
    }

    fn resolve_function_identity(
        &self,
        module: &[String],
        path: &[String],
        name: &str,
    ) -> Result<N13FunctionIdentity, String> {
        let identities = self.function_identities(module, path, name);
        match identities.len() {
            1 => identities
                .into_iter()
                .next()
                .ok_or_else(|| "N13_UNRESOLVED empty function identity".into()),
            0 => Err(format!(
                "N13_UNRESOLVED {}::{name} from {}",
                path.join("::"),
                module.join("::")
            )),
            _ => Err(format!(
                "N13_AMBIGUOUS_RECEIVER {}::{name} from {}: {identities:?}",
                path.join("::"),
                module.join("::")
            )),
        }
    }

    // Keep the shared PC2 API: N13 additionally needs the declaration's actual
    // member spelling so a renamed facade cannot lose its original identity.
    fn resolve_function(
        &self,
        module: &[String],
        path: &[String],
        name: &str,
    ) -> Result<N13ModuleKey, String> {
        self.resolve_function_identity(module, path, name)
            .map(|identity| identity.module)
    }

    fn resolve(&self, module: &[String], path: &[String]) -> Result<N13ModuleKey, String> {
        let mut result = BTreeSet::new();
        for candidate in self.anchored(module, path) {
            result.extend(self.resolve_absolute(&candidate, &mut BTreeSet::new()));
        }
        match result.len() {
            1 => result
                .into_iter()
                .next()
                .ok_or_else(|| "N13_UNRESOLVED empty result".into()),
            0 => Err(format!(
                "N13_UNRESOLVED {} from {}",
                path.join("::"),
                module.join("::")
            )),
            _ => Err(format!(
                "N13_AMBIGUOUS_RECEIVER {} from {}: {result:?}",
                path.join("::"),
                module.join("::")
            )),
        }
    }
}

fn n13_module_catalog(sources: &[GuardSource]) -> Result<N13Modules, String> {
    let mut catalog = N13Modules::default();
    let mut members = BTreeMap::new();
    for source in sources {
        members.insert(source.package.clone(), source.member_root.clone());
    }
    for (package, root) in members {
        let manifest = root.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("N13_UNRESOLVED {}: {e}", manifest.display()))?;
        let manifest: toml::Value =
            toml::from_str(&text).map_err(|e| format!("N13_UNRESOLVED manifest: {e}"))?;
        let name = manifest
            .get("lib")
            .and_then(|v| v.get("name"))
            .and_then(toml::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| package.replace('-', "_"));
        let lib = manifest
            .get("lib")
            .and_then(|v| v.get("path"))
            .and_then(toml::Value::as_str)
            .unwrap_or("src/lib.rs");
        if root.join(lib).is_file() {
            catalog.add_file(vec![name.clone()], &root.join(lib))?;
        }
        let main = root.join("src/main.rs");
        if main.is_file() {
            catalog.add_file(vec![format!("{name}@main")], &main)?;
        }
        if root.join("build.rs").is_file() {
            catalog.add_file(vec![format!("{name}@build")], &root.join("build.rs"))?;
        }
        for kind in ["bin", "test", "bench", "example"] {
            if let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) {
                for target in targets {
                    if let Some(path) = target.get("path").and_then(toml::Value::as_str) {
                        catalog
                            .add_file(vec![format!("{name}@{kind}:{path}")], &root.join(path))?;
                    }
                }
            }
        }
        // Autodiscovered test/bin/example/bench roots not listed explicitly.
        for source in sources.iter().filter(|s| s.package == package) {
            let Ok(relative) = source.path.strip_prefix(&root) else {
                continue;
            };
            let parent = relative.parent().and_then(Path::to_str);
            if matches!(parent, Some("tests" | "benches" | "examples" | "src/bin")) {
                let canonical = source.path.canonicalize().map_err(|e| e.to_string())?;
                if !catalog.files.contains_key(&canonical) {
                    catalog.add_file(
                        vec![format!("{name}@target:{}", relative.display())],
                        &canonical,
                    )?;
                }
            }
        }
    }
    Ok(catalog)
}

fn carried_pairs(path: &Path) -> Result<Vec<CarriedPair>, String> {
    let (_, file) = parse_file(path)?;
    let tables: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Const(c) if c.ident == "CARRIERS" => Some(c),
            _ => None,
        })
        .collect();
    if tables.len() != 1 {
        return Err("N13_UNRESOLVED expected exactly one CARRIERS table".into());
    }
    let Expr::Reference(reference) = &*tables[0].expr else {
        return Err("N13_UNRESOLVED CARRIERS reference".into());
    };
    let Expr::Array(array) = &*reference.expr else {
        return Err("N13_UNRESOLVED CARRIERS array".into());
    };
    let mut pairs = Vec::new();
    for element in &array.elems {
        let Expr::Tuple(tuple) = element else {
            return Err("N13_UNRESOLVED CARRIERS tuple".into());
        };
        if tuple.elems.len() != 2 {
            return Err("N13_UNRESOLVED CARRIERS pair arity".into());
        }
        let mut members = Vec::new();
        for expr in &tuple.elems {
            let Expr::Path(p) = expr else {
                return Err("N13_UNRESOLVED CARRIERS member path".into());
            };
            let mut path = path_names(&p.path);
            let name = path.pop().ok_or("N13_UNRESOLVED empty CARRIERS member")?;
            if path.is_empty() {
                return Err("N13_UNRESOLVED unqualified CARRIERS member".into());
            }
            members.push((path, name));
        }
        let members = members
            .try_into()
            .map_err(|_| "N13_UNRESOLVED CARRIERS pair arity")?;
        pairs.push(CarriedPair { members });
    }
    if pairs.is_empty() {
        return Err("N13_UNRESOLVED empty CARRIERS table".into());
    }
    Ok(pairs)
}

struct N13Visitor<'a> {
    names: &'a BTreeSet<String>,
    defining: &'a BTreeMap<String, BTreeSet<N13ModuleKey>>,
    catalog: &'a N13Modules,
    module: N13ModuleKey,
    fn_stack: Vec<String>,
    aliases: BTreeSet<String>,
    allowed: BTreeMap<(usize, String), usize>,
    errors: Vec<String>,
}

impl N13Visitor<'_> {
    fn allow(&mut self, ident: &syn::Ident) {
        *self
            .allowed
            .entry((line_of(ident.span()), ident.to_string()))
            .or_default() += 1;
    }

    fn delegation(&self, path: &[String], name: &str) -> Result<bool, String> {
        if self.fn_stack.len() != 1
            || self.fn_stack.first().map(String::as_str) != Some(name)
            || !self
                .defining
                .get(name)
                .is_some_and(|modules| modules.contains(&self.module))
        {
            return Ok(false);
        }
        if path.len() != 2 {
            return Ok(false);
        }
        let Some(module) = self.catalog.modules.get(&self.module) else {
            return Err("N13_UNRESOLVED current module".into());
        };
        if !module.children.contains(&path[0]) {
            return Err(format!(
                "N13_UNRESOLVED undeclared child {} of {}",
                path[0],
                self.module.join("::")
            ));
        }
        let _child = self.catalog.resolve(&self.module, &path[..1])?;
        Ok(true)
    }

    fn macro_tokens(&mut self, stream: proc_macro2::TokenStream) {
        if let Ok(file) = syn::parse2::<syn::File>(stream.clone()) {
            if !file.items.is_empty() {
                self.visit_file(&file);
                return;
            }
        }
        let tokens: Vec<_> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                proc_macro2::TokenTree::Group(g) => self.macro_tokens(g.stream()),
                proc_macro2::TokenTree::Ident(ident) if self.names.contains(&ident.to_string()) => {
                    let name = ident.to_string();
                    let mut path = vec![name.clone()];
                    let mut cursor = index;
                    while cursor >= 3
                        && matches!(&tokens[cursor - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                        && matches!(&tokens[cursor - 2], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                    {
                        let proc_macro2::TokenTree::Ident(prefix) = &tokens[cursor - 3] else {
                            break;
                        };
                        path.insert(0, prefix.to_string());
                        cursor -= 3;
                    }
                    let is_call = matches!(tokens.get(index + 1), Some(proc_macro2::TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
                    if is_call {
                        match self.delegation(&path, &name) {
                            Ok(true) => self.allow(ident),
                            Ok(false) => {}
                            Err(e) => self.errors.push(e),
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for N13Visitor<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if self.names.contains(&item.sig.ident.to_string()) {
            self.allow(&item.sig.ident);
        }
        self.fn_stack.push(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.fn_stack.pop();
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            self.module.push(item.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
        }
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        n13_use_leaves(&item.tree, &[], &mut leaves);
        let mut relevant = false;
        for leaf in leaves {
            if let Some(name) = leaf.path.last().filter(|s| self.names.contains(*s)) {
                relevant = true;
                match self.catalog.resolve_function(
                    &self.module,
                    &leaf.path[..leaf.path.len() - 1],
                    name,
                ) {
                    Ok(_) => {}
                    Err(error) => self.errors.push(error),
                }
                if let Some(alias) = leaf.alias {
                    if alias != *name {
                        self.aliases.insert(alias);
                    }
                }
            }
        }
        if relevant {
            // Use trees are accounted token-for-token; later alias uses remain
            // prohibited. Resolve before allowing the declaration, never grant
            // authority from a matching leaf spelling alone.
            fn account(visitor: &mut N13Visitor<'_>, tree: &UseTree) {
                match tree {
                    UseTree::Path(p) => {
                        if visitor.names.contains(&p.ident.to_string()) {
                            visitor.allow(&p.ident);
                        }
                        account(visitor, &p.tree);
                    }
                    UseTree::Name(n) => {
                        if visitor.names.contains(&n.ident.to_string()) {
                            visitor.allow(&n.ident);
                        }
                    }
                    UseTree::Rename(n) => {
                        if visitor.names.contains(&n.ident.to_string()) {
                            visitor.allow(&n.ident);
                            visitor.allow(&n.rename);
                        }
                    }
                    UseTree::Group(g) => {
                        for item in &g.items {
                            account(visitor, item);
                        }
                    }
                    UseTree::Glob(_) => {}
                }
            }
            account(self, &item.tree);
        }
    }
    fn visit_expr_call(&mut self, item: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*item.func {
            if let Some(last) = path.path.segments.last() {
                let name = last.ident.to_string();
                if self.names.contains(&name) {
                    match self.delegation(&path_names(&path.path), &name) {
                        Ok(true) => self.allow(&last.ident),
                        Ok(false) => {}
                        Err(e) => self.errors.push(e),
                    }
                }
            }
        }
        visit::visit_expr_call(self, item);
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        self.macro_tokens(item.mac.tokens.clone());
    }
    fn visit_expr_macro(&mut self, item: &'ast syn::ExprMacro) {
        self.macro_tokens(item.mac.tokens.clone());
    }
    fn visit_stmt_macro(&mut self, item: &'ast syn::StmtMacro) {
        self.macro_tokens(item.mac.tokens.clone());
    }
}

fn count_occurrences(cleaned: &str, names: &BTreeSet<String>) -> BTreeMap<(usize, String), usize> {
    let mut out = BTreeMap::new();
    let mut line = 1;
    let mut current = String::new();
    for c in cleaned.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphanumeric() || c == '_' {
            current.push(c);
            continue;
        }
        if names.contains(&current) {
            *out.entry((line, current.clone())).or_default() += 1;
        }
        current.clear();
        if c == '\n' {
            line += 1;
        }
    }
    out
}

pub(crate) fn scan_carried_workspace(workspace: &Path) -> Result<(), Vec<String>> {
    let sources = discover_sources(workspace).map_err(|e| vec![e])?;
    let catalog = n13_module_catalog(&sources).map_err(|e| vec![e])?;
    let mut candidates = Vec::new();
    for (file, modules) in &catalog.files {
        for module in modules {
            if module.first().is_some_and(|s| s == "neovm_core")
                && module.ends_with(&["eval".into(), "green".into(), "carried".into()])
            {
                candidates.push((file.clone(), module.clone()));
            }
        }
    }
    if candidates.len() != 1 {
        return Err(vec![format!(
            "N13_UNRESOLVED expected one upper eval::green::carried, got {candidates:?}"
        )]);
    }
    let (carried, carried_module) = &candidates[0];
    let pairs = carried_pairs(carried).map_err(|e| vec![e])?;
    let mut names = BTreeSet::new();
    let mut defining: BTreeMap<String, BTreeSet<N13ModuleKey>> = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for pair in pairs {
        for (path, name) in pair.members {
            let identity = catalog
                .resolve_function_identity(carried_module, &path, &name)
                .map_err(|e| vec![e])?;
            names.insert(name);
            names.insert(identity.name.clone());
            defining
                .entry(identity.name.clone())
                .or_default()
                .insert(identity.module.clone());
            identities.insert(identity);
        }
    }
    let mut violations = Vec::new();
    // All module-level aliases of a carried declaration join the global fence,
    // including aliases NOT used by CARRIERS. Otherwise a new reexport in one
    // file could be called under its new spelling from another file. Resolution
    // follows the complete alias chain, not a name-only alias allowlist.
    for (module, info) in &catalog.modules {
        for import in &info.imports {
            let Some(alias) = &import.alias else {
                continue;
            };
            let Some(original) = import.path.last() else {
                continue;
            };
            let prefix = &import.path[..import.path.len() - 1];
            let targets = catalog.function_identities(module, prefix, original);
            if targets.iter().any(|target| identities.contains(target)) {
                names.insert(alias.clone());
                if targets.len() != 1 {
                    violations.push(format!(
                        "N13_AMBIGUOUS_RECEIVER carried alias {}::{alias}: {targets:?}",
                        module.join("::")
                    ));
                }
            }
        }
    }
    let mut scan_paths: BTreeSet<PathBuf> = catalog.files.keys().cloned().collect();
    for source in sources {
        scan_paths.insert(
            source
                .path
                .canonicalize()
                .map_err(|e| vec![e.to_string()])?,
        );
    }
    for path in scan_paths {
        if path == *carried {
            continue;
        }
        let (text, file) = parse_file(&path).map_err(|e| vec![e])?;
        let mut counts = count_occurrences(&cleaned_code(&text), &names);
        if counts.is_empty() {
            continue;
        }
        let modules = catalog.files.get(&path);
        let Some(module) = modules
            .filter(|set| set.len() == 1)
            .and_then(|set| set.iter().next())
        else {
            violations.push(format!(
                "N13_UNRESOLVED {}: relevant file has unknown/ambiguous module {modules:?}",
                path.display()
            ));
            continue;
        };
        let mut visitor = N13Visitor {
            names: &names,
            defining: &defining,
            catalog: &catalog,
            module: module.clone(),
            fn_stack: Vec::new(),
            aliases: BTreeSet::new(),
            allowed: BTreeMap::new(),
            errors: Vec::new(),
        };
        visitor.visit_file(&file);
        let mut all_names = names.clone();
        all_names.extend(visitor.aliases);
        counts = count_occurrences(&cleaned_code(&text), &all_names);
        for (key, allowed) in visitor.allowed {
            if let Some(count) = counts.get_mut(&key) {
                *count = count.saturating_sub(allowed);
            }
        }
        violations.extend(
            visitor
                .errors
                .into_iter()
                .map(|e| format!("{}: {e}", path.display())),
        );
        for ((line, name), count) in counts {
            if count > 0 {
                violations.push(format!(
                    "{}:{line}: N13_FORBIDDEN_REFERENCE {name} x{count}",
                    path.display()
                ));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[test]
fn architecture_n13_carried_pairs_have_one_reference_site() {
    if let Err(violations) = scan_carried_workspace(&workspace_root()) {
        panic!(
            "{} carried-state reference violations:\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

#[test]
fn architecture_n13_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for package in ["neovm-core", "neovm-value"] {
        std::fs::create_dir_all(root.join(format!("crates/{package}/src"))).unwrap();
        std::fs::write(
            root.join(format!("crates/{package}/Cargo.toml")),
            format!("[package]\nname = {package:?}\nversion = \"0.0.0\"\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/neovm-core\", \"crates/neovm-value\"]\n",
    )
    .unwrap();
    let src = root.join("crates/neovm-core/src");
    std::fs::create_dir_all(src.join("eval/green")).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "mod pair; mod aliases; mod external; mod eval { pub mod green { pub mod carried; } }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/neovm-value/src/lib.rs"),
        "pub struct Lower;\n",
    )
    .unwrap();
    std::fs::write(src.join("eval/green/carried.rs"), "const CARRIERS: &[(P, S)] = &[(crate::aliases::carried_placeholder, crate::aliases::carried_swap)];\n").unwrap();
    let aliases =
        "pub use crate::pair::{placeholder as carried_placeholder, swap as carried_swap};\n";
    std::fs::write(src.join("aliases.rs"), aliases).unwrap();
    let positive = "mod child { pub fn swap() {} }\npub fn placeholder() {}\npub fn swap() { child::swap(); }\n";
    std::fs::write(src.join("pair.rs"), positive).unwrap();
    std::fs::write(src.join("external.rs"), "fn unrelated() {}\n").unwrap();
    let green = scan_carried_workspace(root);
    assert!(green.is_ok(), "false positive: {green:?}");
    std::fs::write(
        src.join("external.rs"),
        "fn violate() { crate::pair::swap(); }\n",
    )
    .unwrap();
    let red = scan_carried_workspace(root);
    assert!(red.is_err(), "external mutation not detected");
    assert!(
        format!("{red:?}").contains("external.rs"),
        "wrong violation: {red:?}"
    );
    std::fs::write(src.join("external.rs"), "fn unrelated() {}\n").unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "external restoration not green"
    );
    // The original outer() fixture is now the exact negative control: merely
    // sharing a file with the definition does not authorize delegation.
    std::fs::write(
        src.join("pair.rs"),
        format!("{positive}\nfn outer() {{ child::swap(); }}\n"),
    )
    .unwrap();
    let red = scan_carried_workspace(root);
    assert!(
        red.is_err(),
        "same-file wrong-function mutation not detected"
    );
    assert!(
        format!("{red:?}").contains("pair.rs"),
        "wrong violation: {red:?}"
    );
    std::fs::write(src.join("pair.rs"), positive).unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "delegation restoration not green"
    );
    std::fs::write(src.join("pair.rs"), "mod child { pub fn swap() {} }\npub fn placeholder() {}\npub fn swap() { std::cfg_select! { any() => { child::swap(); } _ => {} } }\n").unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "cfg_select delegation false positive"
    );
    std::fs::write(src.join("pair.rs"), "mod child { pub fn swap() {} }\npub fn placeholder() {}\npub fn swap() {}\npub fn outer() { std::cfg_select! { any() => { child::swap(); } _ => {} } }\n").unwrap();
    assert!(
        scan_carried_workspace(root).is_err(),
        "cfg_select wrong-function mutation not detected"
    );
    std::fs::write(src.join("pair.rs"), positive).unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "macro restoration not green"
    );
    // A forged alias absent from CARRIERS must still fence calls from another
    // file. Its source-resolved identity, not the spelling, connects it.
    std::fs::write(
        src.join("aliases.rs"),
        format!("{aliases}pub use crate::pair::swap as forged_swap;\n"),
    )
    .unwrap();
    std::fs::write(
        src.join("external.rs"),
        "fn violate() { crate::aliases::forged_swap(); }\n",
    )
    .unwrap();
    let red = scan_carried_workspace(root);
    assert!(
        red.is_err(),
        "cross-file forged alias mutation not detected"
    );
    let message = format!("{red:?}");
    assert!(
        message.contains("external.rs") && message.contains("forged_swap"),
        "wrong alias violation: {message}"
    );
    std::fs::write(src.join("external.rs"), "fn unrelated() {}\n").unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "unused alias declaration must not cause a false positive"
    );
    std::fs::write(src.join("aliases.rs"), aliases).unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "alias restoration not green"
    );
}

// ---------------------------------------------------------------------------
// PC1: hot-path fenced method table.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Pc1Row {
    type_name: String,
    method: String,
    protected_caller: String,
}

fn parse_pc1_table(text: &str) -> Result<Vec<Pc1Row>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'));
    let Some((_, header)) = lines.next() else {
        return Err("PC1_TABLE_MISSING_HEADER".into());
    };
    if header != "type\tmethod\tprotected_caller" {
        return Err("PC1_TABLE_UNKNOWN_SCHEMA".into());
    }
    let mut rows = Vec::new();
    let mut keys = BTreeSet::new();
    for (index, line) in lines {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 || fields.iter().any(|field| field.trim().is_empty()) {
            return Err(format!(
                "PC1_TABLE_ROW {}: expected three nonempty fields",
                index + 1
            ));
        }
        syn::parse_str::<syn::Path>(fields[0])
            .map_err(|error| format!("PC1_UNKNOWN_OWNER {}: {error}", fields[0]))?;
        syn::parse_str::<syn::Ident>(fields[1])
            .map_err(|error| format!("PC1_UNKNOWN_METHOD {}: {error}", fields[1]))?;
        if !keys.insert((fields[0], fields[1])) {
            return Err(format!("PC1_DUPLICATE_ROW {}::{}", fields[0], fields[1]));
        }
        rows.push(Pc1Row {
            type_name: fields[0].into(),
            method: fields[1].into(),
            protected_caller: fields[2].into(),
        });
    }
    // Header-only is the signed pre-promotion base state. The separate API
    // projector must still prove that its actual exception set is empty.
    Ok(rows)
}

#[derive(Clone, Copy, Debug)]
enum Pc1Refusal {
    AmbiguousReceiver,
    UnresolvedReference,
}

impl Pc1Refusal {
    fn code(self) -> &'static str {
        match self {
            Self::AmbiguousReceiver => "PC1_AMBIGUOUS_RECEIVER",
            Self::UnresolvedReference => "PC1_UNRESOLVED_REFERENCE",
        }
    }
}

struct Pc1References<'a> {
    rows: &'a [Pc1Row],
    file: &'a Path,
    violations: Vec<String>,
}

impl Pc1References<'_> {
    fn refuse(&mut self, ident: &syn::Ident, kind: Pc1Refusal) {
        for row in self.rows.iter().filter(|row| ident == row.method.as_str()) {
            self.violations.push(format!(
                "{} {}:{}: {}::{}; protected caller {}",
                kind.code(),
                self.file.display(),
                line_of(ident.span()),
                row.type_name,
                row.method,
                row.protected_caller
            ));
        }
    }

    fn tokens(&mut self, stream: proc_macro2::TokenStream) {
        let tokens: Vec<_> = stream.into_iter().collect();
        for token in &tokens {
            match token {
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Ident(ident)
                    if self.rows.iter().any(|row| ident == row.method.as_str()) =>
                {
                    self.refuse(ident, Pc1Refusal::UnresolvedReference);
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Pc1References<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // syn cannot prove Rust receiver types. A matching method spelling
        // outside the permitted crates is a reviewable ambiguity, never an
        // implicit exemption or a guessed type identity.
        self.refuse(&call.method, Pc1Refusal::AmbiguousReceiver);
        visit::visit_expr_method_call(self, call);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(segment) = path.segments.last() {
            self.refuse(&segment.ident, Pc1Refusal::UnresolvedReference);
        }
        visit::visit_path(self, path);
    }
    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        match tree {
            UseTree::Name(name) => self.refuse(&name.ident, Pc1Refusal::UnresolvedReference),
            UseTree::Rename(rename) => self.refuse(&rename.ident, Pc1Refusal::UnresolvedReference),
            _ => {}
        }
        visit::visit_use_tree(self, tree);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

pub(crate) fn scan_pc1_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc1_table(text)?;
    let sources = discover_sources(workspace)?;
    let mut violations = Vec::new();
    for source in sources {
        let (_, syntax) = parse_file(&source.path)?;
        if matches!(source.package.as_str(), "neovm-core" | "neovm-value") {
            continue;
        }
        let mut visitor = Pc1References {
            rows: &rows,
            file: &source.path,
            violations: Vec::new(),
        };
        visitor.visit_file(&syntax);
        violations.extend(visitor.violations);
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}

#[test]
fn architecture_pc1_fenced_hot_path_methods_are_called_only_from_core_or_value() {
    match scan_pc1_table(&workspace_root(), PC1_TABLE) {
        Ok(violations) if violations.is_empty() => {}
        Ok(violations) => panic!("PC1 fence violations:\n{}", violations.join("\n")),
        Err(error) => panic!("PC1 input: {error}"),
    }
}

#[test]
fn architecture_pc1_guard_self_test() {
    let dir = tempfile::tempdir().expect("fixture");
    guard_fixture_workspace(dir.path(), &["neovm-core", "neovm-value", "neomacs-other"]);
    let table = "type\tmethod\tprotected_caller\nValue\tfenced_raw_read\tsrc/vm.rs:Vm::run_loop\n";
    let allowed = dir.path().join("crates/neovm-value/src/lib.rs");
    std::fs::write(allowed, "fn f(v: V) { v.fenced_raw_read(); }").expect("allowed fixture");
    assert!(
        scan_pc1_table(dir.path(), table)
            .expect("positive")
            .is_empty()
    );
    let foreign = dir.path().join("crates/neomacs-other/src/lib.rs");
    let clean = std::fs::read(&foreign).expect("save");
    std::fs::write(&foreign, "fn g(v: V) { v.fenced_raw_read(); }").expect("mutation");
    let red = scan_pc1_table(dir.path(), table).expect("mutated inputs parse");
    assert!(
        red.iter().any(
            |error| error.contains("PC1_AMBIGUOUS_RECEIVER") && error.contains("neomacs-other")
        ),
        "{red:?}"
    );
    std::fs::write(&foreign, &clean).expect("restore");
    assert!(
        scan_pc1_table(dir.path(), table)
            .expect("restored")
            .is_empty()
    );
    std::fs::write(
        &foreign,
        "use neovm_core::Value::fenced_raw_read as renamed; fn g() { renamed(); }",
    )
    .expect("function-item mutation");
    let red = scan_pc1_table(dir.path(), table).expect("alias syntax");
    assert!(
        red.iter()
            .any(|error| error.contains("PC1_UNRESOLVED_REFERENCE")),
        "{red:?}"
    );
    std::fs::write(&foreign, clean).expect("restore alias");
    assert!(
        scan_pc1_table(dir.path(), table)
            .expect("restored alias")
            .is_empty()
    );
}

#[test]
fn architecture_pc1_explicit_test_target_and_macro_references_are_fenced() {
    let dir = tempfile::tempdir().expect("fixture");
    guard_fixture_workspace(dir.path(), &["neovm-core", "neovm-value", "neomacs-other"]);
    let other = dir.path().join("crates/neomacs-other");
    let manifest = other.join("Cargo.toml");
    let mut text = std::fs::read_to_string(&manifest).expect("manifest");
    text.push_str("\n[[test]]\nname='custom'\npath='custom.rs'\n");
    std::fs::write(manifest, text).expect("declare test");
    let target = other.join("custom.rs");
    std::fs::write(&target, "fn clean() {}").expect("positive target");
    let table = "type\tmethod\tprotected_caller\nValue\tfenced_raw_read\tsrc/vm.rs:Vm::run_loop\n";
    assert!(
        scan_pc1_table(dir.path(), table)
            .expect("positive")
            .is_empty()
    );
    std::fs::write(
        &target,
        "macro_rules! violate { () => { Value::fenced_raw_read() }; }",
    )
    .expect("macro mutation");
    let red = scan_pc1_table(dir.path(), table).expect("macro source");
    assert!(
        red.iter()
            .any(|error| error.contains("custom.rs") && error.contains("PC1_UNRESOLVED_REFERENCE")),
        "{red:?}"
    );
    std::fs::write(target, "fn clean() {}").expect("restore");
    assert!(
        scan_pc1_table(dir.path(), table)
            .expect("restored")
            .is_empty()
    );
    assert!(parse_pc1_table("").is_err());
    assert!(parse_pc1_table("unknown\theader\n").is_err());
    assert!(parse_pc1_table("type\tmethod\tprotected_caller\nValue\tf\t\n").is_err());
}

#[test]
fn architecture_pc1_data_approvals_match_the_real_promotion_ledger() {
    fn rows(text: &str, header: &str, columns: usize) -> Vec<Vec<String>> {
        let mut lines = text
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'));
        assert_eq!(lines.next(), Some(header), "PC1_DATA_TABLE_SCHEMA");
        lines
            .map(|line| {
                let fields: Vec<_> = line.split('\t').map(str::to_string).collect();
                assert_eq!(fields.len(), columns, "PC1_DATA_TABLE_SCHEMA: {line}");
                fields
            })
            .collect()
    }
    let ledger = rows(
        include_str!("PROMOTION-LEDGER.tsv"),
        "item\tkind\told_vis\tfile:line\tmodule\treason",
        6,
    );
    let expected: BTreeSet<_> = ledger
        .into_iter()
        .filter(|row| row[5].to_ascii_lowercase().contains("pc1-d"))
        .map(|row| {
            let (file, _) = row[3].rsplit_once(':').expect("PC1_DATA_FILE_IDENTITY");
            vec![
                row[0].clone(),
                row[1].clone(),
                file.to_string(),
                row[5].clone(),
                "PC1-D".to_string(),
            ]
        })
        .collect();
    let actual = rows(
        include_str!("pc1_data_promotions.tsv"),
        "item\tkind\tfile\tprotected_caller\tapproval",
        5,
    );
    let unique: BTreeSet<_> = actual.iter().cloned().collect();
    assert_eq!(actual.len(), unique.len(), "PC1_DATA_DUPLICATE_ROW");
    assert_eq!(unique, expected, "PC1_DATA_APPROVAL_LEDGER_MISMATCH");
}

// ---------------------------------------------------------------------------
// PC2: exact hook adapter chains, typed test-only rows and completeness.
// Requires the shared N13Modules source/module catalog, not a second index.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pc2Site {
    file: String,
    function: String,
}

#[derive(Clone, Debug)]
enum Pc2TestFence {
    AdapterOnly,
    ExactCaller(Pc2Site),
}

#[derive(Clone, Debug)]
enum Pc2Kind {
    Production {
        adapter: Option<Pc2Site>,
        seams: Vec<Pc2Site>,
    },
    TestOnly {
        adapter: Pc2Site,
        fence: Pc2TestFence,
    },
    DataInstall {
        seam: Pc2Site,
    },
}

#[derive(Clone, Debug)]
struct Pc2Row {
    static_path: Vec<String>,
    installer: Vec<String>,
    kind: Pc2Kind,
}

#[derive(Clone, Copy, Debug)]
enum Pc2SealedCrate {
    Value,
}
const PC2_SEALED_LOWER_CRATES: &[Pc2SealedCrate] = &[Pc2SealedCrate::Value];
impl Pc2SealedCrate {
    fn crate_name(self) -> &'static str {
        match self {
            Self::Value => "neovm_value",
        }
    }
}

fn pc2_site(text: &str) -> Result<Pc2Site, String> {
    let (file, function) = text
        .split_once(':')
        .ok_or_else(|| format!("PC2_TABLE: expected exact file:function, got {text:?}"))?;
    let path = Path::new(file);
    if file.is_empty()
        || function.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || !file.ends_with(".rs")
        || function
            .split("::")
            .any(|part| syn::parse_str::<syn::Ident>(part).is_err())
    {
        return Err(format!("PC2_TABLE: malformed exact site {text:?}"));
    }
    Ok(Pc2Site {
        file: file.to_string(),
        function: function.to_string(),
    })
}

fn pc2_table_path(text: &str) -> Result<Vec<String>, String> {
    let path = syn::parse_str::<syn::Path>(text).map_err(|error| format!("PC2_TABLE: {error}"))?;
    let names = path_names(&path);
    if names.len() < 2 {
        return Err(format!("PC2_TABLE: unqualified path {text:?}"));
    }
    Ok(names)
}

fn parse_pc2_table(text: &str) -> Result<Vec<Pc2Row>, String> {
    let mut rows = Vec::new();
    let mut header = false;
    let mut unique = BTreeSet::new();
    for (line, source) in text.lines().enumerate() {
        if source.is_empty() || source.starts_with('#') {
            continue;
        }
        if !header {
            if source != "kind\tstatic\tinstaller\tadapter\tseams" {
                return Err(format!(
                    "PC2_TABLE:{}: exact five-column header required",
                    line + 1
                ));
            }
            header = true;
            continue;
        }
        let fields: Vec<_> = source.split('\t').collect();
        if fields.len() != 5 {
            return Err(format!("PC2_TABLE:{}: expected five columns", line + 1));
        }
        if !unique.insert(source.to_string()) {
            return Err("PC2_TABLE: duplicate row".into());
        }
        let static_path = pc2_table_path(fields[1])?;
        let installer = pc2_table_path(fields[2])?;
        let adapter = if matches!(fields[3], "" | "-") {
            None
        } else {
            Some(pc2_site(fields[3])?)
        };
        let mut seams = Vec::new();
        if !matches!(fields[4], "" | "-") {
            for site in fields[4].split(';') {
                seams.push(pc2_site(site)?);
            }
            let distinct: BTreeSet<_> = seams.iter().cloned().collect();
            if distinct.len() != seams.len() {
                return Err("PC2_TABLE: duplicate exact seam".into());
            }
        }
        let kind = match fields[0] {
            "production" if seams.len() >= 2 => Pc2Kind::Production { adapter, seams },
            "test_only" => {
                let adapter = adapter
                    .ok_or_else(|| "PC2_TABLE: test_only requires named adapter".to_string())?;
                let fence = match seams.as_slice() {
                    [] => Pc2TestFence::AdapterOnly,
                    [caller] => Pc2TestFence::ExactCaller(caller.clone()),
                    _ => return Err("PC2_TABLE: test_only accepts one exact caller or '-'".into()),
                };
                Pc2Kind::TestOnly { adapter, fence }
            }
            "data_install" if adapter.is_none() && seams.len() == 1 => Pc2Kind::DataInstall {
                seam: seams.remove(0),
            },
            _ => {
                return Err(format!(
                    "PC2_TABLE:{}: invalid kind/adapter/seam shape",
                    line + 1
                ));
            }
        };
        rows.push(Pc2Row {
            static_path,
            installer,
            kind,
        });
    }
    if !header || rows.is_empty() {
        return Err("PC2_TABLE: existing hooks require a header and rows".into());
    }
    Ok(rows)
}

fn pc2_cfg_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(value) => {
            value.path.is_ident("feature")
                && matches!(&value.value, Expr::Lit(v) if matches!(&v.lit, syn::Lit::Str(s) if s.value() == "test-support"))
        }
        syn::Meta::List(list) => {
            use syn::parse::Parser;
            let Ok(items) =
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
            else {
                return false;
            };
            if list.path.is_ident("all") {
                items.iter().any(pc2_cfg_test)
            } else if list.path.is_ident("any") {
                !items.is_empty() && items.iter().all(pc2_cfg_test)
            } else {
                false
            }
        }
    }
}

fn pc2_test_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<syn::Meta>()
            .is_ok_and(|meta| pc2_cfg_test(&meta))
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pc2Function {
    owner: Vec<String>,
    name: String,
}

// Resolve the function's original declaration spelling through the ONE
// shared module/import catalog, including cross-name facade re-exports.
fn pc2_resolve_function(
    catalog: &N13Modules,
    module: &[String],
    path: &[String],
) -> Result<Pc2Function, String> {
    let Some(name) = path.last() else {
        return Err("PC2_UNRESOLVED_REFERENCE: empty function path".into());
    };
    let prefix = if path.len() == 1 {
        vec!["self".to_string()]
    } else {
        path[..path.len() - 1].to_vec()
    };
    catalog
        .resolve_function_identity(module, &prefix, name)
        .map(|identity| Pc2Function {
            owner: identity.module,
            name: identity.name,
        })
        .map_err(|error| {
            if error.contains("AMBIGUOUS") {
                format!("PC2_AMBIGUOUS_RECEIVER: {error}")
            } else {
                format!("PC2_UNRESOLVED_REFERENCE: {error}")
            }
        })
}

#[derive(Clone)]
struct Pc2Definition {
    symbol: Pc2Function,
    site: Pc2Site,
    test_only: bool,
}

#[derive(Clone)]
struct Pc2Reference {
    source: PathBuf,
    line: usize,
    symbol: Pc2Function,
    caller: Option<Pc2Site>,
    test_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pc2Static(Pc2Function);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pc2ParamIndex(usize);

#[derive(Clone, Debug)]
enum Pc2CellOrigin {
    Static(Pc2Static),
    Parameter(Pc2ParamIndex),
    Local,
    Unresolved,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pc2Pass {
    Declarations,
    HelperSummaries,
    References,
}

#[derive(Clone)]
struct Pc2Write {
    cell: Pc2Static,
    writer: Pc2Function,
    source: PathBuf,
    line: usize,
}

#[derive(Default)]
struct Pc2Facts {
    definitions: Vec<Pc2Definition>,
    references: Vec<Pc2Reference>,
    once_statics: BTreeSet<(Vec<String>, String)>,
    all_statics: BTreeSet<(Vec<String>, String)>,
    helper_writes: BTreeMap<Pc2Function, BTreeSet<Pc2ParamIndex>>,
    writes: Vec<Pc2Write>,
    errors: Vec<String>,
}

fn pc2_once_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(reference) => pc2_once_type(&reference.elem),
        syn::Type::Paren(paren) => pc2_once_type(&paren.elem),
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "OnceLock"),
        _ => false,
    }
}

fn pc2_pattern_names(pattern: &syn::Pat) -> BTreeSet<String> {
    struct Names(BTreeSet<String>);
    impl<'ast> Visit<'ast> for Names {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.0.insert(pattern.ident.to_string());
            visit::visit_pat_ident(self, pattern);
        }
    }
    let mut names = Names(BTreeSet::new());
    names.visit_pat(pattern);
    names.0
}

fn pc2_static_path(
    catalog: &N13Modules,
    facts: &Pc2Facts,
    module: &[String],
    path: &[String],
) -> Result<Pc2Static, String> {
    let name = path
        .last()
        .ok_or_else(|| "PC2_UNRESOLVED_REFERENCE: empty static path".to_string())?;
    let owner = if path.len() == 1 {
        module.to_vec()
    } else {
        catalog
            .resolve(module, &path[..path.len() - 1])
            .map_err(|error| format!("PC2_UNRESOLVED_REFERENCE: static module: {error}"))?
    };
    if !facts.once_statics.contains(&(owner.clone(), name.clone())) {
        return Err(format!(
            "PC2_UNRESOLVED_REFERENCE: {} is not a declared OnceLock static",
            path.join("::")
        ));
    }
    Ok(Pc2Static(Pc2Function {
        owner,
        name: name.clone(),
    }))
}

struct Pc2Visitor<'a> {
    workspace: &'a Path,
    source: &'a Path,
    catalog: &'a N13Modules,
    names: &'a BTreeSet<String>,
    module: Vec<String>,
    current_fn: Option<String>,
    current_impl: Option<String>,
    test_only: bool,
    aliases: BTreeMap<String, BTreeSet<Vec<String>>>,
    locals: BTreeMap<String, Pc2CellOrigin>,
    facts: &'a mut Pc2Facts,
    pass: Pc2Pass,
}

impl Pc2Visitor<'_> {
    fn writer(&self) -> Option<Pc2Function> {
        self.current_fn.as_ref().map(|name| Pc2Function {
            owner: self.module.clone(),
            name: name.clone(),
        })
    }

    fn origin(&self, expr: &Expr) -> Pc2CellOrigin {
        match expr {
            Expr::Reference(reference) => self.origin(&reference.expr),
            Expr::Paren(paren) => self.origin(&paren.expr),
            Expr::Group(group) => self.origin(&group.expr),
            Expr::Path(path) if path.qself.is_none() => {
                let names = path_names(&path.path);
                if names.len() == 1 {
                    if let Some(origin) = names.first().and_then(|name| self.locals.get(name)) {
                        return origin.clone();
                    }
                }
                pc2_static_path(self.catalog, self.facts, &self.module, &names)
                    .map(Pc2CellOrigin::Static)
                    .unwrap_or(Pc2CellOrigin::Unresolved)
            }
            Expr::Call(call) => {
                if let Expr::Path(path) = &*call.func {
                    let names = path_names(&path.path);
                    if names
                        .last()
                        .is_some_and(|name| name == "new" || name == "default")
                        && names.iter().any(|name| name == "OnceLock")
                    {
                        return Pc2CellOrigin::Local;
                    }
                }
                Pc2CellOrigin::Unresolved
            }
            _ => Pc2CellOrigin::Unresolved,
        }
    }

    fn forward_write(&mut self, origin: Pc2CellOrigin, span: Span) {
        let Some(writer) = self.writer() else {
            if matches!(origin, Pc2CellOrigin::Static(_)) {
                self.error(
                    span,
                    "PC2_UNRESOLVED_REFERENCE: static write outside a named function".into(),
                );
            }
            return;
        };
        match (self.pass, origin) {
            (Pc2Pass::HelperSummaries, Pc2CellOrigin::Parameter(index)) => {
                self.facts.helper_writes.entry(writer).or_default().insert(index);
            }
            (Pc2Pass::References, Pc2CellOrigin::Static(cell)) => self.facts.writes.push(Pc2Write { cell, writer, source: self.source.to_path_buf(), line: line_of(span) }),
            (_, Pc2CellOrigin::Local | Pc2CellOrigin::Parameter(_)) => {},
            (Pc2Pass::Declarations | Pc2Pass::HelperSummaries, Pc2CellOrigin::Static(_) | Pc2CellOrigin::Unresolved) => {},
            (Pc2Pass::References, Pc2CellOrigin::Unresolved) => self.error(span, "PC2_UNRESOLVED_REFERENCE: OnceLock writer argument/receiver has no proved static, typed parameter or local-cell origin".into()),
        }
    }

    fn function_parameters(
        &mut self,
        inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
    ) {
        self.locals.clear();
        for (index, input) in inputs.iter().enumerate() {
            if let syn::FnArg::Typed(argument) = input {
                for name in pc2_pattern_names(&argument.pat) {
                    let origin = if pc2_once_type(&argument.ty) {
                        Pc2CellOrigin::Parameter(Pc2ParamIndex(index))
                    } else {
                        Pc2CellOrigin::Unresolved
                    };
                    self.locals.insert(name, origin);
                }
            }
        }
    }

    fn site(&self, function: &str) -> Option<Pc2Site> {
        let file = match self.source.strip_prefix(self.workspace) {
            Ok(path) => path.to_string_lossy().replace('\\', "/"),
            Err(_) => return None,
        };
        Some(Pc2Site {
            file,
            function: function.to_string(),
        })
    }
    fn error(&mut self, span: Span, detail: String) {
        self.facts.errors.push(format!(
            "{}:{}: {detail}",
            self.source.display(),
            line_of(span)
        ));
    }
    fn caller(&self) -> Option<Pc2Site> {
        self.current_fn.as_deref().and_then(|name| self.site(name))
    }

    fn resolve(&self, path: &[String]) -> Result<Pc2Function, String> {
        fn expand(
            visitor: &Pc2Visitor<'_>,
            path: &[String],
            seen: &mut BTreeSet<String>,
        ) -> Result<Pc2Function, String> {
            if let Some(first) = path.first() {
                if let Some(aliases) = visitor.aliases.get(first) {
                    if !seen.insert(first.clone()) {
                        return Err(format!(
                            "PC2_AMBIGUOUS_RECEIVER: cyclic local alias {first}"
                        ));
                    }
                    let mut resolved = BTreeSet::new();
                    for alias in aliases {
                        let mut full = alias.clone();
                        full.extend_from_slice(&path[1..]);
                        let symbol = if full.first() == Some(first) {
                            pc2_resolve_function(visitor.catalog, &visitor.module, &full)?
                        } else {
                            expand(visitor, &full, &mut seen.clone())?
                        };
                        resolved.insert(symbol);
                    }
                    return match resolved.len() {
                        1 => resolved
                            .into_iter()
                            .next()
                            .ok_or_else(|| "PC2_UNRESOLVED_REFERENCE: alias".into()),
                        _ => Err(format!(
                            "PC2_AMBIGUOUS_RECEIVER: alias {first}: {resolved:?}"
                        )),
                    };
                }
            }
            pc2_resolve_function(visitor.catalog, &visitor.module, path)
        }
        expand(self, path, &mut BTreeSet::new())
    }

    fn alias_relevant(&self, alias: &str, seen: &mut BTreeSet<String>) -> bool {
        if !seen.insert(alias.to_string()) {
            return false;
        }
        self.aliases.get(alias).is_some_and(|paths| {
            paths.iter().any(|path| {
                path.last().is_some_and(|name| self.names.contains(name))
                    || path
                        .first()
                        .is_some_and(|name| self.alias_relevant(name, &mut seen.clone()))
            })
        })
    }

    fn import_aliases(&mut self, items: &[Item]) {
        for item in items {
            if let Item::Use(item) = item {
                let mut leaves = Vec::new();
                n13_use_leaves(&item.tree, &[], &mut leaves);
                for leaf in leaves {
                    if let Some(alias) = leaf.alias {
                        self.aliases.entry(alias).or_default().insert(leaf.path);
                    }
                }
            }
        }
    }

    fn reference(&mut self, names: &[String], span: Span) {
        if self.pass != Pc2Pass::References {
            return;
        }
        let relevant = names.last().is_some_and(|name| self.names.contains(name))
            || names
                .first()
                .is_some_and(|name| self.alias_relevant(name, &mut BTreeSet::new()));
        if !relevant {
            return;
        }
        match self.resolve(names) {
            Ok(symbol) => self.facts.references.push(Pc2Reference {
                source: self.source.to_path_buf(),
                line: line_of(span),
                symbol,
                caller: self.caller(),
                test_only: self.test_only,
            }),
            Err(error) => self.error(span, error),
        }
    }

    fn macro_tokens(&mut self, stream: proc_macro2::TokenStream) {
        if let Ok(file) = syn::parse2::<syn::File>(stream.clone()) {
            if !file.items.is_empty() {
                self.visit_file(&file);
                return;
            }
        }
        if let Ok(block) = syn::parse2::<syn::Block>(stream.clone()) {
            self.visit_block(&block);
            return;
        }
        let wrapped: proc_macro2::TokenStream = std::iter::once(proc_macro2::TokenTree::Group(
            proc_macro2::Group::new(proc_macro2::Delimiter::Brace, stream.clone()),
        ))
        .collect();
        if let Ok(block) = syn::parse2::<syn::Block>(wrapped) {
            self.visit_block(&block);
            return;
        }
        if let Ok(expr) = syn::parse2::<Expr>(stream.clone()) {
            self.visit_expr(&expr);
            return;
        }
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            match &tokens[index] {
                proc_macro2::TokenTree::Group(group) => self.macro_tokens(group.stream()),
                proc_macro2::TokenTree::Ident(ident) => {
                    let mut names = vec![ident.to_string()];
                    let mut next = index + 1;
                    while next + 2 < tokens.len()
                        && matches!(&tokens[next], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                        && matches!(&tokens[next + 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                    {
                        let proc_macro2::TokenTree::Ident(ident) = &tokens[next + 2] else {
                            break;
                        };
                        names.push(ident.to_string());
                        next += 3;
                    }
                    if self.names.contains(&ident.to_string())
                        && index > 0
                        && matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == '.')
                    {
                        self.error(
                            ident.span(),
                            "PC2_AMBIGUOUS_RECEIVER: macro method receiver requires review".into(),
                        );
                    } else {
                        self.reference(&names, ident.span());
                    }
                    index = next;
                    continue;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

impl<'ast> Visit<'ast> for Pc2Visitor<'_> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        self.import_aliases(&file.items);
        visit::visit_file(self, file);
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let previous = self.aliases.clone();
        let previous_locals = self.locals.clone();
        for stmt in &block.stmts {
            if let syn::Stmt::Item(item) = stmt {
                self.import_aliases(std::slice::from_ref(item));
            }
        }
        visit::visit_block(self, block);
        self.aliases = previous;
        self.locals = previous_locals;
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if let Some((_, items)) = &module.content {
            let previous = self.test_only;
            self.test_only |= pc2_test_attrs(&module.attrs);
            self.module.push(module.ident.to_string());
            let old_aliases = std::mem::take(&mut self.aliases);
            self.import_aliases(items);
            for item in items {
                self.visit_item(item);
            }
            self.aliases = old_aliases;
            self.module.pop();
            self.test_only = previous;
        }
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.current_impl.take();
        self.current_impl = match &*item.self_ty {
            syn::Type::Path(path) if path.qself.is_none() => {
                path.path.segments.last().map(|s| s.ident.to_string())
            }
            _ => None,
        };
        let old_test = self.test_only;
        self.test_only |= pc2_test_attrs(&item.attrs);
        visit::visit_item_impl(self, item);
        self.test_only = old_test;
        self.current_impl = previous;
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.current_fn.replace(item.sig.ident.to_string());
        let previous_locals = self.locals.clone();
        self.function_parameters(&item.sig.inputs);
        let old_test = self.test_only;
        self.test_only |= pc2_test_attrs(&item.attrs);
        if self.pass == Pc2Pass::Declarations {
            if let Some(site) = self.site(&item.sig.ident.to_string()) {
                self.facts.definitions.push(Pc2Definition {
                    symbol: Pc2Function {
                        owner: self.module.clone(),
                        name: item.sig.ident.to_string(),
                    },
                    site,
                    test_only: self.test_only,
                });
            }
        }
        let old_aliases = self.aliases.clone();
        visit::visit_item_fn(self, item);
        self.aliases = old_aliases;
        self.test_only = old_test;
        self.current_fn = previous;
        self.locals = previous_locals;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let name = self
            .current_impl
            .as_ref()
            .map(|ty| format!("{ty}::{}", item.sig.ident));
        let previous = std::mem::replace(&mut self.current_fn, name);
        let previous_locals = self.locals.clone();
        self.function_parameters(&item.sig.inputs);
        let old_test = self.test_only;
        self.test_only |= pc2_test_attrs(&item.attrs);
        visit::visit_impl_item_fn(self, item);
        self.test_only = old_test;
        self.current_fn = previous;
        self.locals = previous_locals;
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if self.pass == Pc2Pass::Declarations {
            self.facts
                .all_statics
                .insert((self.module.clone(), item.ident.to_string()));
            if matches!(&*item.ty, syn::Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "OnceLock"))
            {
                self.facts
                    .once_statics
                    .insert((self.module.clone(), item.ident.to_string()));
            }
        }
        visit::visit_item_static(self, item);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        n13_use_leaves(&item.tree, &[], &mut leaves);
        for leaf in leaves {
            if let Some(alias) = leaf.alias {
                if leaf
                    .path
                    .last()
                    .is_some_and(|name| self.names.contains(name))
                {
                    // Imports grant no call permission. Retain the exact path
                    // so later uses of a renamed function receive the same check.
                    if self.pass == Pc2Pass::References {
                        if let Err(error) =
                            pc2_resolve_function(self.catalog, &self.module, &leaf.path)
                        {
                            self.error(item.use_token.span, error);
                        }
                    }
                }
                self.aliases.entry(alias).or_default().insert(leaf.path);
            }
        }
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let names = path_names(&path.path);
        if path.qself.is_some() && names.last().is_some_and(|name| self.names.contains(name)) {
            self.error(
                path.path
                    .segments
                    .last()
                    .map(|s| s.ident.span())
                    .unwrap_or_else(Span::call_site),
                "PC2_AMBIGUOUS_RECEIVER: qualified-self installer reference requires review".into(),
            );
        } else {
            self.reference(
                &names,
                path.path
                    .segments
                    .last()
                    .map(|s| s.ident.span())
                    .unwrap_or_else(Span::call_site),
            );
        }
        visit::visit_expr_path(self, path);
    }
    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let previous = self.locals.clone();
        for pattern in &closure.inputs {
            for name in pc2_pattern_names(pattern) {
                self.locals.insert(name, Pc2CellOrigin::Unresolved);
            }
        }
        visit::visit_expr_closure(self, closure);
        self.locals = previous;
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        let previous = self.locals.clone();
        for name in pc2_pattern_names(&expr.pat) {
            self.locals.insert(name, Pc2CellOrigin::Unresolved);
        }
        self.visit_block(&expr.body);
        self.locals = previous;
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let previous = self.locals.clone();
        for name in pc2_pattern_names(&arm.pat) {
            self.locals.insert(name, Pc2CellOrigin::Unresolved);
        }
        visit::visit_arm(self, arm);
        self.locals = previous;
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        visit::visit_local(self, local);
        let origin = local
            .init
            .as_ref()
            .map(|init| self.origin(&init.expr))
            .unwrap_or(Pc2CellOrigin::Unresolved);
        let names = pc2_pattern_names(&local.pat);
        let simple = matches!(&local.pat, syn::Pat::Ident(_) | syn::Pat::Type(_));
        for name in names {
            self.locals.insert(
                name,
                if simple {
                    origin.clone()
                } else {
                    Pc2CellOrigin::Unresolved
                },
            );
        }
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if self.pass != Pc2Pass::Declarations {
            if let Expr::Path(path) = &*call.func {
                let names = path_names(&path.path);
                if let Ok(function) = self.resolve(&names) {
                    let summary = self
                        .facts
                        .helper_writes
                        .get(&function)
                        .cloned()
                        .unwrap_or_default();
                    for index in summary {
                        if let Some(argument) = call.args.iter().nth(index.0) {
                            self.forward_write(
                                self.origin(argument),
                                path.path
                                    .segments
                                    .last()
                                    .map(|s| s.ident.span())
                                    .unwrap_or_else(Span::call_site),
                            );
                        } else {
                            self.error(
                                call.paren_token.span.open(),
                                "PC2_UNRESOLVED_REFERENCE: missing typed OnceLock helper argument"
                                    .into(),
                            );
                        }
                    }
                } else if names.last().is_some_and(|name| {
                    self.facts
                        .helper_writes
                        .keys()
                        .any(|helper| helper.name == *name)
                }) {
                    self.error(
                        path.path
                            .segments
                            .last()
                            .map(|s| s.ident.span())
                            .unwrap_or_else(Span::call_site),
                        "PC2_UNRESOLVED_REFERENCE: typed OnceLock helper call cannot be resolved"
                            .into(),
                    );
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if self.pass == Pc2Pass::References && self.names.contains(&call.method.to_string()) {
            self.error(
                call.method.span(),
                format!(
                    "PC2_AMBIGUOUS_RECEIVER: method {} needs a resolved receiver",
                    call.method
                ),
            );
        }
        if self.pass != Pc2Pass::Declarations && call.method == "set" {
            let origin = self.origin(&call.receiver);
            let install_name = self.current_fn.as_deref().is_some_and(|name| {
                name.split("::")
                    .last()
                    .is_some_and(|name| name.starts_with("install"))
            });
            if matches!(
                origin,
                Pc2CellOrigin::Static(_) | Pc2CellOrigin::Parameter(_)
            ) {
                self.forward_write(origin, call.method.span());
            } else if install_name && matches!(&*call.receiver, Expr::Path(_)) {
                self.forward_write(origin, call.method.span());
            }
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.macro_tokens(mac.tokens.clone());
    }
}

fn pc2_same_site(a: &Pc2Site, b: &Pc2Site) -> bool {
    a.file == b.file && a.function == b.function
}

fn pc2_unique_definition<'a>(
    facts: &'a Pc2Facts,
    site: &Pc2Site,
) -> Result<&'a Pc2Definition, String> {
    let candidates: Vec<_> = facts
        .definitions
        .iter()
        .filter(|d| pc2_same_site(&d.site, site))
        .collect();
    if candidates.len() == 1 {
        Ok(candidates[0])
    } else {
        Err(format!(
            "PC2_UNRESOLVED_REFERENCE: adapter {}:{} has {} definitions",
            site.file,
            site.function,
            candidates.len()
        ))
    }
}

#[derive(Clone)]
enum Pc2Permission {
    ProductionSite(Pc2Site),
    ExactTestSite(Pc2Site),
    AnyTest,
}
impl Pc2Permission {
    fn allows(&self, reference: &Pc2Reference) -> bool {
        match self {
            Self::ProductionSite(site) => reference
                .caller
                .as_ref()
                .is_some_and(|caller| pc2_same_site(caller, site)),
            Self::ExactTestSite(site) => {
                reference.test_only
                    && reference
                        .caller
                        .as_ref()
                        .is_some_and(|caller| pc2_same_site(caller, site))
            }
            Self::AnyTest => reference.test_only,
        }
    }
}

fn pc2_collect(
    workspace: &Path,
    paths: &BTreeSet<PathBuf>,
    catalog: &N13Modules,
    names: &BTreeSet<String>,
    test_modules: &BTreeSet<Vec<String>>,
    pass: Pc2Pass,
    facts: &mut Pc2Facts,
) -> Result<(), String> {
    for path in paths {
        let (text, parsed) = parse_file(path)?;
        let modules = catalog.files.get(path);
        let Some(module) = modules
            .filter(|modules| modules.len() == 1)
            .and_then(|modules| modules.iter().next())
        else {
            if !count_occurrences(&cleaned_code(&text), names).is_empty()
                || text.contains("OnceLock") && text.contains("fn install")
            {
                facts.errors.push(format!(
                    "PC2_UNRESOLVED_REFERENCE: {} has unknown/ambiguous module {modules:?}",
                    path.display()
                ));
            }
            continue;
        };
        let test_only = module
            .first()
            .is_some_and(|key| key.contains("@test:") || key.contains("@target:tests/"))
            || test_modules.iter().any(|prefix| module.starts_with(prefix));
        let mut visitor = Pc2Visitor {
            workspace,
            source: path,
            catalog,
            names,
            module: module.clone(),
            current_fn: None,
            current_impl: None,
            test_only,
            aliases: BTreeMap::new(),
            locals: BTreeMap::new(),
            facts,
            pass,
        };
        visitor.visit_file(&parsed);
    }
    Ok(())
}

pub(crate) fn scan_pc2_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc2_table(text)?;
    let sources = discover_sources(workspace)?;
    let catalog = n13_module_catalog(&sources)?;
    let mut names: BTreeSet<_> = rows
        .iter()
        .filter_map(|row| row.installer.last().cloned())
        .collect();
    for row in &rows {
        let adapter = match &row.kind {
            Pc2Kind::Production { adapter, .. } => adapter.as_ref(),
            Pc2Kind::TestOnly { adapter, .. } => Some(adapter),
            Pc2Kind::DataInstall { .. } => None,
        };
        if let Some(adapter) = adapter {
            if let Some(name) = adapter.function.split("::").last() {
                names.insert(name.to_string());
            }
        }
    }
    let mut paths: BTreeSet<_> = catalog.files.keys().cloned().collect();
    for source in &sources {
        paths.insert(
            source
                .path
                .canonicalize()
                .map_err(|error| format!("PC2_SOURCE: {error}"))?,
        );
    }
    let mut test_modules = BTreeSet::new();
    fn gated(
        items: &[Item],
        module: &mut Vec<String>,
        inherited: bool,
        out: &mut BTreeSet<Vec<String>>,
    ) {
        for item in items {
            if let Item::Mod(item) = item {
                module.push(item.ident.to_string());
                let gate = inherited || pc2_test_attrs(&item.attrs);
                if gate {
                    out.insert(module.clone());
                }
                if let Some((_, items)) = &item.content {
                    gated(items, module, gate, out);
                }
                module.pop();
            }
        }
    }
    for path in &paths {
        let (_, parsed) = parse_file(path)?;
        if let Some(modules) = catalog.files.get(path) {
            for module in modules {
                gated(&parsed.items, &mut module.clone(), false, &mut test_modules);
            }
        }
    }
    let mut facts = Pc2Facts::default();
    pc2_collect(
        workspace,
        &paths,
        &catalog,
        &names,
        &test_modules,
        Pc2Pass::Declarations,
        &mut facts,
    )?;
    let mut protected = BTreeSet::new();
    for row in &rows {
        if let Some(root) = row.installer.first() {
            protected.insert(pc2_resolve_function(
                &catalog,
                &[root.clone()],
                &row.installer,
            )?);
        }
        let site = match &row.kind {
            Pc2Kind::Production { adapter, .. } => adapter.as_ref(),
            Pc2Kind::TestOnly { adapter, .. } => Some(adapter),
            Pc2Kind::DataInstall { .. } => None,
        };
        if let Some(site) = site {
            protected.insert(pc2_unique_definition(&facts, site)?.symbol.clone());
        }
    }
    // Cross-file renamed re-exports join the watched names through canonical
    // declaration identity. A fresh spelling never evades the executable fence.
    for (module, info) in &catalog.modules {
        for import in &info.imports {
            let (Some(alias), Some(original)) = (&import.alias, import.path.last()) else {
                continue;
            };
            let targets = catalog.function_identities(
                module,
                &import.path[..import.path.len() - 1],
                original,
            );
            if targets.iter().any(|target| {
                protected.contains(&Pc2Function {
                    owner: target.module.clone(),
                    name: target.name.clone(),
                })
            }) {
                if targets.len() != 1 {
                    facts.errors.push(format!(
                        "PC2_AMBIGUOUS_RECEIVER: imported alias {}::{alias}",
                        module.join("::")
                    ));
                }
                names.insert(alias.clone());
            }
        }
    }
    // Typed helper summaries are monotone finite sets of formal argument
    // indices. Iterate to a fixed point to cover helper-to-helper forwarding.
    loop {
        let before = facts.helper_writes.clone();
        pc2_collect(
            workspace,
            &paths,
            &catalog,
            &names,
            &test_modules,
            Pc2Pass::HelperSummaries,
            &mut facts,
        )?;
        if facts.helper_writes == before {
            break;
        }
    }
    pc2_collect(
        workspace,
        &paths,
        &catalog,
        &names,
        &test_modules,
        Pc2Pass::References,
        &mut facts,
    )?;
    let mut violations = facts.errors.clone();
    let mut covered = BTreeSet::new();
    let mut low_permissions: BTreeMap<Pc2Function, Vec<Pc2Permission>> = BTreeMap::new();
    let mut adapter_permissions: BTreeMap<Pc2Function, Vec<Pc2Permission>> = BTreeMap::new();
    let mut resolved_rows = Vec::new();
    for row in &rows {
        let root = row
            .installer
            .first()
            .ok_or_else(|| "PC2_TABLE: installer root".to_string())?;
        let installer = pc2_resolve_function(&catalog, &[root.clone()], &row.installer)?;
        let static_root = row
            .static_path
            .first()
            .ok_or_else(|| "PC2_TABLE: static root".to_string())?;
        let cell = pc2_static_path(&catalog, &facts, &[static_root.clone()], &row.static_path)?;
        covered.insert((cell.clone(), installer.clone()));
        let adapter_site = match &row.kind {
            Pc2Kind::Production { adapter, .. } => adapter.as_ref(),
            Pc2Kind::TestOnly { adapter, .. } => Some(adapter),
            Pc2Kind::DataInstall { .. } => None,
        };
        let adapter = adapter_site
            .map(|site| pc2_unique_definition(&facts, site))
            .transpose()?;
        let low = low_permissions.entry(installer.clone()).or_default();
        match &row.kind {
            Pc2Kind::Production {
                adapter: Some(site),
                seams,
            } => {
                low.push(Pc2Permission::ProductionSite(site.clone()));
                let adapter = adapter
                    .ok_or_else(|| "PC2_UNRESOLVED_REFERENCE: production adapter".to_string())?;
                let permissions = adapter_permissions
                    .entry(adapter.symbol.clone())
                    .or_default();
                for seam in seams {
                    permissions.push(Pc2Permission::ProductionSite(seam.clone()));
                }
            }
            Pc2Kind::Production {
                adapter: None,
                seams,
            } => {
                for seam in seams {
                    low.push(Pc2Permission::ProductionSite(seam.clone()));
                }
            }
            Pc2Kind::DataInstall { seam } => low.push(Pc2Permission::ProductionSite(seam.clone())),
            Pc2Kind::TestOnly {
                adapter: site,
                fence: Pc2TestFence::AdapterOnly,
            } => {
                let adapter =
                    adapter.ok_or_else(|| "PC2_UNRESOLVED_REFERENCE: test adapter".to_string())?;
                if !adapter.test_only {
                    violations.push(format!(
                        "PC2_TEST_ONLY_DEFINITION: {}:{} requires test/test-support cfg",
                        site.file, site.function
                    ));
                }
                low.push(Pc2Permission::ExactTestSite(site.clone()));
                adapter_permissions
                    .entry(adapter.symbol.clone())
                    .or_default()
                    .push(Pc2Permission::AnyTest);
            }
            Pc2Kind::TestOnly {
                adapter: site,
                fence: Pc2TestFence::ExactCaller(caller),
            } => {
                let adapter = adapter.ok_or_else(|| {
                    "PC2_UNRESOLVED_REFERENCE: production adapter for exact test caller".to_string()
                })?;
                let caller_definition = pc2_unique_definition(&facts, caller)?;
                if !caller_definition.test_only {
                    violations.push(format!(
                        "PC2_TEST_ONLY_DEFINITION: exact caller {}:{} lacks test cfg",
                        caller.file, caller.function
                    ));
                }
                let paired = rows.iter().any(|production| production.static_path == row.static_path && production.installer == row.installer && matches!(&production.kind, Pc2Kind::Production { adapter: Some(other), .. } if pc2_same_site(other, site)));
                if !paired {
                    violations.push(format!(
                        "PC2_TABLE: exact test caller {} needs paired production row",
                        caller.function
                    ));
                }
                low.push(Pc2Permission::ProductionSite(site.clone()));
                adapter_permissions
                    .entry(adapter.symbol.clone())
                    .or_default()
                    .push(Pc2Permission::ExactTestSite(caller.clone()));
            }
        }
        resolved_rows.push((row, cell, installer, adapter));
    }
    for (row, cell, installer, adapter) in &resolved_rows {
        if !facts
            .writes
            .iter()
            .any(|write| &write.cell == cell && &write.writer == installer)
        {
            violations.push(format!(
                "PC2_STATIC_WRITER_MISMATCH: {}::{} has no proved write of {}::{}",
                installer.owner.join("::"),
                installer.name,
                cell.0.owner.join("::"),
                cell.0.name
            ));
        }
        let adapter_site = match &row.kind {
            Pc2Kind::Production { adapter, .. } => adapter.as_ref(),
            Pc2Kind::TestOnly { adapter, .. } => Some(adapter),
            Pc2Kind::DataInstall { .. } => None,
        };
        if let Some(site) = adapter_site {
            if !facts.references.iter().any(|reference| {
                &reference.symbol == installer
                    && reference
                        .caller
                        .as_ref()
                        .is_some_and(|caller| pc2_same_site(caller, site))
            }) {
                violations.push(format!(
                    "PC2_MISSING_CHAIN_REFERENCE: {}:{}",
                    site.file, site.function
                ));
            }
        }
        let expected: Vec<&Pc2Site> = match &row.kind {
            Pc2Kind::Production { seams, .. } => seams.iter().collect(),
            Pc2Kind::DataInstall { seam } => vec![seam],
            Pc2Kind::TestOnly {
                fence: Pc2TestFence::ExactCaller(caller),
                ..
            } => vec![caller],
            Pc2Kind::TestOnly {
                fence: Pc2TestFence::AdapterOnly,
                ..
            } => Vec::new(),
        };
        let target = adapter
            .map(|definition| &definition.symbol)
            .unwrap_or(installer);
        for seam in expected {
            if !facts.references.iter().any(|reference| {
                &reference.symbol == target
                    && reference
                        .caller
                        .as_ref()
                        .is_some_and(|caller| pc2_same_site(caller, seam))
            }) {
                violations.push(format!(
                    "PC2_MISSING_SEAM_REFERENCE: {}:{}",
                    seam.file, seam.function
                ));
            }
        }
    }
    let mut installer_statics: BTreeMap<Pc2Function, BTreeSet<Pc2Static>> = BTreeMap::new();
    for (cell, installer) in &covered {
        installer_statics
            .entry(installer.clone())
            .or_default()
            .insert(cell.clone());
    }
    for (installer, cells) in installer_statics {
        if cells.len() > 1 {
            violations.push(format!("PC2_AMBIGUOUS_STATIC_SET: {}::{} has multiple table statics {cells:?}; review caller permissions", installer.owner.join("::"), installer.name));
        }
    }
    for reference in &facts.references {
        if let Some(permissions) = low_permissions.get(&reference.symbol) {
            if !permissions
                .iter()
                .any(|permission| permission.allows(reference))
            {
                violations.push(format!(
                    "{}:{}: PC2_INSTALLER_OUTSIDE_EXACT_CHAIN: {}::{} from {:?}",
                    reference.source.display(),
                    reference.line,
                    reference.symbol.owner.join("::"),
                    reference.symbol.name,
                    reference.caller
                ));
            }
        }
        if let Some(permissions) = adapter_permissions.get(&reference.symbol) {
            if !permissions
                .iter()
                .any(|permission| permission.allows(reference))
            {
                violations.push(format!(
                    "{}:{}: PC2_ADAPTER_OUTSIDE_EXACT_SEAMS: {}::{} from {:?}",
                    reference.source.display(),
                    reference.line,
                    reference.symbol.owner.join("::"),
                    reference.symbol.name,
                    reference.caller
                ));
            }
        }
    }
    for write in &facts.writes {
        let sealed = write.cell.0.owner.first().is_some_and(|root| {
            PC2_SEALED_LOWER_CRATES
                .iter()
                .any(|sealed| root == sealed.crate_name())
        });
        let installer_name = write
            .writer
            .name
            .split("::")
            .last()
            .is_some_and(|name| name.starts_with("install"));
        let table_static = covered.iter().any(|(cell, _)| cell == &write.cell);
        if ((sealed && installer_name) || table_static)
            && !covered.contains(&(write.cell.clone(), write.writer.clone()))
        {
            violations.push(format!(
                "{}:{}: PC2_UNLISTED_ONCELOCK_INSTALLER: {}::{} written by {}::{}",
                write.source.display(),
                write.line,
                write.cell.0.owner.join("::"),
                write.cell.0.name,
                write.writer.owner.join("::"),
                write.writer.name
            ));
        }
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}

#[test]
fn architecture_pc2_hook_installers_have_exactly_two_seams() {
    match scan_pc2_table(&workspace_root(), PC2_TABLE) {
        Ok(violations) if violations.is_empty() => {}
        Ok(violations) => panic!(
            "{} PC2 hook-installer violations:\n{}",
            violations.len(),
            violations.join("\n")
        ),
        Err(error) => panic!("PC2 table: {error}"),
    }
}

#[test]
fn architecture_pc2_guard_self_test() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for package in ["neovm-core", "neovm-value"] {
        let member = root.join("crates").join(package);
        std::fs::create_dir_all(member.join("src")).unwrap();
        std::fs::write(
            member.join("Cargo.toml"),
            format!("[package]\nname = {package:?}\nversion = \"0.0.0\"\n"),
        )
        .unwrap();
        std::fs::write(member.join("src/lib.rs"), "").unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/neovm-core\", \"crates/neovm-value\"]\n",
    )
    .unwrap();
    let core = root.join("crates/neovm-core/src");
    std::fs::write(
        core.join("lib.rs"),
        "mod low; mod adapter; mod seam; mod external;\n",
    )
    .unwrap();
    std::fs::write(core.join("low.rs"), "use std::sync::OnceLock; static HOOK: OnceLock<fn()> = OnceLock::new(); pub fn install_hook(f: fn()) { let _ = HOOK.set(f); }\n").unwrap();
    std::fs::write(
        core.join("adapter.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .unwrap();
    let seams = "pub struct Context; impl Context { pub fn new_inner() { crate::adapter::install(); } pub fn from_dump() { crate::adapter::install(); } }\n";
    std::fs::write(core.join("seam.rs"), seams).unwrap();
    std::fs::write(core.join("external.rs"), "fn unrelated() {}\n").unwrap();
    let table = "kind\tstatic\tinstaller\tadapter\tseams\nproduction\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/adapter.rs:install\tcrates/neovm-core/src/seam.rs:Context::new_inner;crates/neovm-core/src/seam.rs:Context::from_dump\n";
    let green = scan_pc2_table(root, table).unwrap();
    assert!(green.is_empty(), "PC2 baseline: {green:?}");
    for (witness, source) in [
        (
            "INSTALLER_ESCAPE",
            "fn third() { crate::low::install_hook(|| {}); }",
        ),
        (
            "ADAPTER_THIRD_CALL",
            "fn third() { crate::adapter::install(); }",
        ),
        (
            "RENAMED_ITEM_REF",
            "use crate::adapter::install as renamed; fn third() { let p = renamed; }",
        ),
        (
            "MACRO_THIRD_CALL",
            "fn third() { sample!({ crate::adapter::install(); }); }",
        ),
        ("AMBIGUOUS_RECEIVER", "fn third(v: V) { v.install_hook(); }"),
        (
            "UNRESOLVED_REFERENCE",
            "fn third() { unknown::install_hook(); }",
        ),
        (
            "UNLISTED_ONCELOCK",
            "use std::sync::OnceLock; static NEW: OnceLock<fn()> = OnceLock::new(); fn install_unlisted(f: fn()) { let _ = NEW.set(f); }",
        ),
    ] {
        let mutation = if witness == "UNLISTED_ONCELOCK" {
            root.join("crates/neovm-value/src/lib.rs")
        } else {
            core.join("external.rs")
        };
        std::fs::write(&mutation, source).unwrap();
        let red = scan_pc2_table(root, table).unwrap();
        assert!(!red.is_empty(), "PC2_MUTATION_{witness}: {red:?}");
        std::fs::write(
            &mutation,
            if witness == "UNLISTED_ONCELOCK" {
                ""
            } else {
                "fn unrelated() {}\n"
            },
        )
        .unwrap();
        assert!(
            scan_pc2_table(root, table).unwrap().is_empty(),
            "PC2_RESTORED_{witness}"
        );
    }
    for invalid in [
        "",
        "kind\tstatic\tinstaller\tadapter\tseams\nunknown\tx::CELL\tx::install\t-\t-\n",
        "kind\tstatic\tinstaller\tadapter\tseams\ntest_only\tx::CELL\tx::install\t-\t-\n",
    ] {
        assert!(
            parse_pc2_table(invalid).is_err(),
            "PC2_MUTATION_MALFORMED_ROW"
        );
    }
    let test_table = "kind\tstatic\tinstaller\tadapter\tseams\ntest_only\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/adapter.rs:install\t-\n";
    std::fs::write(core.join("seam.rs"), "pub struct Context;\n").unwrap();
    std::fs::write(core.join("adapter.rs"), "#[cfg(any(test, feature = \"test-support\"))] pub fn install() { crate::low::install_hook(|| {}); }\n").unwrap();
    std::fs::write(
        core.join("external.rs"),
        "#[test] fn test_caller() { crate::adapter::install(); }\n",
    )
    .unwrap();
    assert!(
        scan_pc2_table(root, test_table).unwrap().is_empty(),
        "PC2_TEST_ONLY_GREEN"
    );
    std::fs::write(
        core.join("adapter.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .unwrap();
    assert!(
        scan_pc2_table(root, test_table)
            .unwrap()
            .iter()
            .any(|e| e.contains("PC2_TEST_ONLY_DEFINITION")),
        "PC2_MUTATION_TEST_ONLY_CFG"
    );
    std::fs::write(
        core.join("adapter.rs"),
        "#[cfg(test)] pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .unwrap();
    assert!(
        scan_pc2_table(root, test_table).unwrap().is_empty(),
        "PC2_RESTORED_TEST_ONLY_CFG"
    );
    std::fs::write(
        core.join("external.rs"),
        "fn product_caller() { crate::adapter::install(); }\n",
    )
    .unwrap();
    assert!(
        scan_pc2_table(root, test_table)
            .unwrap()
            .iter()
            .any(|e| e.contains("PC2_ADAPTER_OUTSIDE_EXACT_SEAMS")),
        "PC2_MUTATION_TEST_ONLY_PRODUCT_REF"
    );
    std::fs::write(
        core.join("external.rs"),
        "#[test] fn test_caller() { crate::adapter::install(); }\n",
    )
    .unwrap();
    assert!(
        scan_pc2_table(root, test_table).unwrap().is_empty(),
        "PC2_RESTORED_TEST_ONLY_PRODUCT_REF"
    );
}

#[test]
fn architecture_pc2_data_install_mutation_red_then_restored() {
    let dir = tempfile::tempdir().expect("DATA fixture");
    let root = dir.path();
    guard_fixture_workspace(root, &["neovm-core", "neovm-value"]);
    let core = root.join("crates/neovm-core/src");
    std::fs::write(core.join("lib.rs"), "mod low; mod seam; mod extra;\n").expect("modules");
    std::fs::write(core.join("low.rs"), "use std::sync::OnceLock; static DATA: OnceLock<u8> = OnceLock::new(); pub fn install_slot_meta_table(value: u8) { let _ = DATA.set(value); }\n").expect("DATA installer");
    std::fs::write(
        core.join("seam.rs"),
        "pub fn buffer_slot_sym_map() { crate::low::install_slot_meta_table(1); }\n",
    )
    .expect("one DATA seam");
    let extra = core.join("extra.rs");
    std::fs::write(&extra, "fn unrelated() {}\n").expect("clean extra");
    let table = "kind\tstatic\tinstaller\tadapter\tseams\ndata_install\tneovm_core::low::DATA\tneovm_core::low::install_slot_meta_table\t-\tcrates/neovm-core/src/seam.rs:buffer_slot_sym_map\n";
    let green = scan_pc2_table(root, table).expect("DATA baseline scan");
    assert!(green.is_empty(), "PC2_DATA_BASELINE: {green:?}");
    std::fs::write(
        &extra,
        "fn second_caller() { crate::low::install_slot_meta_table(2); }\n",
    )
    .expect("second DATA caller mutation");
    let red = scan_pc2_table(root, table).expect("DATA mutation scan");
    assert!(
        red.iter()
            .any(|error| error.contains("PC2_INSTALLER_OUTSIDE_EXACT_CHAIN")
                && error.contains("second_caller")),
        "PC2_DATA_SECOND_CALLER: {red:?}"
    );
    std::fs::write(&extra, "fn unrelated() {}\n").expect("restore DATA caller");
    let restored = scan_pc2_table(root, table).expect("DATA restored scan");
    assert!(restored.is_empty(), "PC2_DATA_RESTORED: {restored:?}");
    for invalid in [
        table.replace(
            ":buffer_slot_sym_map\n",
            ":buffer_slot_sym_map;crates/neovm-core/src/extra.rs:second_caller\n",
        ),
        table.replace(
            "\t-\tcrates",
            "\tcrates/neovm-core/src/extra.rs:second_caller\tcrates",
        ),
    ] {
        assert!(
            parse_pc2_table(&invalid).is_err(),
            "PC2_DATA_REJECTS_EXTRA_SEAM_OR_ADAPTER"
        );
    }
}

fn pc2_sealed_fixture(root: &Path) -> String {
    guard_fixture_workspace(root, &["neovm-core", "neovm-value"]);
    let core = root.join("crates/neovm-core/src");
    std::fs::write(
        core.join("lib.rs"),
        "mod low; mod adapter; mod seam; mod external;\n",
    )
    .unwrap();
    std::fs::write(core.join("low.rs"), "use std::sync::OnceLock; static HOOK: OnceLock<fn()> = OnceLock::new(); pub fn install_hook(f: fn()) { let _ = HOOK.set(f); }\n").unwrap();
    std::fs::write(
        core.join("adapter.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .unwrap();
    std::fs::write(core.join("seam.rs"), "pub struct Context; impl Context { pub fn new_inner() { crate::adapter::install(); } pub fn from_dump() { crate::adapter::install(); } }\n").unwrap();
    std::fs::write(core.join("external.rs"), "fn clean() {}\n").unwrap();
    "kind\tstatic\tinstaller\tadapter\tseams\nproduction\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/adapter.rs:install\tcrates/neovm-core/src/seam.rs:Context::new_inner;crates/neovm-core/src/seam.rs:Context::from_dump\n".into()
}

#[test]
fn architecture_pc2_sqlite_exact_caller_mutation_red_then_restored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut table = pc2_sealed_fixture(root);
    let core = root.join("crates/neovm-core/src");
    std::fs::write(core.join("seam.rs"), "pub struct Context; impl Context { pub fn new_inner() { crate::adapter::install(); } pub fn from_dump() { crate::adapter::install(); } } pub fn make_db_handle() { crate::adapter::install(); } pub fn make_stmt_handle() { crate::adapter::install(); }\n").unwrap();
    table = table.replace("Context::from_dump\n", "Context::from_dump;crates/neovm-core/src/seam.rs:make_db_handle;crates/neovm-core/src/seam.rs:make_stmt_handle\n");
    table.push_str("test_only\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/adapter.rs:install\tcrates/neovm-core/src/external.rs:installed_finalizer_removes_both_registry_kinds\n");
    table.push_str("test_only\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/adapter.rs:install\tcrates/neovm-core/src/external.rs:register_heap_sqlite_pair\n");
    let exact = "#[test] fn installed_finalizer_removes_both_registry_kinds() { crate::adapter::install(); } #[cfg(test)] fn register_heap_sqlite_pair() { crate::adapter::install(); }\n";
    std::fs::write(core.join("external.rs"), exact).unwrap();
    assert!(
        scan_pc2_table(root, &table).unwrap().is_empty(),
        "PC2_SQLITE_EXACT_BASELINE"
    );
    std::fs::write(
        core.join("external.rs"),
        format!("{exact} #[test] fn unexpected_test_caller() {{ crate::adapter::install(); }}\n"),
    )
    .unwrap();
    let red = scan_pc2_table(root, &table).unwrap();
    assert!(
        red.iter()
            .any(|error| error.contains("PC2_ADAPTER_OUTSIDE_EXACT_SEAMS")
                && error.contains("unexpected_test_caller")),
        "PC2_SQLITE_UNEXPECTED_TEST_CALLER: {red:?}"
    );
    std::fs::write(core.join("external.rs"), exact).unwrap();
    assert!(
        scan_pc2_table(root, &table).unwrap().is_empty(),
        "PC2_SQLITE_RESTORED"
    );
}

#[test]
fn architecture_pc2_generic_local_helper_is_not_a_static_write() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let table = pc2_sealed_fixture(root);
    let core = root.join("crates/neovm-core/src");
    std::fs::write(core.join("low.rs"), "use std::sync::OnceLock; static HOOK: OnceLock<fn()> = OnceLock::new(); pub fn install_once(slot: &OnceLock<fn()>, f: fn()) { let _ = slot.set(f); } pub fn install_hook(f: fn()) { install_once(&HOOK, f); }\n").unwrap();
    std::fs::write(core.join("external.rs"), "#[test] fn local_cells() { let slot = std::sync::OnceLock::new(); crate::low::install_once(&slot, || {}); }\n").unwrap();
    let green = scan_pc2_table(root, &table).unwrap();
    assert!(green.is_empty(), "PC2_LOCAL_HELPER_POSITIVE: {green:?}");
    std::fs::write(root.join("crates/neovm-value/src/lib.rs"), "use std::sync::OnceLock; static UNLISTED: OnceLock<fn()> = OnceLock::new(); fn install_unlisted(f: fn()) { crate::write_cell(&UNLISTED, f); } fn write_cell(slot: &OnceLock<fn()>, f: fn()) { let _ = slot.set(f); }\n").unwrap();
    let red = scan_pc2_table(root, &table).unwrap();
    assert!(
        red.iter()
            .any(|error| error.contains("PC2_UNLISTED_ONCELOCK_INSTALLER")
                && error.contains("UNLISTED")),
        "PC2_UNLISTED_VALUE_FORWARDED_STATIC: {red:?}"
    );
    std::fs::write(root.join("crates/neovm-value/src/lib.rs"), "").unwrap();
    assert!(
        scan_pc2_table(root, &table).unwrap().is_empty(),
        "PC2_LOCAL_HELPER_RESTORED"
    );
}

// ---------------------------------------------------------------------------
// TA2: workspace-wide authority references, with defining-module resolution.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Ta2Definitions {
    authority_files: BTreeSet<PathBuf>,
    // Only this declaration is exempt in its parent, never the parent file.
    declaration: Option<(PathBuf, usize)>,
}

fn ta2_error(path: &Path, line: usize, code: &str, detail: &str) -> String {
    format!("{}:{line}: {code}: {detail}", path.display())
}

fn ta2_manifest(root: &Path) -> Result<toml::Value, String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ta2_error(&path, 0, "TA2_MANIFEST", &e.to_string()))?;
    text.parse::<toml::Value>()
        .map_err(|e| ta2_error(&path, 0, "TA2_MANIFEST", &e.to_string()))
}

fn ta2_external_module(
    declaration: &syn::ItemMod,
    declaration_file: &Path,
    module_dir: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let mut explicit = None;
    for attr in &declaration.attrs {
        if attr.path().is_ident("path") {
            let syn::Meta::NameValue(value) = &attr.meta else {
                return Err(ta2_error(
                    declaration_file,
                    line_of(attr.span()),
                    "TA2_MODULE_UNRESOLVED",
                    "invalid #[path]",
                ));
            };
            let Expr::Lit(value) = &value.value else {
                return Err(ta2_error(
                    declaration_file,
                    line_of(attr.span()),
                    "TA2_MODULE_UNRESOLVED",
                    "nonliteral #[path]",
                ));
            };
            let syn::Lit::Str(value) = &value.lit else {
                return Err(ta2_error(
                    declaration_file,
                    line_of(attr.span()),
                    "TA2_MODULE_UNRESOLVED",
                    "nonstring #[path]",
                ));
            };
            if explicit.replace(value.value()).is_some() {
                return Err(ta2_error(
                    declaration_file,
                    line_of(attr.span()),
                    "TA2_MODULE_UNRESOLVED",
                    "multiple #[path] attributes",
                ));
            }
        }
        if attr.path().is_ident("cfg_attr") {
            let text = attr
                .meta
                .require_list()
                .map_err(|e| e.to_string())?
                .tokens
                .to_string();
            if text
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|s| s == "path")
            {
                return Err(ta2_error(
                    declaration_file,
                    line_of(attr.span()),
                    "TA2_MODULE_UNRESOLVED",
                    "conditional #[path] requires review",
                ));
            }
        }
    }
    let path = if let Some(explicit) = explicit {
        module_dir.join(explicit)
    } else {
        let name = declaration.ident.to_string();
        let flat = module_dir.join(format!("{name}.rs"));
        let nested = module_dir.join(&name).join("mod.rs");
        match (flat.try_exists(), nested.try_exists()) {
            (Ok(true), Ok(false)) => flat,
            (Ok(false), Ok(true)) => nested,
            (Ok(a), Ok(b)) => {
                return Err(ta2_error(
                    declaration_file,
                    line_of(declaration.ident.span()),
                    "TA2_MODULE_UNRESOLVED",
                    &format!("module {name}: flat={a}, nested={b}"),
                ));
            }
            (a, b) => {
                return Err(ta2_error(
                    declaration_file,
                    0,
                    "TA2_MODULE_UNRESOLVED",
                    &format!("{a:?}; {b:?}"),
                ));
            }
        }
    };
    let path = path
        .canonicalize()
        .map_err(|e| ta2_error(&path, 0, "TA2_MODULE_UNRESOLVED", &e.to_string()))?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("TA2_MODULE_UNRESOLVED: {} has no parent", path.display()))?;
    let child_dir = if path.file_name().is_some_and(|s| s == "mod.rs") {
        parent.to_path_buf()
    } else {
        parent.join(
            path.file_stem()
                .ok_or_else(|| format!("TA2_MODULE_UNRESOLVED: {} has no stem", path.display()))?,
        )
    };
    Ok((path, child_dir))
}

fn ta2_authority_submodules(
    items: &[Item],
    file: &Path,
    module_dir: &Path,
    definitions: &mut Ta2Definitions,
    active: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for item in items {
        let Item::Mod(module) = item else { continue };
        if let Some((_, items)) = &module.content {
            ta2_authority_submodules(
                items,
                file,
                &module_dir.join(module.ident.to_string()),
                definitions,
                active,
            )?;
        } else {
            let (child, child_dir) = ta2_external_module(module, file, module_dir)?;
            if !active.insert(child.clone()) {
                return Err(ta2_error(
                    &child,
                    0,
                    "TA2_MODULE_UNRESOLVED",
                    "cyclic authority module graph",
                ));
            }
            definitions.authority_files.insert(child.clone());
            let (_, parsed) = parse_file(&child)?;
            ta2_authority_submodules(&parsed.items, &child, &child_dir, definitions, active)?;
            active.remove(&child);
        }
    }
    Ok(())
}

fn ta2_definitions(sources: &[GuardSource]) -> Result<Ta2Definitions, String> {
    let roots: BTreeSet<_> = sources
        .iter()
        .filter(|s| s.package == "neovm-value")
        .map(|s| s.member_root.clone())
        .collect();
    if roots.len() != 1 {
        return Err(format!(
            "TA2_MODULE_UNRESOLVED: expected one neovm-value member, found {}",
            roots.len()
        ));
    }
    let root = roots
        .iter()
        .next()
        .ok_or_else(|| "TA2_MODULE_UNRESOLVED: absent neovm-value".to_string())?;
    let manifest = ta2_manifest(root)?;
    let lib = manifest
        .get("lib")
        .and_then(|v| v.get("path"))
        .and_then(toml::Value::as_str)
        .unwrap_or("src/lib.rs");
    let lib = root
        .join(lib)
        .canonicalize()
        .map_err(|e| format!("TA2_MODULE_UNRESOLVED: value lib: {e}"))?;
    let (_, parsed) = parse_file(&lib)?;
    let support: Vec<_> = parsed
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Mod(m) if m.ident == "test_support" => Some(m),
            _ => None,
        })
        .collect();
    if support.len() != 1 {
        return Err(ta2_error(
            &lib,
            0,
            "TA2_MODULE_UNRESOLVED",
            "expected exactly one test_support declaration",
        ));
    }
    let support = support[0];
    let lib_dir = lib
        .parent()
        .ok_or_else(|| "TA2_MODULE_UNRESOLVED: lib parent".to_string())?;
    let (file, module_dir, items) = if let Some((_, items)) = &support.content {
        (lib.clone(), lib_dir.join("test_support"), items.clone())
    } else {
        let (file, dir) = ta2_external_module(support, &lib, lib_dir)?;
        let (_, parsed) = parse_file(&file)?;
        (file, dir, parsed.items)
    };
    let authority: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            Item::Mod(m) if m.ident == "authority" => Some(m),
            _ => None,
        })
        .collect();
    let mut definitions = Ta2Definitions::default();
    match authority.as_slice() {
        [] => {} // Before the signed authority surface is installed there is no exemption.
        [authority] => {
            definitions.declaration = Some((file.clone(), line_of(authority.ident.span())));
            if let Some((_, items)) = &authority.content {
                ta2_authority_submodules(
                    items,
                    &file,
                    &module_dir.join("authority"),
                    &mut definitions,
                    &mut BTreeSet::new(),
                )?;
            } else {
                let (path, dir) = ta2_external_module(authority, &file, &module_dir)?;
                definitions.authority_files.insert(path.clone());
                let (_, parsed) = parse_file(&path)?;
                let mut active = BTreeSet::from([path.clone()]);
                ta2_authority_submodules(
                    &parsed.items,
                    &path,
                    &dir,
                    &mut definitions,
                    &mut active,
                )?;
            }
        }
        _ => {
            return Err(ta2_error(
                &file,
                0,
                "TA2_MODULE_UNRESOLVED",
                "multiple authority declarations",
            ));
        }
    }
    Ok(definitions)
}

fn ta2_value_aliases(workspace: &Path, member: &Path) -> Result<BTreeSet<String>, String> {
    let manifest = ta2_manifest(member)?;
    let workspace_manifest = ta2_manifest(workspace)?;
    let inherited = workspace_manifest
        .get("workspace")
        .and_then(|v| v.get("dependencies"))
        .and_then(toml::Value::as_table);
    let mut names = BTreeSet::from(["neovm_value".to_string()]);
    fn table(
        table: &toml::map::Map<String, toml::Value>,
        inherited: Option<&toml::map::Map<String, toml::Value>>,
        names: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        for (alias, dependency) in table {
            let dependency =
                if dependency.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                    inherited
                        .and_then(|table| table.get(alias))
                        .ok_or_else(|| {
                            format!("TA2_MANIFEST: unresolved inherited dependency {alias}")
                        })?
                } else {
                    dependency
                };
            let package = dependency
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(alias);
            if package == "neovm-value" {
                names.insert(alias.replace('-', "_"));
            }
        }
        Ok(())
    }
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = manifest.get(kind).and_then(toml::Value::as_table) {
            table(dependencies, inherited, &mut names)?;
        }
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for target in targets.values() {
                if let Some(dependencies) = target.get(kind).and_then(toml::Value::as_table) {
                    table(dependencies, inherited, &mut names)?;
                }
            }
        }
    }
    if manifest
        .get("package")
        .and_then(|v| v.get("name"))
        .and_then(toml::Value::as_str)
        == Some("neovm-value")
    {
        names.insert("crate".to_string());
    }
    Ok(names)
}

struct Ta2Visitor<'a> {
    file: &'a Path,
    roots: BTreeSet<String>,
    definition_line: Option<usize>,
    violations: Vec<String>,
    value_scopes: Vec<BTreeSet<String>>,
}

fn ta2_pattern_bindings(pattern: &syn::Pat) -> BTreeSet<String> {
    struct Bindings(BTreeSet<String>);
    impl<'ast> Visit<'ast> for Bindings {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.0.insert(pattern.ident.to_string());
            visit::visit_pat_ident(self, pattern);
        }
    }
    let mut bindings = Bindings(BTreeSet::new());
    bindings.visit_pat(pattern);
    bindings.0
}

fn ta2_condition_bindings(expr: &Expr) -> BTreeSet<String> {
    match expr {
        Expr::Let(expr) => ta2_pattern_bindings(&expr.pat),
        Expr::Binary(expr) if matches!(expr.op, syn::BinOp::And(_)) => {
            let mut names = ta2_condition_bindings(&expr.left);
            names.extend(ta2_condition_bindings(&expr.right));
            names
        }
        Expr::Paren(expr) => ta2_condition_bindings(&expr.expr),
        Expr::Group(expr) => ta2_condition_bindings(&expr.expr),
        _ => BTreeSet::new(),
    }
}

impl Ta2Visitor<'_> {
    fn bound_value(&self, name: &str) -> bool {
        self.value_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }

    fn argument_bindings(
        inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
    ) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for input in inputs {
            if let syn::FnArg::Typed(argument) = input {
                names.extend(ta2_pattern_bindings(&argument.pat));
            }
        }
        names
    }

    fn reject(&mut self, span: Span, code: &str, detail: &str) {
        self.violations
            .push(ta2_error(self.file, line_of(span), code, detail));
    }

    fn check_path(&mut self, names: &[String], span: Span) {
        if let Some(index) = names.iter().position(|s| s == "test_support") {
            if names[index + 1..].iter().any(|s| s == "authority") {
                self.reject(span, "TA2_AUTHORITY_REFERENCE", "authority references are restricted to core_tests and the resolved defining module");
            }
        } else if names.iter().any(|s| s == "authority") {
            self.reject(
                span,
                "TA2_UNRESOLVED_REFERENCE",
                "authority path has no resolved test_support owner",
            );
        }
    }

    fn use_tree(&mut self, tree: &UseTree, prefix: &mut Vec<String>) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.use_tree(&path.tree, prefix);
                prefix.pop();
            }
            UseTree::Name(name) => {
                if name.ident != "self" {
                    prefix.push(name.ident.to_string());
                }
                self.check_path(prefix, name.ident.span());
                if name.ident != "self" {
                    prefix.pop();
                }
            }
            UseTree::Rename(rename) => {
                if rename.ident != "self" {
                    prefix.push(rename.ident.to_string());
                }
                self.check_path(prefix, rename.ident.span());
                if prefix.last().is_some_and(|s| s == "test_support") {
                    self.reject(rename.rename.span(), "TA2_TEST_SUPPORT_RENAME", "any test_support rename is fenced, including unused aliases and self aliases");
                } else if prefix.len() == 1
                    && prefix.first().is_some_and(|name| self.roots.contains(name))
                {
                    self.roots.insert(rename.rename.to_string());
                }
                if rename.ident != "self" {
                    prefix.pop();
                }
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.use_tree(item, prefix);
                }
            }
            UseTree::Glob(glob) => {
                if prefix.iter().any(|s| s == "test_support")
                    || (prefix.len() == 1
                        && prefix.first().is_some_and(|name| self.roots.contains(name)))
                {
                    self.reject(
                        glob.star_token.span,
                        "TA2_ANCESTOR_GLOB",
                        "glob can import test_support or authority",
                    );
                }
            }
        }
    }

    fn tokens(&mut self, stream: proc_macro2::TokenStream) {
        use proc_macro2::TokenTree;
        if let Ok(file) = syn::parse2::<syn::File>(stream.clone()) {
            if !file.items.is_empty() {
                self.visit_file(&file);
                return;
            }
        }
        let wrapped: proc_macro2::TokenStream = std::iter::once(TokenTree::Group(
            proc_macro2::Group::new(proc_macro2::Delimiter::Brace, stream.clone()),
        ))
        .collect();
        if let Ok(block) = syn::parse2::<syn::Block>(wrapped) {
            self.visit_block(&block);
            return;
        }
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            // Recover complete use trees even inside cfg_select!/macro_rules! bodies.
            if matches!(&tokens[index], TokenTree::Ident(id) if id == "use") {
                if let Some(end) = tokens[index..]
                    .iter()
                    .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ';'))
                {
                    let use_tokens: proc_macro2::TokenStream =
                        tokens[index..=index + end].iter().cloned().collect();
                    match syn::parse2::<syn::ItemUse>(use_tokens) {
                        Ok(item) => self.use_tree(&item.tree, &mut Vec::new()),
                        Err(error) => self.reject(
                            tokens[index].span(),
                            "TA2_UNRESOLVED_REFERENCE",
                            &format!("macro use tree: {error}"),
                        ),
                    }
                    index += end + 1;
                    continue;
                }
                self.reject(
                    tokens[index].span(),
                    "TA2_UNRESOLVED_REFERENCE",
                    "unterminated macro use tree",
                );
            }
            match &tokens[index] {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Ident(first) => {
                    let mut names = vec![first.to_string()];
                    let mut next = index + 1;
                    while next + 2 < tokens.len()
                        && matches!(&tokens[next], TokenTree::Punct(p) if p.as_char() == ':')
                        && matches!(&tokens[next + 1], TokenTree::Punct(p) if p.as_char() == ':')
                    {
                        if let TokenTree::Ident(id) = &tokens[next + 2] {
                            names.push(id.to_string());
                            next += 3;
                        } else {
                            break;
                        }
                    }
                    let bare_bound_value = names.len() == 1 && self.bound_value(&names[0]);
                    let field_member = names.len() == 1
                        && index > 0
                        && matches!(&tokens[index - 1], TokenTree::Punct(p) if p.as_char() == '.');
                    // Neither a proven lexical value nor a `.field` member is
                    // a module path. Qualified names still receive the fence.
                    if !bare_bound_value && !field_member {
                        self.check_path(&names, first.span());
                    }
                    index = next;
                    continue;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

impl<'ast> Visit<'ast> for Ta2Visitor<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = std::mem::take(&mut self.value_scopes);
        self.value_scopes
            .push(Self::argument_bindings(&item.sig.inputs));
        visit::visit_item_fn(self, item);
        self.value_scopes = previous;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = std::mem::take(&mut self.value_scopes);
        self.value_scopes
            .push(Self::argument_bindings(&item.sig.inputs));
        visit::visit_impl_item_fn(self, item);
        self.value_scopes = previous;
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.value_scopes.push(BTreeSet::new());
        visit::visit_block(self, block);
        self.value_scopes.pop();
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        visit::visit_local(self, local);
        if let Some(scope) = self.value_scopes.last_mut() {
            scope.extend(ta2_pattern_bindings(&local.pat));
        }
    }
    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        let proven_value = expr.qself.is_none()
            && expr.path.leading_colon.is_none()
            && expr.path.segments.len() == 1
            && expr
                .path
                .segments
                .first()
                .is_some_and(|segment| self.bound_value(&segment.ident.to_string()));
        if let Some(qself) = &expr.qself {
            self.visit_type(&qself.ty);
        }
        if proven_value {
            visit::visit_path(self, &expr.path);
        } else {
            self.visit_path(&expr.path);
        }
    }
    fn visit_expr_closure(&mut self, expr: &'ast syn::ExprClosure) {
        let names = expr.inputs.iter().flat_map(ta2_pattern_bindings).collect();
        self.value_scopes.push(names);
        visit::visit_expr_closure(self, expr);
        self.value_scopes.pop();
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_pat(&expr.pat);
        self.visit_expr(&expr.expr);
        self.value_scopes.push(ta2_pattern_bindings(&expr.pat));
        self.visit_block(&expr.body);
        self.value_scopes.pop();
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.value_scopes.push(ta2_pattern_bindings(&arm.pat));
        visit::visit_arm(self, arm);
        self.value_scopes.pop();
    }
    fn visit_expr_binary(&mut self, expr: &'ast syn::ExprBinary) {
        if matches!(expr.op, syn::BinOp::And(_)) {
            self.visit_expr(&expr.left);
            self.value_scopes.push(ta2_condition_bindings(&expr.left));
            self.visit_expr(&expr.right);
            self.value_scopes.pop();
        } else {
            visit::visit_expr_binary(self, expr);
        }
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        self.visit_expr(&expr.cond);
        self.value_scopes.push(ta2_condition_bindings(&expr.cond));
        self.visit_block(&expr.then_branch);
        self.value_scopes.pop();
        if let Some((_, branch)) = &expr.else_branch {
            self.visit_expr(branch);
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.visit_expr(&expr.cond);
        self.value_scopes.push(ta2_condition_bindings(&expr.cond));
        self.visit_block(&expr.body);
        self.value_scopes.pop();
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.use_tree(&item.tree, &mut Vec::new());
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.check_path(
            &path_names(path),
            path.segments
                .first()
                .map(|s| s.ident.span())
                .unwrap_or_else(Span::call_site),
        );
        visit::visit_path(self, path);
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.ident == "authority" && self.definition_line == Some(line_of(module.ident.span()))
        {
            return; // One actual declaration, including the inline defining body if present.
        }
        visit::visit_item_mod(self, module);
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if self.roots.contains(&item.ident.to_string()) {
            if let Some((_, alias)) = &item.rename {
                self.roots.insert(alias.to_string());
            }
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_path(self, &mac.path);
    }
}

pub(crate) fn scan_authority_workspace(workspace: &Path) -> Result<(), Vec<String>> {
    let sources = discover_sources(workspace).map_err(|error| vec![error])?;
    let definitions = ta2_definitions(&sources).map_err(|error| vec![error])?;
    let mut violations = Vec::new();
    for source in &sources {
        let path = match source.path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                violations.push(ta2_error(&source.path, 0, "TA2_SOURCE", &error.to_string()));
                continue;
            }
        };
        let (_, parsed) = match parse_file(&path) {
            Ok(parsed) => parsed,
            Err(error) => {
                violations.push(error);
                continue;
            }
        };
        let allowed_core = if source.package == "neovm-core" {
            let expected = source.member_root.join("core_tests");
            match expected.try_exists() {
                Ok(true) => match expected.canonicalize() {
                    Ok(root) => path.starts_with(root),
                    Err(error) => {
                        violations.push(ta2_error(&expected, 0, "TA2_SOURCE", &error.to_string()));
                        false
                    }
                },
                Ok(false) => false,
                Err(error) => {
                    violations.push(ta2_error(&expected, 0, "TA2_SOURCE", &error.to_string()));
                    false
                }
            }
        } else {
            false
        };
        if allowed_core
            || (source.package == "neovm-value" && definitions.authority_files.contains(&path))
        {
            continue;
        }
        let roots = match ta2_value_aliases(workspace, &source.member_root) {
            Ok(roots) => roots,
            Err(error) => {
                violations.push(error);
                continue;
            }
        };
        let definition_line = definitions.declaration.as_ref().and_then(|(file, line)| {
            (file == &path && source.package == "neovm-value").then_some(*line)
        });
        let mut visitor = Ta2Visitor {
            file: &path,
            roots,
            definition_line,
            violations: Vec::new(),
            value_scopes: Vec::new(),
        };
        // Resolve crate aliases before visiting references, regardless of declaration order.
        loop {
            let before = visitor.roots.len();
            visitor.visit_file(&parsed);
            if visitor.roots.len() == before {
                break;
            }
        }
        visitor.violations.clear();
        visitor.visit_file(&parsed);
        violations.extend(visitor.violations);
    }
    violations.sort();
    violations.dedup();
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[test]
fn architecture_test_support_authority_is_named_only_from_core_tests() {
    if let Err(violations) = scan_authority_workspace(&workspace_root()) {
        panic!(
            "{} TA2 authority-fence violations:\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

fn ta2_fixture(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"core\", \"value\", \"other\"]\n",
    )
    .unwrap();
    for (dir, package) in [
        ("core", "neovm-core"),
        ("value", "neovm-value"),
        ("other", "other"),
    ] {
        std::fs::create_dir_all(root.join(dir).join("src")).unwrap();
        std::fs::write(
            root.join(dir).join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        std::fs::write(root.join(dir).join("src/lib.rs"), "").unwrap();
    }
    std::fs::write(
        root.join("value/src/lib.rs"),
        "#[cfg(feature = \"test-support\")] pub mod test_support;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("value/src/test_support.rs"),
        "pub mod authority; pub fn query() {}\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("value/src/test_support")).unwrap();
    std::fs::write(
        root.join("value/src/test_support/authority.rs"),
        "pub fn forged() {}\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("core/core_tests")).unwrap();
}

#[test]
fn architecture_ta2_guard_self_test() {
    let dir = tempfile::tempdir().unwrap();
    ta2_fixture(dir.path());
    let root = dir.path();
    let forbidden = root.join("core/src/bad.rs");
    std::fs::write(root.join("core/src/lib.rs"), "fn query() { neovm_value::test_support::query(); } fn local(authority: &Path) { consume(authority); assert!(authority.exists()); let authority = authority; consume(authority); } fn scoped() { let authority = value(); consume(authority); if let Some(authority) = option() { consume(authority); } match option() { Some(authority) => consume(authority), _ => () } }\n").unwrap();
    assert!(scan_authority_workspace(root).is_ok());
    for (witness, source) in [
        ("grouped", "use neovm_value::test_support::{authority};"),
        (
            "grouped_rename",
            "use neovm_value::test_support::{authority as a};",
        ),
        (
            "self_rename",
            "use neovm_value::test_support::{self as ts};",
        ),
        ("unused_rename", "use neovm_value::test_support as unused;"),
        ("glob", "use neovm_value::*;"),
        (
            "macro",
            "macro_rules! m { () => { use neovm_value::test_support::{authority}; }; }",
        ),
        (
            "qualified_despite_local",
            "fn local(authority: T) { authority::forge(); }",
        ),
        (
            "macro_qualified_despite_local",
            "fn local(authority: T) { probe!(authority::forge()); }",
        ),
    ] {
        std::fs::write(&forbidden, source).unwrap();
        let red = scan_authority_workspace(root);
        assert!(red.is_err(), "TA2_MUTATION_{witness}: {red:?}");
        std::fs::remove_file(&forbidden).unwrap();
        assert!(
            scan_authority_workspace(root).is_ok(),
            "TA2_RESTORED_{witness}"
        );
    }
    std::fs::write(
        root.join("core/core_tests/allowed.rs"),
        "use neovm_value::test_support::authority;\n",
    )
    .unwrap();
    assert!(scan_authority_workspace(root).is_ok());
    // This path is deliberately outside the conventional tests/ root.
    std::fs::create_dir_all(root.join("other/external")).unwrap();
    std::fs::write(
        root.join("other/external/probe.rs"),
        "use neovm_value::test_support::{authority as a};\n",
    )
    .unwrap();
    let manifest = root.join("other/Cargo.toml");
    let original = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{original}\n[[test]]\nname = \"probe\"\npath = \"external/probe.rs\"\n"),
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_OTHER_CRATE_EXPLICIT_TEST"
    );
    std::fs::write(&manifest, original).unwrap();
    std::fs::remove_file(root.join("other/external/probe.rs")).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_OTHER_CRATE_EXPLICIT_TEST"
    );
    // Defining-directory siblings are not exempt merely by physical adjacency.
    std::fs::write(
        root.join("value/src/test_support/unrelated.rs"),
        "use crate::test_support::authority;\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_DEFINITION_SIBLING"
    );
    std::fs::remove_file(root.join("value/src/test_support/unrelated.rs")).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_DEFINITION_SIBLING"
    );
}
