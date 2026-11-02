//! Architecture guard: the tagged lower layer never names the compiler.
//!
//! The bytecode storage that moved into `tagged::bytecode` carries only storage
//! vocabulary. `CompiledLeaf` and every `jit::compile` type stay upper: the
//! heap/GC and value layers must not depend on the compiler, and the storage
//! move must not reintroduce a tagged -> compile reference. Test-only items
//! are skipped.

use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};

fn cfg_is_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) => {
            list.path.is_ident("all")
                && list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .is_ok_and(|predicates| predicates.iter().any(cfg_is_test))
        }
        syn::Meta::NameValue(_) => false,
    }
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|meta| cfg_is_test(&meta)))
    })
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn impl_item_attrs(item: &syn::ImplItem) -> &[syn::Attribute] {
    match item {
        syn::ImplItem::Const(item) => &item.attrs,
        syn::ImplItem::Fn(item) => &item.attrs,
        syn::ImplItem::Type(item) => &item.attrs,
        syn::ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

fn trait_item_attrs(item: &syn::TraitItem) -> &[syn::Attribute] {
    match item {
        syn::TraitItem::Const(item) => &item.attrs,
        syn::TraitItem::Fn(item) => &item.attrs,
        syn::TraitItem::Type(item) => &item.attrs,
        syn::TraitItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

#[derive(Default)]
struct CompilerRefs {
    hits: Vec<(usize, String)>,
}

impl<'ast> Visit<'ast> for CompilerRefs {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if !test_only(item_attrs(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if !test_only(impl_item_attrs(item)) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        if !test_only(trait_item_attrs(item)) {
            visit::visit_trait_item(self, item);
        }
    }

    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        if ident == "CompiledLeaf" {
            self.hits
                .push((ident.span().start().line, "CompiledLeaf".to_owned()));
        }
        visit::visit_ident(self, ident);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        for window in segments.windows(2) {
            if window == ["jit", "compile"] {
                use syn::spanned::Spanned;
                self.hits
                    .push((path.span().start().line, "jit::compile".to_owned()));
                break;
            }
        }
        visit::visit_path(self, path);
    }
}

fn compiler_refs(path: &Path) -> Vec<(usize, String)> {
    let source = std::fs::read_to_string(path).expect("read source file");
    let syntax = syn::parse_file(&source).expect("parse source file");
    if syntax.attrs.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| cfg_is_test(&meta))
    }) {
        return Vec::new();
    }
    let mut scan = CompilerRefs::default();
    scan.visit_file(&syntax);
    scan.hits
}

/// Every production `*.rs` under `root`, excluding `tests/` directories.
fn sources(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("tests") {
                continue;
            }
            sources(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn tagged_sources_name_no_compiler_type() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tagged");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let mut offenders = Vec::new();
    for path in files {
        let hits = compiler_refs(&path);
        if hits.is_empty() {
            continue;
        }
        let relative = path
            .strip_prefix(&root)
            .expect("source lives under tagged")
            .to_string_lossy()
            .replace('\\', "/");
        for (line, item) in hits {
            offenders.push(format!("tagged/{relative}:{line}: {item}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "tagged sources name compiler types: {offenders:?}"
    );
}

#[test]
fn the_scan_flags_a_planted_compiler_reference() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("planted.rs");
    std::fs::write(
        &path,
        "fn f(leaf: *const CompiledLeaf) { let _ = crate::emacs_core::jit::compile::X; }\n",
    )
    .expect("write fixture");
    let hits = compiler_refs(&path);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!(hits.iter().any(|(_, item)| item == "CompiledLeaf"));
    assert!(hits.iter().any(|(_, item)| item == "jit::compile"));
}

#[test]
fn the_scan_skips_test_only_references() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("test_only.rs");
    std::fs::write(
        &path,
        "#[test]\nfn t() { let _ = crate::emacs_core::jit::compile::X; let _: *const CompiledLeaf; }\n",
    )
    .expect("write fixture");
    assert!(compiler_refs(&path).is_empty());
}
