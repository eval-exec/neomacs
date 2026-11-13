//! B3 plain-pub surface arm (peer finding 04:3xZ; lead R-c).
//!
//! The B3 promotion ledgers cover `pub` definitions that carry
//! `#[doc(hidden)]`: that is the deliberate cross-crate K1 surface and it is
//! invisible in rustdoc. They say nothing about PLAIN `pub`, so a crate move
//! can widen production visibility without any ledger change. The eval move
//! did exactly that for ~39 `Context` fields that were `pub(crate)` before
//! the move.
//!
//! This arm freezes each extracted crate's plain-pub surface at the reviewed
//! baseline (`PLAIN-PUB-<CRATE>.tsv`), with a `.ceiling` ratchet:
//!
//! * every plain-pub identity found in the crate must already be in the
//!   baseline (`B3_PLAIN_PUB_GROWTH` otherwise). Adding an identity is the
//!   review point;
//! * the baseline row count must not exceed the ceiling, and the ceiling may
//!   only grow in a ceiling-only commit whose subject starts with the B3 bump
//!   subject;
//! * removals (`doc(hidden)`, `pub(crate)`, deleted items) are allowed and
//!   need no baseline edit, so R-a/R-b style narrowing never blocks.
//!
//! The surface is conservative: every declaration with plain `pub` in a
//! production `.rs` file, including named struct/union fields, inherent impl
//! items and `pub use` re-exports (identity: file, use-tree leaf, rename; a
//! glob statement is one row `use <path>::*`, so a new glob re-export is
//! visible as growth and the item identities behind it stay with their
//! defining crate's scan).
//! `#[doc(hidden)]` items and items gated by `cfg(test)`/
//! `cfg(feature = "test-support")` are excluded (other classes). A test-gated
//! `mod` declaration also removes the whole file it names, even when the file
//! name does not look like a test; an ungated route to the same file wins. A
//! test-gated struct or union gates its fields too: the test arm of a
//! cfg-split struct pair declares `pub` fields for cross-crate test access,
//! which is not production surface.
//! Test files (`.../tests/...`, `tests.rs`, `*_test.rs`) are excluded.
//! Macro-generated items are not scanned; the API projector is not a live
//! gate, so plain `pub use` (including one row per glob statement) is
//! enforced here. Duplicate identical identities in one file collapse into
//! one row; reordering or duplication is not growth.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::{Attribute, ImplItem, Item, Visibility};

use super::architecture_production_source_roots::workspace_root;

const ARCHITECTURE_DIR: &str = "crates/neovm-core/src/emacs_core/tests/architecture";
const PREFIX: &str = "PLAIN-PUB-";
const HEADER: &str = "file\tkind\titem";
const BUMP_SUBJECT: &str = "PC11 PROMOTION-LEDGER ceiling bump:";
const CRATES: &[(&str, &str)] = &[
    ("neovm-value", "VALUE"),
    ("neovm-edit", "EDIT"),
    ("neovm-text", "TEXT"),
    ("neovm-eval", "EVAL"),
    ("neovm-display", "DISPLAY"),
    ("neovm-system", "SYSTEM"),
];

/// `(workspace-relative file, kind, item identity)`.
type Row = (String, String, String);

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_dir_sorted(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
        entries.push(entry.map_err(|error| error.to_string())?.path());
    }
    entries.sort();
    Ok(entries)
}

fn walk_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in read_dir_sorted(path)? {
        if entry.is_dir() {
            walk_files(&entry, out)?;
        } else {
            out.push(entry);
        }
    }
    Ok(())
}

fn is_test_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_str() == Some("tests"))
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "tests.rs" || name.ends_with("_test.rs"))
}

#[derive(Clone, Copy, PartialEq)]
enum Truth {
    True,
    False,
    Unknown,
}

