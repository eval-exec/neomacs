//! TA2 authority fence: a test-support route cannot reach product callers.
//!
//! Each signed `test_support::authority` fence (`neovm-value`, `neovm-eval`,
//! ...) may be named only from `crates/neovm-core/core_tests/**`, a fence's
//! own definition files, or (B2) a src file whose EVERY module route is
//! test-support-gated in a HIGHER crate, through a resolved fence alias.
//! Globs and renames are resolved-module matched (lread::test_support is not
//! the fence), macro tokens treat a lone `authority` as a field, and
//! unparseable sources are scanned for the exact `test_support :: authority`
//! sequence instead of failing the whole guard.
//!
//! Source discovery follows every workspace member's manifest (src, tests,
//! benches, examples, build scripts, explicit targets, and core's
//! `core_tests`/`core_test_facades`). All scans are fail-closed: unparseable
//! sources, missing members and any occurrence not accounted for by an
//! allowed site fail.
//!
//! Extracted from the retired split-era shared guard set: its TA2 half and
//! the source discovery that half uses. The N13/PC1/PC2 halves were dropped
//! with their ledgers.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::workspace_root as shared_workspace_root;
use syn::{Attribute, Expr, Item, UseTree};
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

/// The segment names of `path`, with any raw-identifier `r#` prefix removed.
fn path_names(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| {
            let name = segment.ident.to_string();
            name.strip_prefix("r#").map(str::to_string).unwrap_or(name)
        })
        .collect()
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
