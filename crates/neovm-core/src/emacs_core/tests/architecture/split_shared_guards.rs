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
//! 4. **TA2 authority fence**: each signed `test_support::authority` fence
//!    (`neovm-value`, `neovm-eval`) may be named only from
//!    `crates/neovm-core/core_tests/**`, a fence's own definition files, or
//!    (B2) a src file whose EVERY module route is test-support-gated in a
//!    HIGHER crate, through a resolved fence alias. Globs and renames are
//!    resolved-module matched (lread::test_support is not the fence), macro
//!    tokens treat a lone `authority` as a field, and unparseable sources are
//!    scanned for the exact `test_support :: authority` sequence instead of
//!    failing the whole guard.
//!
//! All scans are fail-closed: unparseable sources, missing tables, malformed
//! rows and any occurrence not accounted for by an allowed site fail.

use proc_macro2::Span;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::architecture_moving_set;
use super::architecture_production_source_roots::workspace_root as shared_workspace_root;
use syn::{Attribute, Expr, Item, UseTree};

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
    /// Modules synthesized by a modeled macro expansion (for example the
    /// `absent_layer_slots` module generated by `eval_absent_layer_slots!`).
    /// Their members are generated stub definitions, not hand-written child
    /// modules, so the N13 subtree rule must not treat them as local names.
    synthetic_modules: BTreeSet<N13ModuleKey>,
    /// Files consumed by a literal `include!` item. Their items are indexed
    /// in the including module, and the standalone source sweep must not
    /// visit them a second time with no module identity.
    include_targets: BTreeSet<PathBuf>,
    /// Function names the `eval_absent_layer_slots!` transcriber defines.
    absent_slot_functions: BTreeSet<String>,
    include_depth: usize,
    /// Resolution caches. The guards resolve the same facade paths thousands
    /// of times (PC2 resolves every installer and reference); without these
    /// a segment-stepping BFS over the whole workspace is quadratic.
    module_path_cache: RefCell<BTreeMap<(N13ModuleKey, N13ModuleKey), BTreeSet<N13ModuleKey>>>,
    module_path_in_progress: RefCell<BTreeSet<(N13ModuleKey, N13ModuleKey)>>,
    member_cache: RefCell<BTreeMap<(N13ModuleKey, String), BTreeSet<N13FunctionIdentity>>>,
}

/// A module/function path name as rustc resolves it: a raw identifier keeps
/// its spelling in source (`r#if`) but names the same item and the same
/// `if/` directory/file as `if`. The split catalog previously used the raw
/// spelling for file lookup and reported N13_UNRESOLVED for
/// neovm_oracle_tests::r#if.
fn n13_ident(ident: &syn::Ident) -> String {
    let name = ident.to_string();
    name.strip_prefix("r#").map(str::to_string).unwrap_or(name)
}

fn path_names(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| n13_ident(&s.ident)).collect()
}

