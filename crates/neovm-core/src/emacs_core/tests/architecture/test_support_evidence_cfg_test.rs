//! Source guard: a test-support evidence reader must find its writers
//! compiled in the release workspace test build.
//!
//! A workspace release test build compiles the crate under test with
//! `cfg(test)`, but every dependency (neovm-eval, neovm-value, ...) with
//! `cfg(test)` and `debug_assertions` OFF; the dev-dependency edge enables
//! `feature = "test-support"`. A counter or TLS witness whose reader is
//! exposed under `test-support` but whose writer is gated
//! `#[cfg(any(test, debug_assertions))]` therefore reads a frozen zero in the
//! release suite while the debug suite passes. That is the
//! RELEASE-TEST-SUPPORT-CFG contract
//! (`neovm-eval/.../jit/compile/shims.rs`, `jit/cache.rs`): widen the callee,
//! never narrow the reader.
//!
//! The scan is source based:
//! 1. **Readers.** A function is *exposed* when it is compiled in the release
//!    test-support build and either lives in a `src/test_support.rs` (the
//!    module is compiled only for tests) or carries a `cfg_attr` whose
//!    predicate names `feature = "test-support"` (the cross-crate visibility
//!    marker). A `#[cfg(test)]` function is not exposed: it is not in that
//!    build.
//! 2. **Evidence.** The statics those readers reference, plus
//!    test-support-declared statics reachable from them through the local
//!    call graph (`native_depth_for_test` -> `cache::native_depth` is one such
//!    hop). Only declared statics count, so incidental identifiers never
//!    match.
//! 3. **Writers.** A use of an evidence static through a mutating receiver
//!    method (`fetch_add`, `store`, `set`, `compare_exchange`, ...; a
//!    `.with` closure counts only when it mutates through its parameter) is
//!    checked against two configurations:
//!    * *debug dependency*: `test = false`, `debug_assertions = true`;
//!    * *release test-support*: `test = false`, `debug_assertions = false`,
//!      `test-support = true`.
//!    A writer compiled in the debug dependency build but NOT in the release
//!    test-support build is the release-only-red shape and is reported. A
//!    `#[cfg(test)]`-only writer is compiled in neither (it belongs to the
//!    crate's own unit tests) and is allowed. Other host predicates and
//!    features are evaluated optimistically on; only `test`, `debug_assertions`
//!    and `feature = "test-support"` separate the two configurations.
//!
//! Macro transcribers are scanned as token trees with the same rule for the
//! attribute immediately preceding a statement (the spec shims are written
//! that way; a `.with` inside a macro is treated as a read and is not
//! classified). The tripwire at the end pins the readers that the three
//! release-only reds exercised.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

/// Receiver methods that always mutate, without a closure to inspect.
const WRITER_METHODS: &[&str] = &[
    "compare_exchange",
    "compare_exchange_weak",
    "fetch_add",
    "fetch_and",
    "fetch_max",
    "fetch_min",
    "fetch_nand",
    "fetch_or",
    "fetch_sub",
    "fetch_update",
    "fetch_xor",
    "replace",
    "set",
    "store",
    "swap",
];

/// Mutators a `Cell`/`Atomic` closure parameter can call inside `.with`.
const CLOSURE_MUTATORS: &[&str] = &[
    "compare_exchange",
    "compare_exchange_weak",
    "fetch_add",
    "replace",
    "set",
    "store",
    "swap",
    "take",
];

/// The two builds whose difference is the release-only-red class.
#[derive(Clone, Copy)]
struct TestConfig {
    test: bool,
    debug_assertions: bool,
}

const DEBUG_DEPENDENCY: TestConfig = TestConfig {
    test: false,
    debug_assertions: true,
};
const RELEASE_TEST_SUPPORT: TestConfig = TestConfig {
    test: false,
    debug_assertions: false,
};
/// The crate under test compiles its `tests/` sources with `cfg(test)`.
const TEST_DEBUG: TestConfig = TestConfig {
    test: true,
    debug_assertions: true,
};
const TEST_RELEASE: TestConfig = TestConfig {
    test: true,
    debug_assertions: false,
};

fn parse_metas(list: &syn::MetaList) -> Option<Vec<syn::Meta>> {
    Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()
        .map(|punctuated| punctuated.into_iter().collect())
}

