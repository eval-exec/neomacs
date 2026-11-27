//! No safe production path reaches `TaggedHeap` collection, and
//! `with_tagged_heap` stays `unsafe fn` and never mints `HeapRef`/`HeapMut`.
//!
//! `collect`, `collect_exact`, `set_tagged_heap_root`, `with_tagged_heap`,
//! `alloc_user_ptr`, `alloc_module_function`, and the sweep entries
//! (`begin_stw_collection`, `complete_collection`, `begin_minor_collection`,
//! `complete_minor_collection`, `incremental_finish`,
//! `incremental_sweep_slice`, `finish_incremental_sweep_now`) are `unsafe fn`. `seed_root` stays safe:
//! it does not free. A safe function, including one that hides the call in
//! an `unsafe` block, must not call `collect` or `collect_exact`. The same
//! scan covers the seven sweep entries. Exactly two safe callers may reach
//! them: `gc_collect_from_current_roots_body` in `command_loop.rs`, and
//! `StoppedCollectorJob::run` in `collection.rs`. A new safe wrapper fails.
//! `Context::gc_collect_exact` stays safe. `&mut Context` is the capability
//! only after Phase 1 stops returning `&'static` payload borrows. Until then
//! a Rust local can keep a freed object. P1.1 through P1.4 place-anchor fifteen
//! readers in `value/mod.rs`. The borrow is the `Value` place, so it still does not
//! conflict with a collection. That conflict is Phase 1c (`HeapRef<'h>`).
//! Dropping a `TaggedHeap` and `shutdown(self)` still free from safe code.
//! Those are Phase 1c too. This guard does not cover them. Test support calls
//! `collect_rooted` and the `*_for_test` sweep wrappers, which are cfg'd out
//! of production builds and are deliberately unsound for unrooted borrows.
//!
//! The scan parses source syntax, not expanded macros or resolved types.
//! `macro_rules` bodies (other than `era_operations!`), `<TaggedHeap>::method`
//! (qself), renamed imports, and function pointers are invisible here; the
//! compiler still requires `unsafe` at those calls. UFCS calls (`Type::method`
//! and `<T as Trait>::method`) to the era collection wrappers ARE scanned.
//! `Iterator::collect` takes no arguments, so a one-argument `.collect` is the
//! heap entry. Items whose `cfg` needs `test` or `test-support` are not
//! production. A child module declared `#[cfg(test)]` in its parent is still
//! scanned, which can only false-positive.

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ItemImpl, Meta, Type};

use super::architecture_production_source_roots::{self, ProductionSourceFile};

const HEAP_METHODS: &[&str] = &[
    "collect",
    "collect_exact",
    "alloc_user_ptr",
    "alloc_module_function",
    "begin_stw_collection",
    "complete_collection",
    "begin_minor_collection",
    "complete_minor_collection",
    "incremental_finish",
    "incremental_sweep_slice",
    "finish_incremental_sweep_now",
    "finish_first_partition_cycle",
];

const INSTALL: &str = "set_tagged_heap_root";
const TLS_HEAP_ACCESS: &str = "with_tagged_heap";

/// Era-level unsafe collection entries defined inside `era_operations!` in
/// `mutator_registration/wrappers.rs` (lead #13986 N2).
const ERA_COLLECT_METHODS: &[&str] = &[
    "collect_stopped_with",
    "collect_stopped_minor_with",
    "collect_requested",
];

const ERA_WRAPPERS_FILE: &str = "neovm-value/src/tagged/gc/mutator_registration/wrappers.rs";

/// Safe production functions allowed to wrap an era-level unsafe collection
/// entry by supplying the canonical Baton World-root walk.
const ERA_COLLECT_SAFE_CALLERS: &[(&str, &str)] = &[(
    "neovm-eval/src/emacs_core/runtime/eval/parallel_entry/owned_source/keeper_service.rs",
    "collect_requested",
)];

/// Safe production functions that already own a sweep cycle. The pair is
/// file plus function, so another `run` does not inherit the allowance.
const SWEEP_SAFE_CALLERS: &[(&str, &str)] = &[
    (
        "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
        "gc_collect_from_current_roots_body",
    ),
    (
        "neovm-value/src/tagged/gc/mutator_registration/collection.rs",
        "run",
    ),
];

fn is_sweep_method(name: &str) -> bool {
    matches!(
        name,
        "begin_stw_collection"
            | "complete_collection"
            | "begin_minor_collection"
            | "complete_minor_collection"
            | "incremental_finish"
            | "incremental_sweep_slice"
            | "finish_incremental_sweep_now"
            | "finish_first_partition_cycle"
    )
}

struct Definition {
    name: &'static str,
    file: String,
    unsafe_fn: bool,
}

struct Violation {
    file: String,
    line: usize,
    function: String,
    call: String,
}

struct Survey {
    definitions: Vec<Definition>,
    era_definitions: Vec<Definition>,
    violations: Vec<Violation>,
    tls_token_violations: Vec<Violation>,
}

struct TokenMintFinder {
    mints: Vec<(String, usize)>,
}

impl<'ast> Visit<'ast> for TokenMintFinder {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if matches!(name.as_str(), "heap_mut" | "heap_ref") {
            self.mints.push((name, call.method.span().start().line));
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        if let Some(last) = expr.path.segments.last() {
            let ident = last.ident.to_string();
            if matches!(ident.as_str(), "heap_mut" | "heap_ref") {
                self.mints.push((ident, last.ident.span().start().line));
            }
        }
        visit::visit_expr_path(self, expr);
    }
}

fn find_token_mints_in_expr(expr: &Expr) -> Vec<(String, usize)> {
    let mut finder = TokenMintFinder { mints: Vec::new() };
    finder.visit_expr(expr);
    finder.mints
}

fn find_token_mints_in_block(block: &syn::Block) -> Vec<(String, usize)> {
    let mut finder = TokenMintFinder { mints: Vec::new() };
    finder.visit_block(block);
    finder.mints
}

struct Scanner<'a> {
    file: &'a str,
    excluded: u32,
    unsafe_fn: u32,
    in_tagged_heap: bool,
    in_with_tagged_heap_arg: u32,
    function: Vec<String>,
    local_token_mints: Vec<std::collections::BTreeMap<String, Vec<(String, usize)>>>,
    fn_token_mints: std::collections::BTreeMap<String, Vec<(String, usize)>>,
    pending_tls_ident_args: Vec<(String, String)>,
    survey: Survey,
}

impl<'a> Scanner<'a> {
    fn new(file: &'a str) -> Self {
        Self {
            file,
            excluded: 0,
            unsafe_fn: 0,
            in_tagged_heap: false,
            in_with_tagged_heap_arg: 0,
            function: Vec::new(),
            local_token_mints: vec![std::collections::BTreeMap::new()],
            fn_token_mints: std::collections::BTreeMap::new(),
            pending_tls_ident_args: Vec::new(),
            survey: Survey {
                definitions: Vec::new(),
                era_definitions: Vec::new(),
                violations: Vec::new(),
                tls_token_violations: Vec::new(),
            },
        }
    }

    fn enter_item(
        &mut self,
        name: &str,
        attrs: &[Attribute],
        unsafe_fn: bool,
        body: impl FnOnce(&mut Self),
    ) {
        let excluded = production_excluded(attrs);
        if excluded {
            self.excluded += 1;
        }
        if unsafe_fn {
            self.unsafe_fn += 1;
        }
        self.function.push(name.to_owned());
        self.local_token_mints
            .push(std::collections::BTreeMap::new());
        body(self);
        self.local_token_mints.pop();
        self.function.pop();
        if unsafe_fn {
            self.unsafe_fn -= 1;
        }
        if excluded {
            self.excluded -= 1;
        }
    }

    fn resolve_pending_tls_helpers(&mut self) {
        let pending = std::mem::take(&mut self.pending_tls_ident_args);
        for (caller, callee) in pending {
            if let Some(mints) = self.fn_token_mints.get(&callee) {
                for (mint, line) in mints {
                    self.survey.tls_token_violations.push(Violation {
                        file: self.file.to_owned(),
                        line: *line,
                        function: caller.clone(),
                        call: format!("{mint} (via `{callee}`)"),
                    });
                }
            }
        }
    }

    fn note_definition(&mut self, name: &'static str, attrs: &[Attribute], unsafe_fn: bool) {
        if self.excluded > 0 || production_excluded(attrs) {
            return;
        }
        self.survey.definitions.push(Definition {
            name,
            file: self.file.to_owned(),
            unsafe_fn,
        });
    }

    fn sweep_caller_allowed(&self) -> bool {
        let Some(function) = self.function.last() else {
            return false;
        };
        SWEEP_SAFE_CALLERS
            .iter()
            .any(|(file, name)| *file == self.file && *name == function)
    }

    fn era_collect_caller_allowed(&self) -> bool {
        let Some(function) = self.function.last() else {
            return false;
        };
        ERA_COLLECT_SAFE_CALLERS
            .iter()
            .any(|(file, name)| *file == self.file && *name == function)
    }

    fn scan_era_macro_tokens(&mut self, stream: proc_macro2::TokenStream) {
        use proc_macro2::TokenTree;
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        for (i, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.scan_era_macro_tokens(group.stream()),
                TokenTree::Ident(ident) if ident == "fn" => {
                    let Some(TokenTree::Ident(name_ident)) = tokens.get(i + 1) else {
                        continue;
                    };
                    let name_str = name_ident.to_string();
                    let Some(expected) = ERA_COLLECT_METHODS
                        .iter()
                        .copied()
                        .find(|method| *method == name_str)
                    else {
                        continue;
                    };
                    let prev_unsafe = i > 0
                        && matches!(&tokens[i - 1], TokenTree::Ident(prev) if prev == "unsafe");
                    let is_pub = (prev_unsafe
                        && i >= 2
                        && matches!(&tokens[i - 2], TokenTree::Ident(p) if p == "pub"))
                        || (!prev_unsafe
                            && i >= 1
                            && matches!(&tokens[i - 1], TokenTree::Ident(p) if p == "pub"));
                    if is_pub {
                        self.survey.era_definitions.push(Definition {
                            name: expected,
                            file: self.file.to_owned(),
                            unsafe_fn: prev_unsafe,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn note_call(&mut self, call: &str, line: usize) {
        if self.excluded > 0 || self.unsafe_fn > 0 {
            return;
        }
        self.survey.violations.push(Violation {
            file: self.file.to_owned(),
            line,
            function: self
                .function
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".to_owned()),
            call: call.to_owned(),
        });
    }

    fn note_tls_token_mint(&mut self, call: &str, line: usize) {
        if self.excluded > 0 {
            return;
        }
        self.survey.tls_token_violations.push(Violation {
            file: self.file.to_owned(),
            line,
            function: self
                .function
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".to_owned()),
            call: call.to_owned(),
        });
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.enter_item(&item.ident.to_string(), &item.attrs, false, |scanner| {
            visit::visit_item_mod(scanner, item);
        });
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let name = item.sig.ident.to_string();
        if name == INSTALL {
            self.note_definition(
                INSTALL,
                &item.attrs,
                matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            );
        } else if name == TLS_HEAP_ACCESS {
            self.note_definition(
                TLS_HEAP_ACCESS,
                &item.attrs,
                matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            );
        }
        if self.excluded == 0 && !production_excluded(&item.attrs) {
            let mints = find_token_mints_in_block(&item.block);
            if !mints.is_empty() {
                self.fn_token_mints.insert(name.clone(), mints);
            }
        }
        self.enter_item(
            &name,
            &item.attrs,
            matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            |scanner| {
                visit::visit_item_fn(scanner, item);
            },
        );
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if self.excluded == 0
            && let Some(init) = &local.init
            && let syn::Pat::Ident(pat_ident) = &local.pat
        {
            let mints = find_token_mints_in_expr(&init.expr);
            if !mints.is_empty()
                && let Some(scope) = self.local_token_mints.last_mut()
            {
                scope.insert(pat_ident.ident.to_string(), mints);
            }
        }
        visit::visit_local(self, local);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let tagged = item.trait_.is_none() && type_is_tagged_heap(&item.self_ty);
        let previous = self.in_tagged_heap;
        self.in_tagged_heap = tagged;
        self.enter_item("<impl>", &item.attrs, false, |scanner| {
            visit::visit_item_impl(scanner, item);
        });
        self.in_tagged_heap = previous;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let name = item.sig.ident.to_string();
        if self.in_tagged_heap
            && let Some(expected) = HEAP_METHODS.iter().copied().find(|method| *method == name)
        {
            self.note_definition(
                expected,
                &item.attrs,
                matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            );
        }
        self.enter_item(
            &name,
            &item.attrs,
            matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            |scanner| {
                visit::visit_impl_item_fn(scanner, item);
            },
        );
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if self.excluded == 0
            && !production_excluded(&item.attrs)
            && self.file == ERA_WRAPPERS_FILE
            && item.mac.path.is_ident("macro_rules")
            && item.ident.as_ref().is_some_and(|id| id == "era_operations")
        {
            self.scan_era_macro_tokens(item.mac.tokens.clone());
        }
        visit::visit_item_macro(self, item);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        // `collect` / `collect_exact` must not appear in any safe function.
        // Sweep names share that rule except for the two cycle owners.
        // Era-level unsafe collection wrappers share that rule except for
        // the canonical Baton World-root adapter (`ERA_COLLECT_SAFE_CALLERS`).
        let sweep = is_sweep_method(&name);
        // Era-level unsafe collection wrappers (`collect_stopped_with`,
        // `collect_stopped_minor_with`, `collect_requested`) all take two
        // arguments `(stopped, roots)`. A one-argument `KeeperEra::collect_requested`
        // delegates to the allowlisted Baton adapter in `keeper_service.rs`.
        let era_collect = ERA_COLLECT_METHODS.contains(&name.as_str()) && call.args.len() == 2;
        // `Iterator::collect()` takes 0 args; `port.collect(lend, returned, reply)` takes 3 args.
        // `TaggedHeap::collect` takes 2 args `(stopped, roots)` (1 arg in legacy synthetic fixtures).
        let collect =
            name == "collect_exact" || (name == "collect" && matches!(call.args.len(), 1 | 2));
        if collect
            || (sweep && !self.sweep_caller_allowed())
            || (era_collect && !self.era_collect_caller_allowed())
        {
            self.note_call(&name, call.method.span().start().line);
        }
        if self.in_with_tagged_heap_arg > 0 && matches!(name.as_str(), "heap_mut" | "heap_ref") {
            self.note_tls_token_mint(&name, call.method.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let mut entered_tls_call = false;
        let mut is_stopped_world_scope = false;
        if let Expr::Path(path) = &*call.func {
            if let Some(name) = heap_path_method(&path.path) {
                let sweep = is_sweep_method(&name);
                if !sweep || !self.sweep_caller_allowed() {
                    self.note_call(&name, path.path.span().start().line);
                }
            } else if let Some(name) = era_collect_path_method(path, call.args.len())
                && !self.era_collect_caller_allowed()
            {
                self.note_call(&name, path.path.span().start().line);
            }
            if is_stopped_world_scope_call(&path.path, call.args.len()) {
                is_stopped_world_scope = true;
            }
            if let Some(last) = path.path.segments.last() {
                let ident = last.ident.to_string();
                if ident == TLS_HEAP_ACCESS {
                    entered_tls_call = true;
                } else if self.in_with_tagged_heap_arg > 0
                    && matches!(ident.as_str(), "heap_mut" | "heap_ref")
                {
                    self.note_tls_token_mint(&ident, last.ident.span().start().line);
                }
            }
        }
        if is_stopped_world_scope {
            // `StoppedWorldProof::with_stopped_world(heap, |heap, stopped| body)`
            // and `with_all_stopped(heap, parallel, |heap, stopped| body)` are
            // synchronous HRTB scope combinators whose closure (the last
            // argument) runs inline and cannot escape. Visit the direct
            // closure body under the enclosing function's safety and allowlist
            // identity; any nested closure inside `body` still visits
            // `visit_expr_closure` and resets `unsafe_fn` to 0.
            self.visit_expr(&call.func);
            let args: Vec<&Expr> = call.args.iter().collect();
            let (body, leading) = args.split_last().expect("scope call has a closure");
            for arg in leading {
                self.visit_expr(arg);
            }
            if let Expr::Closure(closure) = body {
                for attr in &closure.attrs {
                    self.visit_attribute(attr);
                }
                for pat in &closure.inputs {
                    self.visit_pat(pat);
                }
                self.visit_return_type(&closure.output);
                self.visit_expr(&closure.body);
            } else {
                self.visit_expr(body);
            }
        } else if entered_tls_call {
            self.visit_expr(&call.func);
            self.in_with_tagged_heap_arg += 1;
            for arg in &call.args {
                if self.excluded == 0
                    && let Expr::Path(arg_path) = arg
                    && let Some(arg_ident) = arg_path.path.get_ident()
                {
                    let ident_str = arg_ident.to_string();
                    if let Some(mints) = self
                        .local_token_mints
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(&ident_str))
                        .cloned()
                    {
                        for (mint, line) in mints {
                            self.note_tls_token_mint(&format!("{mint} (via `{ident_str}`)"), line);
                        }
                    } else {
                        let caller = self
                            .function
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "<module>".to_owned());
                        self.pending_tls_ident_args.push((caller, ident_str));
                    }
                }
                self.visit_expr(arg);
            }
            self.in_with_tagged_heap_arg -= 1;
        } else {
            visit::visit_expr_call(self, call);
        }
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        if self.in_with_tagged_heap_arg > 0
            && let Some(last) = expr.path.segments.last()
            && matches!(last.ident.to_string().as_str(), "heap_mut" | "heap_ref")
        {
            self.note_tls_token_mint(&last.ident.to_string(), last.ident.span().start().line);
        }
        visit::visit_expr_path(self, expr);
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        // A closure body is a safe context even when the enclosing fn is unsafe.
        let saved = self.unsafe_fn;
        self.unsafe_fn = 0;
        self.function.push("<closure>".to_owned());
        self.local_token_mints
            .push(std::collections::BTreeMap::new());
        visit::visit_expr_closure(self, closure);
        self.local_token_mints.pop();
        self.function.pop();
        self.unsafe_fn = saved;
    }
}

fn type_is_tagged_heap(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "TaggedHeap"),
        Type::Group(group) => type_is_tagged_heap(&group.elem),
        Type::Paren(paren) => type_is_tagged_heap(&paren.elem),
        _ => false,
    }
}

fn is_stopped_world_scope_call(path: &syn::Path, arg_count: usize) -> bool {
    let mut rev = path.segments.iter().rev();
    let Some(method) = rev.next() else {
        return false;
    };
    let Some(ty) = rev.next() else {
        return false;
    };
    ty.ident == "StoppedWorldProof"
        && ((method.ident == "with_stopped_world" && arg_count == 2)
            || (method.ident == "with_all_stopped" && arg_count == 3))
}

fn heap_path_method(path: &syn::Path) -> Option<String> {
    let names = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let method = names.last()?.clone();
    if method != "collect" && method != "collect_exact" && !is_sweep_method(&method) {
        return None;
    }
    let receiver = names.len() > 1
        && names
            .iter()
            .rev()
            .skip(1)
            .any(|name| name == "TaggedHeap" || name == "TestHeap" || name == "Self");
    receiver.then_some(method)
}

/// Detect UFCS calls to `ERA_COLLECT_METHODS`: both `Type::method(...)`
/// (multi-segment path, including turbofish `Type::<T>::method`) and
/// `<T>::method(...)` / `<T as Trait>::method(...)` (`qself.is_some()`).
///
/// `collect_requested` has a safe 2-arg UFCS homonym (`KeeperEra::collect_requested`),
/// whereas the unsafe era wrappers take 3 arguments `(era, stopped, roots)`.
/// `collect_stopped_with` and `collect_stopped_minor_with` have no safe homonym,
/// so any UFCS call to them is flagged.
fn era_collect_path_method(path: &syn::ExprPath, arg_count: usize) -> Option<String> {
    let method = path.path.segments.last()?.ident.to_string();
    if !ERA_COLLECT_METHODS.contains(&method.as_str()) {
        return None;
    }
    if path.qself.is_none() && path.path.segments.len() <= 1 {
        return None;
    }
    if method == "collect_requested" && arg_count != 3 {
        return None;
    }
    Some(method)
}

/// True when the attribute keeps the item out of a production build
/// (`cfg(test)`, `test-support`, or a combination that needs one of those).
fn production_excluded(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| cfg_needs_test_surface(&meta)))
    })
}

