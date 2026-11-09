//! L-d test-support surface guard (lead #694 / #699).
//!
//! A hit is a `pub` item reachable in the test-support build (`test = false`,
//! `feature = "test-support"`) whose ancestor modules are public, and that is
//! either `#[doc(hidden)]` or gated by a cfg that mentions
//! `any(test, feature = "test-support")`. The mention is recognized inside
//! `all`, and not inside `not`. Pub fields of a hit struct or union are hits
//! (lead #768). Enum variant fields, modules, and `pub use` are not hits.
//!
//! An inherent method is a hit only when its owner type is publicly
//! reachable in that crate (lead #905, the AUTHORITY reachability test):
//! a public module chain, and a `pub` struct, enum, union, type, trait,
//! non-glob `pub use`, or an item widened by
//! `cfg_attr(..., visibility::make(pub))`. A private or `pub(crate)` owner
//! drops the method. Widened modules count for this name set only, so the
//! filter removes hits and does not add any.
//!
//! `TEST-SUPPORT-SURFACE-BASELINE.tsv` is a shrink-only identity list (lead
//! #782): crate, item path, kind, defining file. Git history may remove
//! identities and may not add one. A baseline line that is now rowed, or
//! that is no longer a hit, is RED. Every hit outside the list needs exactly
//! one VIS, AUTHORITY, or PC11 row. The `class` column is a name heuristic,
//! not the routing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::punctuated::Punctuated;
use syn::{Attribute, Fields, Item, Meta, Visibility};

use super::architecture_production_source_roots::workspace_root;

const CRATES: &[&str] = &[
    "neovm-value",
    "neovm-eval",
    "neovm-edit",
    "neovm-text",
    "neovm-system",
    "neovm-display",
];

#[derive(Clone, Copy, PartialEq)]
enum CfgTruth {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Hit {
    crate_name: String,
    file: String,
    path: String,
    kind: String,
    class: String,
}

struct Frame {
    segments: Vec<String>,
    public: bool,
    gated: bool,
}

fn scan_root(root: &Path) -> Vec<Hit> {
    let reachable = reachable_type_names(root);
    let mut hits = Vec::new();
    for crate_name in CRATES {
        let lib = root.join("crates").join(crate_name).join("src/lib.rs");
        if !lib.is_file() {
            panic!("missing {}", lib.display());
        }
        let frame = Frame {
            segments: Vec::new(),
            public: true,
            gated: false,
        };
        walk_file(root, crate_name, &lib, &frame, &mut hits);
    }
    hits.retain(|hit| {
        hit.kind != "method" || method_owner_reachable(&reachable, &hit.crate_name, &hit.path)
    });
    hits.sort();
    hits.dedup();
    hits
}

fn walk_file(root: &Path, crate_name: &str, file: &Path, frame: &Frame, hits: &mut Vec<Hit>) {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("skip {}: {error}", file.display());
            return;
        }
    };
    let parsed = match syn::parse_file(&text) {
        Ok(parsed) => parsed,
        Err(error) => panic!("parse {}: {error}", file.display()),
    };
    if cfg_inactive(&parsed.attrs) {
        return;
    }
    let mut frame = Frame {
        segments: frame.segments.clone(),
        public: frame.public,
        gated: frame.gated || mentions(&parsed.attrs),
    };
    walk_items(root, crate_name, file, &parsed.items, &mut frame, hits);
}