fn meta_mentions_test_support(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(_) => false,
        syn::Meta::List(list) => {
            parse_metas(list).is_some_and(|args| args.iter().any(meta_mentions_test_support))
        }
        syn::Meta::NameValue(named) => {
            named.path.is_ident("feature")
                && matches!(
                    &named.value,
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(name),
                        ..
                    }) if name.value() == "test-support"
                )
        }
    }
}

/// A `cfg` or `cfg_attr` whose predicate (for `cfg_attr`, its first meta)
/// names `test-support`.
fn attrs_mention_test_support(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        if attribute.path().is_ident("cfg") {
            attribute
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| meta_mentions_test_support(&meta))
        } else if attribute.path().is_ident("cfg_attr") {
            attribute
                .parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
                .is_ok_and(|args| args.first().is_some_and(meta_mentions_test_support))
        } else {
            false
        }
    })
}

/// Truth of one cfg predicate in `config`. Features are on in both compared
/// builds (the workspace test build enables jit/sqlite; `test-support` is the
/// dev-edge feature), and host predicates are on for the host running the
/// suite. `test` and `debug_assertions` are the discriminators.
fn cfg_true_in(meta: &syn::Meta, config: TestConfig) -> bool {
    match meta {
        syn::Meta::Path(path) => path.get_ident().is_none_or(|ident| {
            if ident == "test" {
                config.test
            } else if ident == "debug_assertions" {
                config.debug_assertions
            } else {
                true
            }
        }),
        syn::Meta::List(list) => {
            let Some(args) = parse_metas(list) else {
                return true;
            };
            if list.path.is_ident("any") {
                args.iter().any(|arg| cfg_true_in(arg, config))
            } else if list.path.is_ident("all") {
                args.iter().all(|arg| cfg_true_in(arg, config))
            } else if list.path.is_ident("not") {
                args.len() == 1 && !cfg_true_in(&args[0], config)
            } else {
                true
            }
        }
        syn::Meta::NameValue(_) => true,
    }
}

fn cfg_predicates(attrs: &[syn::Attribute]) -> impl Iterator<Item = syn::Meta> + '_ {
    attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .filter_map(|attribute| attribute.parse_args::<syn::Meta>().ok())
}

fn compiled_in(attrs: &[syn::Attribute], config: TestConfig) -> bool {
    cfg_predicates(attrs).all(|meta| cfg_true_in(&meta, config))
}

fn last_segment(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|part| part.ident.to_string())
}

/// Statement-level attributes live on the `Expr` variant in syn 2.
fn expr_attrs(expression: &syn::Expr) -> &[syn::Attribute] {
    match expression {
        syn::Expr::Array(item) => &item.attrs,
        syn::Expr::Assign(item) => &item.attrs,
        syn::Expr::Async(item) => &item.attrs,
        syn::Expr::Await(item) => &item.attrs,
        syn::Expr::Binary(item) => &item.attrs,
        syn::Expr::Block(item) => &item.attrs,
        syn::Expr::Break(item) => &item.attrs,
        syn::Expr::Call(item) => &item.attrs,
        syn::Expr::Cast(item) => &item.attrs,
        syn::Expr::Closure(item) => &item.attrs,
        syn::Expr::Const(item) => &item.attrs,
        syn::Expr::Continue(item) => &item.attrs,
        syn::Expr::Field(item) => &item.attrs,
        syn::Expr::ForLoop(item) => &item.attrs,
        syn::Expr::Group(item) => &item.attrs,
        syn::Expr::If(item) => &item.attrs,
        syn::Expr::Index(item) => &item.attrs,
        syn::Expr::Infer(item) => &item.attrs,
        syn::Expr::Let(item) => &item.attrs,
        syn::Expr::Lit(item) => &item.attrs,
        syn::Expr::Loop(item) => &item.attrs,
        syn::Expr::Macro(item) => &item.attrs,
        syn::Expr::Match(item) => &item.attrs,
        syn::Expr::MethodCall(item) => &item.attrs,
        syn::Expr::Paren(item) => &item.attrs,
        syn::Expr::Path(item) => &item.attrs,
        syn::Expr::Range(item) => &item.attrs,
        syn::Expr::RawAddr(item) => &item.attrs,
        syn::Expr::Reference(item) => &item.attrs,
        syn::Expr::Repeat(item) => &item.attrs,
        syn::Expr::Return(item) => &item.attrs,
        syn::Expr::Struct(item) => &item.attrs,
        syn::Expr::Try(item) => &item.attrs,
        syn::Expr::TryBlock(item) => &item.attrs,
        syn::Expr::Tuple(item) => &item.attrs,
        syn::Expr::Unary(item) => &item.attrs,
        syn::Expr::Unsafe(item) => &item.attrs,
        syn::Expr::While(item) => &item.attrs,
        syn::Expr::Yield(item) => &item.attrs,
        _ => &[],
    }
}

