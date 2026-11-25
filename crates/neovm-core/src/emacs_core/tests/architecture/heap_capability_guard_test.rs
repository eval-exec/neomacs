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

struct Scanner<'a> {
    file: &'a str,
    excluded: u32,
    unsafe_fn: u32,
    in_tagged_heap: bool,
    in_with_tagged_heap_arg: u32,
    function: Vec<String>,
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
        body(self);
        self.function.pop();
        if unsafe_fn {
            self.unsafe_fn -= 1;
        }
        if excluded {
            self.excluded -= 1;
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
        self.enter_item(
            &name,
            &item.attrs,
            matches!(item.sig.safety, syn::Safety::Unsafe(_)),
            |scanner| {
                visit::visit_item_fn(scanner, item);
            },
        );
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
        let collect = name == "collect_exact" || (name == "collect" && call.args.len() == 1);
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
        if entered_tls_call {
            self.visit_expr(&call.func);
            self.in_with_tagged_heap_arg += 1;
            for arg in &call.args {
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
        visit::visit_expr_closure(self, closure);
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
            | "words"
            | "run"
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
