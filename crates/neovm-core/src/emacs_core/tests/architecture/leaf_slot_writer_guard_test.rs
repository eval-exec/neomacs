//! Architecture guard: every MINT writer has one production call site.
//!
//! T1 privacy ledger A1 (`set_leaf_slot_word`) and A2 (`set_compiled_id_word`)
//! install an upper-vouched word into the lowered runtime data: the first a
//! live `CompiledLeaf` address under the current leaf-slot epoch, the second a
//! fresh process-unique compiled-cache identity for the state's single source.
//! Both writers are `unsafe`: the lower layer cannot validate the word, so the
//! calling function is the authority boundary.
//!
//! Production code may call each writer only from its one named upper function
//! (and from its own definition, which contains no call). Test-only items are
//! skipped (tests arm synthetic leaves and ids).

use std::path::Path;

use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

struct Writer {
    name: &'static str,
    definition: (&'static str, &'static str),
    callers: &'static [(&'static str, &'static str)],
}

const WRITERS: &[Writer] = &[
    Writer {
        name: "set_leaf_slot_word",
        definition: (
            "emacs_core/runtime/value/jit_runtime/mod.rs",
            "set_leaf_slot_word",
        ),
        callers: &[("emacs_core/runtime/jit/cache.rs", "arm_leaf_slot")],
    },
    Writer {
        name: "set_compiled_id_word",
        definition: (
            "emacs_core/runtime/value/jit_runtime/mod.rs",
            "set_compiled_id_word",
        ),
        callers: &[("emacs_core/runtime/jit/mod.rs", "compiled_id_or_assign")],
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
    writer: String,
    enclosing: Option<String>,
}

#[derive(Default)]
struct WriterCalls {
    hits: Vec<CallSite>,
    /// Innermost enclosing function item, for the call-site authority check.
    functions: Vec<String>,
}

impl<'ast> Visit<'ast> for WriterCalls {
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
            && let Some(writer) = WRITERS.iter().find(|writer| segment.ident == writer.name)
            && !self.inside_writer_definition(writer.name)
        {
            use syn::spanned::Spanned;
            self.hits.push(CallSite {
                line: call.span().start().line,
                writer: writer.name.to_owned(),
                enclosing: self.functions.last().cloned(),
            });
        }
        visit::visit_expr_call(self, call);
    }
}

impl WriterCalls {
    fn inside_writer_definition(&self, writer: &str) -> bool {
        self.functions
            .last()
            .is_some_and(|function| function.as_str() == writer)
    }
}

fn writer_calls(path: &Path) -> Vec<CallSite> {
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
    let mut scan = WriterCalls::default();
    scan.visit_file(&syntax);
    scan.hits
}

fn allowed_call(writer: &Writer, relative: &str, enclosing: Option<&str>) -> bool {
    writer
        .callers
        .iter()
        .any(|(file, function)| relative == *file && enclosing == Some(*function))
        || (relative == writer.definition.0 && enclosing == Some(writer.definition.1))
}

#[test]
fn each_mint_writer_has_one_production_call_site() {
    let mut offenders = Vec::new();
    for file in production_source_files() {
        // The pre-split walk skipped `tests/` directories (not `test_*` files).
        if file.relative.split('/').any(|part| part == "tests") {
            continue;
        }
        let hits = writer_calls(&file.path);
        if hits.is_empty() {
            continue;
        }
        let relative = file.relative;
        for hit in hits {
            let writer = WRITERS
                .iter()
                .find(|writer| writer.name == hit.writer)
                .expect("scanner only records known writers");
            if !allowed_call(writer, &relative, hit.enclosing.as_deref()) {
                offenders.push(format!(
                    "{relative}:{}: {} called from {}",
                    hit.line,
                    hit.writer,
                    hit.enclosing.as_deref().unwrap_or("<top level>"),
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "MINT writers may be called only from their named authority site; found {offenders:?}"
    );
}

#[test]
fn the_scan_flags_a_planted_writer_call() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("planted.rs");
    std::fs::write(
        &path,
        "fn f(rt: &RuntimeState) { set_leaf_slot_word(rt, 0, 0); set_compiled_id_word(rt, 1); }\n",
    )
    .expect("write fixture");
    let hits = writer_calls(&path);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[1].line, 1);
    assert_eq!(hits[0].writer, "set_leaf_slot_word");
    assert_eq!(hits[1].writer, "set_compiled_id_word");
    assert_eq!(hits[0].enclosing.as_deref(), Some("f"));
    assert_eq!(hits[1].enclosing.as_deref(), Some("f"));
}

#[test]
fn the_scan_records_the_enclosing_impl_method() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("impl_method.rs");
    std::fs::write(
        &path,
        "impl T { fn allowed(&self, rt: &RuntimeState) { set_compiled_id_word(rt, 1); } }\n",
    )
    .expect("write fixture");
    let hits = writer_calls(&path);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].enclosing.as_deref(), Some("allowed"));
}

#[test]
fn a_call_from_the_wrong_function_is_not_allowed() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("wrong_function.rs");
    std::fs::write(
        &path,
        "fn arm_leaf_slot(rt: &RuntimeState) { set_compiled_id_word(rt, 1); }\n",
    )
    .expect("write fixture");
    let hits = writer_calls(&path);
    assert_eq!(hits.len(), 1, "{hits:?}");
    let writer = WRITERS
        .iter()
        .find(|writer| writer.name == "set_compiled_id_word")
        .expect("writer table entry");
    assert!(!allowed_call(
        writer,
        "emacs_core/runtime/jit/cache.rs",
        hits[0].enclosing.as_deref(),
    ));
    assert!(!allowed_call(
        writer,
        "emacs_core/runtime/jit/mod.rs",
        hits[0].enclosing.as_deref(),
    ));
    assert!(allowed_call(
        writer,
        "emacs_core/runtime/jit/mod.rs",
        Some("compiled_id_or_assign"),
    ));
}

#[test]
fn the_scan_skips_test_only_calls() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let path = directory.path().join("test_only.rs");
    std::fs::write(
        &path,
        "#[test]\nfn f(rt: &RuntimeState) { set_leaf_slot_word(rt, 0, 0); set_compiled_id_word(rt, 1); }\n",
    )
    .expect("write fixture");
    assert!(writer_calls(&path).is_empty());
}