#[derive(Default)]
struct FnInfo {
    exposed: bool,
    refs: BTreeSet<String>,
    calls: BTreeSet<String>,
}

/// Identifiers referenced in a function body (paths and macro tokens).
#[derive(Default)]
struct BodyRefs {
    refs: BTreeSet<String>,
    calls: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for BodyRefs {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if let Some(name) = last_segment(&path.path) {
            self.refs.insert(name);
        }
        visit::visit_expr_path(self, path);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func
            && let Some(name) = last_segment(&path.path)
        {
            self.calls.insert(name);
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        collect_token_idents(mac.tokens.clone(), &mut self.refs);
        visit::visit_macro(self, mac);
    }
}

fn collect_token_idents(tokens: TokenStream, out: &mut BTreeSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                out.insert(ident.to_string().trim_start_matches("r#").to_owned());
            }
            TokenTree::Group(group) => collect_token_idents(group.stream(), out),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

fn record_fn(
    fns: &mut BTreeMap<String, FnInfo>,
    exposed_file: bool,
    attrs: &[syn::Attribute],
    name: &syn::Ident,
    block: &syn::Block,
) {
    let mut body = BodyRefs::default();
    body.visit_block(block);
    let entry = fns.entry(name.to_string()).or_default();
    entry.exposed |= exposed_file || attrs_mention_test_support(attrs);
    entry.refs.extend(body.refs);
    entry.calls.extend(body.calls);
}

/// Collects function bodies that exist in the release test-support build.
struct FnCollector<'a> {
    exposed_file: bool,
    compiled_release: bool,
    fns: &'a mut BTreeMap<String, FnInfo>,
}

impl<'ast> Visit<'ast> for FnCollector<'_> {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let saved = self.compiled_release;
        self.compiled_release &= compiled_in(&module.attrs, RELEASE_TEST_SUPPORT);
        visit::visit_item_mod(self, module);
        self.compiled_release = saved;
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let saved = self.compiled_release;
        self.compiled_release &= compiled_in(&item.attrs, RELEASE_TEST_SUPPORT);
        visit::visit_item_impl(self, item);
        self.compiled_release = saved;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if self.compiled_release && compiled_in(&function.attrs, RELEASE_TEST_SUPPORT) {
            record_fn(
                self.fns,
                self.exposed_file,
                &function.attrs,
                &function.sig.ident,
                &function.block,
            );
        }
        visit::visit_item_fn(self, function);
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if self.compiled_release && compiled_in(&function.attrs, RELEASE_TEST_SUPPORT) {
            record_fn(
                self.fns,
                self.exposed_file,
                &function.attrs,
                &function.sig.ident,
                &function.block,
            );
        }
        visit::visit_impl_item_fn(self, function);
    }

    fn visit_trait_item_fn(&mut self, function: &'ast syn::TraitItemFn) {
        if self.compiled_release
            && compiled_in(&function.attrs, RELEASE_TEST_SUPPORT)
            && let Some(block) = &function.default
        {
            record_fn(
                self.fns,
                self.exposed_file,
                &function.attrs,
                &function.sig.ident,
                block,
            );
        }
        visit::visit_trait_item_fn(self, function);
    }
}

#[derive(Default)]
struct Declarations {
    declared: BTreeSet<String>,
    gated: BTreeSet<String>,
    /// Statics that exist in the release test-support build.
    live_release: BTreeSet<String>,
}

impl Declarations {
    fn record(&mut self, attrs: &[syn::Attribute], ident: &syn::Ident) {
        let name = ident.to_string();
        if attrs_mention_test_support(attrs) {
            self.gated.insert(name.clone());
        }
        if compiled_in(attrs, RELEASE_TEST_SUPPORT) {
            self.live_release.insert(name.clone());
        }
        self.declared.insert(name);
    }