fn walk_items(
    root: &Path,
    crate_name: &str,
    file: &Path,
    items: &[Item],
    frame: &mut Frame,
    hits: &mut Vec<Hit>,
) {
    for item in items {
        let attrs = item_attrs(item);
        if cfg_inactive(attrs) {
            continue;
        }
        let saved = frame.gated;
        frame.gated = frame.gated || mentions(attrs);
        match item {
            Item::Mod(module) => {
                let child_public = frame.public && is_pub(&module.vis);
                let mut child = Frame {
                    segments: frame.segments.clone(),
                    public: child_public,
                    gated: frame.gated,
                };
                child.segments.push(module.ident.to_string());
                if let Some((_, inner)) = &module.content {
                    walk_items(root, crate_name, file, inner, &mut child, hits);
                } else if let Some(target) = module_file(file, module) {
                    walk_file(root, crate_name, &target, &child, hits);
                }
            }
            Item::Fn(func) if frame.public && is_pub(&func.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &func.sig.ident.to_string(),
                    "fn",
                    false,
                    hits,
                );
            }
            Item::Struct(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "struct",
                    false,
                    hits,
                );
                record_fields(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    &item.fields,
                    hits,
                );
            }
            Item::Enum(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "enum",
                    false,
                    hits,
                );
            }
            Item::Union(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "union",
                    false,
                    hits,
                );
                for field in &item.fields.named {
                    if !is_pub(&field.vis) {
                        continue;
                    }
                    let Some(name) = field.ident.as_ref() else {
                        continue;
                    };
                    let mut segments = frame.segments.clone();
                    segments.push(item.ident.to_string());
                    record(
                        root,
                        crate_name,
                        file,
                        &segments,
                        &name.to_string(),
                        "field",
                        false,
                        hits,
                    );
                }
            }
            Item::Trait(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "trait",
                    false,
                    hits,
                );
            }
            Item::Type(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "type",
                    false,
                    hits,
                );
            }
            Item::Const(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "const",
                    false,
                    hits,
                );
            }
            Item::Static(item) if frame.public && is_pub(&item.vis) && keep(frame, attrs) => {
                record(
                    root,
                    crate_name,
                    file,
                    &frame.segments,
                    &item.ident.to_string(),
                    "static",
                    false,
                    hits,
                );
            }
            Item::Impl(item) if frame.public && item.trait_.is_none() => {
                let Some(owner) = impl_owner(&item.self_ty) else {
                    frame.gated = saved;
                    continue;
                };
                for member in &item.items {
                    let syn::ImplItem::Fn(method) = member else {
                        continue;
                    };
                    if cfg_inactive(&method.attrs) || !is_pub(&method.vis) {
                        continue;
                    }
                    let gated = frame.gated || mentions(&method.attrs);
                    let hidden = doc_hidden(&method.attrs);
                    if !(hidden || gated) {
                        continue;
                    }
                    let mut_self = method
                        .sig
                        .receiver()
                        .is_some_and(|receiver| receiver.mutability.is_some());
                    let mut segments = frame.segments.clone();
                    segments.push(owner.clone());
                    record(
                        root,
                        crate_name,
                        file,
                        &segments,
                        &method.sig.ident.to_string(),
                        "method",
                        mut_self,
                        hits,
                    );
                }
            }
            _ => {}
        }
        frame.gated = saved;
    }
}

fn keep(frame: &Frame, attrs: &[Attribute]) -> bool {
    frame.gated || doc_hidden(attrs)
}

fn record_fields(
    root: &Path,
    crate_name: &str,
    file: &Path,
    segments: &[String],
    owner: &str,
    fields: &Fields,
    hits: &mut Vec<Hit>,
) {
    let fields = match fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(fields) => &fields.unnamed,
        Fields::Unit => return,
    };
    let mut path_segments = segments.to_vec();
    path_segments.push(owner.to_string());
    for (index, field) in fields.iter().enumerate() {
        if !is_pub(&field.vis) {
            continue;
        }
        let name = field
            .ident
            .as_ref()
            .map(|ident| ident.to_string())
            .unwrap_or_else(|| index.to_string());
        record(
            root,
            crate_name,
            file,
            &path_segments,
            &name,
            "field",
            false,
            hits,
        );
    }
}

fn record(
    root: &Path,
    crate_name: &str,
    file: &Path,
    segments: &[String],
    leaf: &str,
    kind: &str,
    mut_self: bool,
    hits: &mut Vec<Hit>,
) {
    let mut path = segments.join("::");
    if !path.is_empty() {
        path.push_str("::");
    }
    path.push_str(leaf);
    let relative = file
        .strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    hits.push(Hit {
        crate_name: crate_name.to_string(),
        file: relative,
        path,
        kind: kind.to_string(),
        class: class_of(kind, leaf, mut_self).to_string(),
    });
}

fn class_of(kind: &str, leaf: &str, mut_self: bool) -> &'static str {
    match kind {
        "struct" | "enum" | "union" | "type" | "const" | "static" | "trait" | "field" => "VIS",
        "method" | "fn" => {
            if mut_self || write_name(leaf) {
                "AUTHORITY"
            } else if observer_name(leaf) {
                "VIS"
            } else if kind == "fn" {
                "AUTHORITY"
            } else {
                "VIS"
            }
        }
        _ => "AUTHORITY",
    }
}

