//! Architecture guard: the obarray writer capability cannot hand out a raw
//! `LispSymbol` addressed by name.
//!
//! The externally reachable obarray writer surface is `ObarrayWriteAccess<'_>`
//! (`Context::obarray_mut`) and `ObarrayViewMut<'_>`
//! (`Context::obarray_view_mut`). Both proxy the confined `ObarrayView`
//! through the explicit `obarray_method_proxies!` table. A method shaped
//! `fn f(&mut self, name: &str) -> &mut LispSymbol` would bypass the
//! Value/canonical-id domain: callers could read or mutate a symbol cell by
//! name without the membership funnels, the function-clock policy, or the
//! phase dispatch audit. The raw `Obarray` manager keeps such lookups
//! internally (`get_or_intern`), but no writer capability may surface one.
//!
//! This scans production source with syn:
//!
//! * every `impl` of the three capability types, and
//! * every `obarray_method_proxies! { ... }` signature list, the table that
//!   generates the adapter methods.
//!
//! A method is a violation when a parameter type names a string source
//! (`str`, `String`, `LispString`) and the return type names a raw symbol
//! (`LispSymbol`) or raw symbol handle (`SymbolHandle`). Returning `SymId` is
//! the intended canonical-handle spelling and is not flagged; neither is the
//! immutable read access a name probe returns as a `Value`/`Option<Value>`.

use std::collections::BTreeSet;

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

/// Types whose `impl` blocks form the writer surface under guard.
const WRITER_TYPES: &[&str] = &["ObarrayWriteAccess", "ObarrayViewMut", "ObarrayView"];

/// Return-type names that denote a raw symbol cell or raw symbol handle.
const RAW_SYMBOL_RETURN_TYPES: &[&str] = &["LispSymbol", "SymbolHandle"];

/// Parameter-type names that denote an Elisp name source.
const NAME_SOURCE_TYPES: &[&str] = &["str", "String", "LispString"];

const PROXY_MACRO: &str = "obarray_method_proxies";

/// Sanity floors: the proxy table is the generated adapter surface, so a
/// refactor that hides it from this scanner must fail the guard instead of
/// passing it vacuously.
const MIN_WRITER_IMPLS: usize = 3;
const MIN_PROXY_INVOCATIONS: usize = 6;
const MIN_PROXY_SIGNATURES: usize = 40;

#[derive(Debug)]
struct Violation {
    file: String,
    container: String,
    method: String,
    site: &'static str,
    line: usize,
}

/// Collects every path-segment identifier mentioned by a type, including
/// inside references, pointers, groups, tuples and generic arguments.
#[derive(Default)]
struct TypeIdents {
    idents: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for TypeIdents {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        self.idents.insert(segment.ident.to_string());
        visit::visit_path_segment(self, segment);
    }
}

fn type_idents(ty: &syn::Type) -> BTreeSet<String> {
    let mut collector = TypeIdents::default();
    collector.visit_type(ty);
    collector.idents
}

fn mentions_any(idents: &BTreeSet<String>, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| idents.contains(*candidate))
}

/// Last segment of an `impl` self type, unwrapping references and groups:
/// `ObarrayWriteAccess<'_>` -> `ObarrayWriteAccess`.
fn impl_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(reference) => impl_type_name(&reference.elem),
        syn::Type::Group(group) => impl_type_name(&group.elem),
        syn::Type::Paren(paren) => impl_type_name(&paren.elem),
        _ => None,
    }
}