    fn record_thread_local(&mut self, attrs: &[syn::Attribute], tokens: TokenStream) {
        let Ok(body) = syn::parse2::<syn::File>(tokens) else {
            return;
        };
        for item in body.items {
            if let syn::Item::Static(item) = item {
                self.record(attrs, &item.ident);
            }
        }
    }
}

struct DeclarationCollector<'a> {
    declarations: &'a mut Declarations,
}

impl<'ast> Visit<'ast> for DeclarationCollector<'_> {
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.declarations.record(&item.attrs, &item.ident);
        visit::visit_item_static(self, item);
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.declarations.record(&item.attrs, &item.ident);
        visit::visit_item_const(self, item);
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item
            .mac
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "thread_local")
        {
            self.declarations
                .record_thread_local(&item.attrs, item.mac.tokens.clone());
        }
        visit::visit_item_macro(self, item);
    }
}

/// The statics a test-support reader can observe in a release test build.
fn evidence_statics(files: &[(String, syn::File)]) -> BTreeSet<String> {
    let mut declarations = Declarations::default();
    let mut fns: BTreeMap<String, FnInfo> = BTreeMap::new();
    for (relative, syntax) in files {
        let exposed_file = relative.ends_with("test_support.rs");
        let mut decl_collector = DeclarationCollector {
            declarations: &mut declarations,
        };
        decl_collector.visit_file(syntax);
        let mut fn_collector = FnCollector {
            exposed_file,
            compiled_release: true,
            fns: &mut fns,
        };
        fn_collector.visit_file(syntax);
    }

    // Direct references of an exposed reader, and the call closure reachable
    // from them (the depth bound keeps a broad delegate from pulling in
    // unrelated runtime state).
    let direct_refs: BTreeSet<String> = fns
        .values()
        .filter(|info| info.exposed)
        .flat_map(|info| info.refs.iter().cloned())
        .collect();
    let mut closure: BTreeSet<String> = BTreeSet::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<(String, usize)> = fns
        .iter()
        .filter(|(_, info)| info.exposed)
        .map(|(name, _)| (name.clone(), 0))
        .collect();
    while let Some((name, depth)) = queue.pop_front() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(info) = fns.get(&name) else {
            continue;
        };
        closure.extend(info.refs.iter().cloned());
        if depth < 4 {
            for call in &info.calls {
                queue.push_back((call.clone(), depth + 1));
            }
        }
    }

    declarations
        .declared
        .iter()
        .filter(|name| declarations.live_release.contains(*name))
        .filter(|name| {
            direct_refs.contains(*name)
                || (declarations.gated.contains(*name) && closure.contains(*name))
        })
        .cloned()
        .collect()
}

/// `.with(|cell| cell.set(..))` writes; `.with(|cell| cell.get())` reads.
fn with_mutates(call: &syn::ExprMethodCall) -> bool {
    let Some(syn::Expr::Closure(closure)) = call.args.first() else {
        return false;
    };
    let parameter = closure.inputs.first().and_then(|input| match input {
        syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
        _ => None,
    });
    let Some(parameter) = parameter else {
        return false;
    };
    struct Finder {
        parameter: String,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Finder {
        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            if self.found {
                return;
            }
            if let syn::Expr::Path(path) = &*call.receiver
                && last_segment(&path.path).as_deref() == Some(self.parameter.as_str())
                && CLOSURE_MUTATORS.contains(&call.method.to_string().as_str())
            {
                self.found = true;
            }
            visit::visit_expr_method_call(self, call);
        }
    }
    let mut finder = Finder {
        parameter,
        found: false,
    };
    finder.visit_expr(&closure.body);
    finder.found
}

fn is_write(call: &syn::ExprMethodCall) -> bool {
    let method = call.method.to_string();
    if method == "with" {
        return with_mutates(call);
    }
    WRITER_METHODS.contains(&method.as_str())
}

/// Writer uses in ordinary syntax, with the enclosing cfg predicates tracked
/// in both compared configurations.
struct WriterVisitor<'a> {
    file: &'a str,
    /// The crate under test compiles `<crate>/src/**/tests/` with `cfg(test)`.
    test_file: bool,
    evidence: &'a BTreeSet<String>,
    gates: Vec<(bool, bool)>,
    current_fn: Option<String>,
    violations: Vec<String>,
}