fn cfg_needs_test_surface(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::NameValue(pair) => {
            pair.path.is_ident("feature") && feature_is_test_surface(&pair.value)
        }
        Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .is_ok_and(|predicates| predicates.iter().any(cfg_needs_test_surface)),
        Meta::List(list) if list.path.is_ident("any") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .is_ok_and(|predicates| {
                !predicates.is_empty() && predicates.iter().all(cfg_needs_test_surface)
            }),
        _ => false,
    }
}

fn feature_is_test_surface(value: &Expr) -> bool {
    match value {
        Expr::Lit(literal) => match &literal.lit {
            syn::Lit::Str(text) => {
                matches!(
                    text.value().as_str(),
                    "test-support" | "test-thread-fallback-heap"
                )
            }
            _ => false,
        },
        _ => false,
    }
}

fn survey_file(source: &str, file: &str) -> Survey {
    let syntax = syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
    let mut scanner = Scanner::new(file);
    if production_excluded(&syntax.attrs) {
        return scanner.survey;
    }
    scanner.visit_file(&syntax);
    scanner.resolve_pending_tls_helpers();
    scanner.survey
}

fn survey_production() -> Survey {
    let mut survey = Survey {
        definitions: Vec::new(),
        era_definitions: Vec::new(),
        violations: Vec::new(),
        tls_token_violations: Vec::new(),
    };
    for file in architecture_production_source_roots::production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let one = survey_file(&source, &display_path(&file));
        survey.definitions.extend(one.definitions);
        survey.era_definitions.extend(one.era_definitions);
        survey.violations.extend(one.violations);
        survey.tls_token_violations.extend(one.tls_token_violations);
    }
    survey
}

fn display_path(file: &ProductionSourceFile) -> String {
    file.workspace_relative().to_owned()
}

fn capability_problems(survey: &Survey) -> Vec<String> {
    let mut problems = Vec::new();
    for name in HEAP_METHODS
        .iter()
        .copied()
        .chain([INSTALL, TLS_HEAP_ACCESS])
    {
        let found = survey
            .definitions
            .iter()
            .filter(|definition| definition.name == name)
            .collect::<Vec<_>>();
        match found.as_slice() {
            [one] if one.unsafe_fn => {}
            [one] => problems.push(format!(
                "{name} is a safe fn at {}; it must be unsafe fn",
                one.file
            )),
            [] => problems.push(format!("{name} has no production definition")),
            many => problems.push(format!(
                "{name} has {} production definitions ({})",
                many.len(),
                many.iter()
                    .map(|definition| definition.file.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
    for violation in &survey.violations {
        problems.push(format!(
            "safe `{}` calls `{}` at {}:{}",
            violation.function, violation.call, violation.file, violation.line
        ));
    }
    for violation in &survey.tls_token_violations {
        problems.push(format!(
            "`with_tagged_heap` in `{}` mints capability token via `{}` at {}:{}",
            violation.function, violation.call, violation.file, violation.line
        ));
    }
    problems
}

fn era_collect_problems(survey: &Survey) -> Vec<String> {
    let mut problems = Vec::new();
    for &name in ERA_COLLECT_METHODS {
        let found = survey
            .era_definitions
            .iter()
            .filter(|definition| definition.name == name)
            .collect::<Vec<_>>();
        match found.as_slice() {
            [one] if one.unsafe_fn => {}
            [one] => problems.push(format!(
                "era wrapper `{name}` is a safe pub fn at {}; it must be pub unsafe fn",
                one.file
            )),
            [] => problems.push(format!(
                "era wrapper `{name}` has no production definition in {ERA_WRAPPERS_FILE}"
            )),
            many => problems.push(format!(
                "era wrapper `{name}` has {} definitions in {ERA_WRAPPERS_FILE}",
                many.len()
            )),
        }
    }
    problems
}

#[test]
fn no_safe_pub_path_reaches_collect() {
    let survey = survey_production();
    let mut problems = capability_problems(&survey);
    problems.extend(era_collect_problems(&survey));
    assert!(
        problems.is_empty(),
        "a safe production path reaches collection:\n{}",
        problems.join("\n")
    );
}

#[test]
fn heap_capability_guard_flags_a_safe_collect_call() {
    let open = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub fn collect_exact(&mut self, roots: impl Iterator<Item = i32>) {}
            pub fn collect(&mut self, roots: impl Iterator<Item = i32>) {
                self.collect_exact(roots);
            }
            pub fn alloc_user_ptr(&mut self) {}
            pub fn alloc_module_function(&mut self) {}
        }
        pub fn set_tagged_heap_root() {}
        "#,
        "open.rs",
    );
    let problems = capability_problems(&open).join("\n");
    assert!(
        problems.contains("collect_exact is a safe fn"),
        "missing unsafe-fn failure: {problems}"
    );
    assert!(
        problems.contains("safe `collect` calls `collect_exact`"),
        "missing safe-path failure: {problems}"
    );

    let gated = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, roots: impl Iterator<Item = i32>) {
                unsafe { self.collect_exact(roots) }
            }
            pub unsafe fn collect_exact(&mut self, roots: impl Iterator<Item = i32>) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self) {}
            pub unsafe fn complete_collection(&mut self) {}
            pub unsafe fn begin_minor_collection(&mut self) {}
            pub unsafe fn complete_minor_collection(&mut self) {}
            pub unsafe fn incremental_finish(&mut self, _bytes: usize, _pause: i32) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _budget: usize) -> bool { true }
            pub unsafe fn finish_incremental_sweep_now(&mut self) {}
            pub unsafe fn finish_first_partition_cycle(&mut self) {}
            #[cfg(any(test, feature = "test-support"))]
            pub fn collect_rooted(&mut self, roots: impl Iterator<Item = i32>) {
                unsafe { self.collect_exact(roots) }
            }
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub fn gc_collect_exact(&mut self) {}
        "#,
        "gated.rs",
    );
    let gated_problems = capability_problems(&gated);
    assert!(
        gated_problems.is_empty(),
        "gated fixture must pass:\n{}",
        gated_problems.join("\n")
    );
}

#[test]
fn heap_capability_guard_flags_safe_with_tagged_heap_and_token_minting() {
    let bad = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn collect_exact(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self) {}
            pub unsafe fn complete_collection(&mut self) {}
            pub unsafe fn begin_minor_collection(&mut self) {}
            pub unsafe fn complete_minor_collection(&mut self) {}
            pub unsafe fn incremental_finish(&mut self, _bytes: usize, _pause: i32) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _budget: usize) -> bool { true }
            pub unsafe fn finish_incremental_sweep_now(&mut self) {}
            pub unsafe fn finish_first_partition_cycle(&mut self) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub fn with_tagged_heap<R>(_f: impl FnOnce(&mut TaggedHeap) -> R) -> R { todo!() }
        fn bypass_mut() {
            with_tagged_heap(|h| { let _hm = h.heap_mut(); });
        }
        unsafe fn bypass_ref() {
            unsafe { with_tagged_heap(|h| { let _hr = h.heap_ref(); }) };
        }
        fn bypass_ufcs() {
            with_tagged_heap(TaggedHeap::heap_mut);
        }
        fn bypass_hoisted() {
            let op = |h: &mut TaggedHeap| { let _hm = h.heap_mut(); };
            unsafe { with_tagged_heap(op) };
        }
        fn mint_helper(h: &mut TaggedHeap) {
            let _hr = h.heap_ref();
        }
        fn bypass_helper_fn() {
            unsafe { with_tagged_heap(mint_helper) };
        }
        "#,
        "bypass.rs",
    );
    let problems = capability_problems(&bad).join("\n");
    assert!(
        problems.contains("with_tagged_heap is a safe fn"),
        "safe with_tagged_heap must fail: {problems}"
    );
    assert!(
        problems.contains("mints capability token via `heap_mut`"),
        "with_tagged_heap calling heap_mut must fail: {problems}"
    );
    assert!(
        problems.contains("mints capability token via `heap_ref`"),
        "with_tagged_heap calling heap_ref must fail even inside unsafe fn: {problems}"
    );
    assert!(
        problems.contains("`heap_mut (via `op`)`"),
        "hoisted let-op closure calling heap_mut must fail: {problems}"
    );
    assert!(
        problems.contains("`heap_ref (via `mint_helper`)`"),
        "helper fn calling heap_ref passed to with_tagged_heap must fail: {problems}"
    );
}

#[test]
fn heap_capability_guard_ignores_iterator_collect_and_test_modules() {
    let survey = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn collect_exact(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self) {}
            pub unsafe fn complete_collection(&mut self) {}
            pub unsafe fn begin_minor_collection(&mut self) {}
            pub unsafe fn complete_minor_collection(&mut self) {}
            pub unsafe fn incremental_finish(&mut self, _bytes: usize, _pause: i32) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _budget: usize) -> bool { true }
            pub unsafe fn finish_incremental_sweep_now(&mut self) {}
            pub unsafe fn finish_first_partition_cycle(&mut self) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        fn production(values: &[i32]) -> Vec<i32> {
            values.iter().copied().collect()
        }
        #[cfg(test)]
        mod tests {
            fn probe(heap: &mut super::TaggedHeap) {
                heap.collect_exact(std::iter::empty());
            }
        }
        "#,
        "mixed.rs",
    );
    let problems = capability_problems(&survey);
    assert!(
        problems.is_empty(),
        "iterator collect and cfg(test) must not be collection paths:\n{}",
        problems.join("\n")
    );
}

fn sweep_stubs() -> &'static str {
    r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, roots: impl Iterator<Item = i32>) {
                unsafe { self.collect_exact(roots) }
            }
            pub unsafe fn collect_exact(&mut self, roots: impl Iterator<Item = i32>) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self) {}
            pub unsafe fn complete_collection(&mut self) {}
            pub unsafe fn begin_minor_collection(&mut self) {}
            pub unsafe fn complete_minor_collection(&mut self) {}
            pub unsafe fn incremental_finish(&mut self, _bytes: usize, _pause: i32) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _budget: usize) -> bool { true }
            pub unsafe fn finish_incremental_sweep_now(&mut self) {}
            pub unsafe fn finish_first_partition_cycle(&mut self) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub fn sweep_now(heap: &mut TaggedHeap) {
            unsafe { heap.complete_collection(); }
        }
        pub fn gc_collect_from_current_roots_body(heap: &mut TaggedHeap) {
            unsafe { heap.begin_stw_collection(); }
        }
        pub fn run(heap: &mut TaggedHeap) {
            unsafe { heap.incremental_finish(0, 0); }
        }
    "#
}

#[test]
fn heap_capability_guard_flags_a_new_safe_sweep_wrapper() {
    let command_loop = capability_problems(&survey_file(
        sweep_stubs(),
        "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
    ))
    .join("\n");
    assert!(
        command_loop.contains("safe `sweep_now` calls `complete_collection`"),
        "a new safe wrapper must fail: {command_loop}"
    );
    assert!(
        command_loop.contains("safe `run` calls `incremental_finish`"),
        "run is allowed only in collection.rs: {command_loop}"
    );
    assert!(
        !command_loop.contains("gc_collect_from_current_roots_body"),
        "the command-loop cycle owner must stay allowed: {command_loop}"
    );

    let collection = capability_problems(&survey_file(
        sweep_stubs(),
        "neovm-value/src/tagged/gc/mutator_registration/collection.rs",
    ))
    .join("\n");
    assert!(
        collection.contains("safe `sweep_now` calls `complete_collection`"),
        "a new safe wrapper must fail in collection.rs: {collection}"
    );
    assert!(
        collection
            .contains("safe `gc_collect_from_current_roots_body` calls `begin_stw_collection`"),
        "the command-loop name is not allowed from collection.rs: {collection}"
    );
    assert!(
        !collection.contains("safe `run`"),
        "StoppedCollectorJob::run must stay allowed: {collection}"
    );

    let collect_still = capability_problems(&survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn collect_exact(&mut self, _roots: impl Iterator<Item = i32>) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self) {}
            pub unsafe fn complete_collection(&mut self) {}
            pub unsafe fn begin_minor_collection(&mut self) {}
            pub unsafe fn complete_minor_collection(&mut self) {}
            pub unsafe fn incremental_finish(&mut self, _bytes: usize, _pause: i32) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _budget: usize) -> bool { true }
            pub unsafe fn finish_incremental_sweep_now(&mut self) {}
            pub unsafe fn finish_first_partition_cycle(&mut self) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub fn gc_collect_from_current_roots_body(heap: &mut TaggedHeap) {
            unsafe { heap.collect_exact(std::iter::empty()); }
        }
        "#,
        "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
    ))
    .join("\n");
    assert!(
        collect_still.contains("safe `gc_collect_from_current_roots_body` calls `collect_exact`"),
        "the sweep allow-list must not cover collect: {collect_still}"
    );
}

/// P1.1 through P1.4. These readers must take `&self` and must not return `'static`.
/// A place anchor does not make collection conflict with the borrow.
const PLACE_ANCHORED: &[&str] = &[
    "as_window_configuration_data",
    "as_xwidget_view",
    "as_treesit_node_data",
    "as_bool_vector_obj",
    "as_font_data",
    "as_xwidget",
    "closure_params",
    "closure_slots",
    "as_marker_data",
    "as_record_data",
    "as_overlay_data",
    "as_char_table_obj",
    "as_sub_char_table_obj",
    "as_hash_table",
    "as_vector_data",
];

fn walk_items(items: &[syn::Item], visit: &mut dyn FnMut(&syn::ImplItemFn)) {
    for item in items {
        match item {
            syn::Item::Impl(block) => {
                for inner in &block.items {
                    if let syn::ImplItem::Fn(func) = inner {
                        visit(func);
                    }
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    walk_items(nested, visit);
                }
            }
            _ => {}
        }
    }
}

fn type_has_static(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static")
                || type_has_static(&reference.elem)
        }
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                return false;
            };
            args.args.iter().any(|arg| match arg {
                syn::GenericArgument::Lifetime(lifetime) => lifetime.ident == "static",
                syn::GenericArgument::Type(inner) => type_has_static(inner),
                _ => false,
            })
        }),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_has_static),
        Type::Slice(slice) => type_has_static(&slice.elem),
        Type::Array(array) => type_has_static(&array.elem),
        Type::Paren(paren) => type_has_static(&paren.elem),
        Type::Group(group) => type_has_static(&group.elem),
        Type::Ptr(pointer) => type_has_static(&pointer.elem),
        _ => false,
    }
}

fn output_has_static(output: &syn::ReturnType) -> bool {
    match output {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => type_has_static(ty),
    }
}

fn place_anchor_problems(source: &str, file: &str, require_all: bool) -> Vec<String> {
    let syntax = syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    walk_items(&syntax.items, &mut |func| {
        let name = func.sig.ident.to_string();
        if !PLACE_ANCHORED.contains(&name.as_str()) {
            return;
        }
        seen.push(name.clone());
        let anchored = func.sig.receiver().is_some_and(|receiver| {
            matches!(
                &receiver.kind,
                syn::ReceiverKind::Reference(_, lifetime, None)
                    if lifetime.as_ref().is_none_or(|lt| lt.ident != "static")
            )
        });
        if !anchored {
            problems.push(format!("`{name}` in {file} is not `&self`"));
        }
        if output_has_static(&func.sig.output) {
            problems.push(format!("`{name}` in {file} still returns `'static`"));
        }
    });
    if require_all {
        for name in PLACE_ANCHORED {
            if !seen.iter().any(|found| found == name) {
                problems.push(format!("`{name}` missing from {file}"));
            }
        }
    }
    problems
}