/// Evaluate a cfg predicate with `test = false` and `test-support = false`.
/// Only a predicate that is definitively false (a test-only arm) is excluded;
/// unknown predicates (unix/windows/...) keep the item in the production
/// surface, as does a predicate that is true without the test features.
fn cfg_truth(meta: &syn::Meta) -> Truth {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Truth::False,
        syn::Meta::NameValue(name_value) if name_value.path.is_ident("feature") => {
            if let syn::Expr::Lit(expr) = &name_value.value {
                if let syn::Lit::Str(name) = &expr.lit {
                    if name.value() == "test-support" {
                        return Truth::False;
                    }
                }
            }
            Truth::Unknown
        }
        syn::Meta::List(list) if list.path.is_ident("not") => {
            match list.parse_args::<syn::Meta>() {
                Ok(inner) => match cfg_truth(&inner) {
                    Truth::True => Truth::False,
                    Truth::False => Truth::True,
                    Truth::Unknown => Truth::Unknown,
                },
                Err(_) => Truth::Unknown,
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
                        match cfg_truth(meta) {
                            Truth::True if any => return Truth::True,
                            Truth::False if !any => return Truth::False,
                            Truth::Unknown => unknown = true,
                            _ => {}
                        }
                    }
                    if unknown {
                        Truth::Unknown
                    } else if any {
                        Truth::False
                    } else {
                        Truth::True
                    }
                }
                Err(_) => Truth::Unknown,
            }
        }
        _ => Truth::Unknown,
    }
}

fn test_gated(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .map(|meta| cfg_truth(&meta) == Truth::False)
                .unwrap_or(false)
    })
}

fn doc_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("doc") {
            return false;
        }
        let mut hidden = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("hidden") {
                hidden = true;
            }
            Ok(())
        });
        hidden
    })
}

fn is_plain(vis: &Visibility, attrs: &[Attribute]) -> bool {
    matches!(vis, Visibility::Public(_)) && !test_gated(attrs) && !doc_hidden(attrs)
}

fn path_attr(attrs: &[Attribute]) -> Option<PathBuf> {
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

/// The directory Rust resolves out-of-line `mod` declarations from, given the
/// file that is itself a module: `src/lib.rs` -> `src`; `src/x/mod.rs` ->
/// `src/x`; `src/x.rs` -> `src/x`.
fn module_dir_for(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs") | Some("main.rs") | Some("mod.rs") => parent.to_path_buf(),
        Some(name) => parent.join(name.strip_suffix(".rs").unwrap_or(name)),
        None => parent.to_path_buf(),
    }
}

fn module_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(format!("{name}.rs"));
    if candidate.is_file() {
        return Some(candidate);
    }
    let candidate = dir.join(name).join("mod.rs");
    if candidate.is_file() {
        return Some(candidate);
    }
    None
}

/// Mark every file reached from the crate root only through a test-gated
/// (`cfg(test)` / `cfg(feature = "test-support")`) module declaration. A file
/// reached by at least one ungated route is production. The per-file
/// `is_test_path` heuristic cannot see a test-gated module whose file has a
/// different name (for example `error/registry_support.rs`).
fn gated_files(src: &Path) -> Result<BTreeMap<PathBuf, bool>, String> {
    let mut gated = BTreeMap::new();
    let root = if src.join("lib.rs").is_file() {
        src.join("lib.rs")
    } else if src.join("main.rs").is_file() {
        src.join("main.rs")
    } else {
        return Ok(gated);
    };
    visit_source_file(&root, false, &mut gated)?;
    Ok(gated)
}

fn visit_source_file(
    file: &Path,
    gated: bool,
    out: &mut BTreeMap<PathBuf, bool>,
) -> Result<(), String> {
    match out.get(file).copied() {
        Some(previous) if previous == gated => return Ok(()),
        Some(false) => return Ok(()),
        Some(true) => {
            out.insert(file.to_path_buf(), false);
        }
        None => {
            out.insert(file.to_path_buf(), gated);
        }
    }
    if is_test_path(file) {
        return Ok(());
    }
    let source = read(file)?;
    let parsed =
        syn::parse_file(&source).map_err(|error| format!("{}: {error}", file.display()))?;
    visit_module_items(
        &parsed.items,
        gated,
        file.parent().unwrap_or_else(|| Path::new("")),
        &module_dir_for(file),
        true,
        out,
    )
}