fn write_name(leaf: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "set_",
        "reset_",
        "with_",
        "install",
        "arm_",
        "prepare_",
        "suppress",
        "force_",
        "push_",
        "take_",
        "clear_",
        "mark_",
        "admit_",
        "exhaust_",
        "hold_",
        "begin_",
        "publish_",
        "plant_",
        "forge_",
        "override_",
        "enable",
        "disable",
    ];
    PREFIXES.iter().any(|prefix| leaf.starts_with(prefix))
}

fn observer_name(leaf: &str) -> bool {
    leaf.starts_with("get_")
        || leaf.starts_with("is_")
        || leaf.starts_with("has_")
        || leaf.starts_with("fresh_")
        || leaf.ends_with("_facts")
        || leaf.contains("census")
        || leaf.contains("snapshot")
}

fn impl_owner(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Mod(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Const(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn method_owner_reachable(
    reachable: &BTreeSet<(String, String)>,
    crate_name: &str,
    path: &str,
) -> bool {
    let Some(owner) = path.rsplit("::").nth(1) else {
        return false;
    };
    reachable.contains(&(crate_name.to_string(), owner.to_string()))
}

/// Type names another crate can name in the test-support build. A public
/// item inside a private module is not included. `visibility::make(pub)` on
/// a module counts here so a widened module's public type stays reachable;
/// the hit walk does not use that widening, so no hit is added.
fn reachable_type_names(root: &Path) -> BTreeSet<(String, String)> {
    let mut names = BTreeSet::new();
    for crate_name in CRATES {
        let lib = root.join("crates").join(crate_name).join("src/lib.rs");
        let frame = Frame {
            segments: Vec::new(),
            public: true,
            gated: false,
        };
        collect_type_file(&lib, crate_name, &frame, &mut names);
    }
    names
}

fn collect_type_file(
    file: &Path,
    crate_name: &str,
    frame: &Frame,
    names: &mut BTreeSet<(String, String)>,
) {
    let Ok(text) = fs::read_to_string(file) else {
        return;
    };
    let Ok(parsed) = syn::parse_file(&text) else {
        return;
    };
    if cfg_inactive(&parsed.attrs) {
        return;
    }
    collect_type_items(&parsed.items, file, crate_name, frame.public, names);
}

fn collect_type_items(
    items: &[Item],
    file: &Path,
    crate_name: &str,
    public: bool,
    names: &mut BTreeSet<(String, String)>,
) {
    for item in items {
        if cfg_inactive(item_attrs(item)) {
            continue;
        }
        match item {
            Item::Mod(module) => {
                let child_public =
                    public && (is_pub(&module.vis) || test_support_widened(&module.attrs));
                if let Some((_, inner)) = &module.content {
                    collect_type_items(inner, file, crate_name, child_public, names);
                } else if let Some(target) = module_file(file, module) {
                    let child = Frame {
                        segments: Vec::new(),
                        public: child_public,
                        gated: false,
                    };
                    collect_type_file(&target, crate_name, &child, names);
                }
            }
            Item::Struct(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                names.insert((crate_name.to_string(), item.ident.to_string()));
            }
            Item::Enum(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                names.insert((crate_name.to_string(), item.ident.to_string()));
            }
            Item::Union(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                names.insert((crate_name.to_string(), item.ident.to_string()));
            }
            Item::Type(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                names.insert((crate_name.to_string(), item.ident.to_string()));
            }
            Item::Trait(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                names.insert((crate_name.to_string(), item.ident.to_string()));
            }
            Item::Use(item) if public && owner_item_public(&item.vis, &item.attrs) => {
                let mut leaves = Vec::new();
                use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                for (path, alias, glob) in leaves {
                    if glob {
                        continue;
                    }
                    if let Some(name) = alias.or_else(|| path.last().cloned()) {
                        names.insert((crate_name.to_string(), name));
                    }
                }
            }
            _ => {}
        }
    }
}

fn owner_item_public(vis: &Visibility, attrs: &[Attribute]) -> bool {
    is_pub(vis) || test_support_widened(attrs)
}

/// Item-level `#[cfg_attr(..., visibility::make(pub))]` is public in the
/// test-support build. On a field the attribute is a silent no-op (lead
/// #830); this helper is applied only to items.
fn test_support_widened(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg_attr")
            && attr
                .meta
                .require_list()
                .map(|list| {
                    let text = list.tokens.to_string();
                    text.contains("test-support")
                        && text.contains("visibility")
                        && text.contains("make")
                        && text.contains("pub")
                })
                .unwrap_or(false)
    })
}

fn use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>, bool)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_leaves(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.clone();
            path.push(name.ident.to_string());
            out.push((path, None, false));
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            path.push(rename.ident.to_string());
            out.push((path, Some(rename.rename.to_string()), false));
        }
        syn::UseTree::Glob(_) => out.push((prefix.clone(), None, true)),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, prefix, out);
            }
        }
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn module_file(parent: &Path, module: &syn::ItemMod) -> Option<PathBuf> {
    let dir = parent.parent()?;
    if let Some(path) = path_attr(&module.attrs) {
        return Some(dir.join(path));
    }
    let name = module.ident.to_string();
    let nested = if parent
        .file_name()
        .is_some_and(|name| name == "lib.rs" || name == "mod.rs")
    {
        dir.to_path_buf()
    } else {
        parent.with_extension("")
    };
    let file = nested.join(format!("{name}.rs"));
    if file.is_file() {
        return Some(file);
    }
    let module_rs = nested.join(&name).join("mod.rs");
    if module_rs.is_file() {
        return Some(module_rs);
    }
    None
}