#[test]
fn place_anchor_checker_flags_a_static_by_value_reader() {
    let problems = place_anchor_problems(
        r#"
        impl Value {
            pub fn as_xwidget(self) -> Option<&'static XwidgetObj> { None }
        }
        "#,
        "snippet.rs",
        false,
    )
    .join("\n");
    assert!(
        problems.contains("`as_xwidget` in snippet.rs is not `&self`"),
        "missing receiver failure: {problems}"
    );
    assert!(
        problems.contains("`as_xwidget` in snippet.rs still returns `'static`"),
        "missing 'static failure: {problems}"
    );

    let clean = place_anchor_problems(
        r#"
        impl Value {
            pub fn as_xwidget(&self) -> Option<&XwidgetObj> { None }
        }
        "#,
        "clean.rs",
        false,
    )
    .join("\n");
    assert!(clean.is_empty(), "place-anchored reader must pass: {clean}");
}

#[test]
fn place_anchored_payload_readers_are_not_static() {
    let file = architecture_production_source_roots::production_source_files()
        .into_iter()
        .find(|file| file.workspace_relative() == "neovm-value/src/value/mod.rs")
        .expect("value/mod.rs");
    let source = std::fs::read_to_string(&file.path).expect("read value/mod.rs");
    let problems = place_anchor_problems(&source, "neovm-value/src/value/mod.rs", true);
    assert!(
        problems.is_empty(),
        "P1.1-P1.4 readers must be place-anchored:\n{}",
        problems.join("\n")
    );
}

/// Phase 1c.0 allocator-never-collects guard: token-free allocation is sound
/// under `HeapRef<'h>` only because allocation entry points never collect,
/// sweep, or move existing objects — neither directly nor transitively through
/// helpers in other GC files. If any allocator or transitive GC callee reaches
/// a collection/sweep entry (even inside `unsafe fn`), this guard fails and
/// allocation must start taking `&mut HeapMut`.
const ALLOCATOR_SOURCE_FILES: &[&str] = &[
    "neovm-value/src/tagged/gc/allocation.rs",
    "neovm-value/src/tagged/gc/alloc_region.rs",
    "neovm-value/src/tagged/gc/cons_allocator.rs",
    "neovm-value/src/tagged/gc/owner_alloc_region.rs",
];

fn is_allocator_forbidden_call(
    name: &str,
    arg_count: usize,
    is_path: bool,
    is_heap_path: bool,
) -> bool {
    name.starts_with("collect_")
        || (name == "collect" && ((!is_path && arg_count == 1) || is_heap_path))
        || is_sweep_method(name)
        || name.starts_with("gc_safe_point")
        || name.starts_with("gc_collect_")
        || name == "finish_concurrent_mark"
}

fn is_boilerplate_method_name(name: &str) -> bool {
    matches!(
        name,
        "new"
            | "default"
            | "len"
            | "is_empty"
            | "get"
            | "get_mut"
            | "set"
            | "push"
            | "pop"
            | "clear"
            | "insert"
            | "remove"
            | "contains"
            | "iter"
            | "iter_mut"
            | "clone"
            | "drop"
            | "from"
            | "into"
            | "as_ref"
            | "as_mut"
            | "fmt"
            | "eq"
            | "cmp"
            | "hash"
            | "next"
            | "load"
            | "store"
            | "swap"
            | "fetch_add"
            | "fetch_sub"
            | "read"
            | "write"
            | "lock"
            | "unlock"
            | "unwrap"
            | "expect"
            | "ok"
            | "err"
            | "map"
            | "and_then"
            | "or_else"
            | "unwrap_or"
            | "unwrap_or_else"
            | "take"
            | "replace"
            | "reserve"
            | "capacity"
            | "as_ptr"
            | "as_mut_ptr"
            | "cast"
            | "bits"
            | "raw"
            | "min"
            | "max"
            | "saturating_add"
            | "saturating_sub"
            | "checked_add"
            | "checked_sub"
            | "wrapping_add"
            | "wrapping_sub"
            | "count"
            | "first"
            | "last"
            | "collect"
    )
}

fn impl_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Type::Group(group) => impl_type_name(&group.elem),
        Type::Paren(paren) => impl_type_name(&paren.elem),
        _ => None,
    }
}

#[derive(Clone)]
struct AllocCallEdge {
    callee: String,
    qualified_target: Option<String>,
    arg_count: usize,
    is_path: bool,
    is_heap_path: bool,
    file: String,
    line: usize,
}

#[derive(Clone)]
struct AllocFnNode {
    key: String,
    bare_name: String,
    is_method: bool,
    file: String,
    is_root: bool,
    calls: Vec<AllocCallEdge>,
}

struct AllocGraphScanner<'a> {
    file: &'a str,
    is_allocator_file: bool,
    excluded: u32,
    current_impl: Option<String>,
    current_node: Option<usize>,
    nodes: Vec<AllocFnNode>,
}

impl<'a> AllocGraphScanner<'a> {
    fn new(file: &'a str, is_allocator_file: bool) -> Self {
        Self {
            file,
            is_allocator_file,
            excluded: 0,
            current_impl: None,
            current_node: None,
            nodes: Vec::new(),
        }
    }

    fn enter_fn(&mut self, bare_name: String, attrs: &[Attribute], body: impl FnOnce(&mut Self)) {
        let excluded = production_excluded(attrs);
        if excluded {
            self.excluded += 1;
        }
        let saved_node = self.current_node;
        if self.excluded == 0 {
            let (key, is_method) = match &self.current_impl {
                Some(ty) => (format!("{ty}::{bare_name}"), true),
                None => (bare_name.clone(), false),
            };
            let is_root = self.is_allocator_file || bare_name.starts_with("alloc_");
            let idx = self.nodes.len();
            self.nodes.push(AllocFnNode {
                key,
                bare_name,
                is_method,
                file: self.file.to_owned(),
                is_root,
                calls: Vec::new(),
            });
            self.current_node = Some(idx);
        }
        body(self);
        self.current_node = saved_node;
        if excluded {
            self.excluded -= 1;
        }
    }

    fn record_call(&mut self, edge: AllocCallEdge) {
        if self.excluded > 0 {
            return;
        }
        if let Some(idx) = self.current_node {
            self.nodes[idx].calls.push(edge);
        }
    }
}

impl<'ast> Visit<'ast> for AllocGraphScanner<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let excluded = production_excluded(&item.attrs);
        if excluded {
            self.excluded += 1;
        }
        visit::visit_item_mod(self, item);
        if excluded {
            self.excluded -= 1;
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.enter_fn(item.sig.ident.to_string(), &item.attrs, |scanner| {
            visit::visit_item_fn(scanner, item);
        });
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let excluded = production_excluded(&item.attrs);
        if excluded {
            self.excluded += 1;
        }
        let saved_impl = self.current_impl.clone();
        self.current_impl = if item.trait_.is_none() {
            impl_type_name(&item.self_ty)
        } else {
            None
        };
        visit::visit_item_impl(self, item);
        self.current_impl = saved_impl;
        if excluded {
            self.excluded -= 1;
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.enter_fn(item.sig.ident.to_string(), &item.attrs, |scanner| {
            visit::visit_impl_item_fn(scanner, item);
        });
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let callee = call.method.to_string();
        let receiver_is_self = matches!(
            &*call.receiver,
            Expr::Path(path) if path.path.is_ident("self")
        );
        let qualified_target = if receiver_is_self {
            self.current_impl
                .as_ref()
                .map(|ty| format!("{ty}::{callee}"))
        } else {
            None
        };
        self.record_call(AllocCallEdge {
            callee,
            qualified_target,
            arg_count: call.args.len(),
            is_path: false,
            is_heap_path: false,
            file: self.file.to_owned(),
            line: call.method.span().start().line,
        });
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func
            && let Some(last) = path.path.segments.last()
        {
            let callee = last.ident.to_string();
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            let qualified_target = if segments.len() >= 2 {
                let prefix = &segments[segments.len() - 2];
                if prefix == "Self" {
                    self.current_impl
                        .as_ref()
                        .map(|ty| format!("{ty}::{callee}"))
                } else if prefix.chars().next().is_some_and(|c| c.is_uppercase()) {
                    Some(format!("{prefix}::{callee}"))
                } else {
                    None
                }
            } else {
                None
            };
            self.record_call(AllocCallEdge {
                callee,
                qualified_target,
                arg_count: call.args.len(),
                is_path: true,
                is_heap_path: heap_path_method(&path.path).is_some(),
                file: self.file.to_owned(),
                line: path.path.span().start().line,
            });
        }
        visit::visit_expr_call(self, call);
    }
}

fn allocator_graph_problems(sources: &[(&str, &str)]) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    let mut nodes: Vec<AllocFnNode> = Vec::new();
    for &(file, source) in sources {
        let syntax =
            syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
        if production_excluded(&syntax.attrs) {
            continue;
        }
        let is_allocator_file = ALLOCATOR_SOURCE_FILES.contains(&file);
        let mut scanner = AllocGraphScanner::new(file, is_allocator_file);
        scanner.visit_file(&syntax);
        nodes.extend(scanner.nodes);
    }

    let mut by_key: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut by_method: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut by_free_fn: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        by_key.entry(node.key.clone()).or_default().push(idx);
        if !is_boilerplate_method_name(&node.bare_name) {
            if node.is_method {
                by_method
                    .entry(node.bare_name.clone())
                    .or_default()
                    .push(idx);
            } else {
                by_free_fn
                    .entry(node.bare_name.clone())
                    .or_default()
                    .push(idx);
            }
        }
    }

    let mut problems = Vec::new();
    for (root_idx, root) in nodes.iter().enumerate() {
        if !root.is_root {
            continue;
        }
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        let mut queue: VecDeque<(usize, Vec<String>)> = VecDeque::new();
        visited.insert(root_idx);
        queue.push_back((root_idx, vec![format!("{} ({})", root.key, root.file)]));

        while let Some((node_idx, chain)) = queue.pop_front() {
            for edge in &nodes[node_idx].calls {
                if is_allocator_forbidden_call(
                    &edge.callee,
                    edge.arg_count,
                    edge.is_path,
                    edge.is_heap_path,
                ) {
                    problems.push(format!(
                        "{}:{}: `{}` reaches `{}` via {}",
                        edge.file,
                        edge.line,
                        nodes[node_idx].bare_name,
                        edge.callee,
                        chain.join(" -> ")
                    ));
                    continue;
                }
                let targets: &[usize] = if let Some(qualified) = &edge.qualified_target {
                    by_key.get(qualified).map(Vec::as_slice).unwrap_or(&[])
                } else if edge.is_path {
                    by_free_fn
                        .get(&edge.callee)
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                } else {
                    by_method
                        .get(&edge.callee)
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                };
                for &next_idx in targets {
                    if visited.insert(next_idx) {
                        let mut next_chain = chain.clone();
                        next_chain.push(format!(
                            "{} ({}:{})",
                            nodes[next_idx].key, edge.file, edge.line
                        ));
                        queue.push_back((next_idx, next_chain));
                    }
                }
            }
        }
    }

    problems
}

#[test]
fn allocation_guard_flags_collect_inside_allocator() {
    let bad = allocator_graph_problems(&[(
        "neovm-value/src/tagged/gc/allocation.rs",
        r#"
        impl TaggedHeap {
            pub fn alloc_vector(&mut self) {
                unsafe { self.collect_exact(std::iter::empty()); }
            }
            pub unsafe fn alloc_user_ptr(&mut self) {
                self.begin_stw_collection();
            }
        }
        "#,
    )])
    .join("\n");
    assert!(
        bad.contains("`alloc_vector` reaches `collect_exact`"),
        "guard must flag safe allocator collecting: {bad}"
    );
    assert!(
        bad.contains("`alloc_user_ptr` reaches `begin_stw_collection`"),
        "guard must flag unsafe allocator sweeping: {bad}"
    );

    let clean = allocator_graph_problems(&[(
        "neovm-value/src/tagged/gc/allocation.rs",
        r#"
        impl TaggedHeap {
            pub fn alloc_cons(&mut self) -> Vec<i32> {
                [1, 2].into_iter().collect()
            }
        }
        "#,
    )]);
    assert!(
        clean.is_empty(),
        "Iterator::collect must not false-positive: {clean:?}"
    );
}

#[test]
fn allocation_guard_flags_two_hop_cross_file_collection() {
    let bad = allocator_graph_problems(&[
        (
            "neovm-value/src/tagged/gc/allocation.rs",
            r#"
            impl TaggedHeap {
                pub fn alloc_vector(&mut self) {
                    self.grow_arena_slow();
                }
            }
            "#,
        ),
        (
            "neovm-value/src/tagged/gc/arena_pages.rs",
            r#"
            impl TaggedHeap {
                fn grow_arena_slow(&mut self) {
                    self.reclaim_or_grow();
                }
            }
            "#,
        ),
        (
            "neovm-value/src/tagged/gc/mark_sweep.rs",
            r#"
            impl TaggedHeap {
                fn reclaim_or_grow(&mut self) {
                    self.collect_garbage();
                }
            }
            "#,
        ),
    ])
    .join("\n");
    assert!(
        bad.contains("`reclaim_or_grow` reaches `collect_garbage`"),
        "2-hop cross-file allocator -> helper -> collect_* must fail: {bad}"
    );
    assert!(
        bad.contains("TaggedHeap::alloc_vector") && bad.contains("TaggedHeap::grow_arena_slow"),
        "violation must include the full transitive call chain: {bad}"
    );
}

#[test]
fn allocation_guard_flags_run_sweep_entry() {
    let bad = allocator_graph_problems(&[
        (
            "neovm-value/src/tagged/gc/allocation.rs",
            r#"
            impl TaggedHeap {
                pub fn alloc_vector(&mut self, job: StoppedCollectorJob) {
                    job.run(self);
                }
            }
            "#,
        ),
        (
            "neovm-value/src/tagged/gc/mutator_registration/collection.rs",
            r#"
            impl StoppedCollectorJob {
                pub(crate) fn run(self, heap: &mut TaggedHeap) {
                    unsafe { heap.begin_stw_collection(); }
                }
            }
            "#,
        ),
    ])
    .join("\n");
    assert!(
        bad.contains("`run` reaches `begin_stw_collection`"),
        "allocator reaching a `run` sweep entry must fail: {bad}"
    );
}

#[test]
fn allocation_entry_points_never_collect() {
    let files = architecture_production_source_roots::production_source_files();
    let mut gc_sources: Vec<(String, String)> = Vec::new();
    let mut seen_allocator_files = 0usize;
    for file in &files {
        if file.is_test_path() {
            continue;
        }
        let rel = file.workspace_relative();
        if rel.ends_with("/tests.rs")
            || rel.contains("_tests/")
            || rel.ends_with("_tests.rs")
            || rel.ends_with("_test.rs")
            || rel.contains("test_support")
            || rel.contains("testkit")
        {
            continue;
        }
        if rel == "neovm-value/src/tagged/gc.rs" || rel.starts_with("neovm-value/src/tagged/gc/") {
            if ALLOCATOR_SOURCE_FILES.contains(&rel.as_str()) {
                seen_allocator_files += 1;
            }
            let source = std::fs::read_to_string(&file.path)
                .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
            gc_sources.push((rel.to_owned(), source));
        }
    }
    assert_eq!(seen_allocator_files, ALLOCATOR_SOURCE_FILES.len());
    let borrowed: Vec<(&str, &str)> = gc_sources
        .iter()
        .map(|(path, src)| (path.as_str(), src.as_str()))
        .collect();
    let problems = allocator_graph_problems(&borrowed);
    assert!(
        problems.is_empty(),
        "allocator entry points (and their transitive GC callees) must never collect or sweep:\n{}",
        problems.join("\n")
    );
}

#[test]
fn heap_capability_guard_flags_a_safe_era_collect_method_or_caller() {
    let safe_def = survey_file(
        r#"
        macro_rules! era_operations {
            ($era:ident) => {
                impl $era {
                    pub unsafe fn collect_stopped_with(&self) {}
                    pub unsafe fn collect_stopped_minor_with(&self) {}
                    pub fn collect_requested(&self) {}
                }
            };
        }
        "#,
        ERA_WRAPPERS_FILE,
    );
    let def_problems = era_collect_problems(&safe_def).join("\n");
    assert!(
        def_problems.contains("era wrapper `collect_requested` is a safe pub fn"),
        "safe era wrapper must fail: {def_problems}"
    );

    let safe_caller = survey_file(
        r#"
        pub fn bypass(era: &Era, stopped: &mut Stop) {
            unsafe { era.collect_requested(stopped, |_, _, _| Ok(Vec::new())); }
        }
        "#,
        "neovm-eval/src/emacs_core/runtime/eval/parallel_entry/keeper_driver.rs",
    );
    let caller_problems = capability_problems(&safe_caller).join("\n");
    assert!(
        caller_problems.contains("safe `bypass` calls `collect_requested`"),
        "unallowlisted safe caller of collect_requested must fail: {caller_problems}"
    );

    let allowed_caller = survey_file(
        r#"
        pub(crate) fn collect_requested(era: &Era, stopped: &mut Stop) {
            unsafe { era.collect_requested(stopped, |_, _, _| Ok(Vec::new())); }
        }
        "#,
        "neovm-eval/src/emacs_core/runtime/eval/parallel_entry/owned_source/keeper_service.rs",
    );
    let allowed_violations: Vec<_> = allowed_caller
        .violations
        .iter()
        .map(|v| format!("{}:{}", v.function, v.call))
        .collect();
    assert!(
        allowed_violations.is_empty(),
        "canonical KeeperEra::collect_requested adapter must stay allowed: {allowed_violations:?}"
    );
}

