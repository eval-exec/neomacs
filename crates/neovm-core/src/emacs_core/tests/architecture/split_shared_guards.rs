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
//!    `(installer, seam file:function, seam file:function)`; calls to the
//!    installer are allowed only at those two seams.
//! 4. **TA2 authority fence**: `test_support::authority` may be named only
//!    from `crates/neovm-core/core_tests/**`, never from `src/**`, including
//!    aliases and glob imports.
//!
//! All scans are fail-closed: unparseable sources, missing tables, malformed
//! rows and any occurrence not accounted for by an allowed site fail.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Expr, Item, UseTree};

const PC1_TABLE: &str = include_str!("pc1_hot_path_methods.tsv");
const PC2_TABLE: &str = include_str!("pc2_hook_seams.tsv");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/neovm-core is two levels below the workspace root")
        .to_path_buf()
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

fn rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut files = BTreeSet::new();
    guard_walk_sources(root, &mut BTreeSet::new(), &mut files)
        .unwrap_or_else(|error| panic!("{error}"));
    out.extend(files);
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

// ---------------------------------------------------------------------------
// PC2: hook installer two-seam table.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Pc2Row {
    installer: String,
    seams: BTreeSet<(String, String)>,
}

fn parse_pc2_table(text: &str) -> Result<Vec<Pc2Row>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.starts_with("installer\t") || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 3 {
            return Err(format!("pc2 table line {}: expected 3 columns", index + 1));
        }
        let installer = fields[0].to_string();
        let mut seams = BTreeSet::new();
        for pair in &fields[1..] {
            let Some((file, function)) = pair.split_once(':') else {
                return Err(format!(
                    "pc2 table line {}: seam is not file:function",
                    index + 1
                ));
            };
            seams.insert((file.to_string(), function.to_string()));
        }
        if seams.len() != 2 {
            return Err(format!(
                "pc2 table line {}: expected exactly two seams",
                index + 1
            ));
        }
        rows.push(Pc2Row { installer, seams });
    }
    Ok(rows)
}

struct InstallerCallCollector<'a> {
    installer: &'a str,
    current_fn: Option<String>,
    sites: BTreeSet<String>,
}

impl<'a, 'ast> Visit<'ast> for InstallerCallCollector<'a> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.current_fn.replace(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.current_fn = previous;
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            if path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.installer)
            {
                if let Some(enclosing) = self.current_fn.clone() {
                    self.sites.insert(enclosing);
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
}

pub(crate) fn scan_pc2_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc2_table(text)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let crates_dir = workspace.join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            rs_files(&src, &mut files);
        }
        let tests = entry.path().join("tests");
        if tests.is_dir() {
            rs_files(&tests, &mut files);
        }
    }
    let mut violations = Vec::new();
    for row in &rows {
        for file in &files {
            let Ok(text) = std::fs::read_to_string(file) else {
                continue;
            };
            let Ok(parsed) = syn::parse_file(&text) else {
                violations.push(format!("{}: unparseable", file.display()));
                continue;
            };
            let mut collector = InstallerCallCollector {
                installer: &row.installer,
                current_fn: None,
                sites: BTreeSet::new(),
            };
            collector.visit_file(&parsed);
            let relative = file
                .strip_prefix(workspace)
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned();
            for function in &collector.sites {
                let signed = row.seams.iter().any(|(seam_file, seam_fn)| {
                    seam_fn == function
                        && (relative.ends_with(seam_file) || seam_file.ends_with(&relative))
                });
                if !signed {
                    violations.push(format!(
                        "{}:{}: {} installer call outside the two signed seams ({:?})",
                        relative, function, row.installer, row.seams
                    ));
                }
            }
        }
    }
    Ok(violations)
}