fn path_attr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        if let Ok(value) = attr.parse_args::<syn::LitStr>() {
            return Some(value.value());
        }
    }
    None
}

fn doc_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("doc")
            && attr
                .parse_args::<syn::Ident>()
                .ok()
                .is_some_and(|ident| ident == "hidden")
    })
}

fn mentions(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<Meta>()
                .ok()
                .is_some_and(|meta| mentions_meta(&meta))
    })
}

fn mentions_meta(meta: &Meta) -> bool {
    match meta {
        Meta::List(list) if list.path.is_ident("any") => {
            let Ok(metas) =
                list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            else {
                return false;
            };
            let has_test = metas
                .iter()
                .any(|meta| matches!(meta, Meta::Path(path) if path.is_ident("test")));
            let has_feature = metas.iter().any(is_test_support_feature);
            if has_test && has_feature {
                return true;
            }
            metas.iter().any(mentions_meta)
        }
        Meta::List(list) if list.path.is_ident("all") => {
            let Ok(metas) =
                list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            else {
                return false;
            };
            metas.iter().any(mentions_meta)
        }
        _ => false,
    }
}

fn is_test_support_feature(meta: &Meta) -> bool {
    let Meta::NameValue(value) = meta else {
        return false;
    };
    if !value.path.is_ident("feature") {
        return false;
    }
    let syn::Expr::Lit(expr) = &value.value else {
        return false;
    };
    let syn::Lit::Str(text) = &expr.lit else {
        return false;
    };
    text.value() == "test-support"
}

fn cfg_inactive(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<Meta>()
                .ok()
                .is_some_and(|meta| cfg_truth_test_support(&meta) == CfgTruth::False)
    })
}

fn cfg_truth_test_support(meta: &Meta) -> CfgTruth {
    cfg_truth_with(meta, false, true)
}

fn cfg_truth_with(meta: &Meta, test: bool, test_support: bool) -> CfgTruth {
    match meta {
        Meta::Path(path) if path.is_ident("test") => {
            if test {
                CfgTruth::True
            } else {
                CfgTruth::False
            }
        }
        Meta::NameValue(value) if value.path.is_ident("feature") => {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(name) = &expr.lit {
                    if name.value() == "test-support" {
                        return if test_support {
                            CfgTruth::True
                        } else {
                            CfgTruth::False
                        };
                    }
                }
            }
            CfgTruth::Unknown
        }
        Meta::List(list) if list.path.is_ident("not") => match list.parse_args::<Meta>() {
            Ok(inner) => match cfg_truth_with(&inner, test, test_support) {
                CfgTruth::True => CfgTruth::False,
                CfgTruth::False => CfgTruth::True,
                CfgTruth::Unknown => CfgTruth::Unknown,
            },
            Err(_) => CfgTruth::Unknown,
        },
        Meta::List(list) if list.path.is_ident("any") || list.path.is_ident("all") => {
            let any = list.path.is_ident("any");
            match list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated) {
                Ok(metas) => {
                    let mut unknown = false;
                    for meta in &metas {
                        match cfg_truth_with(meta, test, test_support) {
                            CfgTruth::True if any => return CfgTruth::True,
                            CfgTruth::False if !any => return CfgTruth::False,
                            CfgTruth::Unknown => unknown = true,
                            _ => {}
                        }
                    }
                    if unknown {
                        CfgTruth::Unknown
                    } else if any {
                        CfgTruth::False
                    } else {
                        CfgTruth::True
                    }
                }
                Err(_) => CfgTruth::Unknown,
            }
        }
        _ => CfgTruth::Unknown,
    }
}