impl<'a> WriterVisitor<'a> {
    fn configs(&self) -> (TestConfig, TestConfig) {
        if self.test_file {
            (TEST_DEBUG, TEST_RELEASE)
        } else {
            (DEBUG_DEPENDENCY, RELEASE_TEST_SUPPORT)
        }
    }

    fn push_attrs(&mut self, attrs: &[syn::Attribute]) -> usize {
        let before = self.gates.len();
        let (debug_config, release_config) = self.configs();
        for meta in cfg_predicates(attrs) {
            self.gates.push((
                cfg_true_in(&meta, debug_config),
                cfg_true_in(&meta, release_config),
            ));
        }
        before
    }

    fn truncate(&mut self, before: usize) {
        self.gates.truncate(before);
    }

    fn check(&mut self, name: &str, call: &syn::ExprMethodCall) {
        if !is_write(call) {
            return;
        }
        let compiled_debug = self.gates.iter().all(|(debug_dep, _)| *debug_dep);
        let compiled_release = self.gates.iter().all(|(_, release_test)| *release_test);
        if compiled_release || !compiled_debug {
            return;
        }
        let function = self.current_fn.as_deref().unwrap_or("<expr>");
        let method = &call.method;
        self.violations.push(format!(
            "{}::{function}: writer `{name}.{method}` is compiled in the debug dependency build \
             but not in the release test-support build (gate must include \
             `feature = \"test-support\"`)",
            self.file
        ));
    }
}

fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("test"))
}

impl<'ast> Visit<'ast> for WriterVisitor<'_> {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        // A test body is its own setup (and may deliberately trip a
        // debug-only probe); the guard checks production writers.
        if is_test_fn(&function.attrs) {
            return;
        }
        let before = self.push_attrs(&function.attrs);
        let saved = std::mem::replace(&mut self.current_fn, Some(function.sig.ident.to_string()));
        visit::visit_item_fn(self, function);
        self.current_fn = saved;
        self.truncate(before);
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if is_test_fn(&function.attrs) {
            return;
        }
        let before = self.push_attrs(&function.attrs);
        let saved = std::mem::replace(&mut self.current_fn, Some(function.sig.ident.to_string()));
        visit::visit_impl_item_fn(self, function);
        self.current_fn = saved;
        self.truncate(before);
    }

    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        let before = self.push_attrs(expr_attrs(expression));
        visit::visit_expr(self, expression);
        self.truncate(before);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        let before = self.push_attrs(&local.attrs);
        visit::visit_local(self, local);
        self.truncate(before);
    }

    fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
        let before = self.push_attrs(&field.attrs);
        visit::visit_field_value(self, field);
        self.truncate(before);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if let syn::Expr::Path(path) = &*call.receiver
            && let Some(name) = last_segment(&path.path)
            && self.evidence.contains(&name)
        {
            self.check(&name, call);
        }
        visit::visit_expr_method_call(self, call);
    }
}

/// Writer uses inside a `macro_rules!` transcriber. The spec shims put one
/// cfg attribute directly before each gated statement, so the scanner tracks
/// that attribute until the statement or brace block it introduces.
fn scan_macro_tokens(
    tokens: TokenStream,
    gate: (bool, bool),
    evidence: &BTreeSet<String>,
    file: &str,
    macro_name: &str,
    violations: &mut Vec<String>,
) {
    let token_list: Vec<TokenTree> = tokens.into_iter().collect();
    let mut pending = gate;
    let mut index = 0;
    while index < token_list.len() {
        if let Some((attribute_gate, next)) = attribute_gate(&token_list, index) {
            pending = (gate.0 && attribute_gate.0, gate.1 && attribute_gate.1);
            index = next;
            continue;
        }
        match &token_list[index] {
            TokenTree::Ident(ident) if evidence.contains(&ident.to_string()) => {
                let method = match (token_list.get(index + 1), token_list.get(index + 2)) {
                    (Some(TokenTree::Punct(dot)), Some(TokenTree::Ident(method)))
                        if dot.as_char() == '.' =>
                    {
                        Some(method.to_string())
                    }
                    _ => None,
                };
                if let Some(method) = method
                    && WRITER_METHODS.contains(&method.as_str())
                    && pending.0
                    && !pending.1
                {
                    violations.push(format!(
                        "{file}::macro {macro_name}: writer `{ident}.{method}` is compiled in the \
                         debug dependency build but not in the release test-support build (gate \
                         must include `feature = \"test-support\"`)"
                    ));
                }
            }
            TokenTree::Group(group) => {
                let child = if group.delimiter() == Delimiter::Brace {
                    pending
                } else {
                    gate
                };
                scan_macro_tokens(
                    group.stream(),
                    child,
                    evidence,
                    file,
                    macro_name,
                    violations,
                );
                if group.delimiter() == Delimiter::Brace {
                    pending = gate;
                }
            }
            TokenTree::Punct(punct) if punct.as_char() == ';' => pending = gate,
            _ => {}
        }
        index += 1;
    }
}