fn n13_use_leaves(tree: &UseTree, prefix: &[String], out: &mut Vec<N13Use>) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(n13_ident(&p.ident));
            n13_use_leaves(&p.tree, &next, out);
        }
        UseTree::Name(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n13_ident(&n.ident));
            }
            let alias = path.last().cloned();
            out.push(N13Use { path, alias });
        }
        UseTree::Rename(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n13_ident(&n.ident));
            }
            out.push(N13Use {
                path,
                alias: Some(n13_ident(&n.rename)),
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
    /// `path_loaded` is true when the module's source file was named by an
    /// explicit `#[path]` attribute. rustc resolves that module's CHILDREN in
    /// the loaded file's directory (siblings of the loaded file), not in a
    /// directory named after the file; a 3-line probe with
    /// `#[path = "sub/inline_mod.rs"] mod tests;` plus `mod child;` in
    /// `inline_mod.rs` resolves `sub/child.rs`. Without the flag the catalog
    /// reported N13_UNRESOLVED for the display-protocol image inline tests.
    fn add_file(
        &mut self,
        key: N13ModuleKey,
        path: &Path,
        path_loaded: bool,
    ) -> Result<(), String> {
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
        let dir = if path_loaded
            || key.len() == 1
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
                        .insert(n13_ident(&f.sig.ident));
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
                    let name = n13_ident(&m.ident);
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
                        self.add_file(child, &candidates[0], !explicit.is_empty())?;
                    }
                }
                Item::Macro(m) => {
                    let last = m.mac.path.segments.last().map(|s| n13_ident(&s.ident));
                    let definition = m.ident.is_some();
                    if !definition && last.as_deref() == Some("eval_absent_layer_slots") {
                        self.register_absent_slots(key.clone());
                    } else if !definition && m.mac.path.is_ident("include") {
                        self.add_include(key.clone(), path, dir, path_base, &m.mac)?;
                    } else {
                        self.add_items(
                            key.clone(),
                            path,
                            dir,
                            path_base,
                            &n13_macro_items(m.mac.tokens.clone()),
                        )?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Register the module an `eval_absent_layer_slots!` invocation generates.
    fn register_absent_slots(&mut self, key: N13ModuleKey) {
        let functions = self.absent_slot_functions.clone();
        let mut child = key.clone();
        child.push("absent_layer_slots".to_string());
        self.modules
            .entry(key)
            .or_default()
            .children
            .insert("absent_layer_slots".to_string());
        self.modules
            .entry(child.clone())
            .or_default()
            .functions
            .extend(functions);
        self.synthetic_modules.insert(child);
    }

    /// Splice a literal `include!` file's items into the including module.
    fn add_include(
        &mut self,
        key: N13ModuleKey,
        path: &Path,
        dir: &Path,
        path_base: &Path,
        mac: &syn::Macro,
    ) -> Result<(), String> {
        let Ok(literal) = syn::parse2::<syn::LitStr>(mac.tokens.clone()) else {
            return Ok(());
        };
        let base = path.parent().unwrap_or(Path::new("."));
        let candidate = base.join(literal.value());
        let Ok(canonical) = candidate.canonicalize() else {
            return Ok(());
        };
        if !canonical.is_file() || self.include_depth >= 32 {
            return Ok(());
        }
        self.include_targets.insert(canonical.clone());
        let (_, file) = parse_file(&canonical)?;
        self.include_depth += 1;
        let result = self.add_items(key, &canonical, dir, path_base, &file.items);
        self.include_depth -= 1;
        result
    }

    /// Candidate modules named by `path` from `module`, following child
    /// modules, named re-exports (including renames) and glob re-exports.
    ///
    /// Segment-stepping BFS over `(module, consumed segments)`: every state is
    /// visited once and no path vector is concatenated as the search advances.
    /// The previous split walk rebuilt path vectors by appending the unresolved
    /// tail to each import target, so a cyclic re-export produced fresh,
    /// ever-longer paths that the visited set could not catch and exhausted the
    /// address space on the full workspace.
    fn resolve_module_path(&self, module: &[String], path: &[String]) -> BTreeSet<N13ModuleKey> {
        let key = (module.to_vec(), path.to_vec());
        if let Some(cached) = self.module_path_cache.borrow().get(&key) {
            return cached.clone();
        }
        // A cyclic import rewrite that asks for a resolution currently being
        // computed contributes nothing; the caller still fails closed if it
        // needed that branch.
        if !self
            .module_path_in_progress
            .borrow_mut()
            .insert(key.clone())
        {
            return BTreeSet::new();
        }
        let result = self.resolve_module_path_at(module, path, 0);
        self.module_path_in_progress.borrow_mut().remove(&key);
        self.module_path_cache
            .borrow_mut()
            .insert(key, result.clone());
        result
    }

    fn resolve_module_path_at(
        &self,
        module: &[String],
        path: &[String],
        depth: usize,
    ) -> BTreeSet<N13ModuleKey> {
        const MAX_DEPTH: usize = 64;
        let mut reached = BTreeSet::new();
        if depth > MAX_DEPTH || module.is_empty() {
            return reached;
        }
        if path.is_empty() {
            reached.insert(module.to_vec());
            return reached;
        }
        let mut queue: VecDeque<(N13ModuleKey, usize)> = VecDeque::new();
        match path[0].as_str() {
            "crate" => queue.push_back((vec![module[0].clone()], 1)),
            "self" => queue.push_back((module.to_vec(), 1)),
            "super" => {
                let supers = path.iter().take_while(|s| s.as_str() == "super").count();
                if supers < module.len() {
                    queue.push_back((module[..module.len() - supers].to_vec(), supers));
                }
            }
            _ => {
                queue.push_back((module.to_vec(), 0));
                let root = vec![path[0].clone()];
                if self.modules.contains_key(&root) {
                    queue.push_back((root, 1));
                }
            }
        }
        let mut seen: BTreeSet<(N13ModuleKey, usize)> = BTreeSet::new();
        while let Some((current, consumed)) = queue.pop_front() {
            if !seen.insert((current.clone(), consumed)) {
                continue;
            }
            if consumed >= path.len() {
                reached.insert(current);
                continue;
            }
            let segment = &path[consumed];
            match segment.as_str() {
                "self" => {
                    queue.push_back((current, consumed + 1));
                    continue;
                }
                "super" => {
                    if current.len() > 1 {
                        queue.push_back((current[..current.len() - 1].to_vec(), consumed + 1));
                    }
                    continue;
                }
                _ => {}
            }
            let Some(info) = self.modules.get(&current) else {
                continue;
            };
            if info.children.contains(segment) {
                let mut child = current.clone();
                child.push(segment.clone());
                if self.modules.contains_key(&child) {
                    queue.push_back((child, consumed + 1));
                }
            }
            for import in &info.imports {
                match &import.alias {
                    Some(alias) if alias == segment => {
                        for target in self.resolve_module_path(&current, &import.path) {
                            queue.push_back((target, consumed + 1));
                        }
                    }
                    // A glob makes the target module's items visible here: view
                    // it at the same segment without consuming anything.
                    None => {
                        for target in self.resolve_module_path(&current, &import.path) {
                            queue.push_back((target, consumed));
                        }
                    }
                    _ => {}
                }
            }
        }
        reached
    }

    /// Function identities reachable as `name` from `module`: items declared
    /// there, named re-exports (including renames), and glob re-exports. BFS
    /// over modules; every module is visited once.
    fn member_identities(&self, module: &[String], name: &str) -> BTreeSet<N13FunctionIdentity> {
        let key = (module.to_vec(), name.to_string());
        if let Some(cached) = self.member_cache.borrow().get(&key) {
            return cached.clone();
        }
        let result = self.member_identities_uncached(module, name);
        self.member_cache.borrow_mut().insert(key, result.clone());
        result
    }

    fn member_identities_uncached(
        &self,
        module: &[String],
        name: &str,
    ) -> BTreeSet<N13FunctionIdentity> {
        let mut identities = BTreeSet::new();
        let mut seen: BTreeSet<N13ModuleKey> = BTreeSet::new();
        let mut queue: VecDeque<N13ModuleKey> = VecDeque::new();
        queue.push_back(module.to_vec());
        while let Some(current) = queue.pop_front() {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(info) = self.modules.get(&current) else {
                continue;
            };
            if info.functions.contains(name) {
                identities.insert(N13FunctionIdentity {
                    module: current.clone(),
                    name: name.to_string(),
                });
                // A definition in this module shadows any glob-imported item
                // of the same name (Rust forbids a named import that collides
                // with a local item), so do not treat the glob target as a
                // second identity.
                continue;
            }
            for import in &info.imports {
                match &import.alias {
                    Some(alias) if alias == name => {
                        let Some(original) = import.path.last() else {
                            continue;
                        };
                        let prefix = &import.path[..import.path.len() - 1];
                        for target in self.resolve_module_path(&current, prefix) {
                            let Some(target_info) = self.modules.get(&target) else {
                                continue;
                            };
                            if target_info.functions.contains(original) {
                                identities.insert(N13FunctionIdentity {
                                    module: target,
                                    name: original.clone(),
                                });
                            }
                        }
                    }
                    None => {
                        for target in self.resolve_module_path_at(&current, &import.path, 1) {
                            queue.push_back(target);
                        }
                    }
                    _ => {}
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
        for target in self.resolve_module_path(module, path) {
            identities.extend(self.member_identities(&target, name));
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
        let result = self.resolve_module_path(module, path);
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

/// Function names defined by the `eval_absent_layer_slots!` transcriber.
///
/// The macro is a plain `macro_rules!` whose transcriber is the absent-layer
/// module. Its generated functions carry the same spellings as CARRIERS
/// members (they are the stubs the wiring passes), so the catalog must know
/// them to resolve `use absent_layer_slots as s` without a reviewed ledger.
fn collect_absent_slot_functions(items: &[Item], out: &mut BTreeSet<String>) {
    for item in items {
        let Item::Macro(mac) = item else {
            continue;
        };
        if mac
            .ident
            .as_ref()
            .is_none_or(|ident| ident != "eval_absent_layer_slots")
        {
            continue;
        }
        collect_generated_fn_names(mac.mac.tokens.clone(), out);
    }
}

fn collect_generated_fn_names(stream: proc_macro2::TokenStream, out: &mut BTreeSet<String>) {
    let tokens: Vec<_> = stream.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            proc_macro2::TokenTree::Ident(ident) if ident == "fn" => {
                if let Some(proc_macro2::TokenTree::Ident(name)) = tokens.get(index + 1) {
                    out.insert(name.to_string());
                }
            }
            proc_macro2::TokenTree::Group(group) => {
                collect_generated_fn_names(group.stream(), out);
            }
            _ => {}
        }
    }
}

fn n13_module_catalog(sources: &[GuardSource]) -> Result<N13Modules, String> {
    let mut absent_slot_functions = BTreeSet::new();
    for source in sources {
        let Ok(text) = std::fs::read_to_string(&source.path) else {
            continue;
        };
        if !text.contains("macro_rules! eval_absent_layer_slots") {
            continue;
        }
        let (_, file) = parse_file(&source.path)?;
        collect_absent_slot_functions(&file.items, &mut absent_slot_functions);
    }
    let mut catalog = N13Modules::default();
    catalog.absent_slot_functions = absent_slot_functions;
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
            catalog.add_file(vec![name.clone()], &root.join(lib), false)?;
        }
        let main = root.join("src/main.rs");
        if main.is_file() {
            catalog.add_file(vec![format!("{name}@main")], &main, false)?;
        }
        if root.join("build.rs").is_file() {
            catalog.add_file(vec![format!("{name}@build")], &root.join("build.rs"), false)?;
        }
        for kind in ["bin", "test", "bench", "example"] {
            if let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) {
                for target in targets {
                    if let Some(path) = target.get("path").and_then(toml::Value::as_str) {
                        catalog.add_file(
                            vec![format!("{name}@{kind}:{path}")],
                            &root.join(path),
                            false,
                        )?;
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
                        false,
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
        if self.fn_stack.len() != 1 || self.fn_stack.first().map(String::as_str) != Some(name) {
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

    /// Whether a path resolves only to identities that are NOT carried pair
    /// members. The split re-homed same-named helpers (system's own
    /// `world_state_placeholder`, hooks-table methods), so a bare spelling is
    /// not a carried reference when every identity it resolves to lives
    /// outside the CARRIERS defining modules. An unresolved path is not
    /// exempted (fail closed).
    fn resolves_only_outside_carried(&self, path: &[String]) -> Result<bool, String> {
        let Some(name) = path.last() else {
            return Ok(false);
        };
        if !self.names.contains(name) {
            return Ok(false);
        }
        let prefix = &path[..path.len() - 1];
        let identities = self.catalog.function_identities(&self.module, prefix, name);
        if identities.is_empty() {
            return Ok(false);
        }
        // The resolved identity decides, not the queried spelling: a
        // cross-file alias like `forged_swap` resolves to the carried
        // `swap` identity and must stay fenced.
        let carried_hit = identities.iter().any(|identity| {
            self.defining
                .get(&identity.name)
                .is_some_and(|modules| modules.contains(&identity.module))
        });
        // Identities in the current module or below are the module's own
        // names, not the unrelated re-homes this exemption exists for. A
        // same-named child module function is exactly what the delegation
        // rule must keep red (`fn outer() { child::swap(); }`).
        let blocked = identities.iter().any(|identity| {
            !self.catalog.synthetic_modules.contains(&identity.module)
                && identity.module.len() > self.module.len()
                && identity.module.starts_with(&self.module)
        });
        Ok(!carried_hit && !blocked)
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
                    // A macro body can define the slot function itself
                    // (`fn name(...)`) or name a struct field/type ascription
                    // (`name:`); neither is a carried-function reference.
                    if matches!(
                        tokens.get(index.wrapping_sub(1)),
                        Some(proc_macro2::TokenTree::Ident(keyword)) if keyword == "fn"
                    ) {
                        self.allow(ident);
                        continue;
                    }
                    if matches!(
                        tokens.get(index + 1),
                        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ':'
                    ) && !matches!(
                        tokens.get(index + 2),
                        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ':'
                    ) {
                        self.allow(ident);
                        continue;
                    }
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
                    match self.delegation(&path, &name) {
                        Ok(true) => self.allow(ident),
                        Ok(false) => {
                            // A hooks-table entry or a same-named helper from
                            // another module is not a carried reference when the
                            // path resolves only to non-carried identities.
                            if self.resolves_only_outside_carried(&path).unwrap_or(false) {
                                self.allow(ident);
                            }
                        }
                        Err(e) => self.errors.push(e),
                    }
                    let _ = is_call;
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
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if let Some(last) = path.path.segments.last() {
            let name = last.ident.to_string();
            if self.names.contains(&name) {
                let segments = path_names(&path.path);
                match self.delegation(&segments, &name) {
                    Ok(true) => self.allow(&last.ident),
                    Ok(false) => {
                        if self
                            .resolves_only_outside_carried(&segments)
                            .unwrap_or(false)
                        {
                            self.allow(&last.ident);
                        }
                    }
                    Err(error) => self.errors.push(error),
                }
            }
        }
        visit::visit_expr_path(self, path);
    }

    /// A method declaration or method call can never be one of the carried
    /// free functions (they are not methods); same-named hooks-table
    /// accessors are therefore not carried references.
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if self.names.contains(&item.sig.ident.to_string()) {
            self.allow(&item.sig.ident);
        }
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if self.names.contains(&call.method.to_string()) {
            self.allow(&call.method);
        }
        visit::visit_expr_method_call(self, call);
    }

    /// Struct fields, struct-literal field names and field accesses are
    /// storage names, never calls of a carried free function.
    fn visit_field(&mut self, field: &'ast syn::Field) {
        if let Some(ident) = &field.ident {
            if self.names.contains(&ident.to_string()) {
                self.allow(ident);
            }
        }
        visit::visit_field(self, field);
    }

    fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
        if let syn::Member::Named(ident) = &field.member {
            if self.names.contains(&ident.to_string()) {
                self.allow(ident);
            }
        }
        visit::visit_field_value(self, field);
    }

    fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
        if let syn::Member::Named(ident) = &field.member {
            if self.names.contains(&ident.to_string()) {
                self.allow(ident);
            }
        }
        visit::visit_expr_field(self, field);
    }

    /// A pattern binding (function parameter, closure argument, `let` or
    /// struct-pattern field) introduces a local name; it is a definition, not
    /// a reference, and can never call a carried function. The lifecycle hook
    /// constructors spell their parameter slots with the carried names.
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        if self.names.contains(&pattern.ident.to_string()) {
            self.allow(&pattern.ident);
        }
        visit::visit_pat_ident(self, pattern);
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
    // The CARRIERS table lived in neovm-core when N13 was signed; the stage-4
    // split re-homed it to the evaluator crate. Follow the one kernel crate
    // that hosts `eval::green::carried`, still requiring exactly one module:
    // the uniqueness rule is the guard's contract, not the crate spelling.
    let mut candidates = Vec::new();
    for (file, modules) in &catalog.files {
        for module in modules {
            if module.ends_with(&["eval".into(), "green".into(), "carried".into()])
                && module
                    .first()
                    .is_some_and(|s| is_pc1_kernel_package(&s.replace('_', "-")))
            {
                candidates.push((file.clone(), module.clone()));
            }
        }
    }
    if candidates.len() != 1 {
        return Err(vec![format!(
            "N13_UNRESOLVED expected one kernel eval::green::carried, got {candidates:?}"
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
    scan_paths.retain(|path| !catalog.include_targets.contains(path));
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
        "mod pair; mod aliases; mod external; mod r#if; mod eval { pub mod green { pub mod carried; } }\n",
    )
    .unwrap();
    // A raw-identifier module names the ordinary `if/` directory; the split
    // catalog must unraw before the file lookup (oracle-tests has `mod r#if`).
    std::fs::create_dir_all(src.join("if")).unwrap();
    std::fs::write(src.join("if/mod.rs"), "fn unrelated() {}\n").unwrap();
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
    // A #[path]-loaded module's children resolve BESIDE the loaded file
    // (rustc semantics), not in a directory named after that file. Without
    // this, the display-protocol image inline tests report N13_UNRESOLVED.
    std::fs::create_dir_all(src.join("pathed")).unwrap();
    std::fs::write(src.join("pathed/inline_root.rs"), "mod sibling;\n").unwrap();
    std::fs::write(src.join("pathed/sibling.rs"), "fn unrelated() {}\n").unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "mod pair; mod aliases; mod external; mod r#if; mod eval { pub mod green { pub mod carried; } }\n#[path = \"pathed/inline_root.rs\"] mod pathed_tests;\n",
    )
    .unwrap();
    let pathed = scan_carried_workspace(root);
    assert!(
        pathed.is_ok(),
        "path-loaded module child resolution: {pathed:?}"
    );
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
    // The absent-layer slot macro generates a module whose stubs carry
    // carried spellings; the catalog must model that output so wiring through
    // `use absent_layer_slots as s` resolves to the generated definitions
    // instead of needing a reviewed exemption list.
    std::fs::write(
        src.join("lib.rs"),
        "mod pair; mod aliases; mod external; mod r#if; mod absent; mod eval { pub mod green { pub mod carried; } }\n",
    )
    .unwrap();
    std::fs::write(
        src.join("absent.rs"),
        "macro_rules! eval_absent_layer_slots {\n    () => {\n        pub(crate) mod absent_layer_slots {\n            pub(crate) fn swap() {}\n        }\n    };\n}\neval_absent_layer_slots!();\nuse absent_layer_slots as s;\npub fn wiring() { let _ = s::swap; }\n",
    )
    .unwrap();
    let modeled = scan_carried_workspace(root);
    assert!(
        modeled.is_ok(),
        "modeled macro stub wiring must be green: {modeled:?}"
    );
    std::fs::write(
        src.join("absent.rs"),
        "macro_rules! eval_absent_layer_slots {\n    () => {\n        pub(crate) mod absent_layer_slots {\n            pub(crate) fn swap() {}\n        }\n    };\n}\neval_absent_layer_slots!();\nuse absent_layer_slots as s;\npub fn wiring() { let _ = s::swap; }\npub fn violate() { crate::pair::swap(); }\n",
    )
    .unwrap();
    let red = scan_carried_workspace(root);
    assert!(
        red.is_err(),
        "a genuine carried call beside the modeled wiring must stay red"
    );
    std::fs::write(src.join("absent.rs"), "fn unrelated() {}\n").unwrap();
    assert!(
        scan_carried_workspace(root).is_ok(),
        "macro-model restoration not green"
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

    /// A macro body is a token stream, so a bare identifier that merely
    /// spells a fenced method (a local named `identity` inside `assert!`) is
    /// not a reference. Flag only reference shapes: `Owner::method` with the
    /// row's owner, and `receiver.method(...)` call syntax.
    fn tokens(&mut self, stream: proc_macro2::TokenStream) {
        let tokens: Vec<_> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Ident(ident) => {
                    let Some(row) = self.rows.iter().find(|row| ident == row.method.as_str())
                    else {
                        continue;
                    };
                    if macro_path_reference(&tokens, index, &row.type_name)
                        || macro_method_call(&tokens, index)
                    {
                        self.refuse(ident, Pc1Refusal::UnresolvedReference);
                    }
                }
                _ => {}
            }
        }
    }
}

fn punct_is(token: Option<&proc_macro2::TokenTree>, ch: char) -> bool {
    matches!(token, Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ch)
}

/// `Owner::method` in a macro token stream (`Owner` must be the row's type).
fn macro_path_reference(tokens: &[proc_macro2::TokenTree], index: usize, owner: &str) -> bool {
    if index < 3 {
        return false;
    }
    if !punct_is(tokens.get(index - 1), ':') || !punct_is(tokens.get(index - 2), ':') {
        return false;
    }
    matches!(tokens.get(index - 3), Some(proc_macro2::TokenTree::Ident(ident)) if ident == owner)
}

/// `receiver.method(...)`, optionally with a turbofish before the argument
/// list, in a macro token stream.
fn macro_method_call(tokens: &[proc_macro2::TokenTree], index: usize) -> bool {
    if index == 0 || !punct_is(tokens.get(index - 1), '.') {
        return false;
    }
    for token in tokens.iter().skip(index + 1).take(8) {
        match token {
            proc_macro2::TokenTree::Group(group)
                if group.delimiter() == proc_macro2::Delimiter::Parenthesis =>
            {
                return true;
            }
            proc_macro2::TokenTree::Punct(p) if matches!(p.as_char(), ':' | '<' | '>') => {}
            _ => return false,
        }
    }
    false
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
        // Precision (LEAD 06:5xZ): a bare identifier that happens to spell a
        // fenced method (a local named `identity`, a plain function call) is
        // not a call of the fenced method. Flag only a qualified path whose
        // OWNER segment is the row's type, e.g. `TaggedValue::from_fixnum` or
        // `crate::tagged::value::TaggedHeap::tagged_heap_identity`. Receiver
        // types in
        // method-call syntax cannot be inferred, so those stay ambiguous and
        // fail closed in `visit_expr_method_call` below.
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if segments.len() >= 2 {
            let owner = &segments[segments.len() - 2];
            let method = &segments[segments.len() - 1];
            if self
                .rows
                .iter()
                .any(|row| row.type_name == *owner && row.method == *method)
                && let Some(segment) = path.segments.last()
            {
                self.refuse(&segment.ident, Pc1Refusal::UnresolvedReference);
            }
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

/// The post-split PC1 kernel: the code that lived in `neovm-core` when the
/// fence was signed. Item 20 and the split re-homed the evaluator, display,
/// system, text and edit call sites into the extracted crates, so the signed
/// kernel is core plus every moving crate; app crates stay fenced.
fn is_pc1_kernel_package(package: &str) -> bool {
    package == "neovm-core"
        || architecture_moving_set::MOVING_CRATES
            .iter()
            .any(|name| name.replace('_', "-") == package)
}

pub(crate) fn scan_pc1_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc1_table(text)?;
    let sources = discover_sources(workspace)?;
    let mut violations = Vec::new();
    for source in sources {
        let (_, syntax) = parse_file(&source.path)?;
        if is_pc1_kernel_package(&source.package) {
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
    /// File whose text owns the item currently being visited. It differs from
    /// the module's source file only while a literal `include!` is expanded;
    /// sites and error lines then name the included file, which is where the
    /// definition text (and the seam-table adapter path) lives.
    site_source: PathBuf,
    /// Nesting depth of literal include! expansion; a cycle is a scan error,
    /// not a stack overflow.
    include_depth: usize,
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
                self.facts
                    .helper_writes
                    .entry(writer)
                    .or_default()
                    .insert(index);
            }
            (Pc2Pass::References, Pc2CellOrigin::Static(cell)) => {
                self.facts.writes.push(Pc2Write {
                    cell,
                    writer,
                    source: self.site_source.clone(),
                    line: line_of(span),
                })
            }
            (_, Pc2CellOrigin::Local | Pc2CellOrigin::Parameter(_)) => {}
            (
                Pc2Pass::Declarations | Pc2Pass::HelperSummaries,
                Pc2CellOrigin::Static(_) | Pc2CellOrigin::Unresolved,
            ) => {}
            (Pc2Pass::References, Pc2CellOrigin::Unresolved) => {
                // Test-only fixtures (Arc/OnceLock helpers) cannot prove a
                // cell origin and do not need to: every installer reference
                // in test code is still fenced by the exact-seam permission
                // check, which is reference-based.
                if !self.test_only {
                    self.error(span, "PC2_UNRESOLVED_REFERENCE: OnceLock writer argument/receiver has no proved static, typed parameter or local-cell origin".into());
                }
            }
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
        let file = match self.site_source.strip_prefix(self.workspace) {
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
            self.site_source.display(),
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
                source: self.site_source.clone(),
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
        if self.pass != Pc2Pass::Declarations && !self.test_only {
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
            // A watched installer/adapter name, not a naming convention: an
            // app crate's own `install*` helper writes its own cells.
            let install_name = self.current_fn.as_deref().is_some_and(|name| {
                name.split("::")
                    .last()
                    .is_some_and(|last| self.names.contains(last))
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
    /// Expand a literal `include!("relative.rs")` item in the including
    /// module's context. Test adapters are wired this way
    /// (`#[cfg(test)] mod m { include!("tests/x.rs"); }`), so without expansion
    /// the adapter definition is invisible and the seam row cannot resolve.
    /// A literal in-workspace include that does not parse fails closed; the
    /// generated `concat!(env!("OUT_DIR"), ...)` form cannot be resolved from
    /// source and keeps the previous token-only treatment.
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("include")
            && let Ok(literal) = syn::parse2::<syn::LitStr>(item.mac.tokens.clone())
        {
            let base = self
                .site_source
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf();
            let path = base.join(literal.value());
            if let Ok(canonical) = path.canonicalize()
                && canonical.starts_with(self.workspace)
            {
                match parse_file(&canonical) {
                    Ok((_, file)) => {
                        if self.include_depth >= 32 {
                            self.facts.errors.push(format!(
                                "PC2_INCLUDE: include nesting exceeds 32 at {}",
                                canonical.display()
                            ));
                            return;
                        }
                        let previous = std::mem::replace(&mut self.site_source, canonical);
                        self.include_depth += 1;
                        self.import_aliases(&file.items);
                        for item in &file.items {
                            self.visit_item(item);
                        }
                        self.include_depth -= 1;
                        self.site_source = previous;
                        return;
                    }
                    Err(error) => {
                        self.facts.errors.push(format!(
                            "PC2_INCLUDE: literal include {} cannot be parsed: {error}",
                            canonical.display()
                        ));
                        return;
                    }
                }
            }
        }
        self.macro_tokens(item.mac.tokens.clone());
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
        let same_name: Vec<_> = facts
            .definitions
            .iter()
            .filter(|definition| definition.symbol.name == site.function)
            .map(|definition| definition.site.file.clone())
            .take(8)
            .collect();
        Err(format!(
            "PC2_UNRESOLVED_REFERENCE: adapter {}:{} has {} definitions (same-name files: {same_name:?})",
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

/// One parse per source file for the whole PC2 scan. The scan runs several
/// passes (declarations, helper summaries to a fixed point, references), and
/// reparsing the whole workspace on each pass dominated the guard's runtime.
#[derive(Default)]
struct Pc2Parsed {
    files: RefCell<BTreeMap<PathBuf, Rc<(String, syn::File)>>>,
}

impl Pc2Parsed {
    fn get(&self, path: &Path) -> Result<Rc<(String, syn::File)>, String> {
        if let Some(cached) = self.files.borrow().get(path) {
            return Ok(cached.clone());
        }
        let parsed = Rc::new(parse_file(path)?);
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), parsed.clone());
        Ok(parsed)
    }
}

/// A path inside a crate target tree (`crates/<crate>/tests/**`,
/// `benches/**`, `examples/**`). Cargo does not compile nested files there
/// as targets (only direct children are autodiscovered; explicit [[test]]
/// paths are cataloged from the manifest), so these are fixtures or trybuild
/// cases. They cannot ship, and a watched installer name there is a
/// test-only reference, not a production caller the PC2 fence must resolve.
fn pc2_crate_test_tree(workspace: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(workspace) else {
        return false;
    };
    let components: Vec<_> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    components.len() >= 4
        && components[0] == "crates"
        && matches!(components[2].as_str(), "tests" | "benches" | "examples")
}

fn pc2_collect(
    workspace: &Path,
    paths: &BTreeSet<PathBuf>,
    parsed_files: &Pc2Parsed,
    catalog: &N13Modules,
    names: &BTreeSet<String>,
    test_modules: &BTreeSet<Vec<String>>,
    pass: Pc2Pass,
    facts: &mut Pc2Facts,
) -> Result<(), String> {
    for path in paths {
        let parsed = parsed_files.get(path)?;
        let text = &parsed.0;
        let modules = catalog.files.get(path);
        // A crate root is legitimately catalogued under several target keys
        // (lib child, @main, @bin:<path>); the file is one source and any
        // deterministic key resolves its own imports.
        let root_alias = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "main.rs" || name == "lib.rs");
        let Some(module) = modules.and_then(|modules| {
            if modules.len() == 1 || root_alias {
                modules.iter().next()
            } else {
                None
            }
        }) else {
            if (!count_occurrences(&cleaned_code(&text), names).is_empty()
                || text.contains("OnceLock") && text.contains("fn install"))
                && !pc2_crate_test_tree(workspace, path)
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
            site_source: path.clone(),
            include_depth: 0,
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
        visitor.visit_file(&parsed.1);
    }
    Ok(())
}

pub(crate) fn scan_pc2_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    // Canonicalize the root ONCE, at guard entry.  Every source path below is
    // canonicalized before it becomes a catalog key, and `Pc2Visitor::site`
    // derives the definition's workspace-relative name with
    // `source.strip_prefix(workspace)`.  A root that reaches this guard as a
    // symlink (macOS `/tmp` and `/var` are links to `/private/...`) makes that
    // strip fail for every file, so every adapter resolves to "0 definitions"
    // and the pc2 self-tests go red on arm64 while Linux stays green.  The
    // outside-workspace check is unchanged: a file genuinely outside the
    // canonical root still fails to strip.
    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|error| format!("PC2_WORKSPACE: {}: {error}", workspace.display()))?;
    let workspace = canonical_workspace.as_path();
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
    paths.retain(|path| !catalog.include_targets.contains(path));
    let mut test_modules = BTreeSet::new();
    fn gated(
        items: &[Item],
        module: &mut Vec<String>,
        inherited: bool,
        out: &mut BTreeSet<Vec<String>>,
    ) {
        // One module can be declared by several cfg arms (the feature
        // test-support twin beside cfg(not(feature = "test-support"))). A
        // module compiled by a NON-test arm is production: marking it
        // test-only from the other arm would exempt its whole file from the
        // production write analysis.
        let mut declarations: BTreeMap<String, Vec<&syn::ItemMod>> = BTreeMap::new();
        for item in items {
            if let Item::Mod(item) = item {
                declarations
                    .entry(item.ident.to_string())
                    .or_default()
                    .push(item);
            }
        }
        for (name, arms) in declarations {
            module.push(name);
            let gate = inherited || arms.iter().all(|item| pc2_test_attrs(&item.attrs));
            if gate {
                out.insert(module.clone());
            }
            for arm in &arms {
                if let Some((_, nested)) = &arm.content {
                    gated(nested, module, gate, out);
                }
            }
            module.pop();
        }
    }
    let parsed_files = Pc2Parsed::default();
    for path in &paths {
        let parsed = parsed_files.get(path)?;
        if let Some(modules) = catalog.files.get(path) {
            for module in modules {
                gated(
                    &parsed.1.items,
                    &mut module.clone(),
                    false,
                    &mut test_modules,
                );
            }
        }
    }
    let mut facts = Pc2Facts::default();
    pc2_collect(
        workspace,
        &paths,
        &parsed_files,
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
            &parsed_files,
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
        &parsed_files,
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
            let installer_writes: Vec<_> = facts
                .writes
                .iter()
                .filter(|write| &write.writer == installer)
                .map(|write| (write.cell.0.clone(), write.source.clone(), write.line))
                .collect();
            let cell_writers: Vec<_> = facts
                .writes
                .iter()
                .filter(|write| &write.cell == cell)
                .map(|write| write.writer.clone())
                .collect();
            violations.push(format!(
                "PC2_STATIC_WRITER_MISMATCH: {}::{} has no proved write of {}::{} (installer writes: {installer_writes:?}; cell writers: {cell_writers:?})",
                installer.owner.join("::"),
                installer.name,
                cell.0.owner.join("::"),
                cell.0.name,
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
                let target_refs: Vec<_> = facts
                    .references
                    .iter()
                    .filter(|reference| &reference.symbol == target)
                    .map(|reference| reference.caller.clone())
                    .collect();
                violations.push(format!(
                    "PC2_MISSING_SEAM_REFERENCE: {}:{} (target {target:?} refs: {target_refs:?})",
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

/// Two PC2 precision relaxations are pinned here. First, a module declared by
/// several cfg arms is production when ANY arm is production: treating it as
/// test-only (the old any-arm-wins classification) would hide installer calls
/// that escape from that module. Second, an undeclared nested file under a
/// crate's `tests/`/`benches/`/`examples/` tree is exempt from the
/// unknown-module error because cargo does not compile it unless a target
/// names it; the same undeclared file under `src/` must stay an error.
#[test]
fn architecture_pc2_cfg_split_and_crate_test_tree_relaxations() {
    let dir = tempfile::tempdir().expect("SPLIT_CFG_TREE fixture");
    let root = dir.path();
    guard_fixture_workspace(root, &["neovm-core", "neovm-value"]);
    let core = root.join("crates/neovm-core/src");
    std::fs::create_dir_all(&core).expect("SPLIT_CFG_TREE src");
    std::fs::write(
        core.join("lib.rs"),
        "#[cfg(feature = \"test-support\")]\nmod feature_hook;\nmod feature_hook;\nmod low;\nmod seam;\nmod external;\n",
    )
    .expect("SPLIT_CFG_TREE lib");
    std::fs::write(
        core.join("low.rs"),
        "use std::sync::OnceLock;\nstatic HOOK: OnceLock<fn()> = OnceLock::new();\npub fn install_hook(f: fn()) { let _ = HOOK.set(f); }\n",
    )
    .expect("SPLIT_CFG_TREE low");
    std::fs::write(
        core.join("feature_hook.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("SPLIT_CFG_TREE feature_hook");
    std::fs::write(
        core.join("seam.rs"),
        "pub struct Context;\nimpl Context { pub fn new_inner() { crate::feature_hook::install(); } pub fn from_dump() { crate::feature_hook::install(); } }\n",
    )
    .expect("SPLIT_CFG_TREE seam");
    std::fs::write(core.join("external.rs"), "fn unrelated() {}\n")
        .expect("SPLIT_CFG_TREE external");
    let table = "kind\tstatic\tinstaller\tadapter\tseams\nproduction\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/src/feature_hook.rs:install\tcrates/neovm-core/src/seam.rs:Context::new_inner;crates/neovm-core/src/seam.rs:Context::from_dump\n";
    let green = scan_pc2_table(root, table).expect("SPLIT_CFG_TREE scan");
    assert!(green.is_empty(), "SPLIT_CFG_TREE baseline: {green:?}");
    // One cfg arm is unconditional, so the module is production. A second
    // installer caller inside it is an escape and must be seen.
    std::fs::write(
        core.join("feature_hook.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\npub fn escape() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("SPLIT_CFG_TREE mutation");
    let red = scan_pc2_table(root, table).expect("SPLIT_CFG_TREE mutation scan");
    assert!(
        red.iter().any(|error| error.contains("feature_hook.rs")),
        "SPLIT_CFG_TREE_ESCAPE must go red: {red:?}"
    );
    std::fs::write(
        core.join("feature_hook.rs"),
        "pub fn install() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("SPLIT_CFG_TREE restore");
    assert!(
        scan_pc2_table(root, table)
            .expect("SPLIT_CFG_TREE restored")
            .is_empty(),
        "SPLIT_CFG_TREE_RESTORED"
    );
    let tests_helper = root.join("crates/neovm-core/tests/sub/helper.rs");
    std::fs::create_dir_all(tests_helper.parent().expect("tests parent"))
        .expect("SPLIT_CFG_TREE tests dir");
    std::fs::write(
        &tests_helper,
        "fn helper() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("SPLIT_CFG_TREE tests helper");
    assert!(
        scan_pc2_table(root, table)
            .expect("SPLIT_CFG_TREE tests scan")
            .is_empty(),
        "SPLIT_CFG_TREE_TESTS_EXEMPT"
    );
    let src_unknown = core.join("sub/external.rs");
    std::fs::create_dir_all(src_unknown.parent().expect("src parent"))
        .expect("SPLIT_CFG_TREE src subdir");
    std::fs::write(
        &src_unknown,
        "fn helper() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("SPLIT_CFG_TREE src unknown");
    let red = scan_pc2_table(root, table).expect("SPLIT_CFG_TREE src scan");
    assert!(
        red.iter()
            .any(|error| error.contains("PC2_UNRESOLVED_REFERENCE")),
        "SPLIT_CFG_TREE_SRC_UNKNOWN must go red: {red:?}"
    );
}

/// Test adapters are often wired with `include!` inside a `#[cfg(test)]`
/// module, so the adapter definition lives in a file that no module
/// declaration names. The scan must read the included items in the including
/// module's context and attribute their sites to the included file; otherwise
/// the seam-table adapter has no definition and the guard cannot see calls
/// that escape it.
#[test]
fn architecture_pc2_literal_include_adapter_is_scanned() {
    let dir = tempfile::tempdir().expect("INCLUDE fixture");
    let root = dir.path();
    guard_fixture_workspace(root, &["neovm-core", "neovm-value"]);
    let core = root.join("crates/neovm-core/src");
    // The adapter lives outside the walked source roots (src/tests/benches/
    // examples); a file under src/tests would also be swept as a standalone
    // production source and fail the module-identity check.
    std::fs::create_dir_all(core.parent().unwrap().join("fixtures")).expect("INCLUDE dir");
    std::fs::write(
        core.join("lib.rs"),
        "mod low;\n#[cfg(test)] mod included_tests { include!(\"../fixtures/included_adapter.rs\"); }\n",
    )
    .expect("INCLUDE root");
    std::fs::write(
        core.join("low.rs"),
        "use std::sync::OnceLock; static HOOK: OnceLock<fn()> = OnceLock::new(); pub fn install_hook(f: fn()) { let _ = HOOK.set(f); }\n",
    )
    .expect("INCLUDE installer");
    std::fs::write(
        core.parent().unwrap().join("fixtures/included_adapter.rs"),
        "pub fn install_included() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("INCLUDE adapter");
    let table = "kind\tstatic\tinstaller\tadapter\tseams\ntest_only\tneovm_core::low::HOOK\tneovm_core::low::install_hook\tcrates/neovm-core/fixtures/included_adapter.rs:install_included\t-\n";
    let green = scan_pc2_table(root, table).expect("INCLUDE baseline scan");
    assert!(green.is_empty(), "PC2_INCLUDE_BASELINE: {green:?}");
    std::fs::write(
        core.parent().unwrap().join("fixtures/included_adapter.rs"),
        "pub fn install_included() { crate::low::install_hook(|| {}); }\nfn escaped() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("INCLUDE escape");
    let red = scan_pc2_table(root, table).expect("INCLUDE mutation scan");
    assert!(
        red.iter()
            .any(|error| error.contains("PC2_INSTALLER_OUTSIDE_EXACT_CHAIN")
                && error.contains("fixtures/included_adapter.rs")),
        "PC2_INCLUDE_ESCAPE: {red:?}"
    );
    std::fs::write(
        core.parent().unwrap().join("fixtures/included_adapter.rs"),
        "pub fn install_included() { crate::low::install_hook(|| {}); }\n",
    )
    .expect("INCLUDE restore");
    let restored = scan_pc2_table(root, table).expect("INCLUDE restored scan");
    assert!(restored.is_empty(), "PC2_INCLUDE_RESTORED: {restored:?}");
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

/// ARM64-SPLIT: the root may reach the guard as a symlink.  macOS `/tmp` and
/// `/var` are links to `/private/...`, so `tempfile::tempdir()` hands the pc2
/// self-tests a non-canonical root while every source path is canonicalized;
/// `Pc2Visitor::site`'s `strip_prefix(workspace)` then failed for every file
/// and the adapters resolved to "0 definitions" on arm64 only.  A symlinked
/// root must produce exactly the same result as the canonical one.
#[cfg(unix)]
#[test]
fn architecture_pc2_accepts_a_symlinked_root_workspace() {
    let real = tempfile::tempdir().unwrap();
    let alias_parent = tempfile::tempdir().unwrap();
    let alias = alias_parent.path().join("workspace-alias");
    std::os::unix::fs::symlink(real.path(), &alias).expect("symlink root");
    let table = pc2_sealed_fixture(real.path());
    let violations =
        scan_pc2_table(&alias, &table).expect("the guard must accept a symlinked root");
    assert!(
        violations.is_empty(),
        "PC2_SYMLINKED_ROOT: the symlinked root changed the guard's answer: {violations:?}"
    );
    assert!(
        scan_pc2_table(real.path(), &table).unwrap().is_empty(),
        "PC2_SYMLINKED_ROOT_CANONICAL"
    );
}

// ---------------------------------------------------------------------------
// TA2: workspace-wide authority references, with defining-module resolution.
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Ta2Fence {
    package: String,
    /// Every file that belongs to the fence module itself: the `test_support`
    /// module file plus the `authority` submodule files. A fence's own
    /// definition files may name it; nothing else in `src/**` may.
    definition_files: BTreeSet<PathBuf>,
    /// The `mod authority` declaration inside the test_support file.
    declaration: Option<(PathBuf, usize)>,
}

fn ta2_error(path: &Path, line: usize, code: &str, detail: &str) -> String {
    format!("{}:{line}: {code}: {detail}", path.display())
}

fn ta2_manifest(root: &Path) -> Result<toml::Value, String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ta2_error(&path, 0, "TA2_MANIFEST", &e.to_string()))?;
    toml::from_str::<toml::Value>(&text)
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
    files: &mut BTreeSet<PathBuf>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for item in items {
        let Item::Mod(module) = item else { continue };
        if let Some((_, items)) = &module.content {
            ta2_authority_submodules(
                items,
                file,
                &module_dir.join(module.ident.to_string()),
                files,
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
            files.insert(child.clone());
            let (_, parsed) = parse_file(&child)?;
            ta2_authority_submodules(&parsed.items, &child, &child_dir, files, active)?;
            active.remove(&child);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Ta2CfgTruth {
    True,
    False,
    Unknown,
}

fn ta2_cfg_truth(meta: &syn::Meta) -> Ta2CfgTruth {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Ta2CfgTruth::False,
        syn::Meta::NameValue(value) if value.path.is_ident("feature") => {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(name) = &expr.lit {
                    if name.value() == "test-support" {
                        return Ta2CfgTruth::False;
                    }
                }
            }
            Ta2CfgTruth::Unknown
        }
        syn::Meta::List(list) if list.path.is_ident("not") => {
            match list.parse_args::<syn::Meta>() {
                Ok(inner) => match ta2_cfg_truth(&inner) {
                    Ta2CfgTruth::True => Ta2CfgTruth::False,
                    Ta2CfgTruth::False => Ta2CfgTruth::True,
                    Ta2CfgTruth::Unknown => Ta2CfgTruth::Unknown,
                },
                Err(_) => Ta2CfgTruth::Unknown,
            }
        }
        syn::Meta::List(list) if list.path.is_ident("any") || list.path.is_ident("all") => {
            let any = list.path.is_ident("any");
            match list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) {
                Ok(metas) => {
                    let mut unknown = false;
                    for meta in &metas {
                        match ta2_cfg_truth(meta) {
                            Ta2CfgTruth::True if any => return Ta2CfgTruth::True,
                            Ta2CfgTruth::False if !any => return Ta2CfgTruth::False,
                            Ta2CfgTruth::Unknown => unknown = true,
                            _ => {}
                        }
                    }
                    if unknown {
                        Ta2CfgTruth::Unknown
                    } else if any {
                        Ta2CfgTruth::False
                    } else {
                        Ta2CfgTruth::True
                    }
                }
                Err(_) => Ta2CfgTruth::Unknown,
            }
        }
        _ => Ta2CfgTruth::Unknown,
    }
}

fn ta2_test_gated(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .map(|meta| ta2_cfg_truth(&meta) == Ta2CfgTruth::False)
                .unwrap_or(false)
    })
}

/// Peer-signed B-minus (21:1xZ): item-level test gating is allowed only for
/// the pure test predicates - `cfg(test)`, `cfg(feature = "test-support")`,
/// and any()/all() combinations of exactly those. A product feature, `not`,
/// or `cfg_attr` disqualifies the item.
fn ta2_pure_test_meta(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(value) if value.path.is_ident("feature") => {
            matches!(&value.value, syn::Expr::Lit(expr)
                if matches!(&expr.lit, syn::Lit::Str(name) if name.value() == "test-support"))
        }
        syn::Meta::List(list) if list.path.is_ident("any") || list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .map(|metas| !metas.is_empty() && metas.iter().all(ta2_pure_test_meta))
            .unwrap_or(false),
        _ => false,
    }
}

fn ta2_pure_test_gated(attrs: &[Attribute]) -> bool {
    let mut any_cfg = false;
    for attr in attrs {
        // cfg_attr can add a cfg in a product feature configuration, so an
        // item carrying one never counts as purely test-gated.
        if attr.path().is_ident("cfg_attr") {
            return false;
        }
        if !attr.path().is_ident("cfg") {
            continue;
        }
        let Ok(meta) = attr.parse_args::<syn::Meta>() else {
            return false;
        };
        if !ta2_pure_test_meta(&meta) {
            return false;
        }
        any_cfg = true;
    }
    any_cfg
}

fn ta2_item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn ta2_impl_item_attrs(item: &syn::ImplItem) -> &[Attribute] {
    match item {
        syn::ImplItem::Const(item) => &item.attrs,
        syn::ImplItem::Fn(item) => &item.attrs,
        syn::ImplItem::Type(item) => &item.attrs,
        syn::ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

fn ta2_trait_item_attrs(item: &syn::TraitItem) -> &[Attribute] {
    match item {
        syn::TraitItem::Const(item) => &item.attrs,
        syn::TraitItem::Fn(item) => &item.attrs,
        syn::TraitItem::Type(item) => &item.attrs,
        syn::TraitItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

fn ta2_path_attr(attrs: &[Attribute]) -> Option<PathBuf> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        if let syn::Meta::NameValue(value) = &attr.meta {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(text) = &expr.lit {
                    return Some(PathBuf::from(text.value()));
                }
            }
        }
    }
    None
}

fn ta2_module_dir(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs") | Some("main.rs") | Some("mod.rs") => parent.to_path_buf(),
        Some(name) => parent.join(name.strip_suffix(".rs").unwrap_or(name)),
        None => parent.to_path_buf(),
    }
}

fn ta2_module_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let flat = dir.join(format!("{name}.rs"));
    if flat.is_file() {
        return Some(flat);
    }
    let nested = dir.join(name).join("mod.rs");
    if nested.is_file() {
        return Some(nested);
    }
    None
}

/// Map every `src/**` file reached from the crate root to whether EVERY module
/// route to it is test-gated. An ungated route wins. This is the B2 rule's
/// "every route" test.
fn ta2_gated_route_files(member_root: &Path) -> Result<BTreeMap<PathBuf, bool>, String> {
    let src = member_root.join("src");
    let root = if src.join("lib.rs").is_file() {
        src.join("lib.rs")
    } else if src.join("main.rs").is_file() {
        src.join("main.rs")
    } else {
        return Ok(BTreeMap::new());
    };
    let mut map = BTreeMap::new();
    ta2_walk_route_file(&root, false, &mut map)?;
    Ok(map)
}

fn ta2_walk_route_file(
    file: &Path,
    gated: bool,
    map: &mut BTreeMap<PathBuf, bool>,
) -> Result<(), String> {
    let key = file
        .canonicalize()
        .map_err(|error| format!("{}: {error}", file.display()))?;
    match map.get(&key).copied() {
        Some(previous) if previous == gated => return Ok(()),
        Some(false) => return Ok(()),
        Some(true) => {
            map.insert(key, false);
        }
        None => {
            map.insert(key, gated);
        }
    }
    let (_, parsed) = parse_file(file)?;
    ta2_walk_route_items(
        &parsed.items,
        gated,
        file.parent().unwrap_or_else(|| Path::new("")),
        &ta2_module_dir(file),
        true,
        map,
    )
}

fn ta2_walk_route_items(
    items: &[Item],
    gated: bool,
    file_dir: &Path,
    module_dir: &Path,
    top_level: bool,
    map: &mut BTreeMap<PathBuf, bool>,
) -> Result<(), String> {
    for item in items {
        let Item::Mod(module) = item else { continue };
        let declaration_gated = gated || ta2_test_gated(&module.attrs);
        if let Some((_, inner)) = &module.content {
            ta2_walk_route_items(
                inner,
                declaration_gated,
                file_dir,
                &module_dir.join(module.ident.to_string()),
                false,
                map,
            )?;
            continue;
        }
        let target = match ta2_path_attr(&module.attrs) {
            Some(path) => {
                let base = if top_level { file_dir } else { module_dir };
                Some(base.join(path))
            }
            None => ta2_module_file(module_dir, &module.ident.to_string()),
        };
        if let Some(target) = target {
            if target.is_file() {
                ta2_walk_route_file(&target, declaration_gated, map)?;
            }
        }
    }
    Ok(())
}

fn ta2_fence(sources: &[GuardSource], package: &str) -> Result<Option<Ta2Fence>, String> {
    let roots: BTreeSet<_> = sources
        .iter()
        .filter(|s| s.package == package)
        .map(|s| s.member_root.clone())
        .collect();
    if roots.len() != 1 {
        return Err(format!(
            "TA2_MODULE_UNRESOLVED: expected one {package} member, found {}",
            roots.len()
        ));
    }
    let root = roots
        .iter()
        .next()
        .ok_or_else(|| format!("TA2_MODULE_UNRESOLVED: absent {package}"))?;
    let manifest = ta2_manifest(root)?;
    let lib = manifest
        .get("lib")
        .and_then(|v| v.get("path"))
        .and_then(toml::Value::as_str)
        .unwrap_or("src/lib.rs");
    let lib = match root.join(lib).canonicalize() {
        Ok(lib) => lib,
        // A bin-only member (no library target) cannot host a cross-crate
        // `test_support::authority` fence; discovery skips it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("TA2_MODULE_UNRESOLVED: {package} lib: {error}"));
        }
    };
    let (_, parsed) = parse_file(&lib)?;
    let support: Vec<_> = parsed
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Mod(m) if m.ident == "test_support" => Some(m),
            _ => None,
        })
        .collect();
    let support = match support.as_slice() {
        // Not a fence crate: TA2 discovery is the presence of the signed
        // `test_support::authority` module, never a hand-maintained list.
        [] => return Ok(None),
        [support] => *support,
        _ => {
            return Err(ta2_error(
                &lib,
                0,
                "TA2_MODULE_UNRESOLVED",
                &format!("{package}: multiple test_support declarations"),
            ));
        }
    };
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
    let mut fence = Ta2Fence {
        package: package.to_string(),
        ..Ta2Fence::default()
    };
    // The out-of-line test_support module file is the fence's own definition
    // file. An inline `test_support` in lib.rs is NOT exempt wholesale: the
    // crate root hosts every module, so only the authority declaration is.
    if !(support.content.is_some() && file == lib) {
        fence.definition_files.insert(file.clone());
    }
    // The fence's definition surface is the authority MODULE TREE: the
    // authority declaration file plus its DECLARED child modules (collected
    // below). Physical siblings in the same directory are not exempt: a
    // test_support file the authority module does not declare stays product
    // surface and cannot name the fence.
    let authority: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            Item::Mod(m) if m.ident == "authority" => Some(m),
            _ => None,
        })
        .collect();
    match authority.as_slice() {
        [] => return Ok(None), // test_support without a signed authority fence.
        [authority] => {
            fence.declaration = Some((file.clone(), line_of(authority.ident.span())));
            if let Some((_, items)) = &authority.content {
                ta2_authority_submodules(
                    items,
                    &file,
                    &module_dir.join("authority"),
                    &mut fence.definition_files,
                    &mut BTreeSet::new(),
                )?;
            } else {
                let (path, dir) = ta2_external_module(authority, &file, &module_dir)?;
                fence.definition_files.insert(path.clone());
                let (_, parsed) = parse_file(&path)?;
                let mut active = BTreeSet::from([path.clone()]);
                ta2_authority_submodules(
                    &parsed.items,
                    &path,
                    &dir,
                    &mut fence.definition_files,
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
    Ok(Some(fence))
}

/// Discover every workspace member that declares a `test_support::authority`
/// module, so a newly signed fence is policed automatically.
fn ta2_fences(sources: &[GuardSource]) -> Result<Vec<Ta2Fence>, String> {
    let mut packages: BTreeSet<String> = BTreeSet::new();
    for source in sources {
        packages.insert(source.package.clone());
    }
    let mut fences = Vec::new();
    for package in packages {
        if let Some(fence) = ta2_fence(sources, &package)? {
            fences.push(fence);
        }
    }
    Ok(fences)
}

fn ta2_workspace_inherited(
    workspace: &Path,
) -> Result<Option<toml::map::Map<String, toml::Value>>, String> {
    let manifest = ta2_manifest(workspace)?;
    Ok(manifest
        .get("workspace")
        .and_then(|v| v.get("dependencies"))
        .and_then(toml::Value::as_table)
        .cloned())
}

fn ta2_manifest_dependencies(
    manifest: &toml::Value,
    inherited: Option<&toml::map::Map<String, toml::Value>>,
) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    fn table(
        table: &toml::map::Map<String, toml::Value>,
        inherited: Option<&toml::map::Map<String, toml::Value>>,
        pairs: &mut Vec<(String, String)>,
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
            pairs.push((alias.replace('-', "_"), package.to_string()));
        }
        Ok(())
    }
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = manifest.get(kind).and_then(toml::Value::as_table) {
            table(dependencies, inherited, &mut pairs)?;
        }
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for target in targets.values() {
                if let Some(dependencies) = target.get(kind).and_then(toml::Value::as_table) {
                    table(dependencies, inherited, &mut pairs)?;
                }
            }
        }
    }
    Ok(pairs)
}

fn ta2_dependency_graph(
    members: &BTreeMap<PathBuf, String>,
    workspace: &Path,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let inherited = ta2_workspace_inherited(workspace)?;
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (root, package) in members {
        let manifest = ta2_manifest(root)?;
        let dependencies = ta2_manifest_dependencies(&manifest, inherited.as_ref())?;
        graph
            .entry(package.clone())
            .or_default()
            .extend(dependencies.into_iter().map(|(_, package)| package));
    }
    Ok(graph)
}

fn ta2_above(graph: &BTreeMap<String, BTreeSet<String>>, from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::from([from.to_string()]);
    let mut queue = vec![from.to_string()];
    while let Some(package) = queue.pop() {
        for dependency in graph.get(&package).into_iter().flatten() {
            if dependency == target {
                return true;
            }
            if seen.insert(dependency.clone()) {
                queue.push(dependency.clone());
            }
        }
    }
    false
}

struct Ta2MemberFacts {
    /// Fence-crate aliases, including `crate` inside a fence crate.
    alias_owners: BTreeMap<String, BTreeSet<usize>>,
    /// Fences a FULLY test-gated file of this member may name: the member is a
    /// higher crate (it transitively depends on the fence crate).
    gated_fences: BTreeSet<usize>,
}

fn ta2_member_facts(
    members: &BTreeMap<PathBuf, String>,
    workspace: &Path,
    fences: &[Ta2Fence],
) -> Result<BTreeMap<PathBuf, Ta2MemberFacts>, String> {
    let inherited = ta2_workspace_inherited(workspace)?;
    let graph = ta2_dependency_graph(members, workspace)?;
    let mut facts = BTreeMap::new();
    for (root, package) in members {
        let manifest = ta2_manifest(root)?;
        let dependencies = ta2_manifest_dependencies(&manifest, inherited.as_ref())?;
        let mut alias_owners: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut gated_fences = BTreeSet::new();
        for (index, fence) in fences.iter().enumerate() {
            let mut names = BTreeSet::from([fence.package.replace('-', "_")]);
            if *package == fence.package {
                names.insert("crate".to_string());
            }
            for (alias, target) in &dependencies {
                if *target == fence.package {
                    names.insert(alias.clone());
                }
            }
            for name in names {
                alias_owners.entry(name).or_default().insert(index);
            }
            if *package != fence.package && ta2_above(&graph, package, &fence.package) {
                gated_fences.insert(index);
            }
        }
        facts.insert(
            root.clone(),
            Ta2MemberFacts {
                alias_owners,
                gated_fences,
            },
        );
    }
    Ok(facts)
}

enum Ta2UseModule {
    /// The module resolves to one or more fences; the second field is the
    /// module path relative to the fence crate root.
    Fence(Vec<usize>, Vec<String>),
    /// The module resolves to a non-fence module (for example
    /// `neovm_eval::emacs_core::lread::test_support`).
    NotFence,
    /// The module cannot be resolved (bare `test_support`, `self`/`super`
    /// containing a test_support segment): fail closed.
    Unresolved,
}

struct Ta2Visitor<'a> {
    file: &'a Path,
    package: &'a str,
    fences: &'a [Ta2Fence],
    /// Fence-crate aliases visible in this file, including `crate` inside a
    /// fence crate; `use neovm_value as v;` adds `v` during the scan.
    alias_owners: BTreeMap<String, BTreeSet<usize>>,
    /// B2: the file's every module route is test-support-gated, so it may name
    /// a LOWER crate's fence through a resolved fence alias. Only fences whose
    /// crate this member transitively depends on are listed.
    gated_fences: BTreeSet<usize>,
    /// Every fence crate this member transitively depends on, regardless of
    /// any file-level gating. An item gated by a pure test predicate may name
    /// one of these (peer-signed B-minus).
    dependent_fences: BTreeSet<usize>,
    /// The current item nesting carries a pure test-gating cfg.
    item_test_gated: bool,
    /// Function-body nesting depth. A cfg on a statement item inside a block
    /// is statement-level gating, which the signed rule does not allow.
    block_depth: usize,
    /// The `use` item currently being visited is a plain `pub use`.
    public_use_item: bool,
    /// The fence whose definition file this is (same crate only): it may name
    /// its own authority module.
    own_definition: Option<usize>,
    declaration_line: Option<usize>,
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

    fn fence_of_package(&self, package: &str) -> Option<usize> {
        self.fences
            .iter()
            .position(|fence| fence.package == package)
    }

    /// FX-2 resolved-module matching: a module path imports a fence when it is
    /// the `test_support` module itself, one of its ancestors (the crate
    /// root), or lies inside `test_support::authority`. Any other module with
    /// a `test_support` segment (lread, dbusbind, ...) is a different module.
    fn fence_ancestor(path: &[String]) -> bool {
        const FENCE: [&str; 2] = ["test_support", "authority"];
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        path.is_empty() || path == ["test_support"] || path.starts_with(&FENCE)
    }

    /// Renaming an ITEM inside the fence (`authority::ExactTier as T`) is a
    /// normal import; only renaming the fence module or one of its module
    /// ancestors would move the module under an unchecked name.
    fn fence_module_path(path: &[String]) -> bool {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        path == ["test_support"] || path == ["test_support", "authority"]
    }

    fn require_fence(&mut self, fence: usize, span: Span) {
        if self.own_definition == Some(fence) {
            return;
        }
        // B2: a gated reference to the crate's OWN fence is still two routes
        // to one item; re-point it to the pub(crate) original instead.
        if self.fence_of_package(self.package) == Some(fence) {
            self.reject(
                span,
                "TA2_OWN_FENCE_REFERENCE",
                "a crate may not name its own test_support::authority fence outside the fence definition; use the pub(crate) original",
            );
            return;
        }
        if self.gated_fences.contains(&fence)
            || (self.item_test_gated && self.dependent_fences.contains(&fence))
        {
            return;
        }
        self.reject(
            span,
            "TA2_AUTHORITY_REFERENCE",
            "authority references are restricted to core_tests, a fence's own definition files, and fully gated files or pure-test-gated items of higher crates",
        );
    }

    fn check_path(&mut self, names: &[String], span: Span) {
        if let Some(index) = names.iter().position(|s| s == "test_support") {
            if names[index + 1..].iter().any(|s| s == "authority") {
                if index > 0 {
                    if let Some(owners) = self.alias_owners.get(&names[0]).cloned() {
                        for fence in owners {
                            self.require_fence(fence, span);
                        }
                        return;
                    }
                    if names[0] == "crate" {
                        if let Some(fence) = self.fence_of_package(self.package) {
                            self.require_fence(fence, span);
                        } else if self.own_definition.is_none() {
                            self.reject(
                                span,
                                "TA2_AUTHORITY_REFERENCE",
                                "crate::test_support::authority does not resolve to a signed fence module",
                            );
                        }
                        return;
                    }
                }
                // Bare `test_support::authority`, `self`/`super`, or an
                // unknown root: no allowed alias resolves the fence. A fence
                // definition file may still name its own submodule.
                if self.own_definition.is_none() {
                    self.reject(
                        span,
                        "TA2_AUTHORITY_REFERENCE",
                        "authority path does not resolve through a fence crate alias",
                    );
                }
            }
        } else if names.iter().any(|s| s == "authority") {
            if self.own_definition.is_some() {
                return;
            }
            self.reject(
                span,
                "TA2_UNRESOLVED_REFERENCE",
                "authority path has no resolved test_support owner",
            );
        }
    }

    fn token_names(&mut self, names: &[String], span: Span) {
        // FX-2: inside a macro token stream a lone `authority` ident is a
        // field or local name, not a module path (`offset_of!(T, authority)`).
        // Reaching the fence unqualified still requires an import that the
        // use-tree rules reject, so this does not open a bypass.
        if names.len() == 1 && names[0] == "authority" {
            return;
        }
        self.check_path(names, span);
    }

    fn use_module(&self, prefix: &[String]) -> Ta2UseModule {
        let Some(first) = prefix.first() else {
            return Ta2UseModule::Unresolved;
        };
        if first == "crate" {
            return match self.fence_of_package(self.package) {
                Some(fence) => Ta2UseModule::Fence(vec![fence], prefix[1..].to_vec()),
                None => Ta2UseModule::NotFence,
            };
        }
        if let Some(owners) = self.alias_owners.get(first) {
            return Ta2UseModule::Fence(owners.iter().copied().collect(), prefix[1..].to_vec());
        }
        if first == "self" || first == "super" {
            if prefix.iter().any(|segment| segment == "test_support") {
                return Ta2UseModule::Unresolved;
            }
            return Ta2UseModule::NotFence;
        }
        if first == "test_support" {
            return Ta2UseModule::Unresolved;
        }
        Ta2UseModule::NotFence
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
                match self.use_module(prefix) {
                    Ta2UseModule::Fence(owners, module) if module.is_empty() => {
                        // `use neovm_value as v;` imports the crate root, not
                        // the fence: track the alias so a later
                        // `v::test_support::authority` is still checked.
                        for fence in owners {
                            self.alias_owners
                                .entry(rename.rename.to_string())
                                .or_default()
                                .insert(fence);
                        }
                    }
                    Ta2UseModule::Fence(_, module) if Self::fence_module_path(&module) => {
                        self.reject(
                            rename.rename.span(),
                            "TA2_TEST_SUPPORT_RENAME",
                            "rename resolves to a fence module or one of its ancestors",
                        );
                    }
                    Ta2UseModule::Unresolved => {
                        self.reject(
                            rename.rename.span(),
                            "TA2_TEST_SUPPORT_RENAME",
                            "test_support rename does not resolve to a known non-fence module",
                        );
                    }
                    _ => {}
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
            UseTree::Glob(glob) => match self.use_module(prefix) {
                Ta2UseModule::Fence(_, module) if Self::fence_ancestor(&module) => {
                    self.reject(
                        glob.star_token.span,
                        "TA2_ANCESTOR_GLOB",
                        "glob resolves to a fence module or one of its ancestors",
                    );
                }
                Ta2UseModule::Unresolved => {
                    self.reject(
                        glob.star_token.span,
                        "TA2_ANCESTOR_GLOB",
                        "glob module does not resolve to a known non-fence module",
                    );
                }
                _ => {}
            },
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
                    // `use $crate::...;` appears inside exported macros; the
                    // hygiene token is not part of a syn UseTree. Drop the `$`
                    // before `crate` so the tree resolves as the defining
                    // crate's root (the same meaning `$crate` has).
                    let slice = &tokens[index..=index + end];
                    let use_tokens: proc_macro2::TokenStream = slice
                        .iter()
                        .enumerate()
                        .filter(|(offset, token)| {
                            !(matches!(token, TokenTree::Punct(p) if p.as_char() == '$')
                                && matches!(slice.get(offset + 1),
                                    Some(TokenTree::Ident(id)) if id == "crate"))
                        })
                        .map(|(_, token)| token.clone())
                        .collect();
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
                        self.token_names(&names, first.span());
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

/// Whether a use tree textually contains the fence sequence
/// `test_support::authority` (used for the plain-`pub use` ban).
fn ta2_use_names_fence(tree: &UseTree) -> bool {
    fn walk(tree: &UseTree, prefix: &mut Vec<String>) -> bool {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                let hit = walk(&path.tree, prefix);
                prefix.pop();
                hit
            }
            UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                let hit = prefix
                    .windows(2)
                    .any(|w| w == ["test_support", "authority"]);
                prefix.pop();
                hit
            }
            UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                let hit = prefix
                    .windows(2)
                    .any(|w| w == ["test_support", "authority"]);
                prefix.pop();
                hit
            }
            UseTree::Glob(_) => prefix
                .windows(2)
                .any(|w| w == ["test_support", "authority"]),
            UseTree::Group(group) => group.items.iter().any(|item| walk(item, prefix)),
        }
    }
    walk(tree, &mut Vec::new())
}

impl<'ast> Visit<'ast> for Ta2Visitor<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        let saved = self.item_test_gated;
        if self.block_depth == 0 && ta2_pure_test_gated(ta2_item_attrs(item)) {
            self.item_test_gated = true;
        }
        visit::visit_item(self, item);
        self.item_test_gated = saved;
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let saved = self.item_test_gated;
        if self.block_depth == 0 && ta2_pure_test_gated(ta2_impl_item_attrs(item)) {
            self.item_test_gated = true;
        }
        visit::visit_impl_item(self, item);
        self.item_test_gated = saved;
    }
    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let saved = self.item_test_gated;
        if self.block_depth == 0 && ta2_pure_test_gated(ta2_trait_item_attrs(item)) {
            self.item_test_gated = true;
        }
        visit::visit_trait_item(self, item);
        self.item_test_gated = saved;
    }
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
        self.block_depth += 1;
        visit::visit_block(self, block);
        self.block_depth -= 1;
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
        let saved = self.public_use_item;
        self.public_use_item = matches!(item.vis, syn::Visibility::Public(_));
        if self.public_use_item && ta2_use_names_fence(&item.tree) {
            self.reject(
                item.use_token.span,
                "TA2_PUB_USE_FENCE",
                "`pub use` may not re-export a test_support::authority fence item; use at most pub(crate) in a pure-test-gated item",
            );
        }
        self.use_tree(&item.tree, &mut Vec::new());
        self.public_use_item = saved;
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
        if module.ident == "authority"
            && self.declaration_line == Some(line_of(module.ident.span()))
            && module.content.is_none()
        {
            return; // The out-of-line declaration is not a reference site.
        }
        visit::visit_item_mod(self, module);
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if let Some(owners) = self.alias_owners.get(&item.ident.to_string()).cloned() {
            if let Some((_, alias)) = &item.rename {
                for fence in owners {
                    self.alias_owners
                        .entry(alias.to_string())
                        .or_default()
                        .insert(fence);
                }
            }
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_path(self, &mac.path);
    }
}

/// FX-2: a source that syn cannot parse (a truncated fixture or an incomplete
/// macro body) no longer fails the whole scan. Such a file cannot participate
/// in a resolved fence path, so only the exact token sequence
/// `test_support :: authority` remains fail-closed.
fn ta2_raw_fence_sequence(text: &str) -> Option<usize> {
    let cleaned = cleaned_code(text);
    let mut tokens: Vec<(String, usize)> = Vec::new();
    let mut chars = cleaned.char_indices().peekable();
    let mut line = 1usize;
    while let Some((index, ch)) = chars.next() {
        if ch == '\n' {
            line += 1;
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = index;
            let mut end = index + ch.len_utf8();
            while let Some(&(next, inner)) = chars.peek() {
                if inner.is_ascii_alphanumeric() || inner == '_' {
                    end = next + inner.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push((cleaned[start..end].to_string(), line));
        } else {
            tokens.push((ch.to_string(), line));
        }
    }
    for window in tokens.windows(4) {
        if window[0].0 == "test_support"
            && window[1].0 == ":"
            && window[2].0 == ":"
            && window[3].0 == "authority"
        {
            return Some(window[0].1);
        }
    }
    None
}

/// Integration/bench/example targets are compiled only by test commands; a
/// fence reference there is test-gated by construction.
fn ta2_is_test_target(path: &Path, member_root: &Path) -> bool {
    ["tests", "benches", "examples"].iter().any(|directory| {
        path.strip_prefix(member_root)
            .ok()
            .and_then(|relative| relative.components().next())
            .map(|first| first.as_os_str().to_str() == Some(directory))
            .unwrap_or(false)
    })
}

pub(crate) fn scan_authority_workspace(workspace: &Path) -> Result<(), Vec<String>> {
    let sources = discover_sources(workspace).map_err(|error| vec![error])?;
    let fences = ta2_fences(&sources).map_err(|error| vec![error])?;
    let mut members: BTreeMap<PathBuf, String> = BTreeMap::new();
    for source in &sources {
        members.insert(source.member_root.clone(), source.package.clone());
    }
    let member_facts =
        ta2_member_facts(&members, workspace, &fences).map_err(|error| vec![error])?;
    let mut violations = Vec::new();
    // B2: a src file's every module route must be test-support-gated before it
    // may name a lower crate's fence. Compute the route map once per member.
    let mut gated_routes: BTreeMap<PathBuf, BTreeMap<PathBuf, bool>> = BTreeMap::new();
    for source in &sources {
        if gated_routes.contains_key(&source.member_root) {
            continue;
        }
        match ta2_gated_route_files(&source.member_root) {
            Ok(map) => {
                gated_routes.insert(source.member_root.clone(), map);
            }
            Err(error) => violations.push(error),
        }
    }
    for source in &sources {
        let path = match source.path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                violations.push(ta2_error(&source.path, 0, "TA2_SOURCE", &error.to_string()));
                continue;
            }
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                violations.push(ta2_error(&path, 0, "TA2_SOURCE", &error.to_string()));
                continue;
            }
        };
        let parsed = match syn::parse_file(&text) {
            Ok(parsed) => Some(parsed),
            Err(_) => {
                // FX-2 token fallback instead of failing the whole scan.
                if let Some(line) = ta2_raw_fence_sequence(&text) {
                    violations.push(ta2_error(
                        &path,
                        line,
                        "TA2_AUTHORITY_REFERENCE",
                        "unparseable source names test_support::authority",
                    ));
                }
                None
            }
        };
        let Some(parsed) = parsed else {
            continue;
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
        if allowed_core {
            continue;
        }
        let facts = match member_facts.get(&source.member_root) {
            Some(facts) => facts,
            None => {
                violations.push(ta2_error(&path, 0, "TA2_SOURCE", "member facts missing"));
                continue;
            }
        };
        let own_definition = fences.iter().position(|fence| {
            fence.package == source.package && fence.definition_files.contains(&path)
        });
        let declaration_line = fences
            .iter()
            .filter(|fence| fence.package == source.package)
            .find_map(|fence| {
                fence
                    .declaration
                    .as_ref()
                    .and_then(|(file, line)| (file == &path).then_some(*line))
            });
        let fully_gated = gated_routes
            .get(&source.member_root)
            .and_then(|map| map.get(&path).copied())
            .unwrap_or(false)
            || ta2_is_test_target(&path, &source.member_root);
        let mut visitor = Ta2Visitor {
            file: &path,
            package: &source.package,
            fences: &fences,
            alias_owners: facts.alias_owners.clone(),
            gated_fences: if fully_gated {
                facts.gated_fences.clone()
            } else {
                BTreeSet::new()
            },
            dependent_fences: facts.gated_fences.clone(),
            item_test_gated: false,
            block_depth: 0,
            public_use_item: false,
            own_definition,
            declaration_line,
            violations: Vec::new(),
            value_scopes: Vec::new(),
        };
        // Resolve crate aliases before visiting references, regardless of declaration order.
        loop {
            let before = visitor.alias_owners.len();
            visitor.visit_file(&parsed);
            if visitor.alias_owners.len() == before {
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
        "[workspace]\nmembers = [\"core\", \"value\", \"eval\", \"other\"]\n",
    )
    .unwrap();
    for (dir, package, dependencies) in [
        (
            "core",
            "neovm-core",
            "neovm-value = { path = \"../value\" }\nneovm-eval = { path = \"../eval\" }\n",
        ),
        ("value", "neovm-value", ""),
        (
            "eval",
            "neovm-eval",
            "neovm-value = { path = \"../value\" }\n",
        ),
        ("other", "other", ""),
    ] {
        std::fs::create_dir_all(root.join(dir).join("src")).unwrap();
        std::fs::write(
            root.join(dir).join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n{dependencies}"),
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
    std::fs::write(
        root.join("eval/src/lib.rs"),
        "#[cfg(any(test, feature = \"test-support\"))] pub mod test_support;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("eval/src/test_support.rs"),
        "pub mod authority { pub fn harness() {} }\n",
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
    // B2: a src file whose EVERY module route is test-support-gated may name
    // a LOWER crate's fence through a resolved value alias.
    let core_lib = root.join("core/src/lib.rs");
    let core_lib_original = std::fs::read_to_string(&core_lib).unwrap();
    let gated_probe = root.join("core/src/gated_fence_probe.rs");
    let gated_declaration = format!(
        "{core_lib_original}\n#[cfg(any(test, feature = \"test-support\"))]\npub mod gated_fence_probe;\n"
    );
    std::fs::write(&core_lib, &gated_declaration).unwrap();
    std::fs::write(&gated_probe, "use neovm_value::test_support::authority;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_GATED_LOWER_FENCE"
    );
    // (i) an ungated route makes the same file production src: red.
    std::fs::write(
        &core_lib,
        format!("{core_lib_original}\npub mod gated_fence_probe;\n"),
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_UNGATED_FENCE"
    );
    // A gated file may not name its OWN crate's fence.
    std::fs::write(&core_lib, &gated_declaration).unwrap();
    std::fs::write(&gated_probe, "use crate::test_support::authority;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_OWN_FENCE"
    );
    std::fs::write(&core_lib, &core_lib_original).unwrap();
    std::fs::remove_file(&gated_probe).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_GATED_FENCE"
    );
    // FX-2 (i): MULTI-FENCE. The eval fence follows the same rules as the
    // value fence: core_tests or a fully gated higher-crate src file.
    let eval_probe = root.join("core/src/eval_fence_probe.rs");
    std::fs::write(
        &eval_probe,
        "use neovm_eval::test_support::authority::harness;\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_EVAL_FENCE_SRC"
    );
    std::fs::write(
        &core_lib,
        format!(
            "{core_lib_original}\n#[cfg(any(test, feature = \"test-support\"))]\npub mod eval_fence_probe;\n"
        ),
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_GATED_EVAL_FENCE"
    );
    std::fs::write(
        root.join("core/core_tests/eval_fence_allowed.rs"),
        "use neovm_eval::test_support::authority::harness;\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_CORE_TESTS_EVAL_FENCE"
    );
    std::fs::remove_file(root.join("core/core_tests/eval_fence_allowed.rs")).unwrap();
    std::fs::write(&core_lib, &core_lib_original).unwrap();
    std::fs::remove_file(&eval_probe).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_EVAL_FENCE"
    );
    // FX-2 (i): each crate's own fence definition file may name its own
    // authority module; no other src file of that crate may.
    let eval_support = root.join("eval/src/test_support.rs");
    let eval_support_original = std::fs::read_to_string(&eval_support).unwrap();
    std::fs::write(
        &eval_support,
        format!(
            "{eval_support_original}\npub fn own_probe() {{ crate::test_support::authority::harness(); }}\n"
        ),
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_OWN_FENCE_DEFINITION"
    );
    let eval_other = root.join("eval/src/own_probe.rs");
    std::fs::write(
        &eval_other,
        "pub fn own_probe() { crate::test_support::authority::harness(); }\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_OWN_FENCE_SRC"
    );
    std::fs::remove_file(&eval_other).unwrap();
    std::fs::write(&eval_support, eval_support_original).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_OWN_FENCE_DEFINITION"
    );
    // FX-2 (ii): RESOLVED-MODULE matching. A glob or rename of a test_support
    // module that resolves elsewhere is not the fence (lread, dbusbind).
    let precision = root.join("core/src/precision_probe.rs");
    std::fs::write(
        &precision,
        "use neovm_eval::emacs_core::lread::test_support::*;\nuse crate::emacs_core::dbusbind::test_support as event;\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESOLVED_MODULE_PRECISION"
    );
    std::fs::write(&precision, "use neovm_eval::test_support::*;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_FENCE_GLOB"
    );
    std::fs::write(&precision, "use crate::test_support as ts;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESOLVED_CRATE_MODULE"
    );
    std::fs::write(&precision, "use neovm_eval::test_support as ts;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_FENCE_RENAME"
    );
    std::fs::write(
        &precision,
        "use neovm_value as v;\nfn probe() { v::test_support::authority::forged(); }\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_ALIASED_FENCE"
    );
    std::fs::write(&precision, "use neovm_value as v;\n").unwrap();
    assert!(scan_authority_workspace(root).is_ok(), "TA2_ALIAS_ALONE");
    // FX-2 (ii): a lone `authority` token inside a macro stream is a field,
    // not a module path (`offset_of!`); qualified uses stay fenced.
    std::fs::write(
        &precision,
        "fn probe() { let _ = offset_of!(HashTableObj, table.data.authority); let _ = offset_of!(HashTableStorage, authority); }\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_FIELD_TOKEN_PRECISION"
    );
    std::fs::write(&precision, "fn probe() { probe!(authority::forge()); }\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_QUALIFIED_TOKEN"
    );
    std::fs::write(
        &precision,
        "macro_rules! m { () => { neovm_eval::test_support::authority::harness(); }; }\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_FENCE_TOKEN_SEQUENCE"
    );
    std::fs::remove_file(&precision).unwrap();
    // FX-2 (ii): TOKEN FALLBACK for unparseable sources. Only the exact
    // `test_support :: authority` sequence stays fail-closed.
    let broken = root.join("other/src/broken_fixture.rs");
    std::fs::write(&broken, "fn truncated() {\n    let x = 1;\n").unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_UNPARSEABLE_FALLBACK"
    );
    std::fs::write(
        &broken,
        "use neovm_value::test_support::authority;\nfn truncated() {\n",
    )
    .unwrap();
    assert!(
        scan_authority_workspace(root).is_err(),
        "TA2_MUTATION_UNPARSEABLE_FENCE_SEQUENCE"
    );
    std::fs::remove_file(&broken).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORED_UNPARSEABLE"
    );
}

// ---------------------------------------------------------------------------
// LEAD 21:1xZ B-minus: discovery must police EVERY crate that declares a
// `test_support::authority` module, and item-level gating is allowed only for
// lower-crate fences under the pure test predicates. Each mutation below is
// planted, shown RED, reverted, and followed by a GREEN scan.
// ---------------------------------------------------------------------------

fn ta2_discovery_fixture(root: &Path) {
    let fences = ["value", "eval", "edit", "text", "system", "display"];
    let mut members = String::from("\"top\", \"core\"");
    for name in fences {
        members.push_str(&format!(", \"{name}\""));
    }
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers = [{members}]\n"),
    )
    .unwrap();
    let mut top_deps = String::new();
    for name in fences {
        let package = format!("neovm-{name}");
        top_deps.push_str(&format!("neovm-{name} = {{ path = \"../{name}\" }}\n"));
        std::fs::create_dir_all(root.join(name).join("src/test_support")).unwrap();
        std::fs::write(
            root.join(name).join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        std::fs::write(
            root.join(name).join("src/lib.rs"),
            "#[cfg(feature = \"test-support\")] pub mod test_support;\n",
        )
        .unwrap();
        std::fs::write(
            root.join(name).join("src/test_support.rs"),
            "pub mod authority;\n",
        )
        .unwrap();
        std::fs::write(
            root.join(name).join("src/test_support/authority.rs"),
            "pub fn forged() {}\n",
        )
        .unwrap();
    }
    std::fs::create_dir_all(root.join("core/src")).unwrap();
    std::fs::write(
        root.join("core/Cargo.toml"),
        "[package]\nname = \"neovm-core\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(root.join("core/src/lib.rs"), "").unwrap();
    std::fs::create_dir_all(root.join("top/src")).unwrap();
    std::fs::write(
        root.join("top/Cargo.toml"),
        format!("[package]\nname = \"top\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n{top_deps}"),
    )
    .unwrap();
    std::fs::write(root.join("top/src/lib.rs"), "").unwrap();
}

#[test]
fn architecture_ta2_fence_discovery_and_bminus_mutations() {
    let dir = tempfile::tempdir().unwrap();
    ta2_discovery_fixture(dir.path());
    let root = dir.path();
    let top_lib = root.join("top/src/lib.rs");
    let value_lib = root.join("value/src/lib.rs");
    if let Err(errors) = scan_authority_workspace(root) {
        panic!("TA2_DISCOVERY_GREEN: {errors:?}");
    }

    // Discovery: an ungated reference to EACH fence crate is red. A crate
    // missing from discovery would not be recognized as a fence at all.
    for name in ["value", "eval", "edit", "text", "system", "display"] {
        let source = format!("use neovm_{name}::test_support::authority::forged;\n");
        std::fs::write(&top_lib, &source).unwrap();
        let red = scan_authority_workspace(root).expect_err(&format!("TA2_DISCOVERY_RED_{name}"));
        assert!(
            red.iter()
                .any(|error| error.contains("TA2_AUTHORITY_REFERENCE")
                    && error.contains("top/src/lib.rs")),
            "TA2_DISCOVERY_RED_{name}: {red:?}"
        );
        std::fs::write(&top_lib, "").unwrap();
        if let Err(errors) = scan_authority_workspace(root) {
            panic!("TA2_DISCOVERY_RESTORE_{name}: {errors:?}");
        }
    }

    // Allowed: a lower-crate fence named by an explicit pub(crate) alias
    // inside a pure-test-gated item.
    let allowed = "#[cfg(test)]\npub(crate) use neovm_value::test_support::authority::forged;\n";
    std::fs::write(&top_lib, allowed).unwrap();
    assert!(scan_authority_workspace(root).is_ok(), "TA2_BMINUS_ALLOWED");
    let allowed_all = "#[cfg(all(test, feature = \"test-support\"))]\npub(crate) use neovm_value::test_support::authority::forged;\n";
    std::fs::write(&top_lib, allowed_all).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_BMINUS_ALLOWED_ALL"
    );

    let cases = [
        (
            "UNGATED_TWIN",
            "pub(crate) use neovm_value::test_support::authority::forged;\n",
            "TA2_AUTHORITY_REFERENCE",
        ),
        (
            "PRODUCT_FEATURE",
            "#[cfg(any(test, feature = \"jit\"))]\npub(crate) use neovm_value::test_support::authority::forged;\n",
            "TA2_AUTHORITY_REFERENCE",
        ),
        (
            "STATEMENT_CFG",
            "fn local() {\n    #[cfg(test)]\n    use neovm_value::test_support::authority::forged;\n}\n",
            "TA2_AUTHORITY_REFERENCE",
        ),
        (
            "GLOB_ALIAS",
            "#[cfg(test)]\npub(crate) use neovm_value::test_support::authority::*;\n",
            "TA2_ANCESTOR_GLOB",
        ),
        (
            "PUB_USE",
            "#[cfg(test)]\npub use neovm_value::test_support::authority::forged;\n",
            "TA2_PUB_USE_FENCE",
        ),
        (
            "CFG_ATTR_GATE",
            "#[cfg(test)]\n#[cfg_attr(feature = \"x\", allow(dead_code))]\npub(crate) use neovm_value::test_support::authority::forged;\n",
            "TA2_AUTHORITY_REFERENCE",
        ),
        (
            "NOT_GATE",
            "#[cfg(not(feature = \"jit\"))]\npub(crate) use neovm_value::test_support::authority::forged;\n",
            "TA2_AUTHORITY_REFERENCE",
        ),
        (
            "MODULE_RENAME",
            "#[cfg(test)]\nuse neovm_value::test_support::authority as a;\n",
            "TA2_TEST_SUPPORT_RENAME",
        ),
    ];
    for (witness, source, code) in cases {
        std::fs::write(&top_lib, source).unwrap();
        let red = scan_authority_workspace(root).expect_err(&format!("TA2_MUTATION_{witness}"));
        assert!(
            red.iter().any(|error| error.contains(code)),
            "TA2_MUTATION_{witness}: expected {code}: {red:?}"
        );
        std::fs::write(&top_lib, allowed).unwrap();
        assert!(
            scan_authority_workspace(root).is_ok(),
            "TA2_RESTORE_{witness}"
        );
    }

    // Own-crate fence reference from a gated item stays red (B2).
    let value_original = std::fs::read_to_string(&value_lib).unwrap();
    std::fs::write(
        &value_lib,
        format!(
            "{value_original}#[cfg(test)]\npub(crate) use crate::test_support::authority::forged;\n"
        ),
    )
    .unwrap();
    let red = scan_authority_workspace(root).expect_err("TA2_MUTATION_OWN_CRATE");
    assert!(
        red.iter()
            .any(|error| error.contains("TA2_OWN_FENCE_REFERENCE")),
        "TA2_MUTATION_OWN_CRATE: {red:?}"
    );
    std::fs::write(&value_lib, value_original).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORE_OWN_CRATE"
    );

    // Fail closed: a malformed manifest is red, not silently skipped. The
    // discovery reader catches it before TA2's own manifest read; both parse
    // through toml::from_str and neither may treat it as absent.
    let top_manifest = root.join("top/Cargo.toml");
    let manifest_original = std::fs::read_to_string(&top_manifest).unwrap();
    std::fs::write(&top_manifest, "not = [toml\n").unwrap();
    let red = scan_authority_workspace(root).expect_err("TA2_MUTATION_MALFORMED_MANIFEST");
    assert!(
        red.iter()
            .any(|error| error.contains("SPLIT_DISCOVERY_PARSE") || error.contains("TA2_MANIFEST")),
        "TA2_MUTATION_MALFORMED_MANIFEST: {red:?}"
    );
    std::fs::write(&top_manifest, manifest_original).unwrap();
    assert!(
        scan_authority_workspace(root).is_ok(),
        "TA2_RESTORE_MANIFEST"
    );

    // ta2_manifest itself fails closed on a malformed document.
    let bad_dir = root.join("badmanifest");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("Cargo.toml"), "not = [toml\n").unwrap();
    let error = ta2_manifest(&bad_dir).expect_err("TA2_MANIFEST_MALFORMED_DIRECT");
    assert!(
        error.contains("TA2_MANIFEST"),
        "TA2_MANIFEST_MALFORMED_DIRECT: {error}"
    );
}
