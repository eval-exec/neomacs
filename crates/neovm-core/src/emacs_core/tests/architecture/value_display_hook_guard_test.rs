//! Architecture guard: the value Display hook installs only at its two seams.
//!
//! `Display for TaggedValue` lives in the value layer and delegates through a
//! typed table the upper printer installs. The unsafe installer's contract
//! allows exactly one upper construction site (the printer's wrapper, which
//! supplies the algorithm), and the wrapper may run only at the two one-time
//! install seams: `Context::new_inner` and `Context::from_dump`. Reinstalling
//! the same table is idempotent; a different table is a typed refusal.
//!
//! This guard keeps that call graph exact in production code. The seal's
//! compile half lives in the thin contract target
//! (`thread_safety_ui/value_display_hook_safe_call.rs` and
//! `..._forged.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

struct Installer {
    name: &'static str,
    /// The function that defines the installer; a call from within it (for
    /// example a recursive helper) is the definition itself, not a site.
    definition: (&'static str, &'static str),
    /// Exact production call sites: `(file relative to src/, enclosing fn)`.
    callers: &'static [(&'static str, &'static str)],
}

const INSTALLERS: &[Installer] = &[
    Installer {
        name: "install_eval_error_formatter",
        definition: ("error/format_hook.rs", "install_eval_error_formatter"),
        callers: &[(
            "emacs_core/lisp/print/error_format.rs",
            "install_eval_error_format_hook",
        )],
    },
    Installer {
        name: "install_eval_error_format_hook",
        definition: (
            "emacs_core/lisp/print/error_format.rs",
            "install_eval_error_format_hook",
        ),
        callers: &[
            ("emacs_core/runtime/eval/construct.rs", "new_inner"),
            ("emacs_core/runtime/eval/pdump_reconstruct.rs", "from_dump"),
        ],
    },
    Installer {
        name: "install_value_display_formatter",
        definition: (
            "emacs_core/runtime/value/mod.rs",
            "install_value_display_formatter",
        ),
        callers: &[(
            "emacs_core/lisp/print/mod.rs",
            "install_value_display_format_hook",
        )],
    },
    Installer {
        name: "install_value_display_format_hook",
        definition: (
            "emacs_core/lisp/print/mod.rs",
            "install_value_display_format_hook",
        ),
        callers: &[
            ("emacs_core/runtime/eval/construct.rs", "new_inner"),
            ("emacs_core/runtime/eval/pdump_reconstruct.rs", "from_dump"),
        ],
    },
];

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

#[derive(Debug)]
struct CallSite {
    line: usize,
    installer: String,
    enclosing: Option<String>,
}

#[derive(Default)]
struct InstallerCalls {
    hits: Vec<CallSite>,
    /// Innermost enclosing function item, for the call-site authority check.
    functions: Vec<String>,
}

impl<'ast> Visit<'ast> for InstallerCalls {
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

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        self.functions.push(function.sig.ident.to_string());
        visit::visit_item_fn(self, function);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.functions.push(function.sig.ident.to_string());
        visit::visit_impl_item_fn(self, function);
        self.functions.pop();
    }

    fn visit_trait_item_fn(&mut self, function: &'ast syn::TraitItemFn) {
        self.functions.push(function.sig.ident.to_string());
        visit::visit_trait_item_fn(self, function);
        self.functions.pop();
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func
            && let Some(segment) = path.path.segments.last()
            && let Some(installer) = INSTALLERS
                .iter()
                .find(|installer| segment.ident == installer.name)
            && !self.inside_installer_definition(installer.name)
        {
            use syn::spanned::Spanned;
            self.hits.push(CallSite {
                line: call.span().start().line,
                installer: installer.name.to_owned(),
                enclosing: self.functions.last().cloned(),
            });
        }
        visit::visit_expr_call(self, call);
    }
}

impl InstallerCalls {
    fn inside_installer_definition(&self, installer: &str) -> bool {
        self.functions
            .last()
            .is_some_and(|function| function.as_str() == installer)
    }
}

fn installer_calls(path: &Path) -> Vec<CallSite> {
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
    let mut scan = InstallerCalls::default();
    scan.visit_file(&syntax);
    scan.hits
}

#[test]
fn the_value_display_hook_installs_only_at_its_two_seams() {
    let mut actual: BTreeMap<&str, Vec<(String, Option<String>, usize)>> = BTreeMap::new();
    for file in production_source_files() {
        if file.relative.split('/').any(|part| part == "tests") {
            continue;
        }
        let hits = installer_calls(&file.path);
        if hits.is_empty() {
            continue;
        }
        let relative = file.relative;
        for hit in hits {
            let installer = INSTALLERS
                .iter()
                .find(|installer| installer.name == hit.installer)
                .expect("scanner only records known installers");
            // A call from the installer's own definition is the definition
            // itself (there is none today), not a site.
            if relative == installer.definition.0
                && hit.enclosing.as_deref() == Some(installer.definition.1)
            {
                continue;
            }
            actual.entry(installer.name).or_default().push((
                relative.clone(),
                hit.enclosing,
                hit.line,
            ));
        }
    }
    for installer in INSTALLERS {
        let mut got: Vec<(String, Option<String>)> = actual
            .remove(installer.name)
            .unwrap_or_default()
            .into_iter()
            .map(|(file, enclosing, _line)| (file, enclosing))
            .collect();
        let mut expected: Vec<(String, Option<String>)> = installer
            .callers
            .iter()
            .map(|(file, function)| ((*file).to_owned(), Some((*function).to_owned())))
            .collect();
        got.sort();
        expected.sort();
        assert_eq!(
            got, expected,
            "{} must be called exactly at its signed sites (no more, no fewer)",
            installer.name
        );
    }
    assert!(
        actual.is_empty(),
        "unexpected installer names were recorded: {:?}",
        actual.keys().collect::<Vec<_>>()
    );
}

#[test]
fn the_scan_flags_a_planted_installer_call() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("planted.rs");
    std::fs::write(&path, "fn f() { install_value_display_format_hook(); }\n")
        .expect("write fixture");
    let hits = installer_calls(&path);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].installer, "install_value_display_format_hook");
    assert_eq!(hits[0].enclosing.as_deref(), Some("f"));
}

#[test]
fn the_scan_records_the_enclosing_impl_method() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("impl_method.rs");
    std::fs::write(
        &path,
        "impl T { fn allowed(&self) { install_value_display_formatter(t); } }\n",
    )
    .expect("write fixture");
    let hits = installer_calls(&path);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].enclosing.as_deref(), Some("allowed"));
}

#[test]
fn a_test_only_call_is_not_a_production_site() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("test_call.rs");
    std::fs::write(
        &path,
        "#[cfg(test)]\nmod tests { #[test] fn t() { install_value_display_formatter(t); } }\n",
    )
    .expect("write fixture");
    assert!(installer_calls(&path).is_empty());
}
