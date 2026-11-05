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
//! production `.rs` file, including named struct/union fields and inherent
//! impl items. `#[doc(hidden)]` items and items gated by
//! `cfg(test)`/`cfg(feature = "test-support")` are excluded (other classes).
//! Test files (`.../tests/...`, `tests.rs`, `*_test.rs`) are excluded.
//! `pub use` re-exports and macro-generated items are not scanned; the API
//! projector covers those separately. Duplicate identical identities in one
//! file collapse into one row; reordering or duplication is not growth.
use std::collections::BTreeSet;
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
            Item::Impl(item) => walk_impl(item, modules, out),
            _ => {}
        }
    }
}

fn plain_pub_rows(workspace: &Path, crate_dir: &str) -> Result<BTreeSet<Row>, String> {
    let src = workspace.join("crates").join(crate_dir).join("src");
    let mut files = Vec::new();
    walk_files(&src, &mut files)?;
    let mut rows = BTreeSet::new();
    for path in files {
        if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
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
    let mut detail = String::new();
    for row in missing.iter().take(8) {
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

fn validate(workspace: &Path) -> Result<(), String> {
    for (crate_dir, suffix) in CRATES {
        let baseline_rel = format!("{ARCHITECTURE_DIR}/{PREFIX}{suffix}.tsv");
        let baseline_path = workspace.join(&baseline_rel);
        let baseline = parse_baseline(&baseline_path, &read(&baseline_path)?)?;
        let ceiling_rel = format!("{ARCHITECTURE_DIR}/{PREFIX}{suffix}.ceiling");
        ratchet(workspace, baseline.len(), &ceiling_rel)?;
        let scanned = plain_pub_rows(workspace, crate_dir)?;
        check_surface(&scanned, &baseline).map_err(|error| format!("{crate_dir}: {error}"))?;
    }
    Ok(())
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
}