#[test]
fn heap_capability_guard_flags_ufcs_era_collect_calls() {
    let ufcs_callers = survey_file(
        r#"
        pub fn ufcs_type(era: &ParallelHeapEra<Baton>, stopped: &mut Stop) {
            unsafe { ParallelHeapEra::collect_requested(era, stopped, roots); }
        }
        pub fn ufcs_turbofish(era: &ParallelHeapEra<Baton>, stopped: &mut Stop) {
            unsafe { ParallelHeapEra::<Baton>::collect_stopped_with(era, stopped, roots); }
        }
        pub fn ufcs_trait(era: &E, stopped: &mut Stop) {
            unsafe { <E as EraExt>::collect_stopped_minor_with(era, stopped, roots); }
        }
        pub fn ufcs_qself_inherent(era: &ParallelHeapEra<Baton>, stopped: &mut Stop) {
            unsafe { <ParallelHeapEra<Baton>>::collect_requested(era, stopped, roots); }
        }
        pub fn safe_keeper_ufcs(keeper: &KeeperEra, stopped: &mut Stop) {
            let _ = KeeperEra::collect_requested(keeper, stopped);
        }
        "#,
        "neovm-eval/src/emacs_core/runtime/eval/parallel_entry/keeper_driver.rs",
    );
    let problems = capability_problems(&ufcs_callers).join("\n");
    assert!(
        problems.contains("safe `ufcs_type` calls `collect_requested`"),
        "Type::collect_requested UFCS must fail: {problems}"
    );
    assert!(
        problems.contains("safe `ufcs_turbofish` calls `collect_stopped_with`"),
        "Type::<T>::collect_stopped_with UFCS must fail: {problems}"
    );
    assert!(
        problems.contains("safe `ufcs_trait` calls `collect_stopped_minor_with`"),
        "<T as Trait>::collect_stopped_minor_with UFCS must fail: {problems}"
    );
    assert!(
        problems.contains("safe `ufcs_qself_inherent` calls `collect_requested`"),
        "<Type>::collect_requested UFCS must fail: {problems}"
    );
    assert!(
        !problems.contains("safe_keeper_ufcs"),
        "2-arg safe KeeperEra::collect_requested UFCS must stay allowed: {problems}"
    );

    let allowed_ufcs = survey_file(
        r#"
        pub(crate) fn collect_requested(era: &ParallelHeapEra<Baton>, stopped: &mut Stop) {
            unsafe { ParallelHeapEra::collect_requested(era, stopped, roots); }
        }
        "#,
        "neovm-eval/src/emacs_core/runtime/eval/parallel_entry/owned_source/keeper_service.rs",
    );
    assert!(
        allowed_ufcs.violations.is_empty(),
        "allowlisted keeper_service::collect_requested may use UFCS: {:?}",
        allowed_ufcs
            .violations
            .iter()
            .map(|v| (&v.function, &v.call))
            .collect::<Vec<_>>()
    );
}

/// Unsafe-budget fence: audits only neovm-value/src/tagged/slice_view.rs; unsafe transmutes written in other modules are governed by workspace unsafe review and Miri SB+TB, not by this fence.
///
/// Closed allowlist over `neovm-value/src/tagged/slice_view.rs`:
/// 1. Nested items (`Stmt::Item`) inside any function/block body and every `mod` item are forbidden outright.
/// 2. Every `use` declaration (`Item::Use`) and every macro (`Item::Macro`, `ImplItem::Macro`,
///    `Stmt::Macro`, `Expr::Macro`, `Type::Macro`, `Pat::Macro`, `TraitItem::Macro`) is forbidden
///    outright — zero exceptions.
/// 3. Attributes are a closed allowlist (`//! doc = "..."` inner only; outer `doc = "..."`,
///    `inline` / `inline(always|never)`, `repr(transparent)`,
///    `derive(::core::clone::Clone | ::core::fmt::Debug)`, and
///    `allow(clippy::should_implement_trait)` only); all other outer, inner, and proc-macro
///    attributes (including bare `#[derive(Debug)]`, `#[visibility::make]`, and `#[cfg_attr]`)
///    are violations.
/// 4. Trait impls must use full canonical `::core::...` paths matching the closed per-target
///    allowlist; `Expr::Verbatim` / `Type::Verbatim` / `Pat::Verbatim` / `TypeParamBound::Verbatim`
///    fail closed.
/// 5. Every `fn` signature (receiver, parameters, generics, return type, and no `async`/`extern`)
///    and associated `type` is governed by a closed allowlist so safe code in `slice_view.rs`
///    cannot leak `&[AtomicTaggedValue]`, `&AtomicTaggedValue`, `&[TaggedValue]`, `&TaggedValue`,
///    callbacks, or any unanalysable type. (Body-vs-signature boundary: inside an allowlisted
///    signature, a method body could still call `AtomicTaggedValue::store_atomic` on its own cell;
///    writing is the owner's prerogative and no reference escapes.)
/// 6. The `unsafe` budget is strictly 2 `Expr::Unsafe` blocks, one inside
///    `LispValueSlice::from_atomic_slice` and one inside `LispValueSlice::from_raw_atomic_parts`.
const SLICE_VIEW_FILE: &str = "neovm-value/src/tagged/slice_view.rs";

const SLICE_VIEW_ALLOWED_SLICE_TRAITS: &[&str] = &[
    "::core::ops::Index",
    "::core::fmt::Debug",
    "::core::cmp::PartialEq",
    "::core::cmp::Eq",
    "::core::iter::IntoIterator",
];

const SLICE_VIEW_ALLOWED_ITER_TRAITS: &[&str] = &[
    "::core::iter::Iterator",
    "::core::iter::DoubleEndedIterator",
    "::core::iter::ExactSizeIterator",
    "::core::iter::FusedIterator",
];

const SLICE_VIEW_ALLOWED_FOREIGN_TRAIT: &str = "::core::cmp::PartialEq";

fn slice_view_self_type_name(ty: &Type) -> Option<String> {
    match ty {
        syn::Type::Reference(r) => slice_view_self_type_name(&r.elem),
        syn::Type::Group(g) => slice_view_self_type_name(&g.elem),
        syn::Type::Paren(p) => slice_view_self_type_name(&p.elem),
        syn::Type::Path(p)
            if p.qself.is_none()
                && p.path.leading_colon.is_none()
                && p.path.segments.len() == 1 =>
        {
            Some(p.path.segments[0].ident.to_string())
        }
        _ => None,
    }
}

fn slice_view_is_exact_path(ty: &Type, leading_colon: bool, expected: &[&str]) -> bool {
    let syn::Type::Path(p) = ty else {
        return false;
    };
    if p.qself.is_some()
        || p.path.leading_colon.is_some() != leading_colon
        || !p.path.segments.iter().all(|s| s.arguments.is_empty())
    {
        return false;
    }
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
    refs.as_slice() == expected
}

fn slice_view_is_exact_atomic_tagged_value_type(ty: &Type) -> bool {
    slice_view_is_exact_path(ty, false, &["super", "header", "AtomicTaggedValue"])
}

fn slice_view_is_exact_tagged_value_type(ty: &Type) -> bool {
    slice_view_is_exact_path(ty, false, &["super", "value", "TaggedValue"])
}

fn slice_view_is_exact_index_output_type(ty: &Type) -> bool {
    slice_view_is_exact_path(ty, false, &["Self"])
        || slice_view_is_exact_path(ty, false, &["LispValueSlice"])
}

fn slice_view_is_exact_self_ref(ty: &Type) -> bool {
    matches!(
        ty,
        syn::Type::Reference(r)
            if r.mutability.is_none() && slice_view_is_exact_path(&r.elem, false, &["Self"])
    )
}

fn slice_view_is_exact_iter_type(ty: &Type) -> bool {
    let syn::Type::Path(p) = ty else {
        return false;
    };
    if p.qself.is_some() || p.path.leading_colon.is_some() || p.path.segments.len() != 1 {
        return false;
    }
    let seg = &p.path.segments[0];
    if seg.ident != "LispValueSliceIter" {
        return false;
    }
    match &seg.arguments {
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
            matches!(&args.args[0], syn::GenericArgument::Lifetime(_))
        }
        _ => false,
    }
}

fn slice_view_extract_single_type_arg<'a>(
    ty: &'a Type,
    leading_colon: bool,
    expected: &[&str],
) -> Option<&'a Type> {
    let syn::Type::Path(p) = ty else {
        return None;
    };
    if p.qself.is_some() || p.path.leading_colon.is_some() != leading_colon {
        return None;
    }
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
    if refs.as_slice() != expected {
        return None;
    }
    let last_idx = p.path.segments.len() - 1;
    if p.path
        .segments
        .iter()
        .take(last_idx)
        .any(|s| !s.arguments.is_empty())
    {
        return None;
    }
    match &p.path.segments[last_idx].arguments {
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => match &args.args[0] {
            syn::GenericArgument::Type(inner) => Some(inner),
            _ => None,
        },
        _ => None,
    }
}

fn slice_view_is_pub_crate(vis: &syn::Visibility) -> bool {
    matches!(
        vis,
        syn::Visibility::Restricted(r)
            if r.in_token.is_none() && r.path.is_ident("crate")
    )
}

fn slice_view_is_allowed_range_type(ty: &Type) -> bool {
    if slice_view_is_exact_path(ty, true, &["core", "ops", "RangeFull"]) {
        return true;
    }
    for range_name in [
        "Range",
        "RangeFrom",
        "RangeTo",
        "RangeInclusive",
        "RangeToInclusive",
    ] {
        if let Some(inner) =
            slice_view_extract_single_type_arg(ty, true, &["core", "ops", range_name])
            && slice_view_is_exact_path(inner, false, &["usize"])
        {
            return true;
        }
    }
    false
}

fn slice_view_is_allowed_tagged_value_slice_or_array(ty: &Type) -> bool {
    match ty {
        syn::Type::Slice(s) => slice_view_is_exact_tagged_value_type(&s.elem),
        syn::Type::Array(a) => slice_view_is_exact_tagged_value_type(&a.elem),
        _ => false,
    }
}

fn slice_view_is_allowed_foreign_eq_target(ty: &Type) -> bool {
    if slice_view_is_allowed_tagged_value_slice_or_array(ty) {
        return true;
    }
    if let Some(vec_elem) = slice_view_extract_single_type_arg(ty, true, &["std", "vec", "Vec"])
        && slice_view_is_exact_tagged_value_type(vec_elem)
    {
        return true;
    }
    matches!(
        ty,
        syn::Type::Reference(r)
            if r.mutability.is_none()
                && slice_view_is_allowed_tagged_value_slice_or_array(&r.elem)
    )
}

fn slice_view_is_allowed_eq_rhs_param(ty: &Type) -> bool {
    let syn::Type::Reference(r) = ty else {
        return false;
    };
    if r.mutability.is_some() {
        return false;
    }
    let inner = &*r.elem;
    if slice_view_is_exact_path(inner, false, &["Self"])
        || slice_view_is_exact_path(inner, false, &["LispValueSlice"])
        || slice_view_is_allowed_tagged_value_slice_or_array(inner)
    {
        return true;
    }
    if let Some(vec_elem) = slice_view_extract_single_type_arg(inner, true, &["std", "vec", "Vec"])
        && slice_view_is_exact_tagged_value_type(vec_elem)
    {
        return true;
    }
    matches!(
        inner,
        syn::Type::Reference(r2)
            if r2.mutability.is_none()
                && slice_view_is_allowed_tagged_value_slice_or_array(&r2.elem)
    )
}

fn slice_view_is_allowed_fmt_formatter_param(ty: &Type) -> bool {
    let syn::Type::Reference(r) = ty else {
        return false;
    };
    if r.mutability.is_none() {
        return false;
    }
    let syn::Type::Path(p) = &*r.elem else {
        return false;
    };
    if p.qself.is_some() || p.path.leading_colon.is_none() {
        return false;
    }
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
    if refs.as_slice() != ["core", "fmt", "Formatter"] {
        return false;
    }
    if p.path.segments[0].arguments.is_empty() && p.path.segments[1].arguments.is_empty() {
        matches!(
            &p.path.segments[2].arguments,
            syn::PathArguments::AngleBracketed(args)
                if args.args.len() == 1
                    && matches!(&args.args[0], syn::GenericArgument::Lifetime(_))
        )
    } else {
        false
    }
}

fn slice_view_is_allowed_get_slice_generics(generics: &syn::Generics) -> bool {
    if generics.params.len() != 1 {
        return false;
    }
    let syn::GenericParam::Type(tp) = &generics.params[0] else {
        return false;
    };
    if tp.ident != "I" || !tp.bounds.is_empty() || tp.default.is_some() {
        return false;
    }
    let Some(wc) = &generics.where_clause else {
        return false;
    };
    if wc.predicates.len() != 1 {
        return false;
    }
    let syn::WherePredicate::Type(pred) = &wc.predicates[0] else {
        return false;
    };
    if !slice_view_is_exact_path(&pred.bounded_ty, false, &["I"]) || pred.bounds.len() != 1 {
        return false;
    }
    let syn::TypeParamBound::Trait(tb) = &pred.bounds[0] else {
        return false;
    };
    let path = &tb.path;
    if path.leading_colon.is_none() {
        return false;
    }
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
    if refs.as_slice() != ["core", "slice", "SliceIndex"]
        || !path.segments[0].arguments.is_empty()
        || !path.segments[1].arguments.is_empty()
    {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &path.segments[2].arguments else {
        return false;
    };
    if args.args.len() != 2 {
        return false;
    }
    let arg0_ok = matches!(
        &args.args[0],
        syn::GenericArgument::Type(syn::Type::Slice(s))
            if slice_view_is_exact_atomic_tagged_value_type(&s.elem)
    );
    let arg1_ok = matches!(
        &args.args[1],
        syn::GenericArgument::AssocType(assoc)
            if assoc.ident == "Output"
                && assoc.generics.is_none()
                && matches!(
                    &assoc.ty,
                    syn::Type::Slice(s) if slice_view_is_exact_atomic_tagged_value_type(&s.elem)
                )
    );
    arg0_ok && arg1_ok
}

fn slice_view_return_type_is_allowed(ret: &syn::ReturnType, canonical_trait: Option<&str>) -> bool {
    let syn::ReturnType::Type(_, ty) = ret else {
        return true;
    };
    let ty = &**ty;
    if let syn::Type::Tuple(t) = ty {
        if t.elems.is_empty() {
            return true;
        }
        if t.elems.len() == 2
            && slice_view_is_exact_path(&t.elems[0], false, &["usize"])
            && slice_view_extract_single_type_arg(&t.elems[1], true, &["core", "option", "Option"])
                .is_some_and(|inner| slice_view_is_exact_path(inner, false, &["usize"]))
        {
            return true;
        }
        return false;
    }
    if slice_view_is_exact_path(ty, false, &["usize"])
        || slice_view_is_exact_path(ty, false, &["bool"])
        || slice_view_is_exact_tagged_value_type(ty)
        || slice_view_is_exact_self_ref(ty)
        || slice_view_is_exact_iter_type(ty)
        || slice_view_is_exact_path(ty, true, &["core", "fmt", "Result"])
    {
        return true;
    }
    if let Some(vec_elem) = slice_view_extract_single_type_arg(ty, true, &["std", "vec", "Vec"])
        && slice_view_is_exact_tagged_value_type(vec_elem)
    {
        return true;
    }
    if let Some(opt_inner) =
        slice_view_extract_single_type_arg(ty, true, &["core", "option", "Option"])
    {
        if slice_view_is_exact_tagged_value_type(opt_inner)
            || slice_view_is_exact_self_ref(opt_inner)
        {
            return true;
        }
        if matches!(
            canonical_trait,
            Some("::core::iter::Iterator" | "::core::iter::DoubleEndedIterator")
        ) && slice_view_is_exact_path(opt_inner, false, &["Self", "Item"])
        {
            return true;
        }
        if let syn::Type::Tuple(t) = opt_inner
            && t.elems.len() == 2
            && slice_view_is_exact_tagged_value_type(&t.elems[0])
            && slice_view_is_exact_self_ref(&t.elems[1])
        {
            return true;
        }
        return false;
    }
    if canonical_trait == Some("::core::ops::Index")
        && matches!(
            ty,
            syn::Type::Reference(r)
                if r.mutability.is_none()
                    && slice_view_is_exact_path(&r.elem, false, &["Self", "Output"])
        )
    {
        return true;
    }
    if canonical_trait == Some("::core::iter::IntoIterator")
        && slice_view_is_exact_path(ty, false, &["Self", "IntoIter"])
    {
        return true;
    }
    false
}

fn slice_view_check_fn_signature(
    sig: &syn::Signature,
    vis: &syn::Visibility,
    target_name: Option<&str>,
    canonical_trait: Option<&str>,
    problems: &mut Vec<String>,
) {
    let fn_name = sig.ident.to_string();
    let is_inherent_slice = canonical_trait.is_none() && target_name == Some("LispValueSlice");
    let is_from_atomic_slice = is_inherent_slice && fn_name == "from_atomic_slice";
    let is_from_raw_parts = is_inherent_slice && fn_name == "from_raw_atomic_parts";
    let is_get_slice = is_inherent_slice && fn_name == "get_slice";

    if sig.asyncness.is_some() {
        problems.push(format!(
            "{SLICE_VIEW_FILE}: `async fn {fn_name}` is forbidden in slice_view.rs (fail closed)"
        ));
    }
    if sig.abi.is_some() {
        problems.push(format!(
            "{SLICE_VIEW_FILE}: `extern fn {fn_name}` is forbidden in slice_view.rs (fail closed)"
        ));
    }

    if is_from_atomic_slice || is_from_raw_parts {
        if !slice_view_is_pub_crate(vis) {
            problems.push(format!(
                "{SLICE_VIEW_FILE}: constructor `LispValueSlice::{fn_name}` must stay `pub(crate)` (fail closed)"
            ));
        }
    }

    if is_get_slice {
        if !slice_view_is_allowed_get_slice_generics(&sig.generics) {
            problems.push(format!(
                "{SLICE_VIEW_FILE}: `LispValueSlice::get_slice` has disallowed generics/where bounds (fail closed)"
            ));
        }
    } else if sig
        .generics
        .params
        .iter()
        .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)))
        || sig.generics.where_clause.is_some()
    {
        problems.push(format!(
            "{SLICE_VIEW_FILE}: `fn {fn_name}` has disallowed type/const generics or where clause (fail closed)"
        ));
    }

    if !slice_view_return_type_is_allowed(&sig.output, canonical_trait) {
        problems.push(format!(
            "{SLICE_VIEW_FILE}: `fn {fn_name}` has disallowed return type (fail closed)"
        ));
    }

    for (idx, arg) in sig.inputs.iter().enumerate() {
        match arg {
            syn::FnArg::Receiver(r) => {
                if !matches!(
                    r.kind,
                    syn::ReceiverKind::Value | syn::ReceiverKind::Reference(..)
                ) {
                    problems.push(format!(
                        "{SLICE_VIEW_FILE}: `fn {fn_name}` has disallowed typed receiver (fail closed)"
                    ));
                }
            }
            syn::FnArg::Typed(pat_ty) => {
                let ty = &*pat_ty.ty;
                let param_ok = if is_from_atomic_slice && idx == 0 {
                    matches!(
                        ty,
                        syn::Type::Reference(r)
                            if r.mutability.is_none()
                                && matches!(
                                    &*r.elem,
                                    syn::Type::Slice(s)
                                        if slice_view_is_exact_atomic_tagged_value_type(&s.elem)
                                )
                    )
                } else if is_from_raw_parts && idx == 0 {
                    matches!(
                        ty,
                        syn::Type::Ptr(p)
                            if matches!(p.mutability, syn::PointerMutability::Const(_))
                                && slice_view_is_exact_atomic_tagged_value_type(&p.elem)
                    )
                } else if is_from_raw_parts && idx == 1 {
                    slice_view_is_exact_path(ty, false, &["usize"])
                } else if is_get_slice && idx == 1 {
                    slice_view_is_exact_path(ty, false, &["I"])
                } else if canonical_trait == Some("::core::fmt::Debug") && fn_name == "fmt" {
                    slice_view_is_allowed_fmt_formatter_param(ty)
                } else if canonical_trait == Some("::core::ops::Index") && fn_name == "index" {
                    slice_view_is_allowed_range_type(ty)
                } else if canonical_trait == Some("::core::cmp::PartialEq") && fn_name == "eq" {
                    slice_view_is_allowed_eq_rhs_param(ty)
                } else {
                    slice_view_is_exact_path(ty, false, &["usize"])
                        || matches!(
                            ty,
                            syn::Type::Reference(r)
                                if r.mutability.is_none()
                                    && slice_view_is_exact_tagged_value_type(&r.elem)
                        )
                };
                if !param_ok {
                    problems.push(format!(
                        "{SLICE_VIEW_FILE}: `fn {fn_name}` parameter {idx} has disallowed type (fail closed)"
                    ));
                }
            }
        }
    }
}