struct Rows {
    vis: BTreeSet<(String, String)>,
    authority: Vec<(String, String, String, String)>,
    pc11: Vec<(String, String)>,
}

fn load_rows(root: &Path) -> Rows {
    let dir = root.join("crates/neovm-core/src/emacs_core/tests/architecture");
    let mut rows = Rows {
        vis: BTreeSet::new(),
        authority: Vec::new(),
        pc11: Vec::new(),
    };
    for entry in fs::read_dir(&dir).expect("architecture dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with("TEST-SUPPORT-VIS-") && name.ends_with(".tsv") {
            for line in fs::read_to_string(&path).unwrap().lines().skip(1) {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() < 2 {
                    continue;
                }
                rows.vis
                    .insert((fields[0].to_string(), fields[1].to_string()));
            }
        } else if name == "AUTHORITY-ITEMS.tsv" {
            for line in fs::read_to_string(&path).unwrap().lines().skip(1) {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() < 5 {
                    continue;
                }
                rows.authority.push((
                    fields[0].to_string(),
                    fields[1].to_string(),
                    fields[3].to_string(),
                    fields[4].to_string(),
                ));
            }
        } else if name.starts_with("PROMOTION-LEDGER") && name.ends_with(".tsv") {
            for line in fs::read_to_string(&path).unwrap().lines().skip(1) {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() < 5 {
                    continue;
                }
                let file = fields[3].split(':').next().unwrap_or("");
                let crate_name = file.split('/').nth(1).unwrap_or("");
                let path = if fields[4].is_empty() {
                    fields[0].to_string()
                } else {
                    format!("{}::{}", fields[4], fields[0])
                };
                rows.pc11.push((crate_name.to_string(), path));
            }
        }
    }
    rows
}

fn cover_count(rows: &Rows, hit: &Hit) -> usize {
    let mut count = 0usize;
    if rows
        .vis
        .contains(&(hit.crate_name.clone(), hit.path.clone()))
    {
        count += 1;
    }
    let snake = hit.crate_name.replace('-', "_");
    let fenced = format!("{snake}::{}", hit.path);
    for (crate_name, item, file, fence) in &rows.authority {
        if crate_name != &hit.crate_name {
            continue;
        }
        if fence == &fenced || item == &hit.path || (item == leaf(&hit.path) && file == &hit.file) {
            count += 1;
        }
    }
    for (crate_name, path) in &rows.pc11 {
        if crate_name == &hit.crate_name && path == &hit.path {
            count += 1;
        }
    }
    count
}

fn leaf(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

const BASELINE_HEADER: &str = "crate\tfile\titem\tkind\tclass\torigin";
const BASELINE_REL: &str =
    "crates/neovm-core/src/emacs_core/tests/architecture/TEST-SUPPORT-SURFACE-BASELINE.tsv";

/// Identity is crate, item path, kind, defining file (lead #782). Path alone
/// collides: `IntervalBPlusTree::records` is both a field and a method.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SurfaceId {
    crate_name: String,
    path: String,
    kind: String,
    file: String,
}

fn format_id(id: &SurfaceId) -> String {
    format!("{}\t{}\t{}\t{}", id.crate_name, id.path, id.kind, id.file)
}