fn visit_module_items(
    items: &[Item],
    gated: bool,
    file_dir: &Path,
    module_dir: &Path,
    top_level: bool,
    out: &mut BTreeMap<PathBuf, bool>,
) -> Result<(), String> {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let declaration_gated = gated || test_gated(&module.attrs);
        if let Some((_, inner)) = &module.content {
            let inner_dir = module_dir.join(module.ident.to_string());
            visit_module_items(inner, declaration_gated, file_dir, &inner_dir, false, out)?;
            continue;
        }
        let target = match path_attr(&module.attrs) {
            Some(path) => {
                let base = if top_level { file_dir } else { module_dir };
                Some(base.join(path))
            }
            None => module_file(module_dir, &module.ident.to_string()),
        };
        if let Some(target) = target {
            if target.is_file() {
                visit_source_file(&target, declaration_gated, out)?;
            }
        }
    }
    Ok(())
}

fn prefix(modules: &[String]) -> String {
    if modules.is_empty() {
        String::new()
    } else {
        format!("{}::", modules.join("::"))
    }
}

fn push_row(
    out: &mut Vec<(&'static str, String)>,
    modules: &[String],
    kind: &'static str,
    name: &str,
    vis: &Visibility,
    attrs: &[Attribute],
) {
    if !is_plain(vis, attrs) {
        return;
    }
    out.push((kind, format!("{}{kind} {name}", prefix(modules))));
}

fn push_field(
    out: &mut Vec<(&'static str, String)>,
    modules: &[String],
    owner: &str,
    field: &syn::Field,
) {
    if !is_plain(&field.vis, &field.attrs) {
        return;
    }
    let Some(name) = &field.ident else {
        return;
    };
    out.push(("field", format!("{}{owner} field {name}", prefix(modules))));
}

/// A plain `pub use` re-export is production surface too: it exposes the
/// source item under the re-export's name. Identity is (file, use-tree leaf,
/// rename), so `pub use a::Bar` and `pub use b::Bar as Baz` are two rows.
/// FX-1 (peer 05:45Z): one identity per glob statement. A glob re-export has
/// no leaf identity, so the row is the written module path: a NEW glob such as
/// `pub use neovm_value::x::*;` is visible as growth. Item identities behind
/// the glob still belong to their defining crate's scan.
fn push_use(out: &mut Vec<(&'static str, String)>, item: &syn::ItemUse) {
    if !is_plain(&item.vis, &item.attrs) {
        return;
    }
    fn expand(
        tree: &syn::UseTree,
        prefix: &mut Vec<String>,
        out: &mut Vec<(&'static str, String)>,
    ) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                expand(&path.tree, prefix, out);
                prefix.pop();
            }
            syn::UseTree::Name(name) => out.push(("use", format!("use {}", name.ident))),
            syn::UseTree::Rename(rename) => {
                out.push(("use", format!("use {} as {}", rename.ident, rename.rename)))
            }
            syn::UseTree::Glob(_) => {
                if !prefix.is_empty() {
                    out.push(("use", format!("use {}::*", prefix.join("::"))));
                }
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    expand(tree, prefix, out);
                }
            }
        }
    }
    expand(&item.tree, &mut Vec::new(), out);
}

fn impl_type_last(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(reference) => impl_type_last(&reference.elem),
        _ => None,
    }
}