fn slice_view_display_path(path: &syn::Path) -> String {
    let prefix = if path.leading_colon.is_some() {
        "::"
    } else {
        ""
    };
    let joined = path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    format!("{prefix}{joined}")
}

/// Full canonical `::crate::...` path for trait impls (leading `::` required,
/// non-final segments must carry no generic arguments).
fn slice_view_canonical_trait_path(path: &syn::Path) -> Option<String> {
    if path.leading_colon.is_none() || path.segments.is_empty() {
        return None;
    }
    let last_idx = path.segments.len() - 1;
    if path
        .segments
        .iter()
        .take(last_idx)
        .any(|s| !s.arguments.is_empty())
    {
        return None;
    }
    Some(slice_view_display_path(path))
}

fn slice_view_item_kind(item: &syn::Item) -> &'static str {
    match item {
        syn::Item::Const(_) => "const",
        syn::Item::Enum(_) => "enum",
        syn::Item::ExternCrate(_) => "extern crate",
        syn::Item::Fn(_) => "fn",
        syn::Item::ForeignMod(_) => "extern",
        syn::Item::Impl(_) => "impl",
        syn::Item::Macro(_) => "macro",
        syn::Item::Mod(_) => "mod",
        syn::Item::Static(_) => "static",
        syn::Item::Struct(_) => "struct",
        syn::Item::Trait(_) => "trait",
        syn::Item::TraitAlias(_) => "trait alias",
        syn::Item::Type(_) => "type",
        syn::Item::Union(_) => "union",
        syn::Item::Use(_) => "use",
        _ => "item",
    }
}

/// Closed attribute allowlist for `slice_view.rs`.
///
/// Inner attributes: ONLY `doc = "..."` (`//! ...`).
/// Outer attributes: ONLY single-segment `doc = "..."`, `inline` / `inline(always|never)`,
/// `repr(transparent)`, `derive(::core::clone::Clone, ::core::fmt::Debug)`, and
/// `allow(clippy::should_implement_trait)`.
/// Every other attribute (including bare `#[derive(Debug)]`, `#[cfg]`, `#[cfg_attr]`,
/// `#[path]`, `#[visibility::make(pub)]`, `#[allow(unsafe_op_in_unsafe_fn)]`, arbitrary
/// derives, and all inner attributes other than `//!`) fails closed.
fn slice_view_attribute_is_allowed(attr: &Attribute) -> bool {
    let Some(ident) = attr.path().get_ident() else {
        // Multi-segment attribute path (e.g. `#[visibility::make(pub)]`) is forbidden.
        return false;
    };
    let name = ident.to_string();
    let is_plain_doc_str = matches!(
        &attr.meta,
        syn::Meta::NameValue(nv)
            if matches!(
                &nv.value,
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(_),
                    ..
                })
            )
    );
    if matches!(attr.style, syn::AttrStyle::Inner(_)) {
        return name == "doc" && is_plain_doc_str;
    }
    match name.as_str() {
        "doc" => is_plain_doc_str,
        "inline" => {
            matches!(&attr.meta, syn::Meta::Path(_))
                || attr
                    .parse_args::<syn::Ident>()
                    .is_ok_and(|arg| arg == "always" || arg == "never")
        }
        "repr" => attr
            .parse_args::<syn::Ident>()
            .is_ok_and(|arg| arg == "transparent"),
        "derive" => attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|paths| {
                !paths.is_empty()
                    && paths.iter().all(|p| {
                        p.leading_colon.is_some()
                            && p.segments.iter().all(|s| s.arguments.is_empty())
                            && matches!(
                                slice_view_display_path(p).as_str(),
                                "::core::clone::Clone" | "::core::fmt::Debug"
                            )
                    })
            }),
        "allow" => attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|paths| {
                !paths.is_empty()
                    && paths.iter().all(|p| {
                        p.leading_colon.is_none()
                            && p.segments.iter().all(|s| s.arguments.is_empty())
                            && slice_view_display_path(p) == "clippy::should_implement_trait"
                    })
            }),
        _ => false,
    }
}

struct SliceViewUnsafeBudgetVisitor {
    saw_slice_struct: bool,
    saw_iter_struct: bool,
    impl_target_stack: Vec<String>,
    owner_stack: Vec<String>,
    unsafe_locations: Vec<(Option<String>, Option<String>)>,
    problems: Vec<String>,
}

impl SliceViewUnsafeBudgetVisitor {
    fn check_top_level_item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Struct(s) => {
                let name = s.ident.to_string();
                match name.as_str() {
                    "LispValueSlice" => {
                        self.saw_slice_struct = true;
                        let has_repr_transparent = s.attrs.iter().any(|attr| {
                            attr.path().is_ident("repr")
                                && attr
                                    .parse_args::<syn::Ident>()
                                    .is_ok_and(|ident| ident == "transparent")
                        });
                        if !has_repr_transparent {
                            self.problems.push(format!(
                                "{SLICE_VIEW_FILE}: `LispValueSlice` must keep `#[repr(transparent)]`"
                            ));
                        }
                        match &s.fields {
                            syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                                let field = &fields.unnamed[0];
                                if !matches!(field.vis, syn::Visibility::Inherited) {
                                    self.problems.push(format!(
                                        "{SLICE_VIEW_FILE}: `LispValueSlice` field 0 must stay private"
                                    ));
                                }
                                let wraps_atomic_slice = match &field.ty {
                                    syn::Type::Slice(slice) => {
                                        slice_view_is_exact_atomic_tagged_value_type(&slice.elem)
                                    }
                                    _ => false,
                                };
                                if !wraps_atomic_slice {
                                    self.problems.push(format!(
                                        "{SLICE_VIEW_FILE}: `LispValueSlice` must wrap `[super::header::AtomicTaggedValue]`"
                                    ));
                                }
                            }
                            _ => self.problems.push(format!(
                                "{SLICE_VIEW_FILE}: `LispValueSlice` must be a single-field tuple struct"
                            )),
                        }
                    }
                    "LispValueSliceIter" => {
                        self.saw_iter_struct = true;
                        for field in &s.fields {
                            if !matches!(field.vis, syn::Visibility::Inherited) {
                                let fname = field
                                    .ident
                                    .as_ref()
                                    .map(ToString::to_string)
                                    .unwrap_or_else(|| "<unnamed>".to_owned());
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: `LispValueSliceIter::{fname}` field must stay private"
                                ));
                            }
                        }
                    }
                    other => {
                        self.problems.push(format!(
                            "{SLICE_VIEW_FILE}: unexpected struct `{other}` inside sealed slice_view module (fail closed)"
                        ));
                    }
                }
            }
            syn::Item::Impl(imp) => {
                if imp.unsafety.is_some() {
                    self.problems.push(format!(
                        "{SLICE_VIEW_FILE}: `unsafe impl` is forbidden in slice_view.rs"
                    ));
                }
                if imp
                    .generics
                    .params
                    .iter()
                    .any(|p| matches!(p, syn::GenericParam::Type(_)))
                    || imp.generics.where_clause.is_some()
                {
                    self.problems.push(format!(
                        "{SLICE_VIEW_FILE}: `impl` block has disallowed type generics or where clause (fail closed)"
                    ));
                }
                let target_name = slice_view_self_type_name(&imp.self_ty);
                if let Some((trait_path, _)) = &imp.trait_ {
                    let display_trait = slice_view_display_path(trait_path);
                    let canonical_trait = slice_view_canonical_trait_path(trait_path);
                    match target_name.as_deref() {
                        Some("LispValueSlice") => {
                            let allowed = canonical_trait
                                .as_deref()
                                .is_some_and(|p| SLICE_VIEW_ALLOWED_SLICE_TRAITS.contains(&p));
                            if !allowed {
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: `LispValueSlice` must not implement `{display_trait}`"
                                ));
                            }
                            if canonical_trait.as_deref() == Some("::core::ops::Index") {
                                let output_ok = imp.items.iter().any(|item| match item {
                                    syn::ImplItem::Type(assoc) if assoc.ident == "Output" => {
                                        slice_view_is_exact_index_output_type(&assoc.ty)
                                    }
                                    _ => false,
                                });
                                if !output_ok {
                                    self.problems.push(format!(
                                        "{SLICE_VIEW_FILE}: `::core::ops::Index` impl on `LispValueSlice` must have `type Output = Self` or `LispValueSlice`"
                                    ));
                                }
                            }
                        }
                        Some("LispValueSliceIter") => {
                            let allowed = canonical_trait
                                .as_deref()
                                .is_some_and(|p| SLICE_VIEW_ALLOWED_ITER_TRAITS.contains(&p));
                            if !allowed {
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: `LispValueSliceIter` must not implement `{display_trait}`"
                                ));
                            }
                        }
                        _ => {
                            if canonical_trait.as_deref() != Some(SLICE_VIEW_ALLOWED_FOREIGN_TRAIT)
                                || !slice_view_is_allowed_foreign_eq_target(&imp.self_ty)
                            {
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: unexpected trait impl `{display_trait}` on foreign type in slice_view.rs (fail closed)"
                                ));
                            }
                        }
                    }
                } else if !matches!(
                    target_name.as_deref(),
                    Some("LispValueSlice" | "LispValueSliceIter")
                ) {
                    self.problems.push(format!(
                        "{SLICE_VIEW_FILE}: unexpected inherent impl target in slice_view.rs (fail closed)"
                    ));
                }

                let canonical_trait = imp
                    .trait_
                    .as_ref()
                    .and_then(|(trait_path, _)| slice_view_canonical_trait_path(trait_path));

                for impl_item in &imp.items {
                    match impl_item {
                        syn::ImplItem::Fn(func) => {
                            if matches!(func.sig.safety, syn::Safety::Unsafe(_))
                                && !(target_name.as_deref() == Some("LispValueSlice")
                                    && func.sig.ident == "from_raw_atomic_parts")
                            {
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: unexpected `unsafe fn {}` in slice_view.rs",
                                    func.sig.ident
                                ));
                            }
                            slice_view_check_fn_signature(
                                &func.sig,
                                &func.vis,
                                target_name.as_deref(),
                                canonical_trait.as_deref(),
                                &mut self.problems,
                            );
                        }
                        syn::ImplItem::Type(assoc) => {
                            let assoc_ok = match (
                                canonical_trait.as_deref(),
                                assoc.ident.to_string().as_str(),
                            ) {
                                (Some("::core::ops::Index"), "Output") => {
                                    slice_view_is_exact_index_output_type(&assoc.ty)
                                }
                                (
                                    Some("::core::iter::Iterator" | "::core::iter::IntoIterator"),
                                    "Item",
                                ) => slice_view_is_exact_tagged_value_type(&assoc.ty),
                                (Some("::core::iter::IntoIterator"), "IntoIter") => {
                                    slice_view_is_exact_iter_type(&assoc.ty)
                                }
                                _ => false,
                            };
                            if !assoc_ok {
                                self.problems.push(format!(
                                    "{SLICE_VIEW_FILE}: disallowed associated type `{}` in impl (fail closed)",
                                    assoc.ident
                                ));
                            }
                        }
                        syn::ImplItem::Macro(_) => {
                            // Caught unconditionally by `visit_macro`.
                        }
                        _ => {
                            self.problems.push(format!(
                                "{SLICE_VIEW_FILE}: unanalysable `ImplItem` in slice_view.rs (fail closed)"
                            ));
                        }
                    }
                }
            }
            syn::Item::Use(_) => {
                // Caught unconditionally by `visit_item_use`.
            }
            syn::Item::Macro(_) => {
                // Caught unconditionally by `visit_macro`.
            }
            syn::Item::Mod(_) => {
                // Caught unconditionally by `visit_item_mod`.
            }
            other => {
                self.problems.push(format!(
                    "{SLICE_VIEW_FILE}: unanalysable top-level `{}` item in slice_view.rs (fail closed)",
                    slice_view_item_kind(other)
                ));
            }
        }
    }
}

impl<'ast> Visit<'ast> for SliceViewUnsafeBudgetVisitor {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if !slice_view_attribute_is_allowed(attr) {
            let style = match attr.style {
                syn::AttrStyle::Inner(_) => "inner",
                syn::AttrStyle::Outer => "outer",
            };
            let prefix = match attr.style {
                syn::AttrStyle::Inner(_) => "#!",
                syn::AttrStyle::Outer => "#",
            };
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: disallowed {style} attribute `{prefix}[{}]` (fail closed)",
                slice_view_display_path(attr.path())
            ));
        }
        visit::visit_attribute(self, attr);
    }

    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
        if let syn::Stmt::Item(item) = node {
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: nested item `{}` inside function/block body is forbidden (fail closed)",
                slice_view_item_kind(item)
            ));
        }
        visit::visit_stmt(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.problems.push(format!(
            "{SLICE_VIEW_FILE}: `use` declarations are forbidden in slice_view.rs (fail closed)"
        ));
        visit::visit_item_use(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.problems.push(format!(
            "{SLICE_VIEW_FILE}: nested/external `mod {}` is forbidden in slice_view.rs (fail closed)",
            node.ident
        ));
        visit::visit_item_mod(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.impl_target_stack.push(
            slice_view_self_type_name(&node.self_ty).unwrap_or_else(|| "<foreign>".to_owned()),
        );
        visit::visit_item_impl(self, node);
        self.impl_target_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.owner_stack.push(node.sig.ident.to_string());
        visit::visit_impl_item_fn(self, node);
        self.owner_stack.pop();
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.owner_stack.push(format!("fn {}", node.sig.ident));
        visit::visit_item_fn(self, node);
        self.owner_stack.pop();
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.owner_stack.push(format!("const {}", node.ident));
        visit::visit_item_const(self, node);
        self.owner_stack.pop();
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.owner_stack.push(format!("static {}", node.ident));
        visit::visit_item_static(self, node);
        self.owner_stack.pop();
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.owner_stack.push("<closure>".to_owned());
        visit::visit_expr_closure(self, node);
        self.owner_stack.pop();
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.unsafe_locations.push((
            self.impl_target_stack.last().cloned(),
            self.owner_stack.last().cloned(),
        ));
        visit::visit_expr_unsafe(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.problems.push(format!(
            "{SLICE_VIEW_FILE}: macro `{}!` is forbidden in slice_view.rs (fail closed)",
            slice_view_display_path(&node.path)
        ));
        visit::visit_macro(self, node);
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        if matches!(node, syn::Expr::Verbatim(_)) {
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: unanalysable `Expr::Verbatim` in slice_view.rs (fail closed)"
            ));
        }
        visit::visit_expr(self, node);
    }

    fn visit_type(&mut self, node: &'ast syn::Type) {
        if matches!(node, syn::Type::Verbatim(_)) {
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: unanalysable `Type::Verbatim` in slice_view.rs (fail closed)"
            ));
        }
        visit::visit_type(self, node);
    }

    fn visit_pat(&mut self, node: &'ast syn::Pat) {
        if matches!(node, syn::Pat::Verbatim(_)) {
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: unanalysable `Pat::Verbatim` in slice_view.rs (fail closed)"
            ));
        }
        visit::visit_pat(self, node);
    }

    fn visit_type_param_bound(&mut self, node: &'ast syn::TypeParamBound) {
        if matches!(node, syn::TypeParamBound::Verbatim(_)) {
            self.problems.push(format!(
                "{SLICE_VIEW_FILE}: unanalysable `TypeParamBound::Verbatim` in slice_view.rs (fail closed)"
            ));
        }
        visit::visit_type_param_bound(self, node);
    }
}

