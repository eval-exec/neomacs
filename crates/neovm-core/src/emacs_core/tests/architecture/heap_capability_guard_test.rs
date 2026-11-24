//! No safe production path reaches `TaggedHeap` collection.
//!
//! `collect`, `collect_exact`, `set_tagged_heap_root`, `alloc_user_ptr`,
//! `alloc_module_function`, and the sweep entries (`begin_stw_collection`,
//! `complete_collection`, `begin_minor_collection`, `complete_minor_collection`,
//! `incremental_finish`, `incremental_sweep_slice`,
//! `finish_incremental_sweep_now`) are `unsafe fn`. `seed_root` stays safe:
//! it does not free. A safe function, including one that hides the call in
//! an `unsafe` block, must not call `collect` or `collect_exact`. The same
//! scan covers the seven sweep entries. Exactly two safe callers may reach
//! them: `gc_collect_from_current_roots_body` in `command_loop.rs`, and
//! `StoppedCollectorJob::run` in `collection.rs`. A new safe wrapper fails.
//! `Context::gc_collect_exact` stays safe. `&mut Context` is the capability
//! only after Phase 1 stops returning `&'static` payload borrows. Until then
//! a Rust local can keep a freed object. P1.1 through P1.3 place-anchor fourteen
//! readers in `value/mod.rs`. The borrow is the `Value` place, so it still does not
//! conflict with a collection. That conflict is Phase 1c (`HeapRef<'h>`).
//! Dropping a `TaggedHeap` and `shutdown(self)` still free from safe code.
//! Those are Phase 1c too. This guard does not cover them. Test support calls
//! `collect_rooted` and the `*_for_test` sweep wrappers, which are cfg'd out
//! of production builds and are deliberately unsound for unrooted borrows.
//!
//! The scan parses source syntax, not expanded macros or resolved types.
//! `macro_rules` bodies, `<TaggedHeap>::method` (qself), renamed imports,
//! and function pointers are invisible here; the compiler still requires
//! `unsafe` at those calls. `Iterator::collect` takes no arguments, so a
//! one-argument `.collect` is the heap entry. Items whose `cfg` needs `test`
//! or `test-support` are not production. A child module declared
//! `#[cfg(test)]` in its parent is still scanned, which can only false-positive.

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
    violations: Vec<Violation>,
}

struct Scanner<'a> {
    file: &'a str,
    excluded: u32,
    unsafe_fn: u32,
    in_tagged_heap: bool,
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
            function: Vec::new(),
            survey: Survey {
                definitions: Vec::new(),
                violations: Vec::new(),
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

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        // `collect` / `collect_exact` must not appear in any safe function.
        // Sweep names share that rule except for the two cycle owners.
        let sweep = is_sweep_method(&name);
        let collect = name == "collect_exact" || (name == "collect" && call.args.len() == 1);
        if collect || (sweep && !self.sweep_caller_allowed()) {
            self.note_call(&name, call.method.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func
            && let Some(name) = heap_path_method(&path.path)
        {
            let sweep = is_sweep_method(&name);
            if !sweep || !self.sweep_caller_allowed() {
                self.note_call(&name, path.path.span().start().line);
            }
        }
        visit::visit_expr_call(self, call);
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
        violations: Vec::new(),
    };
    for file in architecture_production_source_roots::production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let one = survey_file(&source, &display_path(&file));
        survey.definitions.extend(one.definitions);
        survey.violations.extend(one.violations);
    }
    survey
}

fn display_path(file: &ProductionSourceFile) -> String {
    file.workspace_relative().to_owned()
}

fn capability_problems(survey: &Survey) -> Vec<String> {
    let mut problems = Vec::new();
    for name in HEAP_METHODS.iter().copied().chain(std::iter::once(INSTALL)) {
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
    problems
}

#[test]
fn no_safe_pub_path_reaches_collect() {
    let problems = capability_problems(&survey_production());
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

/// P1.1 through P1.3. These readers must take `&self` and must not return `'static`.
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
        "P1.1 readers must be place-anchored:\n{}",
        problems.join("\n")
    );
}