fn attribute_gate(tokens: &[TokenTree], at: usize) -> Option<((bool, bool), usize)> {
    let TokenTree::Punct(hash) = tokens.get(at)? else {
        return None;
    };
    if hash.as_char() != '#' {
        return None;
    }
    let TokenTree::Group(group) = tokens.get(at + 1)? else {
        return None;
    };
    if group.delimiter() != Delimiter::Bracket {
        return None;
    }
    let meta = syn::parse2::<syn::Meta>(group.stream()).ok()?;
    let gate = if meta.path().is_ident("cfg") {
        match &meta {
            syn::Meta::List(list) => parse_metas(list).map_or((true, true), |args| {
                (
                    args.iter().all(|arg| cfg_true_in(arg, DEBUG_DEPENDENCY)),
                    args.iter()
                        .all(|arg| cfg_true_in(arg, RELEASE_TEST_SUPPORT)),
                )
            }),
            _ => (true, true),
        }
    } else {
        (true, true)
    };
    Some((gate, at + 2))
}

fn macro_violations(
    syntax: &syn::File,
    evidence: &BTreeSet<String>,
    file: &str,
    violations: &mut Vec<String>,
) {
    struct MacroWalker<'a> {
        evidence: &'a BTreeSet<String>,
        file: &'a str,
        violations: &'a mut Vec<String>,
    }

    impl<'ast> Visit<'ast> for MacroWalker<'_> {
        fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
            if let Some(ident) = &item.ident {
                let inherited = (
                    compiled_in(&item.attrs, DEBUG_DEPENDENCY),
                    compiled_in(&item.attrs, RELEASE_TEST_SUPPORT),
                );
                let mut own = Vec::new();
                scan_macro_tokens(
                    item.mac.tokens.clone(),
                    inherited,
                    self.evidence,
                    self.file,
                    &ident.to_string(),
                    &mut own,
                );
                self.violations.extend(own);
            }
            visit::visit_item_macro(self, item);
        }
    }

    let mut walker = MacroWalker {
        evidence,
        file,
        violations,
    };
    walker.visit_file(syntax);
}

#[test]
fn every_test_support_evidence_writer_is_compiled_in_the_release_test_build() {
    let mut files = Vec::new();
    for source in production_source_files() {
        let text = std::fs::read_to_string(&source.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", source.path.display()));
        let syntax = syn::parse_file(&text)
            .unwrap_or_else(|error| panic!("parse {}: {error}", source.path.display()));
        files.push((source.relative.clone(), syntax));
    }

    let evidence = evidence_statics(&files);
    // Tripwire: these are the readers the three release-only reds exercised
    // (the spec fast-call count and the native-depth F5 witness; the native
    // run counters are read by core's green release witness). Losing one from
    // discovery means the scan stopped seeing the class it guards.
    for required in ["SPEC_FAST_CALL_COUNT", "NATIVE_DEPTH", "NATIVE_OK_COUNT"] {
        assert!(
            evidence.contains(required),
            "evidence discovery lost `{required}`; the scan no longer covers the \
             release-only red class (found: {evidence:?})"
        );
    }

    let mut violations = Vec::new();
    for (relative, syntax) in &files {
        let mut visitor = WriterVisitor {
            file: relative,
            test_file: relative.contains("/tests/") || relative.starts_with("tests/"),
            evidence: &evidence,
            gates: Vec::new(),
            current_fn: None,
            violations: Vec::new(),
        };
        visitor.visit_file(syntax);
        violations.append(&mut visitor.violations);
        macro_violations(syntax, &evidence, relative, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "test-support evidence writers missing from the release test-support build:\n{:#?}",
        violations
    );
}