fn slice_view_unsafe_budget_problems(source: &str) -> Vec<String> {
    let parsed = syn::parse_file(source)
        .unwrap_or_else(|error| panic!("parse {SLICE_VIEW_FILE} for unsafe-budget fence: {error}"));
    let mut visitor = SliceViewUnsafeBudgetVisitor {
        saw_slice_struct: false,
        saw_iter_struct: false,
        impl_target_stack: Vec::new(),
        owner_stack: Vec::new(),
        unsafe_locations: Vec::new(),
        problems: Vec::new(),
    };
    for item in &parsed.items {
        visitor.check_top_level_item(item);
    }
    visitor.visit_file(&parsed);

    if !visitor.saw_slice_struct {
        visitor.problems.push(format!(
            "{SLICE_VIEW_FILE}: missing `struct LispValueSlice`"
        ));
    }
    if !visitor.saw_iter_struct {
        visitor.problems.push(format!(
            "{SLICE_VIEW_FILE}: missing `struct LispValueSliceIter`"
        ));
    }

    let mut from_atomic_count = 0usize;
    let mut from_raw_count = 0usize;
    for (impl_target, owner_name) in &visitor.unsafe_locations {
        match (impl_target.as_deref(), owner_name.as_deref()) {
            (Some("LispValueSlice"), Some("from_atomic_slice")) => from_atomic_count += 1,
            (Some("LispValueSlice"), Some("from_raw_atomic_parts")) => from_raw_count += 1,
            _ => visitor.problems.push(format!(
                "{SLICE_VIEW_FILE}: unexpected `unsafe` block in {}::{} (budget is strictly 2 constructor blocks: from_atomic_slice and from_raw_atomic_parts)",
                impl_target.as_deref().unwrap_or("<top-level>"),
                owner_name.as_deref().unwrap_or("<non-fn>")
            )),
        }
    }
    if from_atomic_count != 1 || from_raw_count != 1 || visitor.unsafe_locations.len() != 2 {
        visitor.problems.push(format!(
            "{SLICE_VIEW_FILE}: unsafe-block budget mismatch (expected 1 in from_atomic_slice + 1 in from_raw_atomic_parts = 2 total; found {from_atomic_count} + {from_raw_count}, total {})",
            visitor.unsafe_locations.len()
        ));
    }

    visitor.problems
}

/// Moved out of `slice_view.rs` so the sealed module contains zero macros.
#[test]
fn slice_view_types_are_not_send_or_sync() {
    static_assertions::assert_not_impl_any!(crate::tagged::header::LispValueSlice: Send, Sync);
    static_assertions::assert_not_impl_any!(
        crate::tagged::header::LispValueSliceIter<'static>: Send,
        Sync
    );
}

/// Unsafe-budget fence: audits only neovm-value/src/tagged/slice_view.rs; unsafe transmutes written in other modules are governed by workspace unsafe review and Miri SB+TB, not by this fence.
#[test]
fn slice_view_unsafe_budget_fence() {
    let slice_view = architecture_production_source_roots::production_source_files()
        .into_iter()
        .find(|file| file.workspace_relative() == SLICE_VIEW_FILE)
        .unwrap_or_else(|| panic!("locate {SLICE_VIEW_FILE} in production source roots"));
    let source = std::fs::read_to_string(&slice_view.path)
        .unwrap_or_else(|error| panic!("read {}: {error}", slice_view.path.display()));
    let problems = slice_view_unsafe_budget_problems(&source);
    assert!(
        problems.is_empty(),
        "slice_view.rs unsafe-budget fence violated:\n{}",
        problems.join("\n")
    );
}

#[test]
#[should_panic(expected = "for unsafe-budget fence")]
fn slice_view_unsafe_budget_fence_panics_on_unparseable_source() {
    let _ = slice_view_unsafe_budget_problems("pub struct LispValueSlice([");
}