fn parse_baseline(text: &str) -> Result<BTreeSet<SurfaceId>, String> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header != BASELINE_HEADER {
        return Err(format!("RED baseline header {header:?}"));
    }
    let mut ids = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 || fields.iter().any(|field| field.is_empty()) {
            return Err(format!(
                "RED baseline line {} is not a six-column identity",
                index + 2
            ));
        }
        let id = SurfaceId {
            crate_name: fields[0].to_string(),
            path: fields[2].to_string(),
            kind: fields[3].to_string(),
            file: fields[1].to_string(),
        };
        if !ids.insert(id.clone()) {
            return Err(format!(
                "RED duplicate baseline identity {}",
                format_id(&id)
            ));
        }
    }
    Ok(ids)
}

fn added_identities(previous: &BTreeSet<SurfaceId>, next: &BTreeSet<SurfaceId>) -> Vec<SurfaceId> {
    next.difference(previous).cloned().collect()
}

fn hit_id(hit: &Hit) -> SurfaceId {
    SurfaceId {
        crate_name: hit.crate_name.clone(),
        path: hit.path.clone(),
        kind: hit.kind.clone(),
        file: hit.file.clone(),
    }
}

fn stale_baseline(hits: &[Hit], rows: &Rows, base: &BTreeSet<SurfaceId>) -> Vec<String> {
    let mut by_id = BTreeMap::new();
    for hit in hits {
        by_id.insert(hit_id(hit), hit);
    }
    let mut stale = Vec::new();
    for id in base {
        match by_id.get(id) {
            None => stale.push(format!("RED stale missing {}", format_id(id))),
            Some(hit) => {
                let covered = cover_count(rows, hit);
                if covered != 0 {
                    stale.push(format!("RED stale rowed {covered} {}", format_id(id)));
                }
            }
        }
    }
    stale
}

fn git_text(workspace: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|error| format!("RED baseline git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "RED baseline git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("RED baseline non-UTF8 history: {error}"))
}

/// First-parent history of the baseline, same walk as the PC11 ceiling
/// ratchet. The first tracked revision is the anchor. Every later revision,
/// and the working tree, may only remove identities.
fn baseline_growth(workspace: &Path) -> Result<(), String> {
    let log = git_text(
        workspace,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H",
            "HEAD",
            "--",
            BASELINE_REL,
        ],
    )?;
    let revisions: Vec<&str> = log.lines().filter(|line| !line.is_empty()).collect();
    if revisions.is_empty() {
        return Err("RED baseline has no tracked history anchor".into());
    }
    let mut previous = None;
    for revision in &revisions {
        let text = git_text(workspace, &["show", &format!("{revision}:{BASELINE_REL}")])?;
        let ids = parse_baseline(&text)?;
        if let Some(earlier) = &previous {
            let added = added_identities(earlier, &ids);
            if !added.is_empty() {
                return Err(format!(
                    "RED baseline added {} identities at {revision}; first {}",
                    added.len(),
                    format_id(&added[0])
                ));
            }
        }
        previous = Some(ids);
    }
    let head = previous.ok_or("RED baseline missing history anchor")?;
    let current = parse_baseline(
        &fs::read_to_string(workspace.join(BASELINE_REL))
            .map_err(|error| format!("RED baseline read: {error}"))?,
    )?;
    let added = added_identities(&head, &current);
    if !added.is_empty() {
        return Err(format!(
            "RED uncommitted baseline addition {}; first {}",
            added.len(),
            format_id(&added[0])
        ));
    }
    Ok(())
}