#[test]
fn architecture_pc2_hook_installers_have_exactly_two_seams() {
    match scan_pc2_table(&workspace_root(), PC2_TABLE) {
        Ok(violations) if violations.is_empty() => {}
        Ok(violations) => panic!(
            "{} PC2 hook-installer violations:\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Err(error) => panic!("PC2 table: {error}"),
    }
}

#[test]
fn architecture_pc2_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(
        core.join("construct.rs"),
        "pub fn new_inner() { unsafe { install_scratch_hook(1); } }\n",
    )
    .unwrap();
    std::fs::write(
        core.join("pdump_reconstruct.rs"),
        "pub fn restore() { unsafe { install_scratch_hook(2); } }\n",
    )
    .unwrap();
    let table = "installer\tseam1\tseam2\n\
                 install_scratch_hook\tcrates/neovm-core/src/construct.rs:new_inner\tcrates/neovm-core/src/pdump_reconstruct.rs:restore\n";
    let green = scan_pc2_table(dir.path(), table).unwrap();
    assert!(green.is_empty(), "false positive: {green:?}");
    std::fs::write(
        core.join("extra.rs"),
        "pub fn break_it() { unsafe { install_scratch_hook(3); } }\n",
    )
    .unwrap();
    let red = scan_pc2_table(dir.path(), table).unwrap();
    assert!(!red.is_empty(), "mutation not detected");
    assert!(red[0].contains("extra.rs"), "wrong site: {red:?}");
}

// ---------------------------------------------------------------------------
// TA2: authority fence is core_tests-only.
// ---------------------------------------------------------------------------

fn compact_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub(crate) fn scan_authority_fence(core_root: &Path) -> Result<(), Vec<String>> {
    let src = core_root.join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    let mut violations = Vec::new();
    for path in files {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                violations.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let cleaned = cleaned_code(&text);
        let compact = compact_whitespace(&cleaned);
        if compact.contains("test_support::authority") {
            violations.push(format!(
                "{}: names test_support::authority from core src (fence is core_tests/** only)",
                path.display()
            ));
        }
        if compact.contains("test_support::*") {
            violations.push(format!(
                "{}: glob-imports test_support (can reach authority)",
                path.display()
            ));
        }
        if let Some(rest) = compact.split("test_supportas").nth(1) {
            let alias: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !alias.is_empty() && compact.contains(&format!("{alias}::authority")) {
                violations.push(format!(
                    "{}: aliases test_support as {alias} and names {alias}::authority",
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
fn architecture_test_support_authority_is_named_only_from_core_tests() {
    if let Err(violations) = scan_authority_fence(&workspace_root().join("crates/neovm-core")) {
        panic!(
            "{} TA2 authority-fence violations:\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn architecture_ta2_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = dir.path().join("crates/neovm-core");
    std::fs::create_dir_all(core.join("src")).unwrap();
    std::fs::create_dir_all(core.join("core_tests")).unwrap();
    std::fs::write(
        core.join("src/clean.rs"),
        "pub fn f() { neovm_value::test_support::some_query(); }\n",
    )
    .unwrap();
    let green = scan_authority_fence(&core);
    assert!(green.is_ok(), "false positive: {green:?}");
    std::fs::write(
        core.join("src/bad.rs"),
        "pub fn g() { neovm_value::test_support::authority::forge(); }\n",
    )
    .unwrap();
    let red = scan_authority_fence(&core);
    assert!(red.is_err(), "mutation not detected");
    std::fs::remove_file(core.join("src/bad.rs")).unwrap();
    std::fs::write(
        core.join("core_tests/allowed.rs"),
        "pub fn h() { neovm_value::test_support::authority::forge(); }\n",
    )
    .unwrap();
    let allowed = scan_authority_fence(&core);
    assert!(allowed.is_ok(), "core_tests must be allowed: {allowed:?}");
    std::fs::write(
        core.join("src/alias.rs"),
        "use neovm_value::test_support as ts;\npub fn i() { ts::authority::forge(); }\n",
    )
    .unwrap();
    let alias = scan_authority_fence(&core);
    assert!(alias.is_err(), "alias mutation not detected");
}