/// Every top-level `fn ...;` signature in a proxy macro invocation's token
/// stream. The invocation grammar is `phase; mode; <signature>; ...`; the
/// signatures are parsed as trait-item functions because they end in `;`.
fn proxy_signatures(tokens: TokenStream) -> Vec<syn::Signature> {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut signatures = Vec::new();
    let mut index = 0;
    while index < trees.len() {
        let is_fn = matches!(&trees[index], TokenTree::Ident(ident) if ident == "fn");
        if !is_fn {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        let mut end = None;
        while index < trees.len() {
            if matches!(&trees[index], TokenTree::Punct(punct) if punct.as_char() == ';') {
                end = Some(index);
                break;
            }
            index += 1;
        }
        let Some(end) = end else { break };
        let signature_tokens: TokenStream = trees[start..=end].iter().cloned().collect();
        if let Ok(item) = syn::parse2::<syn::TraitItemFn>(signature_tokens) {
            signatures.push(item.sig);
        }
        index = end + 1;
    }
    signatures
}

#[derive(Default)]
struct Scan {
    file: String,
    violations: Vec<Violation>,
    writer_impls: usize,
    proxy_invocations: usize,
    proxy_signatures: usize,
}

impl Scan {
    fn check_signature(
        &mut self,
        container: &str,
        site: &'static str,
        signature: &syn::Signature,
        line: usize,
    ) {
        let takes_name = signature.inputs.iter().any(|argument| match argument {
            syn::FnArg::Receiver(_) => false,
            syn::FnArg::Typed(typed) => mentions_any(&type_idents(&typed.ty), NAME_SOURCE_TYPES),
        });
        if !takes_name {
            return;
        }
        let syn::ReturnType::Type(_, returned) = &signature.output else {
            return;
        };
        if !mentions_any(&type_idents(returned), RAW_SYMBOL_RETURN_TYPES) {
            return;
        }
        self.violations.push(Violation {
            file: self.file.clone(),
            container: container.to_owned(),
            method: signature.ident.to_string(),
            site,
            line,
        });
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if let Some(container) =
            impl_type_name(&item.self_ty).filter(|name| WRITER_TYPES.contains(&name.as_str()))
        {
            self.writer_impls += 1;
            for impl_item in &item.items {
                if let syn::ImplItem::Fn(function) = impl_item {
                    let line = function.sig.ident.span().start().line;
                    self.check_signature(&container, "impl", &function.sig, line);
                }
            }
        }
        visit::visit_item_impl(self, item);
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident(PROXY_MACRO) {
            self.proxy_invocations += 1;
            for signature in proxy_signatures(item.mac.tokens.clone()) {
                self.proxy_signatures += 1;
                let line = signature.ident.span().start().line;
                self.check_signature(PROXY_MACRO, "proxy", &signature, line);
            }
        }
        visit::visit_item_macro(self, item);
    }
}

#[test]
fn obarray_writer_surface_has_no_name_to_raw_symbol_method() {
    let mut scan = Scan::default();
    for file in production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let parsed = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", file.path.display()));
        scan.file = file.workspace_relative();
        scan.visit_file(&parsed);
    }

    assert!(
        scan.writer_impls >= MIN_WRITER_IMPLS,
        "writer-surface scanner found only {} impl blocks of {:?}; the writer \
         types likely moved out of the scanned production roots",
        scan.writer_impls,
        WRITER_TYPES,
    );
    assert!(
        scan.proxy_invocations >= MIN_PROXY_INVOCATIONS
            && scan.proxy_signatures >= MIN_PROXY_SIGNATURES,
        "writer-surface scanner found {} {PROXY_MACRO}! invocations with {} \
         signatures; expected at least {MIN_PROXY_INVOCATIONS} and \
         {MIN_PROXY_SIGNATURES}, so the adapter table may have moved",
        scan.proxy_invocations,
        scan.proxy_signatures,
    );
    let rendered: Vec<String> = scan
        .violations
        .iter()
        .map(|violation| {
            format!(
                "{}:{} {}::{} at {}",
                violation.file,
                violation.line,
                violation.container,
                violation.method,
                violation.site,
            )
        })
        .collect();
    assert!(
        rendered.is_empty(),
        "the obarray writer surface hands out a raw LispSymbol (or raw symbol \
         handle) addressed by an Elisp name; route the operation through the \
         membership/canonical-id funnels instead:\n{}",
        rendered.join("\n"),
    );
}