fn walk_impl(
    item: &syn::ItemImpl,
    modules: &mut Vec<String>,
    out: &mut Vec<(&'static str, String)>,
) {
    if test_gated(&item.attrs) {
        return;
    }
    let Some(type_name) = impl_type_last(&item.self_ty) else {
        return;
    };
    for member in &item.items {
        match member {
            ImplItem::Fn(method) => push_row(
                out,
                modules,
                "fn",
                &format!("impl {type_name}::{}", method.sig.ident),
                &method.vis,
                &method.attrs,
            ),
            ImplItem::Const(constant) => push_row(
                out,
                modules,
                "const",
                &format!("impl {type_name}::{}", constant.ident),
                &constant.vis,
                &constant.attrs,
            ),
            ImplItem::Type(alias) => push_row(
                out,
                modules,
                "type",
                &format!("impl {type_name}::{}", alias.ident),
                &alias.vis,
                &alias.attrs,
            ),
            _ => {}
        }
    }
}

fn walk(items: &[Item], modules: &mut Vec<String>, out: &mut Vec<(&'static str, String)>) {
    for item in items {
        match item {
            Item::Mod(module) => {
                if test_gated(&module.attrs) {
                    continue;
                }
                push_row(
                    out,
                    modules,
                    "mod",
                    &module.ident.to_string(),
                    &module.vis,
                    &module.attrs,
                );
                if let Some((_, inner)) = &module.content {
                    modules.push(module.ident.to_string());
                    walk(inner, modules, out);
                    modules.pop();
                }
            }
            Item::Struct(item) => {
                // A cfg-gated struct's fields are gated with it: the test arm
                // of a cfg-split struct pair declares `pub` fields for
                // cross-crate test access, which is not production surface.
                if test_gated(&item.attrs) {
                    continue;
                }
                push_row(
                    out,
                    modules,
                    "struct",
                    &item.ident.to_string(),
                    &item.vis,
                    &item.attrs,
                );
                for field in item.fields.iter() {
                    push_field(out, modules, &item.ident.to_string(), field);
                }
            }
            Item::Enum(item) => push_row(
                out,
                modules,
                "enum",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Union(item) => {
                if test_gated(&item.attrs) {
                    continue;
                }
                push_row(
                    out,
                    modules,
                    "union",
                    &item.ident.to_string(),
                    &item.vis,
                    &item.attrs,
                );
                for field in &item.fields.named {
                    push_field(out, modules, &item.ident.to_string(), field);
                }
            }
            Item::Trait(item) => push_row(
                out,
                modules,
                "trait",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Type(item) => push_row(
                out,
                modules,
                "type",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Fn(item) => push_row(
                out,
                modules,
                "fn",
                &item.sig.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Const(item) => push_row(
                out,
                modules,
                "const",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Static(item) => push_row(
                out,
                modules,
                "static",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::ExternCrate(item) => push_row(
                out,
                modules,
                "extern crate",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            Item::Use(item) => push_use(out, item),
            Item::Impl(item) => walk_impl(item, modules, out),
            _ => {}
        }
    }
}

fn plain_pub_rows(workspace: &Path, crate_dir: &str) -> Result<BTreeSet<Row>, String> {
    let src = workspace.join("crates").join(crate_dir).join("src");
    let gated = gated_files(&src)?;
    let mut files = Vec::new();
    walk_files(&src, &mut files)?;
    let mut rows = BTreeSet::new();
    for path in files {
        if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
            continue;
        }
        if gated.get(&path).copied().unwrap_or(false) {
            continue;
        }
        let relative = path
            .strip_prefix(workspace)
            .map_err(|_| format!("{} is outside the workspace", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let source = read(&path)?;
        let parsed = syn::parse_file(&source)
            .map_err(|error| format!("B3_PLAIN_PUB_SOURCE {relative}: {error}"))?;
        let mut entries = Vec::new();
        walk(&parsed.items, &mut Vec::new(), &mut entries);
        for (kind, item) in entries {
            rows.insert((relative.clone(), kind.to_owned(), item));
        }
    }
    Ok(rows)
}

fn parse_baseline(path: &Path, text: &str) -> Result<BTreeSet<Row>, String> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(format!(
            "{}: exact three-column header required: {HEADER}",
            path.display()
        ));
    }
    let mut rows = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 3 || fields.iter().any(|field| field.trim().is_empty()) {
            return Err(format!(
                "{}: invalid plain-pub row at line {}",
                path.display(),
                index + 2
            ));
        }
        if !rows.insert((
            fields[0].to_owned(),
            fields[1].to_owned(),
            fields[2].to_owned(),
        )) {
            return Err(format!(
                "{}: duplicate plain-pub row at line {}",
                path.display(),
                index + 2
            ));
        }
    }
    Ok(rows)
}

fn check_surface(scanned: &BTreeSet<Row>, baseline: &BTreeSet<Row>) -> Result<(), String> {
    let missing: Vec<&Row> = scanned.difference(baseline).collect();
    if missing.is_empty() {
        return Ok(());
    }
    // Report every row: the count below is `missing.len()`, so a truncated
    // list hides rows the reviewer must see.  The verdict and the missing set
    // are unchanged; only the report is complete.
    let mut detail = String::new();
    for row in &missing {
        detail.push_str(&format!("\n  {}: {} {}", row.0, row.1, row.2));
    }
    Err(format!(
        "B3_PLAIN_PUB_GROWTH {} new plain-pub item(s) not in the reviewed baseline; \
         add the rows after review and a named ceiling bump when over the ceiling:{detail}",
        missing.len()
    ))
}

fn ceiling(text: &str) -> Result<usize, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("B3_PLAIN_PUB_RATCHET expected one nonnegative integer".into());
    }
    text.parse()
        .map_err(|_| "B3_PLAIN_PUB_RATCHET count overflow".into())
}

fn at_most(rows: usize, limit: usize) -> Result<(), String> {
    if rows <= limit {
        Ok(())
    } else {
        Err(format!(
            "B3_PLAIN_PUB_RATCHET {rows} baseline rows exceed ceiling {limit}"
        ))
    }
}

fn monotonic(
    previous: usize,
    next: usize,
    subject: &str,
    changed: &[String],
    ceiling_path: &str,
) -> Result<(), String> {
    if next <= previous {
        return Ok(());
    }
    if subject.starts_with(BUMP_SUBJECT)
        && !subject[BUMP_SUBJECT.len()..].trim().is_empty()
        && changed.len() == 1
        && changed[0] == ceiling_path
    {
        return Ok(());
    }
    Err(format!(
        "B3_PLAIN_PUB_RATCHET silent ceiling increase {previous} -> {next}; \
         requires a named ceiling-only bump commit"
    ))
}

fn git(workspace: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|error| format!("B3_PLAIN_PUB_RATCHET git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "B3_PLAIN_PUB_RATCHET git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("B3_PLAIN_PUB_RATCHET non-UTF8 history: {error}"))
}

fn ratchet(workspace: &Path, rows: usize, ceiling_path: &str) -> Result<(), String> {
    let current = ceiling(&read(&workspace.join(ceiling_path))?)?;
    at_most(rows, current)?;
    let log = git(
        workspace,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H",
            "HEAD",
            "--",
            ceiling_path,
        ],
    )?;
    let revisions: Vec<_> = log.lines().filter(|line| !line.is_empty()).collect();
    if revisions.is_empty() {
        return Err("B3_PLAIN_PUB_RATCHET ceiling must have a tracked history anchor".into());
    }
    let mut previous = None;
    for revision in revisions {
        let value = ceiling(&git(
            workspace,
            &["show", &format!("{revision}:{ceiling_path}")],
        )?)?;
        if let Some(previous) = previous {
            let subject = git(workspace, &["show", "-s", "--format=%s", revision])?;
            let files = git(
                workspace,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", revision],
            )?;
            let files: Vec<_> = files.lines().map(str::to_string).collect();
            monotonic(previous, value, subject.trim(), &files, ceiling_path)?;
        }
        previous = Some(value);
    }
    let previous = previous.ok_or("B3_PLAIN_PUB_RATCHET missing history anchor")?;
    if current > previous {
        return Err("B3_PLAIN_PUB_RATCHET uncommitted ceiling increase".into());
    }
    Ok(())
}

fn validate_crate(workspace: &Path, crate_dir: &str, suffix: &str) -> Result<(), String> {
    let baseline_rel = format!("{ARCHITECTURE_DIR}/{PREFIX}{suffix}.tsv");
    let baseline_path = workspace.join(&baseline_rel);
    let baseline = parse_baseline(&baseline_path, &read(&baseline_path)?)?;
    let ceiling_rel = format!("{ARCHITECTURE_DIR}/{PREFIX}{suffix}.ceiling");
    ratchet(workspace, baseline.len(), &ceiling_rel)?;
    let scanned = plain_pub_rows(workspace, crate_dir)?;
    check_surface(&scanned, &baseline)
}

/// Join every crate's validation result.  The verdict is unchanged (`Err` iff
/// any crate fails); the report carries every failing crate and every missing
/// row instead of stopping at the first crate.
fn aggregate_crate_errors<'a>(
    results: impl IntoIterator<Item = (&'a str, Result<(), String>)>,
) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();
    for (crate_dir, result) in results {
        if let Err(error) = result {
            errors.push(format!("{crate_dir}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn validate(workspace: &Path) -> Result<(), String> {
    aggregate_crate_errors(
        CRATES
            .iter()
            .map(|(crate_dir, suffix)| (*crate_dir, validate_crate(workspace, crate_dir, suffix))),
    )
}

#[test]
fn plain_pub_surface_is_frozen_at_the_reviewed_baseline() {
    let workspace = workspace_root();
    validate(&workspace).unwrap_or_else(|error| panic!("B3_PLAIN_PUB: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/neovm-value/src/tests")).expect("tests dir");
        std::fs::write(
            root.join("crates/neovm-value/src/lib.rs"),
            r#"
pub fn visible() {}
#[doc(hidden)]
pub fn hidden() {}
#[cfg(test)]
pub fn test_only() {}
#[cfg(any(test, feature = "test-support"))]
pub fn support_only() {}
#[cfg(not(feature = "test-support"))]
pub fn production() {}
pub struct S {
    pub field: u8,
    private: u8,
}
impl S {
    pub fn visible_method(&self) {}
    pub(crate) fn internal(&self) {}
}
"#,
        )
        .expect("lib");
        std::fs::write(
            root.join("crates/neovm-value/src/tests/helper.rs"),
            "pub fn test_helper() {}\n",
        )
        .expect("helpers");
        dir
    }

    #[test]
    fn plain_surface_excludes_hidden_and_test_arms_and_test_files() {
        let dir = sample_workspace();
        let rows = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        let items: Vec<String> = rows.iter().map(|row| row.2.clone()).collect();
        assert!(items.iter().any(|item| item == "fn visible"));
        assert!(items.iter().any(|item| item == "fn production"));
        assert!(items.iter().any(|item| item == "struct S"));
        assert!(items.iter().any(|item| item == "S field field"));
        assert!(items.iter().any(|item| item == "fn impl S::visible_method"));
        for forbidden in [
            "fn hidden",
            "fn test_only",
            "fn support_only",
            "fn test_helper",
            "fn impl S::internal",
        ] {
            assert!(
                !items.iter().any(|item| item == forbidden),
                "{forbidden} must not be production plain pub"
            );
        }
    }

    fn rows(items: &[(&str, &str, &str)]) -> BTreeSet<Row> {
        items
            .iter()
            .map(|(file, kind, item)| ((*file).to_owned(), (*kind).to_owned(), (*item).to_owned()))
            .collect()
    }

    #[test]
    fn planted_plain_pub_item_turns_the_surface_red() {
        let baseline = rows(&[("f.rs", "fn", "fn existing")]);
        let mut scanned = baseline.clone();
        assert_eq!(check_surface(&scanned, &baseline), Ok(()));
        scanned.insert(("f.rs".to_owned(), "fn".to_owned(), "fn planted".to_owned()));
        let error = check_surface(&scanned, &baseline).expect_err("planted plain pub must fail");
        assert!(error.contains("fn planted"), "{error}");
        eprintln!("MUTATION RED: {error}");
    }

    #[test]
    fn every_missing_row_of_every_crate_is_listed() {
        // More than the old take(8) inside ONE crate, plus a second failing
        // crate: the report must carry all 13 rows and both crate prefixes.
        let baseline_a = rows(&[("a.rs", "fn", "fn a00")]);
        let mut scanned_a = baseline_a.clone();
        for index in 1..=10 {
            scanned_a.insert((
                "a.rs".to_owned(),
                "fn".to_owned(),
                format!("fn a{index:02}"),
            ));
        }
        let baseline_b = rows(&[("b.rs", "fn", "fn b00")]);
        let mut scanned_b = baseline_b.clone();
        for index in 1..=3 {
            scanned_b.insert((
                "b.rs".to_owned(),
                "fn".to_owned(),
                format!("fn b{index:02}"),
            ));
        }
        let error = aggregate_crate_errors([
            ("crate_a", check_surface(&scanned_a, &baseline_a)),
            ("crate_b", check_surface(&scanned_b, &baseline_b)),
        ])
        .expect_err("planted plain-pub growth must fail");
        for index in 1..=10 {
            let row = format!("a.rs: fn fn a{index:02}");
            assert!(error.contains(&row), "{row} missing from: {error}");
        }
        for index in 1..=3 {
            let row = format!("b.rs: fn fn b{index:02}");
            assert!(error.contains(&row), "{row} missing from: {error}");
        }
        assert!(error.contains("crate_a:"), "{error}");
        assert!(error.contains("crate_b:"), "{error}");
        // The verdict and the checked set are unchanged: equal surfaces are
        // green and removals stay tolerated (see removals_are_tolerated).
        assert_eq!(
            aggregate_crate_errors([("crate_a", check_surface(&scanned_a, &scanned_a))]),
            Ok(())
        );
        eprintln!("PLAINPUB-TRUNC RED: {error}");
    }

    #[test]
    fn removals_are_tolerated() {
        let baseline = rows(&[("f.rs", "fn", "fn a"), ("f.rs", "fn", "fn b")]);
        let scanned = rows(&[("f.rs", "fn", "fn a")]);
        assert_eq!(check_surface(&scanned, &baseline), Ok(()));
    }

    #[test]
    fn ceilings_only_grow_in_a_named_ceiling_only_bump() {
        assert!(monotonic(10, 9, "", &[], "p").is_ok());
        assert!(
            monotonic(
                10,
                11,
                "PC11 PROMOTION-LEDGER ceiling bump: plain surface",
                &["p".to_owned()],
                "p"
            )
            .is_ok()
        );
        assert!(monotonic(10, 11, "unrelated", &["p".to_owned()], "p").is_err());
        assert!(
            monotonic(
                10,
                11,
                "PC11 PROMOTION-LEDGER ceiling bump: plain surface",
                &["p".to_owned(), "q".to_owned()],
                "p"
            )
            .is_err()
        );
        assert!(at_most(10, 10).is_ok());
        assert!(at_most(11, 10).is_err());
    }

    #[test]
    fn test_gated_module_declarations_gate_their_whole_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(src.join("hidden")).expect("hidden dir");
        std::fs::write(
            src.join("lib.rs"),
            r#"
pub mod production;
#[cfg(feature = "test-support")]
pub mod support;
#[cfg(any(test, feature = "test-support"))]
pub mod live_helpers;
#[cfg(test)]
#[path = "gated_by_path.rs"]
pub mod renamed;
#[cfg(test)]
pub mod hidden {
    pub mod deeper;
}
"#,
        )
        .expect("lib");
        std::fs::write(src.join("production.rs"), "pub fn visible() {}\n").expect("production");
        std::fs::write(src.join("support.rs"), "pub fn support_only() {}\n").expect("support");
        std::fs::write(src.join("live_helpers.rs"), "pub fn helper_only() {}\n").expect("helpers");
        std::fs::write(src.join("gated_by_path.rs"), "pub fn path_only() {}\n").expect("path");
        std::fs::write(src.join("hidden/deeper.rs"), "pub fn deep_only() {}\n").expect("deeper");
        let rows = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        let items: Vec<&str> = rows.iter().map(|row| row.2.as_str()).collect();
        assert!(items.contains(&"fn visible"));
        assert!(items.contains(&"mod production"));
        for forbidden in [
            "fn support_only",
            "fn helper_only",
            "fn path_only",
            "fn deep_only",
        ] {
            assert!(
                !items.contains(&forbidden),
                "{forbidden} must be test-gated"
            );
        }
        for forbidden in ["mod support", "mod live_helpers", "mod renamed"] {
            assert!(
                !items.contains(&forbidden),
                "{forbidden} is a test-only arm"
            );
        }
    }

    #[test]
    fn an_ungated_route_keeps_a_shared_file_in_the_surface() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src dir");
        std::fs::write(
            src.join("lib.rs"),
            "pub mod plain;\n#[cfg(test)]\npub mod plain;\n",
        )
        .expect("lib");
        std::fs::write(src.join("plain.rs"), "pub fn shared() {}\n").expect("plain");
        let rows = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        assert!(rows.iter().any(|row| row.2 == "fn shared"));
    }

    #[test]
    fn plain_pub_use_reexports_are_part_of_the_surface() {
        let dir = sample_workspace();
        std::fs::write(
            dir.path().join("crates/neovm-value/src/reexports.rs"),
            r#"
pub use crate::S;
pub use crate::S as PublicS;
pub use crate::*;
pub use crate::nested::{inner::*, Named};
#[doc(hidden)]
pub use crate::S as HiddenS;
#[doc(hidden)]
pub use crate::hidden::*;
#[cfg(test)]
pub use crate::S as TestS;
#[cfg(test)]
pub use crate::test_only::*;
"#,
        )
        .expect("reexports");
        let rows = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        let use_rows: Vec<(&str, &str)> = rows
            .iter()
            .filter(|row| row.1 == "use")
            .map(|row| (row.0.as_str(), row.2.as_str()))
            .collect();
        let has = |item: &str| {
            use_rows
                .iter()
                .any(|(file, row)| file.ends_with("reexports.rs") && *row == item)
        };
        assert!(has("use S"), "{use_rows:?}");
        assert!(has("use S as PublicS"), "{use_rows:?}");
        // FX-1: one row per glob statement, carrying the written module path.
        assert!(has("use crate::*"), "{use_rows:?}");
        assert!(has("use crate::nested::inner::*"), "{use_rows:?}");
        assert!(has("use Named"), "{use_rows:?}");
        for forbidden in [
            "use S as HiddenS",
            "use hidden::*",
            "use crate::hidden::*",
            "use S as TestS",
            "use crate::test_only::*",
        ] {
            assert!(
                !has(forbidden),
                "{forbidden} must not be production surface"
            );
        }
    }

    #[test]
    fn a_new_glob_statement_is_visible_growth() {
        // FX-1: a new glob re-export is a review point even though its leaves
        // stay with the defining crate's scan.
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src dir");
        std::fs::write(src.join("lib.rs"), "pub use crate::wide::*;\n").expect("lib");
        let scanned = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        let error = check_surface(&scanned, &BTreeSet::new()).expect_err("new glob");
        assert!(
            error.contains("B3_PLAIN_PUB_GROWTH") && error.contains("use crate::wide::*"),
            "{error}"
        );
        let reviewed: BTreeSet<Row> = scanned.clone();
        assert!(check_surface(&scanned, &reviewed).is_ok());
    }

    #[test]
    fn cfg_split_struct_arms_do_not_leak_test_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src dir");
        std::fs::write(
            src.join("lib.rs"),
            r#"
#[cfg(not(any(test, feature = "test-support")))]
pub struct Split {
    pub visible: u8,
    hidden: u8,
}
#[cfg(any(test, feature = "test-support"))]
pub struct Split {
    pub leaked_for_tests: u8,
}
#[cfg(test)]
pub struct TestOnly {
    pub leaked_entirely: u8,
}
"#,
        )
        .expect("lib");
        let rows = plain_pub_rows(dir.path(), "neovm-value").expect("surface");
        let items: Vec<&str> = rows.iter().map(|row| row.2.as_str()).collect();
        assert!(items.contains(&"Split field visible"));
        for forbidden in [
            "Split field leaked_for_tests",
            "TestOnly field leaked_entirely",
            "struct TestOnly",
        ] {
            assert!(!items.contains(&forbidden), "{forbidden} is test-gated");
        }
    }
}