fn load_baseline(workspace: &Path) -> BTreeSet<SurfaceId> {
    parse_baseline(&fs::read_to_string(workspace.join(BASELINE_REL)).expect("read baseline"))
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn test_support_surface_branch_items_have_one_row() {
    let workspace = workspace_root();
    let hits = scan_root(&workspace);
    let base = load_baseline(&workspace);
    let rows = load_rows(&workspace);
    let stale = stale_baseline(&hits, &rows, &base);
    assert!(
        stale.is_empty(),
        "stale baseline lines:\n{}",
        stale.join("\n")
    );
    let mut unrowed = Vec::new();
    let mut saw_branch_item = false;
    for hit in &hits {
        if hit.path == "symbol::parallel::test_facts::FreshRegistryFacts" {
            saw_branch_item = true;
        }
        if base.contains(&hit_id(hit)) {
            continue;
        }
        let covered = cover_count(&rows, hit);
        if covered != 1 {
            unrowed.push(format!(
                "{covered}\t{}\t{}\t{}\t{}",
                hit.crate_name, hit.path, hit.kind, hit.file
            ));
        }
    }
    assert!(saw_branch_item, "branch test_facts surface was not scanned");
    for private_owner in [
        "buffer::buffer::LiveBuffers::get",
        "buffer::buffer::LocalVariableBindings::value",
        "host_effect::wake::NativeEndpoint::new",
        "host_effect::wake::NativeHealthCell::load",
        "host_effect::wake::native_wait::BlockingNativeWait::execute",
        "host_effect::wake::ControlPublication::epoch",
        "error::registry_support::InFlightRegistryObserver::same_registry",
        "tagged::gc::mutator_registration::symbol_authority::GenericProofEra::admit_plain_symbol",
    ] {
        assert!(
            !hits.iter().any(|hit| hit.path == private_owner),
            "private-owner method stayed on the surface: {private_owner}"
        );
    }
    assert!(
        unrowed.is_empty(),
        "unrowed branch hits:\n{}",
        unrowed.join("\n")
    );
}

#[test]
fn test_support_surface_baseline_only_shrinks() {
    let workspace = workspace_root();
    baseline_growth(&workspace).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn test_support_surface_replaced_identity_is_red() {
    let before = parse_baseline(
        "crate\tfile\titem\tkind\tclass\torigin\nneovm-value\ta.rs\told_item\tfn\tVIS\td01843729a\n",
    )
    .expect("before");
    let replaced = parse_baseline(
        "crate\tfile\titem\tkind\tclass\torigin\nneovm-value\ta.rs\tnew_item\tfn\tVIS\td01843729a\n",
    )
    .expect("replaced");
    let added = added_identities(&before, &replaced);
    assert_eq!(
        added.len(),
        1,
        "a same-count replacement is an added identity"
    );
    assert_eq!(added[0].path, "new_item");
    let shrunk = parse_baseline("crate\tfile\titem\tkind\tclass\torigin\n").expect("shrunk");
    assert!(added_identities(&before, &shrunk).is_empty());
}

#[test]
fn test_support_surface_stale_rowed_or_missing_is_red() {
    let live = Hit {
        crate_name: "neovm-value".to_string(),
        file: "a.rs".to_string(),
        path: "live_item".to_string(),
        kind: "fn".to_string(),
        class: "VIS".to_string(),
    };
    let rowed = Hit {
        crate_name: "neovm-value".to_string(),
        file: "a.rs".to_string(),
        path: "rowed_item".to_string(),
        kind: "fn".to_string(),
        class: "VIS".to_string(),
    };
    let missing = SurfaceId {
        crate_name: "neovm-value".to_string(),
        path: "missing_item".to_string(),
        kind: "fn".to_string(),
        file: "a.rs".to_string(),
    };
    let mut rows = Rows {
        vis: BTreeSet::new(),
        authority: Vec::new(),
        pc11: Vec::new(),
    };
    rows.vis
        .insert(("neovm-value".to_string(), "rowed_item".to_string()));
    let mut base = BTreeSet::new();
    base.insert(hit_id(&live));
    base.insert(hit_id(&rowed));
    base.insert(missing);
    let stale = stale_baseline(&[live, rowed], &rows, &base);
    let text = stale.join("\n");
    assert!(text.contains("stale missing"), "{text}");
    assert!(text.contains("missing_item"), "{text}");
    assert!(text.contains("stale rowed 1"), "{text}");
    assert!(text.contains("rowed_item"), "{text}");
    assert!(!text.contains("live_item"), "{text}");
}

#[test]
fn test_support_surface_planted_unrowed_fn_is_red() {
    let workspace = workspace_root();
    let rows = load_rows(&workspace);
    let base = load_baseline(&workspace);
    let planted = Hit {
        crate_name: "neovm-value".to_string(),
        file: "crates/neovm-value/src/symbol/parallel/test_facts.rs".to_string(),
        path: "symbol::parallel::test_facts::planted_unrowed_surface_fn".to_string(),
        kind: "fn".to_string(),
        class: "AUTHORITY".to_string(),
    };
    assert!(
        !base.contains(&hit_id(&planted)),
        "the plant must not be a baseline hit"
    );
    assert_eq!(
        cover_count(&rows, &planted),
        0,
        "RED: planted_unrowed_surface_fn has a ledger row"
    );
}