#[test]
fn slice_view_unsafe_budget_fence_rejects_violations() {
    // Direct unit check on the closed attribute allowlist helper.
    let parsed_allowed = syn::parse_file(
        r#"
        //! module doc
        /// item doc
        #[repr(transparent)]
        #[derive(::core::clone::Clone, ::core::fmt::Debug)]
        #[inline(always)]
        #[allow(clippy::should_implement_trait)]
        struct Ok;
        "#,
    )
    .expect("parse allowed attrs");
    for attr in &parsed_allowed.attrs {
        assert!(
            slice_view_attribute_is_allowed(attr),
            "expected inner doc attr allowed"
        );
    }
    let syn::Item::Struct(ok_struct) = &parsed_allowed.items[0] else {
        panic!("expected struct");
    };
    for attr in &ok_struct.attrs {
        assert!(
            slice_view_attribute_is_allowed(attr),
            "expected outer attr `{}` allowed",
            slice_view_display_path(attr.path())
        );
    }
    let parsed_disallowed = syn::parse_file(
        r#"
        #![cfg(unix)]
        #[cfg(test)]
        #[cfg_attr(all(), path = "../other.rs")]
        #[visibility::make(pub)]
        #[allow(unsafe_op_in_unsafe_fn)]
        #[derive(Debug)]
        #[derive(EvilDerive)]
        #[doc(hidden)]
        struct Bad;
        "#,
    )
    .expect("parse disallowed attrs");
    assert!(
        !slice_view_attribute_is_allowed(&parsed_disallowed.attrs[0]),
        "inner #![cfg(unix)] must be rejected"
    );
    let syn::Item::Struct(bad_struct) = &parsed_disallowed.items[0] else {
        panic!("expected struct");
    };
    for attr in &bad_struct.attrs {
        assert!(
            !slice_view_attribute_is_allowed(attr),
            "expected outer attr `{}` rejected",
            slice_view_display_path(attr.path())
        );
    }

    let valid_base = r#"
        //! Sealed slice view doc.

        #[repr(transparent)]
        pub struct LispValueSlice([super::header::AtomicTaggedValue]);

        #[derive(::core::clone::Clone, ::core::fmt::Debug)]
        pub struct LispValueSliceIter<'a> {
            inner: ::core::slice::Iter<'a, super::header::AtomicTaggedValue>,
        }

        impl LispValueSlice {
            #[inline(always)]
            pub(crate) fn from_atomic_slice(slice: &[super::header::AtomicTaggedValue]) -> &Self {
                unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }
            }
            #[inline(always)]
            pub(crate) unsafe fn from_raw_atomic_parts<'a>(ptr: *const super::header::AtomicTaggedValue, len: usize) -> &'a Self {
                unsafe { &*(::core::ptr::slice_from_raw_parts(ptr, len) as *const Self) }
            }
            #[allow(clippy::should_implement_trait)]
            pub fn clone(&self) -> ::std::vec::Vec<super::value::TaggedValue> { ::std::vec::Vec::new() }
        }
    "#;
    assert!(
        slice_view_unsafe_budget_problems(valid_base).is_empty(),
        "valid baseline must pass unsafe-budget fence: {:?}",
        slice_view_unsafe_budget_problems(valid_base)
    );

    // 1. Third `unsafe` block: method, top-level const, top-level static, closure, constructor, top-level fn.
    let third_unsafe_fn = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn leak(&self) -> &[super::value::TaggedValue] {{\n\
                 unsafe {{ &*(&self.0 as *const [super::header::AtomicTaggedValue] as *const [super::value::TaggedValue]) }}\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&third_unsafe_fn).join("\n");
    assert!(
        p.contains("unexpected `unsafe` block in LispValueSlice::leak"),
        "third unsafe block in method must fail:\n{p}"
    );
    let third_unsafe_const = format!("{valid_base}\nconst LEAK_CONST: usize = unsafe {{ 0 }};");
    let p = slice_view_unsafe_budget_problems(&third_unsafe_const).join("\n");
    assert!(
        p.contains("unexpected `unsafe` block in <top-level>::const LEAK_CONST"),
        "third unsafe block in const must fail:\n{p}"
    );
    let third_unsafe_static = format!("{valid_base}\nstatic LEAK_STATIC: usize = unsafe {{ 0 }};");
    let p = slice_view_unsafe_budget_problems(&third_unsafe_static).join("\n");
    assert!(
        p.contains("unexpected `unsafe` block in <top-level>::static LEAK_STATIC"),
        "third unsafe block in static must fail:\n{p}"
    );
    let third_unsafe_closure = valid_base.replace(
        "unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }",
        "let _f = || unsafe { 0usize }; unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }",
    );
    let p = slice_view_unsafe_budget_problems(&third_unsafe_closure).join("\n");
    assert!(
        p.contains("unexpected `unsafe` block in LispValueSlice::<closure>"),
        "third unsafe block in closure inside constructor must fail:\n{p}"
    );
    let third_unsafe_same_ctor = valid_base.replace(
        "unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }",
        "let _ = unsafe { 0usize }; unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }",
    );
    let p = slice_view_unsafe_budget_problems(&third_unsafe_same_ctor).join("\n");
    assert!(
        p.contains("unsafe-block budget mismatch") && p.contains("total 3"),
        "second unsafe block inside permitted constructor must fail budget:\n{p}"
    );
    let third_unsafe_top_fn = format!("{valid_base}\nfn top_leak() -> usize {{ unsafe {{ 0 }} }}");
    let p = slice_view_unsafe_budget_problems(&third_unsafe_top_fn).join("\n");
    assert!(
        p.contains("unexpected `unsafe` block in <top-level>::fn top_leak"),
        "third unsafe block in top-level fn must fail:\n{p}"
    );
    let top_level_mod = format!("{valid_base}\nmod smuggled;");
    let p = slice_view_unsafe_budget_problems(&top_level_mod).join("\n");
    assert!(
        p.contains("nested/external `mod smuggled` is forbidden"),
        "top-level mod must fail visit_item_mod:\n{p}"
    );
    let extra_unsafe_fn = format!(
        "{valid_base}\nimpl LispValueSlice {{ pub unsafe fn extra(&self) -> usize {{ 0 }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&extra_unsafe_fn).join("\n");
    assert!(
        p.contains("unexpected `unsafe fn extra` in slice_view.rs"),
        "unexpected unsafe fn must fail:\n{p}"
    );
    let unsafe_impl =
        format!("{valid_base}\nunsafe impl ::core::marker::Send for LispValueSlice {{}}");
    let p = slice_view_unsafe_budget_problems(&unsafe_impl).join("\n");
    assert!(
        p.contains("`unsafe impl` is forbidden in slice_view.rs"),
        "unsafe impl must fail:\n{p}"
    );

    // 2. Public fields on `LispValueSlice` and `LispValueSliceIter`.
    let pub_slice_field = valid_base.replace(
        "pub struct LispValueSlice([super::header::AtomicTaggedValue]);",
        "pub struct LispValueSlice(pub [super::header::AtomicTaggedValue]);",
    );
    let p = slice_view_unsafe_budget_problems(&pub_slice_field).join("\n");
    assert!(
        p.contains("`LispValueSlice` field 0 must stay private"),
        "pub field on LispValueSlice must fail:\n{p}"
    );
    let pub_iter_field = valid_base.replace(
        "inner: ::core::slice::Iter<'a, super::header::AtomicTaggedValue>,",
        "pub inner: ::core::slice::Iter<'a, super::header::AtomicTaggedValue>,",
    );
    let p = slice_view_unsafe_budget_problems(&pub_iter_field).join("\n");
    assert!(
        p.contains("`LispValueSliceIter::inner` field must stay private"),
        "pub field on LispValueSliceIter must fail:\n{p}"
    );

    // 3. Nested `Stmt::Item` inside method bodies (#14924 P0 Bypasses 1, 2, 3).
    let nested_macro_impl = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn len(&self) -> usize {{\n\
                 macro_rules! emit_leak {{\n\
                     () => {{ pub fn leak(&self) -> &[super::value::TaggedValue] {{ unsafe {{ &[] }} }} }};\n\
                 }}\n\
                 impl LispValueSlice {{ emit_leak!(); }}\n\
                 self.0.len()\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&nested_macro_impl).join("\n");
    assert!(
        p.contains("nested item `macro` inside function/block body is forbidden")
            && p.contains("nested item `impl` inside function/block body is forbidden")
            && p.contains("macro `emit_leak!` is forbidden"),
        "nested macro_rules! + impl inside fn body must fail:\n{p}"
    );
    let nested_path_mod = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn len(&self) -> usize {{\n\
                 #[path = \"smuggled.rs\"]\n\
                 mod smuggled;\n\
                 self.0.len()\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&nested_path_mod).join("\n");
    assert!(
        p.contains("nested item `mod` inside function/block body is forbidden")
            && p.contains("nested/external `mod smuggled` is forbidden")
            && p.contains("disallowed outer attribute `#[path]`"),
        "nested #[path] mod inside fn body must fail:\n{p}"
    );
    let nested_deref_impl = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn len(&self) -> usize {{\n\
                 impl ::core::ops::Deref for LispValueSlice {{\n\
                     type Target = [super::header::AtomicTaggedValue];\n\
                     fn deref(&self) -> &[super::header::AtomicTaggedValue] {{ &self.0 }}\n\
                 }}\n\
                 self.0.len()\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&nested_deref_impl).join("\n");
    assert!(
        p.contains("nested item `impl` inside function/block body is forbidden"),
        "nested Deref impl inside fn body must fail:\n{p}"
    );

    // 4. Zero-macro ban in every AST position — including #14954 P0 Bypass 1 verbatim.
    let macro_const_unsafe_bypass = format!(
        "{valid_base}\n\
         static_assertions::assert_not_impl_any!((*const LispValueSlice, [u8; {{ unsafe {{ 1 }} }}]): Send, Sync);"
    );
    let p = slice_view_unsafe_budget_problems(&macro_const_unsafe_bypass).join("\n");
    assert!(
        p.contains("macro `static_assertions::assert_not_impl_any!` is forbidden"),
        "#14954 P0 macro-token unsafe bypass must fail:\n{p}"
    );
    let plain_static_assert = format!(
        "{valid_base}\n\
         static_assertions::assert_not_impl_any!(LispValueSlice: Send, Sync);"
    );
    let p = slice_view_unsafe_budget_problems(&plain_static_assert).join("\n");
    assert!(
        p.contains("macro `static_assertions::assert_not_impl_any!` is forbidden"),
        "even plain static_assertions macro must fail (zero macros allowed):\n{p}"
    );
    let impl_item_macro = format!("{valid_base}\nimpl LispValueSlice {{ hidden_escape!(); }}");
    let p = slice_view_unsafe_budget_problems(&impl_item_macro).join("\n");
    assert!(
        p.contains("macro `hidden_escape!` is forbidden"),
        "ImplItem::Macro must fail:\n{p}"
    );
    let stmt_macro = format!(
        "{valid_base}\nimpl LispValueSlice {{ pub fn sneaky(&self) -> usize {{ hidden_stmt!(); 0 }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&stmt_macro).join("\n");
    assert!(
        p.contains("macro `hidden_stmt!` is forbidden"),
        "Stmt::Macro must fail:\n{p}"
    );
    let expr_macro = format!(
        "{valid_base}\nimpl LispValueSlice {{ pub fn em(&self) -> usize {{ hidden_expr!() }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&expr_macro).join("\n");
    assert!(
        p.contains("macro `hidden_expr!` is forbidden"),
        "Expr::Macro must fail:\n{p}"
    );
    let type_macro = format!(
        "{valid_base}\nimpl LispValueSlice {{ pub fn tm(&self) -> imported_type_macro!() {{ loop {{}} }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&type_macro).join("\n");
    assert!(
        p.contains("macro `imported_type_macro!` is forbidden"),
        "Type::Macro must fail:\n{p}"
    );
    let pat_macro = format!(
        "{valid_base}\nimpl LispValueSlice {{ pub fn pm(&self) {{ let imported_pat_macro!() = 0; }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&pat_macro).join("\n");
    assert!(
        p.contains("macro `imported_pat_macro!` is forbidden"),
        "Pat::Macro must fail:\n{p}"
    );
    let with_include = format!("{valid_base}\ninclude!(\"extra.rs\");");
    let p = slice_view_unsafe_budget_problems(&with_include).join("\n");
    assert!(
        p.contains("macro `include!` is forbidden"),
        "include! must fail:\n{p}"
    );

    // 5. Zero-`use` ban — including #14954 P1 Bypass 2 verbatim and collateral shapes.
    let use_self_shadow_bypass = format!(
        "{valid_base}\n\
         use super::evil::static_assertions::{{self}};\n\
         static_assertions::assert_not_impl_any!(LispValueSlice: Send, Sync);"
    );
    let p = slice_view_unsafe_budget_problems(&use_self_shadow_bypass).join("\n");
    assert!(
        p.contains("`use` declarations are forbidden in slice_view.rs")
            && p.contains("macro `static_assertions::assert_not_impl_any!` is forbidden"),
        "#14954 P1 use-self shadow bypass must fail both use and macro checks:\n{p}"
    );
    let use_debug_shadow = format!("{valid_base}\nuse some_proc_macro_crate::Debug;");
    let p = slice_view_unsafe_budget_problems(&use_debug_shadow).join("\n");
    assert!(
        p.contains("`use` declarations are forbidden in slice_view.rs"),
        "use shadowing Debug must fail:\n{p}"
    );
    let pub_use_leak = format!("{valid_base}\npub use super::header::AtomicTaggedValue;");
    let p = slice_view_unsafe_budget_problems(&pub_use_leak).join("\n");
    assert!(
        p.contains("`use` declarations are forbidden in slice_view.rs"),
        "pub use must fail:\n{p}"
    );
    let plain_use = format!("{valid_base}\nuse super::header::AtomicTaggedValue;");
    let p = slice_view_unsafe_budget_problems(&plain_use).join("\n");
    assert!(
        p.contains("`use` declarations are forbidden in slice_view.rs"),
        "plain use must fail (zero use declarations allowed):\n{p}"
    );
    let glob_use = format!("{valid_base}\nuse other::*;");
    let p = slice_view_unsafe_budget_problems(&glob_use).join("\n");
    assert!(
        p.contains("`use` declarations are forbidden in slice_view.rs"),
        "glob use must fail:\n{p}"
    );

    // 6. Disallowed attributes on otherwise-valid items (directly exercises `visit_attribute`).
    let attr_cfg_attr = format!(
        "{valid_base}\nimpl LispValueSlice {{ #[cfg_attr(all(), inline)] pub fn ok(&self) -> usize {{ 0 }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&attr_cfg_attr).join("\n");
    assert!(
        p.contains("disallowed outer attribute `#[cfg_attr]`"),
        "#[cfg_attr] on valid fn must fail attribute check:\n{p}"
    );
    let attr_visibility = format!(
        "{valid_base}\nimpl LispValueSlice {{ #[visibility::make(pub)] fn secret(&self) -> usize {{ 0 }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&attr_visibility).join("\n");
    assert!(
        p.contains("disallowed outer attribute `#[visibility::make]`"),
        "#[visibility::make(pub)] must fail attribute check:\n{p}"
    );
    let attr_inner_cfg = format!("#![cfg(unix)]\n{valid_base}");
    let p = slice_view_unsafe_budget_problems(&attr_inner_cfg).join("\n");
    assert!(
        p.contains("disallowed inner attribute `#![cfg]`"),
        "inner #![cfg(unix)] must fail attribute check:\n{p}"
    );
    let attr_unsafe_allow = format!(
        "{valid_base}\nimpl LispValueSlice {{ #[allow(unsafe_op_in_unsafe_fn)] pub fn ok2(&self) -> usize {{ 0 }} }}"
    );
    let p = slice_view_unsafe_budget_problems(&attr_unsafe_allow).join("\n");
    assert!(
        p.contains("disallowed outer attribute `#[allow]`"),
        "#[allow(unsafe_op_in_unsafe_fn)] must fail attribute check:\n{p}"
    );
    let attr_bare_derive = valid_base.replace(
        "#[derive(::core::clone::Clone, ::core::fmt::Debug)]",
        "#[derive(Clone, Debug)]",
    );
    let p = slice_view_unsafe_budget_problems(&attr_bare_derive).join("\n");
    assert!(
        p.contains("disallowed outer attribute `#[derive]`"),
        "bare #[derive(Clone, Debug)] without ::core:: prefix must fail:\n{p}"
    );

    // 7. Full-path trait allowlist — including #14954 P1 Bypass 3 verbatim, bare trait paths,
    //    banned traits, and tight `Index::Output`.
    let aliased_deref_as_partial_eq = format!(
        "{valid_base}\n\
         impl super::helper::PartialEq for LispValueSlice {{\n\
             type Target = [super::header::AtomicTaggedValue];\n\
             fn deref(&self) -> &[super::header::AtomicTaggedValue] {{\n\
                 &self.0\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&aliased_deref_as_partial_eq).join("\n");
    assert!(
        p.contains("`LispValueSlice` must not implement `super::helper::PartialEq`"),
        "#14954 P1 re-exported PartialEq->Deref bypass must fail full-path trait check:\n{p}"
    );
    let bare_partial_eq = format!(
        "{valid_base}\n\
         impl PartialEq for LispValueSlice {{\n\
             fn eq(&self, _other: &Self) -> bool {{ true }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&bare_partial_eq).join("\n");
    assert!(
        p.contains("`LispValueSlice` must not implement `PartialEq`"),
        "unqualified bare PartialEq trait path must fail:\n{p}"
    );
    let with_deref = format!(
        "{valid_base}\n\
         impl ::core::ops::Deref for LispValueSlice {{\n\
             type Target = [super::value::TaggedValue];\n\
             fn deref(&self) -> &[super::value::TaggedValue] {{ &[] }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&with_deref).join("\n");
    assert!(
        p.contains("`LispValueSlice` must not implement `::core::ops::Deref`"),
        "Deref on LispValueSlice must fail:\n{p}"
    );
    let with_iter_as_ref = format!(
        "{valid_base}\n\
         impl ::core::convert::AsRef<[super::value::TaggedValue]> for LispValueSliceIter<'_> {{\n\
             fn as_ref(&self) -> &[super::value::TaggedValue] {{ &[] }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&with_iter_as_ref).join("\n");
    assert!(
        p.contains("`LispValueSliceIter` must not implement `::core::convert::AsRef`"),
        "AsRef on LispValueSliceIter must fail:\n{p}"
    );
    let bad_index_ref_output = format!(
        "{valid_base}\n\
         impl ::core::ops::Index<usize> for LispValueSlice {{\n\
             type Output = &LispValueSlice;\n\
             fn index(&self, _index: usize) -> &Self::Output {{ loop {{}} }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&bad_index_ref_output).join("\n");
    assert!(
        p.contains("`::core::ops::Index` impl on `LispValueSlice` must have `type Output = Self` or `LispValueSlice`"),
        "Index with reference Output must fail:\n{p}"
    );
    let bad_index_path_output = format!(
        "{valid_base}\n\
         impl ::core::ops::Index<usize> for LispValueSlice {{\n\
             type Output = super::evil::LispValueSlice;\n\
             fn index(&self, _index: usize) -> &Self::Output {{ loop {{}} }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&bad_index_path_output).join("\n");
    assert!(
        p.contains("`::core::ops::Index` impl on `LispValueSlice` must have `type Output = Self` or `LispValueSlice`"),
        "Index with multi-segment Output must fail:\n{p}"
    );

    // 8. Closed function signature & associated-type allowlist (#14958 Rule 5).
    let safe_leak_atomic_slice = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn leak(&self) -> &[super::header::AtomicTaggedValue] {{\n\
                 &self.0\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_atomic_slice).join("\n");
    assert!(
        p.contains("`fn leak` has disallowed return type"),
        "#14958 safe leak(&self) -> &[AtomicTaggedValue] must fail signature check:\n{p}"
    );
    let safe_leak_atomic_elem = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn leak_one(&self) -> &super::header::AtomicTaggedValue {{\n\
                 &self.0[0]\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_atomic_elem).join("\n");
    assert!(
        p.contains("`fn leak_one` has disallowed return type"),
        "#14958 safe leak_one(&self) -> &AtomicTaggedValue must fail signature check:\n{p}"
    );
    let safe_leak_impl_trait = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn leak_iter(&self) -> impl ::core::iter::Iterator<Item = &super::header::AtomicTaggedValue> {{\n\
                 self.0.iter()\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_impl_trait).join("\n");
    assert!(
        p.contains("`fn leak_iter` has disallowed return type"),
        "impl Iterator<Item = &AtomicTaggedValue> return must fail signature check:\n{p}"
    );
    let safe_leak_unknown_ret = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn odd(&self) -> &dyn ::core::fmt::Debug {{\n\
                 self\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_unknown_ret).join("\n");
    assert!(
        p.contains("`fn odd` has disallowed return type"),
        "unallowlisted dyn Debug return must fail signature check:\n{p}"
    );
    let safe_leak_callback_param = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn with_raw<F>(&self, f: F) {{\n\
                 let _ = f;\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_callback_param).join("\n");
    assert!(
        p.contains("`fn with_raw` has disallowed type/const generics")
            && p.contains("`fn with_raw` parameter 1 has disallowed type"),
        "generic callback parameter must fail signature check:\n{p}"
    );
    let safe_leak_out_param = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn smuggle_out(&self, out: &mut &[super::header::AtomicTaggedValue]) {{\n\
                 *out = &self.0;\n\
             }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&safe_leak_out_param).join("\n");
    assert!(
        p.contains("`fn smuggle_out` parameter 1 has disallowed type"),
        "out-parameter leaking &[AtomicTaggedValue] must fail signature check:\n{p}"
    );
    let pub_ctor_widening = valid_base.replace(
        "pub(crate) fn from_atomic_slice",
        "pub fn from_atomic_slice",
    );
    let p = slice_view_unsafe_budget_problems(&pub_ctor_widening).join("\n");
    assert!(
        p.contains("constructor `LispValueSlice::from_atomic_slice` must stay `pub(crate)`"),
        "widening from_atomic_slice to pub must fail signature check:\n{p}"
    );
    let bad_assoc_type = format!(
        "{valid_base}\n\
         impl ::core::iter::Iterator for LispValueSliceIter<'_> {{\n\
             type Item = &super::header::AtomicTaggedValue;\n\
             fn next(&mut self) -> ::core::option::Option<Self::Item> {{ ::core::option::Option::None }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&bad_assoc_type).join("\n");
    assert!(
        p.contains("disallowed associated type `Item` in impl"),
        "Iterator with Item = &AtomicTaggedValue must fail associated-type check:\n{p}"
    );
    let unparseable_signature = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub fn unparseable(&self) -> &[ {{\n\
                 &self.0\n\
             }}\n\
         }}"
    );
    assert!(
        std::panic::catch_unwind(|| slice_view_unsafe_budget_problems(&unparseable_signature))
            .is_err(),
        "#14958 unparseable fn signature must fail closed (panic)"
    );
    let foreign_leak_cell_eq = format!(
        "{valid_base}\n\
         impl ::core::cmp::PartialEq<LispValueSlice> for super::evil::LeakCell {{\n\
             fn eq(&self, _other: &LispValueSlice) -> bool {{ true }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&foreign_leak_cell_eq).join("\n");
    assert!(
        p.contains("unexpected trait impl `::core::cmp::PartialEq` on foreign type"),
        "PartialEq on unallowlisted foreign type must fail:\n{p}"
    );
    let async_fn = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub async fn sneaky_async(&self) -> usize {{ self.0.len() }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&async_fn).join("\n");
    assert!(
        p.contains("`async fn sneaky_async` is forbidden"),
        "async fn in slice_view.rs must fail:\n{p}"
    );
    let extern_fn = format!(
        "{valid_base}\n\
         impl LispValueSlice {{\n\
             pub extern \"C\" fn sneaky_abi(&self) -> usize {{ self.0.len() }}\n\
         }}"
    );
    let p = slice_view_unsafe_budget_problems(&extern_fn).join("\n");
    assert!(
        p.contains("`extern fn sneaky_abi` is forbidden"),
        "extern fn in slice_view.rs must fail:\n{p}"
    );

    // 9. Verbatim fallback syntax (`Expr::Verbatim`, etc.) fails closed.
    let mut verbatim_visitor = SliceViewUnsafeBudgetVisitor {
        saw_slice_struct: true,
        saw_iter_struct: true,
        impl_target_stack: Vec::new(),
        owner_stack: Vec::new(),
        unsafe_locations: Vec::new(),
        problems: Vec::new(),
    };
    verbatim_visitor.visit_expr(&syn::Expr::Verbatim(proc_macro2::TokenStream::new()));
    verbatim_visitor.visit_type(&syn::Type::Verbatim(proc_macro2::TokenStream::new()));
    verbatim_visitor.visit_pat(&syn::Pat::Verbatim(proc_macro2::TokenStream::new()));
    verbatim_visitor.visit_type_param_bound(&syn::TypeParamBound::Verbatim(
        proc_macro2::TokenStream::new(),
    ));
    let p = verbatim_visitor.problems.join("\n");
    assert!(
        p.contains("Expr::Verbatim")
            && p.contains("Type::Verbatim")
            && p.contains("Pat::Verbatim")
            && p.contains("TypeParamBound::Verbatim"),
        "Verbatim AST nodes must fail closed:\n{p}"
    );
}

const C12_STOPPED_WORLD_PROOF_FILE: &str = "neovm-value/src/tagged/gc/stopped_world_proof.rs";

const C12_PROOF_GATED_ENTRIES: &[&str] = &[
    // 9 pub unsafe fn collector-protocol entries:
    "collect",
    "collect_exact",
    "begin_stw_collection",
    "complete_collection",
    "begin_minor_collection",
    "complete_minor_collection",
    "incremental_finish",
    "incremental_sweep_slice",
    "finish_incremental_sweep_now",
    // #859: the first partition cycle's promotion relinks every survivor.
    "finish_first_partition_cycle",
    // 6 internal collector-phase helpers:
    "finalize_collection",
    "begin_collection_with",
    "begin_parallel_minor_collection",
    "finish_incremental_sweep",
    "begin_old_sweep_world_stopped",
    "sweep_old_objects_slice",
];

/// Exact `(file, fn)` allowlist for production serial
/// `StoppedWorldProof::with_stopped_world` call sites (deny-by-default: any
/// other function, even in an allowlisted file, fails the guard).
const C12_ALLOWED_MINT_CALLERS: &[(&str, &str)] = &[
    (
        "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
        "gc_collect_from_current_roots_body",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
        "terminate_concurrent_mark",
    ),
    (
        "neovm-value/src/tagged/gc/facade_mark.rs",
        "drain_to_quiescent",
    ),
    (
        "neovm-value/src/tagged/gc/concurrent.rs",
        "raw_concurrent_begin",
    ),
];

/// Exact `(file, fn)` allowlist for the parallel `with_all_stopped` mint: the
/// only holder of a `ParallelStoppedMutatorsProof` borrowed from `AllStopped`.
const C12_ALLOWED_PARALLEL_MINT_CALLERS: &[(&str, &str)] = &[(
    "neovm-value/src/tagged/gc/mutator_registration/collection.rs",
    "run",
)];

/// Permitted `StoppedWorldProof` constructors: `(name, must_be_unsafe,
/// must_borrow_parallel_proof)`. Neither may take a raw pointer (value-gc C12
/// v2's `*const TaggedHeap` aliased the body's `&mut TaggedHeap`).
const C12_PERMITTED_CONSTRUCTORS: &[(&str, bool, bool)] = &[
    ("StoppedWorldProof::with_stopped_world", true, false),
    ("StoppedWorldProof::with_all_stopped", false, true),
];

fn c12_is_parallel_proof_ref(ty: &Type) -> bool {
    let Type::Reference(r) = ty else {
        return false;
    };
    let Type::Path(p) = &*r.elem else {
        return false;
    };
    p.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "ParallelStoppedMutatorsProof")
}

fn c12_mentions_raw_pointer(ty: &Type) -> bool {
    struct PtrFinder(bool);
    impl<'ast> Visit<'ast> for PtrFinder {
        fn visit_type_ptr(&mut self, node: &'ast syn::TypePtr) {
            self.0 = true;
            visit::visit_type_ptr(self, node);
        }
    }
    let mut finder = PtrFinder(false);
    finder.visit_type(ty);
    finder.0
}

/// `(takes_parallel_proof, takes_raw_pointer)` for a constructor signature.
fn c12_constructor_inputs(sig: &syn::Signature) -> (bool, bool) {
    let typed = sig.inputs.iter().filter_map(|arg| match arg {
        syn::FnArg::Typed(pat) => Some(&*pat.ty),
        syn::FnArg::Receiver(_) => None,
    });
    typed.fold((false, false), |(parallel, raw), ty| {
        (
            parallel || c12_is_parallel_proof_ref(ty),
            raw || c12_mentions_raw_pointer(ty),
        )
    })
}

fn c12_is_stopped_world_proof_ref(ty: &Type) -> bool {
    let Type::Reference(r) = ty else {
        return false;
    };
    let Type::Path(p) = &*r.elem else {
        return false;
    };
    p.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "StoppedWorldProof")
}

fn c12_type_mentions_stopped_world_proof(ty: &Type, self_is_proof: bool) -> bool {
    struct TypeMentionFinder {
        self_is_proof: bool,
        found: bool,
    }
    impl<'ast> Visit<'ast> for TypeMentionFinder {
        fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
            if node.path.segments.iter().any(|seg| {
                seg.ident == "StoppedWorldProof" || (self.self_is_proof && seg.ident == "Self")
            }) {
                self.found = true;
            }
            visit::visit_type_path(self, node);
        }
    }
    let mut finder = TypeMentionFinder {
        self_is_proof,
        found: false,
    };
    finder.visit_type(ty);
    finder.found
}

#[derive(Default)]
struct C12Audit {
    found_entries: std::collections::BTreeMap<String, (String, bool)>,
    constructors: Vec<C12Constructor>,
    problems: Vec<String>,
}

struct C12Constructor {
    file: String,
    name: String,
    is_unsafe: bool,
    takes_parallel_proof: bool,
    takes_raw_pointer: bool,
}

fn c12_audit_file(rel_path: &str, ast: &syn::File, audit: &mut C12Audit) {
    struct C12Visitor<'a> {
        rel_path: &'a str,
        excluded: u32,
        fn_stack: Vec<String>,
        in_proof_impl: bool,
        in_tagged_heap_impl: bool,
        current_fn_constructs_proof: bool,
        audit: &'a mut C12Audit,
    }

    impl<'ast> Visit<'ast> for C12Visitor<'_> {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            let ex = production_excluded(&item.attrs);
            if ex {
                self.excluded += 1;
            }
            visit::visit_item_mod(self, item);
            if ex {
                self.excluded -= 1;
            }
        }

        fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
            let ex = production_excluded(&item.attrs);
            if ex {
                self.excluded += 1;
            }
            let prev_proof = self.in_proof_impl;
            let prev_heap = self.in_tagged_heap_impl;
            if let Type::Path(self_ty) = &*item.self_ty
                && let Some(seg) = self_ty.path.segments.last()
            {
                self.in_proof_impl = seg.ident == "StoppedWorldProof";
                self.in_tagged_heap_impl = item.trait_.is_none() && seg.ident == "TaggedHeap";
            }
            visit::visit_item_impl(self, item);
            self.in_proof_impl = prev_proof;
            self.in_tagged_heap_impl = prev_heap;
            if ex {
                self.excluded -= 1;
            }
        }

        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            let ex = production_excluded(&item.attrs);
            if ex {
                self.excluded += 1;
            }
            let name = item.sig.ident.to_string();
            let prev_constructs = self.current_fn_constructs_proof;
            self.current_fn_constructs_proof = false;
            self.fn_stack.push(name.clone());
            visit::visit_item_fn(self, item);
            self.fn_stack.pop();
            if self.excluded == 0 && self.current_fn_constructs_proof {
                let (takes_parallel_proof, takes_raw_pointer) = c12_constructor_inputs(&item.sig);
                self.audit.constructors.push(C12Constructor {
                    file: self.rel_path.to_owned(),
                    name,
                    is_unsafe: matches!(item.sig.safety, syn::Safety::Unsafe(_)),
                    takes_parallel_proof,
                    takes_raw_pointer,
                });
            }
            self.current_fn_constructs_proof = prev_constructs;
            if ex {
                self.excluded -= 1;
            }
        }

        fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
            let ex = production_excluded(&item.attrs);
            if ex {
                self.excluded += 1;
            }
            let name = item.sig.ident.to_string();
            if self.excluded == 0
                && self.in_tagged_heap_impl
                && C12_PROOF_GATED_ENTRIES.contains(&name.as_str())
            {
                let has_proof = item.sig.inputs.iter().any(|arg| match arg {
                    syn::FnArg::Typed(pat) => c12_is_stopped_world_proof_ref(&pat.ty),
                    syn::FnArg::Receiver(_) => false,
                });
                self.audit
                    .found_entries
                    .insert(name.clone(), (self.rel_path.to_owned(), has_proof));
            }
            if self.excluded == 0 && self.in_tagged_heap_impl && name == "begin_collection" {
                self.audit.problems.push(format!(
                    "{}: TaggedHeap::begin_collection must be test/test-support gated in production",
                    self.rel_path
                ));
            }
            let prev_constructs = self.current_fn_constructs_proof;
            self.current_fn_constructs_proof = false;
            self.fn_stack.push(name.clone());
            visit::visit_impl_item_fn(self, item);
            self.fn_stack.pop();
            if self.excluded == 0 {
                let returns_proof = match &item.sig.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => {
                        c12_type_mentions_stopped_world_proof(ty, self.in_proof_impl)
                    }
                };
                if self.current_fn_constructs_proof || (self.in_proof_impl && returns_proof) {
                    let qualified = if self.in_proof_impl {
                        format!("StoppedWorldProof::{name}")
                    } else {
                        name
                    };
                    let (takes_parallel_proof, takes_raw_pointer) =
                        c12_constructor_inputs(&item.sig);
                    self.audit.constructors.push(C12Constructor {
                        file: self.rel_path.to_owned(),
                        name: qualified,
                        is_unsafe: matches!(item.sig.safety, syn::Safety::Unsafe(_)),
                        takes_parallel_proof,
                        takes_raw_pointer,
                    });
                }
            }
            self.current_fn_constructs_proof = prev_constructs;
            if ex {
                self.excluded -= 1;
            }
        }

        fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
            if self.excluded == 0
                && let Some(last) = expr.path.segments.last()
                && (last.ident == "StoppedWorldProof"
                    || (self.in_proof_impl && last.ident == "Self"))
            {
                self.current_fn_constructs_proof = true;
            }
            visit::visit_expr_struct(self, expr);
        }

        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if self.excluded == 0
                && let Expr::Path(path) = &*call.func
                && let Some(seg) = path.path.segments.last()
                && let Some((mint, allowlist)) = match seg.ident.to_string().as_str() {
                    "with_stopped_world" => Some(("with_stopped_world", C12_ALLOWED_MINT_CALLERS)),
                    "with_all_stopped" => {
                        Some(("with_all_stopped", C12_ALLOWED_PARALLEL_MINT_CALLERS))
                    }
                    _ => None,
                }
            {
                let enclosing_fn = self
                    .fn_stack
                    .last()
                    .map(String::as_str)
                    .unwrap_or("<module>");
                let allowed = allowlist
                    .iter()
                    .any(|&(file, func)| file == self.rel_path && func == enclosing_fn);
                if !allowed {
                    let line = path.path.span().start().line;
                    self.audit.problems.push(format!(
                        "{}:{line}: non-allowlisted production `{mint}` mint in `{enclosing_fn}`",
                        self.rel_path
                    ));
                }
            }
            visit::visit_expr_call(self, call);
        }
    }

    let mut visitor = C12Visitor {
        rel_path,
        excluded: 0,
        fn_stack: Vec::new(),
        in_proof_impl: false,
        in_tagged_heap_impl: false,
        current_fn_constructs_proof: false,
        audit,
    };
    visitor.visit_file(ast);
}

fn c12_finish_audit(audit: &C12Audit) -> Vec<String> {
    let mut problems = audit.problems.clone();
    for &entry in C12_PROOF_GATED_ENTRIES {
        match audit.found_entries.get(entry) {
            None => problems.push(format!("missing TaggedHeap::{entry} definition")),
            Some((file, false)) => problems.push(format!(
                "{file}: TaggedHeap::{entry} is missing a `&StoppedWorldProof` parameter"
            )),
            Some((_, true)) => {}
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for ctor in &audit.constructors {
        let C12Constructor {
            file,
            name,
            is_unsafe,
            takes_parallel_proof,
            takes_raw_pointer,
        } = ctor;
        let permitted = C12_PERMITTED_CONSTRUCTORS
            .iter()
            .find(|(permitted, ..)| file == C12_STOPPED_WORLD_PROOF_FILE && name == permitted);
        let Some(&(_, must_be_unsafe, must_borrow_parallel)) = permitted else {
            let safety = if *is_unsafe { "unsafe fn" } else { "safe fn" };
            problems.push(format!(
                "{file}: unexpected StoppedWorldProof constructor `{name}` ({safety}); only `with_stopped_world` and `with_all_stopped` are permitted"
            ));
            continue;
        };
        seen.insert(name.as_str());
        if *is_unsafe != must_be_unsafe {
            let want = if must_be_unsafe {
                "`unsafe fn`"
            } else {
                "a safe fn"
            };
            problems.push(format!("{file}: {name} must be {want}"));
        }
        if *takes_parallel_proof != must_borrow_parallel {
            let want = if must_borrow_parallel {
                "must"
            } else {
                "must not"
            };
            problems.push(format!(
                "{file}: {name} {want} borrow a `&ParallelStoppedMutatorsProof`"
            ));
        }
        if *takes_raw_pointer {
            problems.push(format!(
                "{file}: {name} must not take a raw pointer; borrow the heap and pass it to `body`"
            ));
        }
    }
    for (name, ..) in C12_PERMITTED_CONSTRUCTORS {
        if !seen.contains(name) {
            problems.push(format!("missing {name} constructor"));
        }
    }
    problems
}

/// value-gc C12: every collector-protocol entry and internal phase helper on
/// `TaggedHeap` must carry a `&StoppedWorldProof` parameter; `StoppedWorldProof`
/// has exactly the two constructors in `C12_PERMITTED_CONSTRUCTORS` (serial
/// `unsafe fn with_stopped_world`, parallel safe `with_all_stopped` borrowing
/// the parallel proof; neither takes a raw pointer; any other constructor is
/// denied); and every production mint sits in its exact `(file, fn)` allowlist.
#[test]
fn c12_collector_protocol_entries_require_stopped_world_proof() {
    let mut audit = C12Audit::default();
    for file in architecture_production_source_roots::production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let content = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let rel_path = display_path(&file);
        let ast = syn::parse_file(&content).expect("production source parses");
        c12_audit_file(&rel_path, &ast, &mut audit);
    }
    let problems = c12_finish_audit(&audit);
    assert!(
        problems.is_empty(),
        "value-gc C12 stopped-world proof contract violated:\n{}",
        problems.join("\n")
    );
}

/// The two permitted constructors, in their real shape; `{serial}`,
/// `{parallel_safety}`, `{parallel}` and `{extra}` are replaced per row to
/// plant one defect at a time.
const C12_CANONICAL_CONSTRUCTORS: &str = r#"
    impl<'stop> StoppedWorldProof<'stop> {
        pub unsafe fn with_stopped_world<R>(
            {serial},
            body: impl FnOnce(&mut TaggedHeap, &StoppedWorldProof<'_>) -> R,
        ) -> R {
            let p = StoppedWorldProof { heap_identity: 1, _scope: PhantomData };
            body(todo!(), &p)
        }
        pub(super) {parallel_safety} fn with_all_stopped<R>(
            heap: &mut TaggedHeap,
            {parallel},
            body: impl FnOnce(&mut TaggedHeap, &StoppedWorldProof<'stop>) -> R,
        ) -> R {
            let p = Self { heap_identity: 1, _scope: PhantomData };
            body(heap, &p)
        }
        {extra}
    }
"#;

fn c12_constructor_problems(
    serial: &str,
    parallel_safety: &str,
    parallel: &str,
    extra: &str,
) -> String {
    let source = C12_CANONICAL_CONSTRUCTORS
        .replace("{serial}", serial)
        .replace("{parallel_safety}", parallel_safety)
        .replace("{parallel}", parallel)
        .replace("{extra}", extra);
    let mut audit = C12Audit::default();
    c12_audit_file(
        C12_STOPPED_WORLD_PROOF_FILE,
        &syn::parse_file(&source).expect("parses"),
        &mut audit,
    );
    // Entry rows are out of scope here; keep only constructor problems.
    c12_finish_audit(&audit)
        .into_iter()
        .filter(|p| !p.contains("TaggedHeap::"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn c12_deny_by_default_rejects_extra_constructors_and_non_allowlisted_fns() {
    const HEAP: &str = "heap: &mut TaggedHeap";
    const PARALLEL: &str = "parallel: &'stop ParallelStoppedMutatorsProof<'stop>";
    // 0. Positive control: the real two-constructor shape has no problem.
    let canonical = c12_constructor_problems(HEAP, "", PARALLEL, "");
    assert!(
        canonical.is_empty(),
        "canonical constructors must pass:\n{canonical}"
    );

    // 1./2. A third constructor, safe or unsafe, is denied by default.
    let safe_extra = c12_constructor_problems(
        HEAP,
        "",
        PARALLEL,
        "pub(super) fn from_parallel() -> StoppedWorldProof<'static> { \
         StoppedWorldProof { heap_identity: 1, _scope: PhantomData } }",
    );
    assert!(
        safe_extra.contains(
            "unexpected StoppedWorldProof constructor `StoppedWorldProof::from_parallel` (safe fn)"
        ),
        "a third safe constructor must be denied:\n{safe_extra}"
    );
    let unsafe_extra = c12_constructor_problems(
        HEAP,
        "",
        PARALLEL,
        "pub(super) unsafe fn forge_for_heap() -> Self { Self { heap_identity: 2, _scope: PhantomData } }",
    );
    assert!(
        unsafe_extra.contains("unexpected StoppedWorldProof constructor `StoppedWorldProof::forge_for_heap` (unsafe fn)"),
        "a third unsafe constructor must be denied:\n{unsafe_extra}"
    );

    // 3. The v2 shape: a raw heap pointer that `body` aliases through `&mut`.
    let raw = c12_constructor_problems("heap: *const TaggedHeap", "", PARALLEL, "");
    assert!(
        raw.contains("StoppedWorldProof::with_stopped_world must not take a raw pointer"),
        "a raw-pointer serial constructor must be denied:\n{raw}"
    );

    // 4./5. The parallel mint must borrow the parallel proof and stay safe.
    let unborrowed = c12_constructor_problems(HEAP, "", "id: HeapIdentity", "");
    assert!(
        unborrowed.contains(
            "StoppedWorldProof::with_all_stopped must borrow a `&ParallelStoppedMutatorsProof`"
        ),
        "a parallel mint without the parallel proof must be denied:\n{unborrowed}"
    );
    let unsafe_parallel = c12_constructor_problems(HEAP, "unsafe", PARALLEL, "");
    assert!(
        unsafe_parallel.contains("StoppedWorldProof::with_all_stopped must be a safe fn"),
        "the parallel mint's authority is its borrow, not an unsafe contract:\n{unsafe_parallel}"
    );

    // 6./7. Mints outside the exact `(file, fn)` allowlists fail, per mint.
    for (file, mint, call) in [
        (
            "neovm-eval/src/emacs_core/runtime/eval/command_loop.rs",
            "with_stopped_world",
            "unsafe { StoppedWorldProof::with_stopped_world(heap, |_, _s| {}) }",
        ),
        (
            "neovm-value/src/tagged/gc/mutator_registration/collection.rs",
            "with_all_stopped",
            "StoppedWorldProof::with_all_stopped(heap, parallel, |_, _s| {})",
        ),
    ] {
        let source = format!("impl Context {{ fn sneaky_helper(&mut self) {{ {call}; }} }}");
        let mut audit = C12Audit::default();
        c12_audit_file(file, &syn::parse_file(&source).expect("parses"), &mut audit);
        let problems = audit.problems.join("\n");
        assert!(
            problems.contains(&format!(
                "non-allowlisted production `{mint}` mint in `sneaky_helper`"
            )),
            "{mint} outside its allowlist must be rejected:\n{problems}"
        );
    }
}

#[test]
fn c12_stopped_world_scope_preserves_enclosing_safety_and_flags_nested_closures() {
    let safe_caller = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn collect_exact(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn begin_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_finish(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_incremental_sweep_now(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_first_partition_cycle(&mut self, _s: &StoppedWorldProof<'_>) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub fn sneaky_safe(heap: &mut TaggedHeap) {
            unsafe {
                StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {
                    heap.begin_stw_collection(stopped);
                });
            }
        }
        "#,
        "sneaky.rs",
    );
    let safe_problems = capability_problems(&safe_caller).join("\n");
    assert!(
        safe_problems.contains("safe `sneaky_safe` calls `begin_stw_collection`"),
        "safe non-allowlisted fn must still be flagged through with_stopped_world:\n{safe_problems}"
    );

    // `with_all_stopped`'s closure is its THIRD argument; it inherits the
    // enclosing fn's identity too, so the finding names the fn, not
    // `<closure>` (fails if the scope rule only knows the 2-arg mint).
    let parallel_caller = survey_file(
        &r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn collect_exact(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn begin_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_finish(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_incremental_sweep_now(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_first_partition_cycle(&mut self, _s: &StoppedWorldProof<'_>) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub fn sneaky_safe(heap: &mut TaggedHeap) {
            unsafe {
                StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {
                    heap.begin_stw_collection(stopped);
                });
            }
        }
        "#
        .replace(
            "StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {",
            "StoppedWorldProof::with_all_stopped(heap, parallel, |heap, stopped| {",
        ),
        "parallel.rs",
    );
    let parallel_problems = capability_problems(&parallel_caller).join("\n");
    assert!(
        parallel_problems.contains("safe `sneaky_safe` calls `begin_stw_collection`")
            && !parallel_problems.contains("`<closure>`"),
        "with_all_stopped's closure must inherit the enclosing fn:\n{parallel_problems}"
    );

    let nested_closure = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn collect_exact(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn begin_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_finish(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_incremental_sweep_now(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_first_partition_cycle(&mut self, _s: &StoppedWorldProof<'_>) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub unsafe fn unsafe_with_nested_closure(heap: &mut TaggedHeap) {
            StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {
                let _escaped = || unsafe {
                    heap.begin_stw_collection(stopped);
                };
            });
        }
        "#,
        "nested.rs",
    );
    let nested_problems = capability_problems(&nested_closure).join("\n");
    assert!(
        nested_problems.contains("safe `<closure>` calls `begin_stw_collection`"),
        "nested closure inside with_stopped_world must still be flagged:\n{nested_problems}"
    );

    let direct_unsafe = survey_file(
        r#"
        struct TaggedHeap;
        impl TaggedHeap {
            pub unsafe fn collect(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn collect_exact(&mut self, _s: &StoppedWorldProof<'_>, _r: i32) {}
            pub unsafe fn alloc_user_ptr(&mut self) {}
            pub unsafe fn alloc_module_function(&mut self) {}
            pub unsafe fn begin_stw_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn begin_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn complete_minor_collection(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_finish(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn incremental_sweep_slice(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_incremental_sweep_now(&mut self, _s: &StoppedWorldProof<'_>) {}
            pub unsafe fn finish_first_partition_cycle(&mut self, _s: &StoppedWorldProof<'_>) {}
        }
        pub unsafe fn set_tagged_heap_root() {}
        pub unsafe fn with_tagged_heap() {}
        pub unsafe fn direct_unsafe_caller(heap: &mut TaggedHeap) {
            StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {
                heap.begin_stw_collection(stopped);
            });
        }
        "#,
        "direct.rs",
    );
    let direct_problems = capability_problems(&direct_unsafe);
    assert!(
        direct_problems.is_empty(),
        "direct unsafe fn using with_stopped_world should pass:\n{}",
        direct_problems.join("\n")
    );
}

/// value-gc C12 family: every stopped-world capability type is minted only at
/// pinned sites (reuses this module's production-scan helpers).
#[path = "capability_family_guard_test.rs"]
mod capability_family;
