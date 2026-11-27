//! value-gc C12 family guard: the stopped-world capability family
//! (`TraceWitness`, `StoppedWorldProof`, `StoppedMutatorsProof`,
//! `ParallelStoppedMutatorsProof`, `AllStopped`, `SingleMutatorWorld`) is minted only at named sites.
//!
//! Two exact multisets, each pinned by its full contents (so a 22nd site
//! fails and the failure names it):
//! - every production struct literal of a family type, keyed by
//!   `(file, enclosing fn, type)`;
//! - every production call of a family constructor, keyed by
//!   `(file, enclosing fn, Type::ctor)`.
//!
//! Fail closed on every route that would let a construction escape those
//! keys: a `use … as` re-spelling or `type` alias of a family type, a
//! constructor used as a value (function-pointer intermediate), a
//! conversion/`Default` impl producing a family type, a macro body that
//! spells a construction, and any `Verbatim` (unparsed) node.

use std::collections::BTreeMap;

use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots;
use super::{display_path, production_excluded};

const FAMILY: [&str; 6] = [
    "TraceWitness",
    "StoppedWorldProof",
    "StoppedMutatorsProof",
    "ParallelStoppedMutatorsProof",
    "AllStopped",
    "SingleMutatorWorld",
];

/// Associated functions that mint a family value.
const CONSTRUCTORS: [(&str, &str); 7] = [
    ("StoppedWorldProof", "with_stopped_world"),
    ("StoppedWorldProof", "with_all_stopped"),
    ("StoppedMutatorsProof", "from_single_world"),
    ("StoppedMutatorsProof", "from_trace"),
    ("StoppedMutatorsProof", "for_test"),
    ("ParallelStoppedMutatorsProof", "from_trace"),
    ("SingleMutatorWorld", "from_heap"),
];

/// Traits whose impl would produce a family value outside the named sites.
const PRODUCING_TRAITS: [&str; 6] = ["From", "Into", "TryFrom", "TryInto", "Default", "FromStr"];

const GC: &str = "neovm-value/src/tagged/gc/";

/// `(file, enclosing fn, type) => count` for production struct literals.
fn expected_literals() -> BTreeMap<(String, String, String), usize> {
    [
        (
            "stopped_world_proof.rs",
            "StoppedWorldProof::with_stopped_world",
            "StoppedWorldProof",
            1,
        ),
        (
            "stopped_world_proof.rs",
            "StoppedWorldProof::with_all_stopped",
            "StoppedWorldProof",
            1,
        ),
        (
            "scan_contract.rs",
            "StoppedMutatorsProof::from_single_world",
            "StoppedMutatorsProof",
            1,
        ),
        (
            "scan_contract.rs",
            "StoppedMutatorsProof::from_trace",
            "StoppedMutatorsProof",
            1,
        ),
        (
            "scan_contract.rs",
            "ParallelStoppedMutatorsProof::from_trace",
            "ParallelStoppedMutatorsProof",
            1,
        ),
        (
            "scan_contract.rs",
            "SingleMutatorWorld::from_heap",
            "SingleMutatorWorld",
            1,
        ),
        (
            "mutator_registration/rendezvous/keeper.rs",
            "AllStopped::trace",
            "TraceWitness",
            1,
        ),
        (
            "mutator_registration/rendezvous/keeper.rs",
            "EraCore::try_stop",
            "AllStopped",
            1,
        ),
        (
            "mutator_registration/rendezvous/keeper.rs",
            "EraCore::stop_by",
            "AllStopped",
            1,
        ),
    ]
    .into_iter()
    .map(|(file, func, ty, n)| ((format!("{GC}{file}"), func.to_owned(), ty.to_owned()), n))
    .collect()
}

/// `(file, enclosing fn, Type::ctor) => count` for production constructor calls.
fn expected_calls() -> BTreeMap<(String, String, String), usize> {
    C12_FAMILY_CALLS
        .iter()
        .map(|&(file, func, callee, n)| ((file.to_owned(), func.to_owned(), callee.to_owned()), n))
        .collect()
}

/// Every production constructor call: `(file, enclosing fn, Type::ctor, count)`.
#[rustfmt::skip]
const C12_FAMILY_CALLS: &[(&str, &str, &str, usize)] = &[
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::gc_collect_from_current_roots_body", "StoppedWorldProof::with_stopped_world", 8),
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::start_concurrent_mark", "SingleMutatorWorld::from_heap", 1),
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::terminate_concurrent_mark", "StoppedWorldProof::with_stopped_world", 1),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::finish_concurrent_mark", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::raw_concurrent_begin", "StoppedWorldProof::with_stopped_world", 1),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::raw_launch_concurrent_mark", "SingleMutatorWorld::from_heap", 3),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "CollectorQuiescence::check", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "TaggedHeap::drain_to_quiescent", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "TaggedHeap::drain_to_quiescent", "StoppedWorldProof::with_stopped_world", 3),
    ("neovm-value/src/tagged/gc/generational.rs", "TaggedHeap::begin_minor_collection", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/generational.rs", "TaggedHeap::begin_parallel_minor_collection", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/generational.rs", "TaggedHeap::promote_survivors_world_stopped", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/incremental.rs", "TaggedHeap::release_empty_cons_blocks", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/incremental.rs", "TaggedHeap::release_empty_cons_candidates", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/incremental.rs", "TaggedHeap::sweep_cons_in_generation", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/mark_sweep.rs", "TaggedHeap::begin_collection_with", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/mutator_registration/collection.rs", "StoppedCollectorJob::run", "StoppedWorldProof::with_all_stopped", 1),
    ("neovm-value/src/tagged/gc/mutator_registration/rendezvous/keeper.rs", "AllStopped::trace", "ParallelStoppedMutatorsProof::from_trace", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "ParallelStoppedMutatorsProof::from_trace", "StoppedMutatorsProof::from_trace", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::begin_satb_cycle", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::ensure_young_logs_stopped", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::fold_satb_counts", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::fold_satb_for_termination", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::take_satb_written_owners", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract.rs", "SingleMutatorWorld::with_stopped_heap", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract_bridge.rs", "SingleMutatorWorld::return_young_log_loan", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract_bridge.rs", "SingleMutatorWorld::take_young_log_loan", "StoppedMutatorsProof::from_single_world", 1),
    ("neovm-value/src/tagged/gc/scan_contract_bridge.rs", "SingleMutatorWorld::with_stopped_young_logs", "StoppedMutatorsProof::from_single_world", 1),
];

#[derive(Default)]
struct FamilyAudit {
    literals: BTreeMap<(String, String, String), usize>,
    calls: BTreeMap<(String, String, String), usize>,
    problems: Vec<String>,
}

struct FamilyVisitor<'a> {
    file: &'a str,
    excluded: u32,
    impl_stack: Vec<String>,
    fn_stack: Vec<String>,
    audit: &'a mut FamilyAudit,
}

fn last_ident(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|seg| seg.ident.to_string())
}

fn type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => last_ident(&p.path),
        _ => None,
    }
}

fn is_family(name: &str) -> bool {
    FAMILY.contains(&name)
}

impl FamilyVisitor<'_> {
    /// `Type::fn` inside an impl, `fn` for a free fn, `<module>` otherwise.
    fn enclosing(&self) -> String {
        match (self.impl_stack.last(), self.fn_stack.last()) {
            (Some(ty), Some(func)) => format!("{ty}::{func}"),
            (None, Some(func)) => func.clone(),
            (_, None) => "<module>".to_owned(),
        }
    }

    fn self_type(&self) -> Option<&str> {
        self.impl_stack.last().map(String::as_str)
    }

    fn problem(&mut self, node: &impl Spanned, what: String) {
        if self.excluded == 0 {
            let line = node.span().start().line;
            self.audit
                .problems
                .push(format!("{}:{line}: {what}", self.file));
        }
    }

    /// `Type::ctor` (or `Self::ctor` inside `impl Type`) when `path` names a
    /// family constructor.
    fn constructor_path(&self, path: &syn::Path) -> Option<String> {
        let mut rev = path.segments.iter().rev();
        let method = rev.next()?.ident.to_string();
        let ty = rev.next()?.ident.to_string();
        let ty = if ty == "Self" {
            self.self_type()?.to_owned()
        } else {
            ty
        };
        CONSTRUCTORS
            .iter()
            .any(|&(t, m)| t == ty && m == method)
            .then(|| format!("{ty}::{method}"))
    }
}

impl<'ast> Visit<'ast> for FamilyVisitor<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_mod(self, item);
        self.excluded -= u32::from(ex);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        let self_ty = type_name(&item.self_ty).unwrap_or_else(|| "<type>".to_owned());
        if let Some((trait_path, _)) = &item.trait_
            && let Some(trait_name) = last_ident(trait_path)
            && PRODUCING_TRAITS.contains(&trait_name.as_str())
        {
            let generic_family = trait_path.segments.last().is_some_and(|seg| {
                matches!(&seg.arguments, syn::PathArguments::AngleBracketed(args)
                    if args.args.iter().any(|arg| matches!(arg,
                        syn::GenericArgument::Type(ty) if type_name(ty).is_some_and(|n| is_family(&n)))))
            });
            if is_family(&self_ty) || generic_family {
                self.problem(
                    item,
                    format!("`impl {trait_name}` would produce a capability-family value outside the named mint sites"),
                );
            }
        }
        self.impl_stack.push(self_ty);
        visit::visit_item_impl(self, item);
        self.impl_stack.pop();
        self.excluded -= u32::from(ex);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        let saved = std::mem::take(&mut self.impl_stack);
        self.fn_stack.push(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.fn_stack.pop();
        self.impl_stack = saved;
        self.excluded -= u32::from(ex);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        self.fn_stack.push(item.sig.ident.to_string());
        visit::visit_impl_item_fn(self, item);
        self.fn_stack.pop();
        self.excluded -= u32::from(ex);
    }

    fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
        if self.excluded == 0
            && let Some(name) = last_ident(&expr.path)
        {
            let name = if name == "Self" {
                self.self_type().unwrap_or("").to_owned()
            } else {
                name
            };
            if is_family(&name) {
                let key = (self.file.to_owned(), self.enclosing(), name);
                *self.audit.literals.entry(key).or_default() += 1;
            }
        }
        visit::visit_expr_struct(self, expr);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func
            && let Some(callee) = self.constructor_path(&path.path)
        {
            if self.excluded == 0 {
                let key = (self.file.to_owned(), self.enclosing(), callee);
                *self.audit.calls.entry(key).or_default() += 1;
            }
            // The callee is accounted for; visit only the arguments so the
            // path is not also reported as a constructor-as-value.
            for arg in &call.args {
                self.visit_expr(arg);
            }
            return;
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        if let Some(callee) = self.constructor_path(&expr.path) {
            self.problem(
                expr,
                format!("`{callee}` used as a value (function-pointer intermediate) escapes the call-site allowlist"),
            );
        }
        visit::visit_expr_path(self, expr);
    }

    fn visit_use_rename(&mut self, rename: &'ast syn::UseRename) {
        let name = rename.ident.to_string();
        if is_family(&name) {
            self.problem(
                rename,
                format!(
                    "`use … {name} as {}` re-spells a capability-family type",
                    rename.rename
                ),
            );
        }
        visit::visit_use_rename(self, rename);
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if !production_excluded(&item.attrs)
            && let Some(name) = type_name(&item.ty)
            && is_family(&name)
        {
            self.problem(
                item,
                format!(
                    "`type {} = {name}` re-spells a capability-family type",
                    item.ident
                ),
            );
        }
        visit::visit_item_type(self, item);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let tokens = mac.tokens.to_string();
        let self_ty = self
            .self_type()
            .filter(|ty| is_family(ty))
            .map(str::to_owned);
        let spells_construction = FAMILY.iter().any(|ty| tokens.contains(&format!("{ty} {{")))
            || CONSTRUCTORS
                .iter()
                .any(|(ty, m)| tokens.contains(&format!("{ty} :: {m}")))
            || (self_ty.is_some() && tokens.contains("Self {"));
        if spells_construction {
            self.problem(
                mac,
                "macro body spells a capability-family construction (unanalysable, fail closed)"
                    .to_owned(),
            );
        }
        visit::visit_macro(self, mac);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if let syn::Item::Verbatim(tokens) = item {
            self.problem(item, format!("unparsed item `{tokens}` (fail closed)"));
        }
        visit::visit_item(self, item);
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if let syn::Expr::Verbatim(tokens) = expr {
            self.problem(
                expr,
                format!("unparsed expression `{tokens}` (fail closed)"),
            );
        }
        visit::visit_expr(self, expr);
    }
}

fn audit_source(file: &str, source: &str, audit: &mut FamilyAudit) {
    let ast = syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
    if production_excluded(&ast.attrs) {
        return;
    }
    FamilyVisitor {
        file,
        excluded: 0,
        impl_stack: Vec::new(),
        fn_stack: Vec::new(),
        audit,
    }
    .visit_file(&ast);
}

/// Every key whose count differs, named: `+` a site the table lacks, `-` a
/// pinned site that is gone.
fn multiset_problems(
    kind: &str,
    actual: &BTreeMap<(String, String, String), usize>,
    expected: &BTreeMap<(String, String, String), usize>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, &n) in actual {
        let want = expected.get(key).copied().unwrap_or(0);
        if n != want {
            problems.push(format!(
                "{kind} {}:{} `{}` count {n}, pinned {want}",
                key.0, key.1, key.2
            ));
        }
    }
    for (key, &want) in expected {
        if !actual.contains_key(key) {
            problems.push(format!(
                "{kind} {}:{} `{}` count 0, pinned {want}",
                key.0, key.1, key.2
            ));
        }
    }
    problems
}

fn family_problems(
    audit: &FamilyAudit,
    literals: &BTreeMap<(String, String, String), usize>,
    calls: &BTreeMap<(String, String, String), usize>,
) -> Vec<String> {
    let mut problems = audit.problems.clone();
    problems.extend(multiset_problems("literal", &audit.literals, literals));
    problems.extend(multiset_problems("call", &audit.calls, calls));
    problems
}

#[test]
fn c12_capability_family_is_minted_only_at_pinned_sites() {
    let mut audit = FamilyAudit::default();
    for file in architecture_production_source_roots::production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        audit_source(&display_path(&file), &source, &mut audit);
    }
    let problems = family_problems(&audit, &expected_literals(), &expected_calls());
    assert!(
        problems.is_empty(),
        "capability-family mint sites differ from the pinned tables:\n{}",
        problems.join("\n")
    );
}

/// One synthetic file per route; each row must name its own defect.
#[test]
fn c12_capability_family_guard_refuses_every_escape_route() {
    const FILE: &str = "neovm-value/src/tagged/gc/synthetic.rs";
    let run = |source: &str| {
        let mut audit = FamilyAudit::default();
        audit_source(FILE, source, &mut audit);
        audit
    };
    let pinned = |entries: &[(&str, &str, &str)]| -> BTreeMap<(String, String, String), usize> {
        entries
            .iter()
            .map(|&(f, func, ty)| ((f.to_owned(), func.to_owned(), ty.to_owned()), 1))
            .collect()
    };

    // 0. Positive control: one pinned literal and one pinned call pass.
    let ok = run(
        "impl StoppedMutatorsProof { fn from_single_world(w: &W) -> Self { Self { _owner: P } } }
         impl SingleMutatorWorld { fn drain(&self) { let p = StoppedMutatorsProof::from_single_world(self); } }",
    );
    let ok_literals = pinned(&[(
        FILE,
        "StoppedMutatorsProof::from_single_world",
        "StoppedMutatorsProof",
    )]);
    let ok_calls = pinned(&[(
        FILE,
        "SingleMutatorWorld::drain",
        "StoppedMutatorsProof::from_single_world",
    )]);
    let control = family_problems(&ok, &ok_literals, &ok_calls);
    assert!(
        control.is_empty(),
        "positive control must pass:\n{}",
        control.join("\n")
    );

    let expect = |source: &str, needle: &str| {
        let problems = family_problems(&run(source), &ok_literals, &ok_calls).join("\n");
        assert!(
            problems.contains(needle),
            "expected `{needle}` in:\n{problems}"
        );
    };
    let base = "impl StoppedMutatorsProof { fn from_single_world(w: &W) -> Self { Self { _owner: P } } }
         impl SingleMutatorWorld { fn drain(&self) { let p = StoppedMutatorsProof::from_single_world(self); } }";

    // 1. A second literal site is named, not just counted.
    expect(
        &format!("{base} fn forge() {{ let _ = StoppedMutatorsProof {{ _owner: P }}; }}"),
        "literal neovm-value/src/tagged/gc/synthetic.rs:forge `StoppedMutatorsProof` count 1, pinned 0",
    );
    // 2. A 22nd constructor call site (here: a new from_heap caller) is named.
    expect(
        &format!(
            "{base} fn sneaky(h: &mut TaggedHeap) {{ unsafe {{ SingleMutatorWorld::from_heap(h) }}; }}"
        ),
        "call neovm-value/src/tagged/gc/synthetic.rs:sneaky `SingleMutatorWorld::from_heap` count 1, pinned 0",
    );
    // 3. A removed pinned site is named too.
    expect(
        "impl StoppedMutatorsProof { fn from_single_world(w: &W) -> Self { Self { _owner: P } } }",
        "call neovm-value/src/tagged/gc/synthetic.rs:SingleMutatorWorld::drain `StoppedMutatorsProof::from_single_world` count 0, pinned 1",
    );
    // 4. `use … as` re-spelling.
    expect(
        &format!("{base} use super::scan_contract::StoppedMutatorsProof as Key;"),
        "`use … StoppedMutatorsProof as Key` re-spells a capability-family type",
    );
    // 5. `type` alias re-spelling.
    expect(
        &format!("{base} type Key = super::scan_contract::StoppedMutatorsProof;"),
        "`type Key = StoppedMutatorsProof` re-spells a capability-family type",
    );
    // 6. Function-pointer intermediate.
    expect(
        &format!("{base} fn ptr() {{ let mint = StoppedMutatorsProof::from_single_world; }}"),
        "`StoppedMutatorsProof::from_single_world` used as a value (function-pointer intermediate)",
    );
    // 7. Conversion impls producing a family value: on the type and via `Into<Family>`.
    expect(
        &format!(
            "{base} impl From<()> for StoppedMutatorsProof {{ fn from(_: ()) -> Self {{ todo!() }} }}"
        ),
        "`impl From` would produce a capability-family value",
    );
    expect(
        &format!(
            "{base} impl Into<StoppedMutatorsProof> for Token {{ fn into(self) -> StoppedMutatorsProof {{ todo!() }} }}"
        ),
        "`impl Into` would produce a capability-family value",
    );
    expect(
        &format!("{base} impl Default for AllStopped {{ fn default() -> Self {{ todo!() }} }}"),
        "`impl Default` would produce a capability-family value",
    );
    // 8. A macro body that spells a construction is unanalysable.
    expect(
        &format!(
            "{base} macro_rules! forge {{ () => {{ StoppedMutatorsProof {{ _owner: P }} }}; }}"
        ),
        "macro body spells a capability-family construction (unanalysable, fail closed)",
    );
    expect(
        &format!("{base} fn m() {{ let _ = vec![SingleMutatorWorld::from_heap(h)]; }}"),
        "macro body spells a capability-family construction (unanalysable, fail closed)",
    );
    // 9. Test-gated items are out of scope (they cannot reach production).
    let gated = family_problems(
        &run(&format!(
            "{base} #[cfg(test)] fn t() {{ let _ = StoppedMutatorsProof {{ _owner: P }}; }}"
        )),
        &ok_literals,
        &ok_calls,
    );
    assert!(
        gated.is_empty(),
        "cfg(test) items are excluded:\n{}",
        gated.join("\n")
    );
}

// --- #859 item 2: no test-path file is compiled into a production build ---
//
// The family and relinking guards skip test-path files (`tests/`, `test_*`)
// because those are declared under a test-surface cfg. This walk turns that
// premise into a check for every production crate: from each crate root it
// visits every `mod` (at any depth, including inside fn bodies and const
// blocks) and every `include!` that a production build keeps — an item under
// `cfg(test)` / a `test-support` feature is dropped with its subtree — and
// fails if a reached file is a test path. Anything it cannot resolve to a
// literal `.rs` file inside that crate's `src/` fails closed: `#[path]` on an
// inline module, a non-inline `mod` inside a block, a `cfg_attr(…, path = …)`,
// a non-literal or non-`.rs` `#[path]` / `include!` (qualified spellings such
// as `core::include!` included), a macro whose body declares a `mod`, a file
// outside `src/` after canonicalisation (`../` escapes, symlinks), and an
// unresolvable, unreadable or unparsable module file.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// `Ok(Some(path))` for a literal `#[path = "…"]`, `Ok(None)` when absent,
/// `Err(())` when the attribute is present but not a string literal.
fn path_attr(attrs: &[syn::Attribute]) -> Result<Option<String>, ()> {
    match attrs.iter().find(|a| a.path().is_ident("path")) {
        None => Ok(None),
        Some(a) => match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Ok(Some(s.value())),
                _ => Err(()),
            },
            _ => Err(()),
        },
    }
}

fn cfg_attr_moves_path(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg_attr")
            && matches!(&a.meta, syn::Meta::List(list) if list.tokens.to_string().contains("path"))
    })
}

/// True if a macro's tokens load another file: a `mod <name>;` (the name
/// may be a `$metavariable`) or an `include!` (any path spelling). An inline
/// `mod name { … }` loads no file.
fn tokens_declare_file_mod(tokens: &str) -> bool {
    let words: Vec<&str> = tokens.split_whitespace().collect();
    if words.windows(2).any(|w| w[0] == "include" && w[1] == "!") {
        return true;
    }
    words.iter().enumerate().any(|(i, w)| {
        *w == "mod"
            && words[i + 1..]
                .iter()
                .take(3)
                .find(|t| **t == ";" || **t == "{")
                .is_some_and(|t| *t == ";")
    })
}

/// One `cfg_select!` arm: a cfg predicate (`None` for `_`) and its body. The
/// macro appears at item position (braced items) and at expression position
/// (braced statements, or a bare expression up to the next comma).
struct CfgSelectArm {
    predicate: Option<syn::Meta>,
    body: CfgSelectBody,
}

enum CfgSelectBody {
    Block(syn::Block),
    Expr(syn::Expr),
}

struct CfgSelect(Vec<CfgSelectArm>);

impl syn::parse::Parse for CfgSelect {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut arms = Vec::new();
        while !input.is_empty() {
            let predicate = if input.peek(syn::Token![_]) {
                input.parse::<syn::Token![_]>()?;
                None
            } else {
                Some(input.parse::<syn::Meta>()?)
            };
            input.parse::<syn::Token![=>]>()?;
            let body = if input.peek(syn::token::Brace) {
                CfgSelectBody::Block(input.parse::<syn::Block>()?)
            } else {
                CfgSelectBody::Expr(input.parse::<syn::Expr>()?)
            };
            arms.push(CfgSelectArm { predicate, body });
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }
        Ok(CfgSelect(arms))
    }
}

/// An item that a production build drops (`cfg(test)` / `test-support`).
fn item_is_test_gated(item: &syn::Item) -> bool {
    let attrs: &[syn::Attribute] = match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    };
    production_excluded(attrs)
}

fn item_label(item: &syn::Item) -> String {
    match item {
        syn::Item::Fn(f) => format!("fn {}", f.sig.ident),
        syn::Item::Mod(m) => format!("mod {}", m.ident),
        syn::Item::Struct(s) => format!("struct {}", s.ident),
        syn::Item::Enum(e) => format!("enum {}", e.ident),
        syn::Item::Const(c) => format!("const {}", c.ident),
        syn::Item::Static(s) => format!("static {}", s.ident),
        syn::Item::Use(_) => "use …".to_owned(),
        syn::Item::Impl(_) => "impl …".to_owned(),
        _ => "item".to_owned(),
    }
}

fn is_test_path_rel(rel: &str) -> bool {
    rel.split('/').any(|c| c == "tests")
        || rel
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("test_"))
}

struct ReachWalk<'r> {
    src: PathBuf,
    read: &'r dyn Fn(&Path) -> Option<String>,
    canon: &'r dyn Fn(&Path) -> PathBuf,
    reached: BTreeSet<PathBuf>,
    problems: Vec<String>,
}

impl ReachWalk<'_> {
    /// `mod_rs`: the file owns its directory (`lib.rs`, `main.rs`, `mod.rs` or
    /// a `#[path]`-mounted file); otherwise children live in `dir/<stem>/`.
    fn walk_file(&mut self, file: &Path, mod_rs: bool, from: &str) {
        let file = (self.canon)(&normalize_path(file));
        if !file.starts_with(&self.src) {
            self.problems.push(format!(
                "{from}: reaches `{}` outside the crate's src/ (fail closed)",
                file.display()
            ));
            return;
        }
        if !self.reached.insert(file.clone()) {
            return;
        }
        let Some(source) = (self.read)(&file) else {
            self.problems.push(format!(
                "{}: unreadable module file (fail closed)",
                file.display()
            ));
            return;
        };
        let rel = file
            .strip_prefix(&self.src)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        // A module or item-level `include!` parses as a file; an `include!` of
        // an expression or statement fragment parses as a block, whose items
        // are visited with `block_depth > 0` (so a file `mod` there fails).
        let parsed = syn::parse_file(&source).or_else(|file_error| {
            syn::parse_file(&format!("const _: () = {{ {source} }};")).map_err(|_| file_error)
        });
        let ast = match parsed {
            Ok(ast) => ast,
            Err(error) => {
                self.problems.push(format!(
                    "{}: unparsable ({error}) (fail closed)",
                    file.display()
                ));
                return;
            }
        };
        if production_excluded(&ast.attrs) {
            return;
        }
        // A test-path file may be reached by a production build only as a
        // facade: every item test-gated, so it adds no production code that
        // the test-path-skipping guards would be blind to.
        if is_test_path_rel(&rel)
            && let Some(item) = ast.items.iter().find(|item| !item_is_test_gated(item))
        {
            self.problems.push(format!(
                "test-path file `{rel}` is compiled into a production build and holds production item `{}` (only test-gated facades may be reached)",
                item_label(item)
            ));
        }
        let pending = {
            let mut items = FileItems {
                walk: self,
                file: file.clone(),
                mod_rs,
                inline: Vec::new(),
                block_depth: 0,
                excluded: 0,
                pending: Vec::new(),
            };
            items.visit_file(&ast);
            items.pending
        };
        for (target, mod_rs, from) in pending {
            self.walk_file(&target, mod_rs, &from);
        }
    }
}

/// Visits one file's items at every depth, queueing the module files and
/// `include!` targets a production build keeps.
struct FileItems<'w, 'r> {
    walk: &'w mut ReachWalk<'r>,
    file: PathBuf,
    mod_rs: bool,
    inline: Vec<String>,
    block_depth: u32,
    excluded: u32,
    pending: Vec<(PathBuf, bool, String)>,
}

impl FileItems<'_, '_> {
    fn dir(&self) -> PathBuf {
        self.file.parent().unwrap_or(Path::new("")).to_path_buf()
    }

    fn owned_dir(&self) -> PathBuf {
        if self.mod_rs {
            self.dir()
        } else {
            self.dir().join(self.file.file_stem().unwrap_or_default())
        }
    }

    fn here(&self, line: usize) -> String {
        format!("{}:{line}", self.file.display())
    }

    fn fail(&mut self, line: usize, what: impl std::fmt::Display) {
        let here = self.here(line);
        self.walk.problems.push(format!("{here}: {what}"));
    }
}

impl<'ast> Visit<'ast> for FileItems<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if self.excluded > 0 || production_excluded(&m.attrs) {
            return;
        }
        let line = m.ident.span().start().line;
        if cfg_attr_moves_path(&m.attrs) {
            self.fail(
                line,
                format_args!(
                    "`mod {}` has a cfg_attr path (unanalysable, fail closed)",
                    m.ident
                ),
            );
            return;
        }
        let path = match path_attr(&m.attrs) {
            Ok(path) => path,
            Err(()) => {
                self.fail(
                    line,
                    format_args!("`mod {}` has a non-literal #[path] (fail closed)", m.ident),
                );
                return;
            }
        };
        match (&m.content, path) {
            (Some(_), Some(_)) => {
                self.fail(
                    line,
                    format_args!(
                        "inline `mod {}` carries #[path] (unanalysable, fail closed)",
                        m.ident
                    ),
                );
            }
            (Some(_), None) => {
                self.inline.push(m.ident.to_string());
                visit::visit_item_mod(self, m);
                self.inline.pop();
            }
            (None, _) if self.block_depth > 0 => {
                self.fail(
                    line,
                    format_args!("non-inline `mod {}` inside a block (fail closed)", m.ident),
                );
            }
            (None, Some(path)) => {
                if !path.ends_with(".rs") {
                    self.fail(
                        line,
                        format_args!("#[path] `{path}` is not a .rs file (fail closed)"),
                    );
                    return;
                }
                let base = if self.inline.is_empty() {
                    self.dir()
                } else {
                    self.owned_dir()
                };
                let base = self.inline.iter().fold(base, |d, c| d.join(c));
                let from = self.here(line);
                self.pending.push((base.join(path), true, from));
            }
            (None, None) => {
                let base = self.inline.iter().fold(self.owned_dir(), |d, c| d.join(c));
                let flat = base.join(format!("{}.rs", m.ident));
                let nested = base.join(m.ident.to_string()).join("mod.rs");
                let from = self.here(line);
                if (self.walk.read)(&normalize_path(&flat)).is_some() {
                    self.pending.push((flat, false, from));
                } else if (self.walk.read)(&normalize_path(&nested)).is_some() {
                    self.pending.push((nested, true, from));
                } else {
                    self.fail(
                        line,
                        format_args!("`mod {}` resolves to no file (fail closed)", m.ident),
                    );
                }
            }
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.block_depth += 1;
        visit::visit_block(self, block);
        self.block_depth -= 1;
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let ex = production_excluded(&f.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_fn(self, f);
        self.excluded -= u32::from(ex);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let ex = production_excluded(&i.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_impl(self, i);
        self.excluded -= u32::from(ex);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        let ex = production_excluded(&f.attrs);
        self.excluded += u32::from(ex);
        visit::visit_impl_item_fn(self, f);
        self.excluded -= u32::from(ex);
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        let ex = production_excluded(&c.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_const(self, c);
        self.excluded -= u32::from(ex);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if self.excluded > 0 {
            return;
        }
        let line = mac
            .path
            .segments
            .last()
            .map_or(0, |s| s.ident.span().start().line);
        if mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "include")
        {
            match mac.parse_body::<syn::LitStr>() {
                Ok(lit) => {
                    let from = self.here(line);
                    let target = self.dir().join(lit.value());
                    self.pending.push((target, true, from));
                }
                Err(_) => self.fail(line, "include! with a non-literal argument (fail closed)"),
            }
            return;
        }
        if mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "cfg_select")
        {
            // Every arm compiles on some production target: walk each arm
            // whose predicate is not test-only.
            match mac.parse_body::<CfgSelect>() {
                Ok(CfgSelect(arms)) => {
                    for arm in arms {
                        if arm
                            .predicate
                            .as_ref()
                            .is_some_and(super::cfg_needs_test_surface)
                        {
                            continue;
                        }
                        match &arm.body {
                            // Items keep their position (item-level resolution);
                            // statements and expressions are visited in place.
                            CfgSelectBody::Block(block) => {
                                for stmt in &block.stmts {
                                    match stmt {
                                        syn::Stmt::Item(item) => self.visit_item(item),
                                        other => self.visit_stmt(other),
                                    }
                                }
                            }
                            CfgSelectBody::Expr(expr) => self.visit_expr(expr),
                        }
                    }
                }
                Err(error) => self.fail(
                    line,
                    format_args!("unparsable cfg_select! ({error}) (fail closed)"),
                ),
            }
            return;
        }
        if tokens_declare_file_mod(&mac.tokens.to_string()) {
            self.fail(
                line,
                "macro body loads another file (`mod name;` or `include!`) (unanalysable, fail closed)",
            );
        }
        visit::visit_macro(self, mac);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if self.excluded == 0
            && let syn::Item::Verbatim(tokens) = item
        {
            self.fail(0, format_args!("unparsed item `{tokens}` (fail closed)"));
        }
        visit::visit_item(self, item);
    }
}

/// Walk one crate from its root; problems plus one per reached test-path file.
fn production_reach(
    src: &Path,
    root: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
    canon: &dyn Fn(&Path) -> PathBuf,
) -> (Vec<String>, BTreeSet<PathBuf>) {
    let mut walk = ReachWalk {
        src: canon(src),
        read,
        canon,
        reached: BTreeSet::new(),
        problems: Vec::new(),
    };
    walk.walk_file(root, true, "<crate root>");
    (walk.problems, walk.reached)
}

#[test]
fn c12_no_test_path_file_in_any_production_crate_reaches_a_production_build() {
    let read = |path: &Path| std::fs::read_to_string(path).ok();
    let canon = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path));
    let mut problems = Vec::new();
    let mut reached_all = BTreeSet::new();
    let mut crates = 0;
    for root in architecture_production_source_roots::production_source_roots() {
        for entry in ["lib.rs", "main.rs"] {
            let file = root.src.join(entry);
            if file.is_file() {
                crates += 1;
                let (p, reached) = production_reach(&root.src, &file, &read, &canon);
                problems.extend(p.into_iter().map(|p| format!("[{}] {p}", root.crate_dir)));
                reached_all.extend(reached);
            }
        }
    }
    // Non-vacuity: every file holding a pinned production mint site is in a
    // production build, so a walk that misses one is broken, not clean.
    let workspace = architecture_production_source_roots::workspace_root().join("crates");
    let minters: BTreeSet<String> = expected_literals()
        .into_keys()
        .chain(expected_calls().into_keys())
        .map(|(file, ..)| file)
        .collect();
    for rel in &minters {
        if !reached_all.contains(&canon(&workspace.join(rel))) {
            problems.push(format!(
                "walk did not reach production minter `{rel}` (walk is broken)"
            ));
        }
    }
    assert!(
        crates >= 8,
        "expected every production crate root, walked {crates}"
    );
    assert!(
        minters.len() >= 10,
        "non-vacuity needs the pinned minter files, found {}",
        minters.len()
    );
    assert!(
        problems.is_empty(),
        "the family guard skips test-path files on the premise that they are test-gated:\n{}",
        problems.join("\n")
    );
}

/// One synthetic tree per route, read from memory.
#[test]
fn c12_production_reach_walk_refuses_every_route_into_a_test_file() {
    const LEAK: &str = "pub fn leak() {}";
    let run = |files: &[(&str, &str)]| {
        let map: BTreeMap<PathBuf, String> = files
            .iter()
            .map(|&(p, s)| (PathBuf::from("/ws/src").join(p), s.to_owned()))
            .collect();
        let read = move |path: &Path| map.get(path).cloned();
        let canon = |path: &Path| normalize_path(path);
        production_reach(
            Path::new("/ws/src"),
            Path::new("/ws/src/lib.rs"),
            &read,
            &canon,
        )
        .0
        .join("\n")
    };
    // 0. Positive control: test files behind cfg(test) and test-support.
    let ok = run(&[
        ("lib.rs", "mod gc; #[cfg(test)] mod tests;"),
        (
            "gc.rs",
            "#[cfg(any(test, feature = \"test-support\"))] #[path = \"gc/tests/a_test.rs\"] mod a;",
        ),
        ("gc/tests/a_test.rs", ""),
        ("tests/mod.rs", ""),
    ]);
    assert!(ok.is_empty(), "positive control must pass:\n{ok}");
    let has = |files: &[(&str, &str)], needle: &str| {
        let p = run(files);
        assert!(p.contains(needle), "expected `{needle}` in:\n{p}");
    };
    // 1. Implicit `mod tests;` → `tests/mod.rs` without a cfg. (A bare
    //    `tests.rs` is not a test path under the shared rule, so the family
    //    guard already scans it as production.)
    has(
        &[("lib.rs", "mod tests;"), ("tests/mod.rs", LEAK)],
        "test-path file `tests/mod.rs`",
    );
    // 2. `#[path]` into a tests/ directory without a cfg.
    has(
        &[
            ("lib.rs", "mod gc;"),
            ("gc.rs", "#[path = \"gc/tests/x.rs\"] mod x;"),
            ("gc/tests/x.rs", LEAK),
        ],
        "test-path file `gc/tests/x.rs`",
    );
    // 3. A `test_*` file reached through an inline module.
    has(
        &[
            ("lib.rs", "mod outer { mod test_helpers; }"),
            ("outer/test_helpers.rs", LEAK),
        ],
        "test-path file `outer/test_helpers.rs`",
    );
    // 4. `include!` of a test file from production code, plain and qualified.
    has(
        &[
            ("lib.rs", "include!(\"tests/inc.rs\");"),
            ("tests/inc.rs", LEAK),
        ],
        "test-path file `tests/inc.rs`",
    );
    has(
        &[
            ("lib.rs", "core::include!(\"tests/inc.rs\");"),
            ("tests/inc.rs", LEAK),
        ],
        "test-path file `tests/inc.rs`",
    );
    // 5. An enclosing cfg(test) inline module drops its subtree; the dropped
    //    file IS a test path, so this row dies if the drop is removed.
    let gated = run(&[
        ("lib.rs", "#[cfg(test)] mod t { mod tests; }"),
        ("t/tests/mod.rs", LEAK),
    ]);
    assert!(gated.is_empty(), "{gated}");
    // 6. Unresolvable `mod` and `cfg_attr` path fail closed.
    has(
        &[("lib.rs", "mod missing;")],
        "`mod missing` resolves to no file (fail closed)",
    );
    has(
        &[
            (
                "lib.rs",
                "#[cfg_attr(feature = \"x\", path = \"tests/y.rs\")] mod y;",
            ),
            ("y.rs", ""),
        ],
        "`mod y` has a cfg_attr path (unanalysable, fail closed)",
    );
    // 7. `#[path]` on an inline module.
    has(
        &[("lib.rs", "#[path = \"tests\"] mod a { mod b; }")],
        "inline `mod a` carries #[path] (unanalysable, fail closed)",
    );
    // 8. A module declared inside a fn body or a const block.
    has(
        &[
            ("lib.rs", "fn f() { #[path = \"tests/x.rs\"] mod x; }"),
            ("tests/x.rs", ""),
        ],
        "non-inline `mod x` inside a block (fail closed)",
    );
    has(
        &[
            (
                "lib.rs",
                "const _: () = { #[path = \"tests/x.rs\"] mod x; };",
            ),
            ("tests/x.rs", ""),
        ],
        "non-inline `mod x` inside a block (fail closed)",
    );
    // 9. Escapes out of src/: `#[path]` and `include!` with `../`.
    has(
        &[("lib.rs", "#[path = \"../tests/evil.rs\"] mod evil;")],
        "outside the crate's src/ (fail closed)",
    );
    has(
        &[("lib.rs", "fn f() { include!(\"../tests/evil.rs\"); }")],
        "outside the crate's src/ (fail closed)",
    );
    // 10. Non-literal and non-.rs `include!` / `#[path]`.
    has(
        &[(
            "lib.rs",
            "fn f() { include!(concat!(\"tests/\", \"x.rs\")); }",
        )],
        "include! with a non-literal argument (fail closed)",
    );
    has(
        &[
            ("lib.rs", "include!(\"tests/x.inc\");"),
            ("tests/x.inc", LEAK),
        ],
        "test-path file `tests/x.inc`",
    );
    has(
        &[("lib.rs", "#[path = \"tests/x\"] mod x;")],
        "#[path] `tests/x` is not a .rs file (fail closed)",
    );
    // 11. A macro-emitted module.
    has(
        &[("lib.rs", "macro_rules! m { () => { mod tests; }; } m!();")],
        "macro body loads another file (`mod name;` or `include!`) (unanalysable, fail closed)",
    );
    // 12. A test-path facade (every item test-gated) may be reached.
    let facade = run(&[
        (
            "lib.rs",
            "#[path = \"tests/facade/mod.rs\"] pub mod facade;",
        ),
        (
            "tests/facade/mod.rs",
            "#[cfg(test)] #[path = \"t.rs\"] mod tests;",
        ),
    ]);
    assert!(facade.is_empty(), "{facade}");
    // 13. `cfg_select!` arms are walked (a production arm into a test file
    //     fails; a test-only arm is dropped), and inline macro modules load
    //     no file.
    has(
        &[
            (
                "lib.rs",
                "std::cfg_select! { unix => { mod tests; } _ => {} }",
            ),
            ("tests/mod.rs", LEAK),
        ],
        "test-path file `tests/mod.rs`",
    );
    let test_arm = run(&[
        ("lib.rs", "cfg_select! { test => { mod tests; } _ => {} }"),
        ("tests/mod.rs", LEAK),
    ]);
    assert!(test_arm.is_empty(), "{test_arm}");
    let inline = run(&[(
        "lib.rs",
        "macro_rules! m { () => { mod kinds { pub struct A; } }; } m!();",
    )]);
    assert!(inline.is_empty(), "{inline}");
    // 14. Expression-position `cfg_select!`: a bare-expression arm is fine;
    //     a braced arm inside a fn that declares a file `mod` fails closed.
    let expr_arm = run(&[(
        "lib.rs",
        "fn f() -> u8 { cfg_select! { unix => 1, _ => 2 } }",
    )]);
    assert!(expr_arm.is_empty(), "{expr_arm}");
    has(
        &[(
            "lib.rs",
            "fn f() { cfg_select! { unix => { #[path = \"tests/x.rs\"] mod x; } _ => {} } }",
        )],
        "non-inline `mod x` inside a block (fail closed)",
    );
    // 15-18. The remaining fail-closed branches, each with its own row.
    has(
        &[("lib.rs", "#[path = concat!(\"tests/x.rs\")] mod x;")],
        "`mod x` has a non-literal #[path] (fail closed)",
    );
    has(
        &[("lib.rs", "#[path = \"gone.rs\"] mod gone;")],
        "unreadable module file (fail closed)",
    );
    has(&[("lib.rs", "mod bad;"), ("bad.rs", "fn (")], "unparsable");
    has(
        &[(
            "lib.rs",
            "macro_rules! m { () => { include!(\"tests/x.rs\"); }; }",
        )],
        "macro body loads another file (`mod name;` or `include!`) (unanalysable, fail closed)",
    );
}

// --- #859 item 3, class closure: stopped-world helpers hold the proof ---
//
// A `TaggedHeap` method named `*_world_stopped` runs on the premise that
// the world is stopped; `promote_and_blacken_world_stopped` relinks
// `all_objects` on it. The premise is a type: such a method takes a
// `&StoppedWorldProof`, so rustc refuses a caller that holds none. The set
// is derived from the source, not kept by hand:
// - every fn of an `impl TaggedHeap` (inherent or trait) whose name ends
//   `_world_stopped` takes a stopped-family proof parameter
//   (`STOPPED_PROOF_TYPES`), unless the production table
//   `WORLD_STOPPED_PREMISE_EXCEPTIONS` (read from source) lists it with its
//   premise; a listed exception that takes one is a stale entry;
// - every production call of a helper taking a `StoppedWorldProof` sits in
//   an inherent `impl TaggedHeap` fn that itself takes one, so no helper is
//   reached from a fresh mint inside some other fn;
// - no derived helper is named by path or inside a macro body.
// Limit: the class is "claims the premise by name". A relinking fn named
// without the suffix and taking no proof is not detected (#859).

/// The production exception table and the file that must define it.
const PREMISE_EXCEPTIONS_CONST: &str = "WORLD_STOPPED_PREMISE_EXCEPTIONS";
const PREMISE_EXCEPTIONS_FILE: &str = "neovm-value/src/tagged/gc/stopped_world_proof.rs";

/// Reads `[(name, premise), ..]` from the one production definition of
/// `WORLD_STOPPED_PREMISE_EXCEPTIONS`; any other shape fails closed.
fn premise_exceptions(sources: &[(String, String)], problems: &mut Vec<String>) -> Vec<String> {
    let lit = |expr: &syn::Expr| match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Some(s.value()),
        _ => None,
    };
    let mut found = Vec::new();
    for (file, source) in sources {
        let Ok(ast) = syn::parse_file(source) else {
            continue;
        };
        for item in &ast.items {
            if let syn::Item::Const(item) = item
                && item.ident == PREMISE_EXCEPTIONS_CONST
            {
                found.push((file.clone(), item.expr.clone()));
            }
        }
    }
    let [(file, expr)] = found.as_slice() else {
        problems.push(format!(
            "`{PREMISE_EXCEPTIONS_CONST}` must be defined exactly once at a module's top level, found {} (fail closed)",
            found.len()
        ));
        return Vec::new();
    };
    if !file.ends_with(PREMISE_EXCEPTIONS_FILE) {
        problems.push(format!(
            "`{PREMISE_EXCEPTIONS_CONST}` is defined in {file}, not {PREMISE_EXCEPTIONS_FILE} (fail closed)"
        ));
    }
    let syn::Expr::Array(array) = &**expr else {
        problems.push(format!(
            "`{PREMISE_EXCEPTIONS_CONST}` is not an array literal (fail closed)"
        ));
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in &array.elems {
        match entry {
            syn::Expr::Tuple(tuple) if tuple.elems.len() == 2 => {
                match (lit(&tuple.elems[0]), lit(&tuple.elems[1])) {
                    (Some(name), Some(premise)) if !premise.trim().is_empty() => names.push(name),
                    _ => problems.push(format!(
                        "`{PREMISE_EXCEPTIONS_CONST}` entry is not (name, non-empty premise) string literals (fail closed)"
                    )),
                }
            }
            _ => problems.push(format!(
                "`{PREMISE_EXCEPTIONS_CONST}` entry is not a 2-tuple (fail closed)"
            )),
        }
    }
    names
}

/// The stopped-world capability types a `_world_stopped` helper may take.
const STOPPED_PROOF_TYPES: [&str; 3] = [
    "StoppedWorldProof",
    "StoppedMutatorsProof",
    "ParallelStoppedMutatorsProof",
];

/// What a signature's parameters prove. Ordered weakest first, so a name
/// defined more than once (cfg variants) keeps its weakest definition.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum StoppedProof {
    None,
    /// `StoppedMutatorsProof` or `ParallelStoppedMutatorsProof` only.
    Mutators,
    /// A `StoppedWorldProof`: the C12 relinking premise.
    World,
}

/// Non-vacuity floor: the helpers #859 item 3 named stay in the derived set.
const WORLD_STOPPED_FLOOR: [&str; 4] = [
    "promote_and_blacken_world_stopped",
    "join_old_boxes_for_partition_world_stopped",
    "recompute_old_bytes_world_stopped",
    "refresh_generation_major_baseline_world_stopped",
];

const WORLD_STOPPED_SUFFIX: &str = "_world_stopped";

fn stopped_proof(sig: &syn::Signature) -> StoppedProof {
    struct Mentions(StoppedProof);
    impl<'ast> Visit<'ast> for Mentions {
        fn visit_path_segment(&mut self, seg: &'ast syn::PathSegment) {
            let kind = if seg.ident == "StoppedWorldProof" {
                StoppedProof::World
            } else if STOPPED_PROOF_TYPES.iter().any(|ty| seg.ident == ty) {
                StoppedProof::Mutators
            } else {
                StoppedProof::None
            };
            self.0 = self.0.max(kind);
            visit::visit_path_segment(self, seg);
        }
    }
    let mut mentions = Mentions(StoppedProof::None);
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(typed) = arg {
            mentions.visit_type(&typed.ty);
        }
    }
    mentions.0
}

/// Identifier tokens of a macro body, recursively. Literals are skipped, so
/// a message string that names a helper is not a reference to it.
fn macro_idents(tokens: proc_macro2::TokenStream, out: &mut Vec<String>) {
    for tree in tokens {
        match tree {
            proc_macro2::TokenTree::Ident(ident) => out.push(ident.to_string()),
            proc_macro2::TokenTree::Group(group) => macro_idents(group.stream(), out),
            proc_macro2::TokenTree::Punct(_) | proc_macro2::TokenTree::Literal(_) => {}
        }
    }
}

/// One derived `_world_stopped` name: its weakest definition's proof, and
/// where that definition is and which parameter types it has.
struct WorldStoppedSig {
    proof: StoppedProof,
    at: String,
    params: String,
}

/// Typed parameters rendered as `name: Type`, for the failure message.
fn parameter_types(sig: &syn::Signature) -> String {
    fn ty(t: &syn::Type) -> String {
        match t {
            syn::Type::Reference(r) => {
                format!(
                    "&{}{}",
                    if r.mutability.is_some() { "mut " } else { "" },
                    ty(&r.elem)
                )
            }
            syn::Type::Ptr(p) => {
                format!(
                    "*{} {}",
                    match p.mutability {
                        syn::PointerMutability::Mut(_) => "mut",
                        syn::PointerMutability::Const(_) => "const",
                    },
                    ty(&p.elem)
                )
            }
            syn::Type::Path(p) => p
                .path
                .segments
                .iter()
                .map(|seg| seg.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
            _ => "_".to_owned(),
        }
    }
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(typed) => Some(match &*typed.pat {
                syn::Pat::Ident(pat) => format!("{}: {}", pat.ident, ty(&typed.ty)),
                _ => format!("_: {}", ty(&typed.ty)),
            }),
            syn::FnArg::Receiver(_) => None,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Pass 1: every `_world_stopped` fn of an `impl TaggedHeap`, with whether
/// each of its definitions takes the proof (a cfg variant without it counts).
struct WorldStoppedSigs<'a> {
    file: &'a str,
    excluded: u32,
    impl_stack: Vec<String>,
    sigs: &'a mut BTreeMap<String, WorldStoppedSig>,
}

impl<'ast> Visit<'ast> for WorldStoppedSigs<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_mod(self, item);
        self.excluded -= u32::from(ex);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        self.impl_stack
            .push(type_name(&item.self_ty).unwrap_or_default());
        visit::visit_item_impl(self, item);
        self.impl_stack.pop();
        self.excluded -= u32::from(ex);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let saved = std::mem::take(&mut self.impl_stack);
        visit::visit_item_fn(self, item);
        self.impl_stack = saved;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let ex = production_excluded(&item.attrs);
        let name = item.sig.ident.to_string();
        if self.excluded == 0
            && !ex
            && self.impl_stack.last().is_some_and(|ty| ty == "TaggedHeap")
            && name.ends_with(WORLD_STOPPED_SUFFIX)
        {
            let at = format!("{}:{}", self.file, item.sig.ident.span().start().line);
            let proof = stopped_proof(&item.sig);
            let params = parameter_types(&item.sig);
            self.sigs
                .entry(name)
                .and_modify(|sig| {
                    // Keep the weakest definition (cfg variants).
                    if proof < sig.proof {
                        *sig = WorldStoppedSig {
                            proof,
                            at: at.clone(),
                            params: params.clone(),
                        };
                    }
                })
                .or_insert(WorldStoppedSig { proof, at, params });
        }
        self.excluded += u32::from(ex);
        visit::visit_impl_item_fn(self, item);
        self.excluded -= u32::from(ex);
    }
}

struct RelinkVisitor<'a> {
    file: &'a str,
    helpers: &'a BTreeMap<String, WorldStoppedSig>,
    excluded: u32,
    impl_stack: Vec<String>,
    /// Enclosing fn names, each with what its signature proves.
    fn_stack: Vec<(String, StoppedProof)>,
    /// Depth of body closures of pinned `StoppedWorldProof` mints
    /// (`C12_FAMILY_CALLS`) around the current expression; a nested fn item
    /// resets it.
    mint_closure: u32,
    calls: &'a mut BTreeMap<String, usize>,
    problems: &'a mut Vec<String>,
}

impl RelinkVisitor<'_> {
    /// A helper of the C12 relinking class: every definition takes a
    /// `StoppedWorldProof`. A listed premise exception never qualifies: one
    /// that took a proof is already reported stale by the signature rule.
    fn is_helper(&self, name: &str) -> bool {
        self.helpers
            .get(name)
            .is_some_and(|sig| sig.proof == StoppedProof::World)
    }

    /// Only an inherent `impl TaggedHeap` method that itself takes the proof
    /// may call a helper; a free fn, another type's method, a trait impl, or
    /// a proofless method minting its own proof does not qualify.
    ///
    /// Deliberately `StoppedWorldProof`-only, unlike the signature rule,
    /// which accepts all three families: the relinking premise (sweep
    /// complete, no unfinished mark, launch epoch unchanged) is what only a
    /// `StoppedWorldProof` asserts. A `StoppedMutatorsProof` helper states a
    /// weaker premise and is outside this layer.
    fn caller_allowed(&self) -> bool {
        // The body closure of a pinned production mint holds the proof as its
        // parameter; the family pins fix where such closures may exist.
        self.mint_closure > 0
            || self.impl_stack.last().is_some_and(|ty| ty == "TaggedHeap")
                && self
                    .fn_stack
                    .last()
                    .is_some_and(|(_, proof)| *proof == StoppedProof::World)
    }
}

impl<'ast> Visit<'ast> for RelinkVisitor<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        visit::visit_item_mod(self, item);
        self.excluded -= u32::from(ex);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        // A trait impl's method is not an inherent proof holder.
        let self_ty = if item.trait_.is_none() {
            type_name(&item.self_ty).unwrap_or_default()
        } else {
            String::new()
        };
        self.impl_stack.push(self_ty);
        visit::visit_item_impl(self, item);
        self.impl_stack.pop();
        self.excluded -= u32::from(ex);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        let saved = std::mem::take(&mut self.impl_stack);
        let saved_mint = std::mem::take(&mut self.mint_closure);
        self.fn_stack
            .push((item.sig.ident.to_string(), stopped_proof(&item.sig)));
        visit::visit_item_fn(self, item);
        self.fn_stack.pop();
        self.mint_closure = saved_mint;
        self.impl_stack = saved;
        self.excluded -= u32::from(ex);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let ex = production_excluded(&item.attrs);
        self.excluded += u32::from(ex);
        let saved_mint = std::mem::take(&mut self.mint_closure);
        self.fn_stack
            .push((item.sig.ident.to_string(), stopped_proof(&item.sig)));
        visit::visit_impl_item_fn(self, item);
        self.fn_stack.pop();
        self.mint_closure = saved_mint;
        self.excluded -= u32::from(ex);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let mut idents = Vec::new();
        macro_idents(mac.tokens.clone(), &mut idents);
        if self.excluded == 0
            && let Some(helper) = idents.iter().find(|ident| self.is_helper(ident))
        {
            self.problems.push(format!(
                "{}:{}: macro body names stopped-world helper `{helper}` (unanalysable, fail closed)",
                self.file,
                mac.path.span().start().line
            ));
        }
        visit::visit_macro(self, mac);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if self.excluded == 0
            && let syn::Item::Verbatim(tokens) = item
        {
            self.problems.push(format!(
                "{}: unparsed item `{tokens}` (fail closed)",
                self.file
            ));
        }
        visit::visit_item(self, item);
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if self.excluded == 0
            && let syn::Expr::Verbatim(tokens) = expr
        {
            self.problems.push(format!(
                "{}: unparsed expression `{tokens}` (fail closed)",
                self.file
            ));
        }
        visit::visit_expr(self, expr);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let mint = match &*call.func {
            syn::Expr::Path(path) => {
                let segs: Vec<String> = path
                    .path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect();
                match segs.as_slice() {
                    [.., ty, ctor]
                        if ty == "StoppedWorldProof"
                            && (ctor == "with_stopped_world" || ctor == "with_all_stopped") =>
                    {
                        Some(format!("StoppedWorldProof::{ctor}"))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        let enclosing = match (self.impl_stack.last(), self.fn_stack.last()) {
            (Some(ty), Some((func, _))) if !ty.is_empty() => format!("{ty}::{func}"),
            (_, Some((func, _))) => func.clone(),
            _ => String::new(),
        };
        let pinned = mint.as_ref().is_some_and(|callee| {
            C12_FAMILY_CALLS
                .iter()
                .any(|&(file, func, pinned_callee, _)| {
                    file == self.file && func == enclosing && pinned_callee == callee
                })
        });
        if !pinned {
            visit::visit_expr_call(self, call);
            return;
        }
        self.visit_expr(&call.func);
        for arg in &call.args {
            if let syn::Expr::Closure(_) = arg {
                self.mint_closure += 1;
                self.visit_expr(arg);
                self.mint_closure -= 1;
            } else {
                self.visit_expr(arg);
            }
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if self.excluded == 0 && self.is_helper(&name) {
            *self.calls.entry(name.clone()).or_default() += 1;
            if !self.caller_allowed() {
                let caller = self
                    .fn_stack
                    .last()
                    .map(|(caller, _)| caller.clone())
                    .unwrap_or_else(|| "<module>".to_owned());
                self.problems.push(format!(
                    "{}:{}: `{caller}` calls stopped-world helper `{name}` without holding a StoppedWorldProof parameter",
                    self.file,
                    call.method.span().start().line
                ));
            }
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        if self.excluded == 0
            && let Some(last) = expr.path.segments.last()
            && self.is_helper(&last.ident.to_string())
        {
            // Covers `Self::helper(..)` / `TaggedHeap::helper(..)` and a
            // helper taken as a value; both bypass the method-call check.
            self.problems.push(format!(
                "{}:{}: stopped-world helper `{}` named by path (UFCS call or function value), fail closed",
                self.file,
                last.ident.span().start().line,
                last.ident
            ));
        }
        visit::visit_expr_path(self, expr);
    }
}

/// Both passes over `sources`: the derived helper set, then every rule.
/// Returns the derived set and the production call counts per helper.
fn world_stopped_problems(
    sources: &[(String, String)],
    exceptions: &[String],
    problems: &mut Vec<String>,
) -> (BTreeMap<String, WorldStoppedSig>, BTreeMap<String, usize>) {
    let parsed: Vec<(&str, syn::File)> = sources
        .iter()
        .map(|(file, source)| {
            let ast =
                syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
            (file.as_str(), ast)
        })
        .filter(|(_, ast)| !production_excluded(&ast.attrs))
        .collect();
    let mut sigs = BTreeMap::new();
    for (file, ast) in &parsed {
        WorldStoppedSigs {
            file,
            excluded: 0,
            impl_stack: Vec::new(),
            sigs: &mut sigs,
        }
        .visit_file(ast);
    }
    for (name, sig) in &sigs {
        let (at, params) = (&sig.at, &sig.params);
        match (exceptions.iter().any(|exception| exception == name), sig.proof) {
            (false, StoppedProof::None) => problems.push(format!(
                "{at}: `{name}` is named `{WORLD_STOPPED_SUFFIX}` but takes no stopped-world proof parameter (accepted: {}; its parameter types: [{params}])",
                STOPPED_PROOF_TYPES.join(", ")
            )),
            (true, StoppedProof::Mutators | StoppedProof::World) => problems.push(format!(
                "{at}: exception `{name}` now takes a stopped-world proof (stale exception entry)"
            )),
            (false, StoppedProof::Mutators | StoppedProof::World) | (true, StoppedProof::None) => {}
        }
    }
    let mut calls = BTreeMap::new();
    for (file, ast) in &parsed {
        RelinkVisitor {
            file,
            helpers: &sigs,
            excluded: 0,
            impl_stack: Vec::new(),
            fn_stack: Vec::new(),
            mint_closure: 0,
            calls: &mut calls,
            problems,
        }
        .visit_file(ast);
    }
    (sigs, calls)
}

#[test]
fn c12_relinking_helpers_are_reached_only_through_proof_gated_entries() {
    let sources: Vec<(String, String)> =
        architecture_production_source_roots::production_source_files()
            .into_iter()
            .filter(|file| !file.is_test_path())
            .map(|file| {
                let source = std::fs::read_to_string(&file.path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
                (display_path(&file), source)
            })
            .collect();
    let mut problems = Vec::new();
    let exceptions = premise_exceptions(&sources, &mut problems);
    let (sigs, calls) = world_stopped_problems(&sources, &exceptions, &mut problems);
    // Non-vacuity: the named helpers are still derived, take the proof, and
    // have a production caller; every exception still exists.
    for helper in WORLD_STOPPED_FLOOR {
        if !sigs
            .get(helper)
            .is_some_and(|sig| sig.proof == StoppedProof::World)
        {
            problems.push(format!(
                "floor helper `{helper}` is not a proof-taking `TaggedHeap::*{WORLD_STOPPED_SUFFIX}` fn (renamed?)"
            ));
        }
        if !calls.contains_key(helper) {
            problems.push(format!("floor helper `{helper}` has no production call"));
        }
    }
    for name in &exceptions {
        if !sigs.contains_key(name) {
            problems.push(format!(
                "exception `{name}` no longer exists (stale exception entry)"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "stopped-world helpers escaped their proof:\n{}",
        problems.join("\n")
    );
}

#[test]
fn c12_relinking_helper_rule_refuses_ungated_callers() {
    let run = |source: &str| {
        let mut problems = Vec::new();
        world_stopped_problems(
            &[(
                "neovm-value/src/tagged/gc/synthetic.rs".to_owned(),
                source.to_owned(),
            )],
            &["publish_persistent_remembered_world_stopped".to_owned()],
            &mut problems,
        );
        problems.join("\n")
    };
    const H: &str = "fn h_world_stopped(&mut self, _s: &StoppedWorldProof<'_>) {}";
    // 0. Positive control: an entry and a helper chain all holding the proof.
    let ok = run(&format!("impl TaggedHeap {{
        pub unsafe fn finish_first_partition_cycle(&mut self, s: &StoppedWorldProof<'_>) {{ self.g_world_stopped(s); }}
        fn g_world_stopped(&mut self, s: &StoppedWorldProof<'_>) {{ self.h_world_stopped(s); }}
        {H}
    }}"));
    assert!(ok.is_empty(), "positive control must pass:\n{ok}");
    // 1. A suffixed helper without the proof, inherent or trait impl.
    let p = run("impl TaggedHeap { fn relink_world_stopped(&mut self) {} }");
    assert!(p.contains("`relink_world_stopped` is named `_world_stopped` but takes no stopped-world proof parameter"), "{p}");
    let p = run("impl Bypass for TaggedHeap { fn relink_world_stopped(&mut self) {} }");
    assert!(
        p.contains(
            "`relink_world_stopped` is named `_world_stopped` but takes no stopped-world proof"
        ),
        "{p}"
    );
    // 1b. One cfg variant without the proof is enough.
    let p = run("impl TaggedHeap {
        #[cfg(unix)] fn relink_world_stopped(&mut self, _s: &StoppedWorldProof<'_>) {}
        #[cfg(not(unix))] fn relink_world_stopped(&mut self) {}
    }");
    assert!(
        p.contains("`relink_world_stopped` is named `_world_stopped` but takes no"),
        "{p}"
    );
    // 1c. The message names the accepted families and the actual parameter
    //     types, so a missing proof reads differently from a wrong family.
    let p = run("impl TaggedHeap { fn relink_world_stopped(&mut self, ptr: *mut GcHeader) {} }");
    assert!(
        p.contains("(accepted: StoppedWorldProof, StoppedMutatorsProof, ParallelStoppedMutatorsProof; its parameter types: [ptr: *mut GcHeader])"),
        "{p}"
    );
    let p =
        run("impl TaggedHeap { fn relink_world_stopped(&mut self, _: &StoppedClaimsProof) {} }");
    assert!(
        p.contains("its parameter types: [_: &StoppedClaimsProof])"),
        "{p}"
    );
    // 2. A stale exception: it took the proof.
    let p = run(
        "impl TaggedHeap { fn publish_persistent_remembered_world_stopped(&mut self, _s: &StoppedWorldProof<'_>) {} }",
    );
    assert!(p.contains("exception `publish_persistent_remembered_world_stopped` now takes a stopped-world proof"), "{p}");
    // 3. A proofless method minting its own proof to reach a helper.
    let p = run(&format!("impl TaggedHeap {{ {H}
        pub fn shortcut(&mut self) {{ StoppedWorldProof::with_stopped_world(self, |heap, s| heap.h_world_stopped(s)); }}
    }}"));
    assert!(p.contains("`shortcut` calls stopped-world helper `h_world_stopped` without holding a StoppedWorldProof parameter"), "{p}");
    // 4. UFCS and function-value spellings fail closed.
    let p = run(&format!(
        "impl TaggedHeap {{ {H}
        fn ufcs(&mut self, s: &StoppedWorldProof<'_>) {{ Self::h_world_stopped(self, s); }} }}"
    ));
    assert!(
        p.contains("stopped-world helper `h_world_stopped` named by path"),
        "{p}"
    );
    let p = run(&format!(
        "impl TaggedHeap {{ {H} }} fn f() {{ let g = TaggedHeap::h_world_stopped; }}"
    ));
    assert!(
        p.contains("stopped-world helper `h_world_stopped` named by path"),
        "{p}"
    );
    // 5. A free fn, another type's method, or a trait impl holding a proof
    //    is not an inherent `TaggedHeap` proof holder.
    let p = run(&format!("impl TaggedHeap {{ {H} }}
        pub fn finalize_collection(h: &mut TaggedHeap, s: &StoppedWorldProof<'_>) {{ h.h_world_stopped(s); }}"));
    assert!(
        p.contains(
            "`finalize_collection` calls stopped-world helper `h_world_stopped` without holding"
        ),
        "{p}"
    );
    let p = run(&format!("impl TaggedHeap {{ {H} }}
        impl Other {{ fn finalize_collection(&mut self, h: &mut TaggedHeap, s: &StoppedWorldProof<'_>) {{ h.h_world_stopped(s); }} }}"));
    assert!(
        p.contains("`finalize_collection` calls stopped-world helper"),
        "{p}"
    );
    let p = run(&format!("impl TaggedHeap {{ {H} }}
        impl Bypass for TaggedHeap {{ fn finalize_collection(&mut self, s: &StoppedWorldProof<'_>) {{ self.h_world_stopped(s); }} }}"));
    assert!(
        p.contains(
            "`finalize_collection` calls stopped-world helper `h_world_stopped` without holding"
        ),
        "{p}"
    );
    // 6. A macro body that names a helper is unanalysable.
    let p = run(&format!(
        "impl TaggedHeap {{ {H} }}
        macro_rules! relink {{ ($h:expr, $s:expr) => {{ $h.h_world_stopped($s) }}; }}"
    ));
    assert!(
        p.contains(
            "macro body names stopped-world helper `h_world_stopped` (unanalysable, fail closed)"
        ),
        "{p}"
    );
    // 7. Unparsed items and expressions fail closed.
    let p = run("pub macro relink() {}");
    assert!(p.contains("unparsed item"), "{p}");
    let p = run("fn f() { let _ = builtin # offset_of(TaggedHeap, mark_parity); }");
    assert!(p.contains("unparsed expression"), "{p}");
    // 8. The exception is not a helper: its proofless caller is allowed.
    let p = run("impl TaggedHeap { fn publish_persistent_remembered_world_stopped(&mut self) {}
        pub fn finish_concurrent_mark(&mut self) { self.publish_persistent_remembered_world_stopped(); } }");
    assert!(p.is_empty(), "{p}");
    // 10. A helper taking another stopped-family proof satisfies the
    //     signature rule, and is outside the relinking call-site class.
    let p = run(
        "impl TaggedHeap { fn entries_world_stopped(&mut self, _: &StoppedMutatorsProof) {}
        fn fold(&mut self, p: &StoppedMutatorsProof) { self.entries_world_stopped(p); } }",
    );
    assert!(p.is_empty(), "{p}");
    // 11. A string literal naming a helper inside a macro is not a reference.
    let p = run(&format!("impl TaggedHeap {{ {H}
        fn g(&mut self, s: &StoppedWorldProof<'_>) {{ assert!(true, \"h_world_stopped must not run\"); self.h_world_stopped(s); }} }}"));
    assert!(p.is_empty(), "{p}");
    // 12. The body closure of a pinned production mint holds the proof: the
    //     same source is allowed at its pinned (file, fn) and refused anywhere
    //     else, and a fn item nested in the closure is not covered.
    let run_at = |file: &str, source: &str| {
        let mut problems = Vec::new();
        world_stopped_problems(
            &[(file.to_owned(), source.to_owned())],
            &["publish_persistent_remembered_world_stopped".to_owned()],
            &mut problems,
        );
        problems.join("\n")
    };
    let pinned_file = "neovm-value/src/tagged/gc/mutator_registration/collection.rs";
    let job = format!(
        "impl TaggedHeap {{ {H} }}
         impl StoppedCollectorJob {{ fn run(self) {{
             StoppedWorldProof::with_all_stopped(heap, self.p, |heap, s| heap.h_world_stopped(s));
         }} }}"
    );
    let p = run_at(pinned_file, &job);
    assert!(p.is_empty(), "{p}");
    let p = run_at("neovm-value/src/tagged/gc/synthetic.rs", &job);
    assert!(
        p.contains("`run` calls stopped-world helper `h_world_stopped` without holding"),
        "{p}"
    );
    let p = run_at(
        pinned_file,
        &job.replace("impl StoppedCollectorJob", "impl OtherJob"),
    );
    assert!(
        p.contains("`run` calls stopped-world helper `h_world_stopped` without holding"),
        "{p}"
    );
    let nested = format!(
        "impl TaggedHeap {{ {H} }}
         impl StoppedCollectorJob {{ fn run(self) {{
             StoppedWorldProof::with_all_stopped(heap, self.p, |heap, s| {{
                 fn inner(h: &mut TaggedHeap, s: &StoppedWorldProof<'_>) {{ h.h_world_stopped(s); }}
             }});
         }} }}"
    );
    let p = run_at(pinned_file, &nested);
    assert!(
        p.contains("`inner` calls stopped-world helper `h_world_stopped` without holding"),
        "{p}"
    );
    // 9. Test-gated definitions and callers are out of scope.
    let gated = run(&format!(
        "impl TaggedHeap {{ {H}
        #[cfg(test)] fn relink_world_stopped(&mut self) {{}}
        #[cfg(test)] pub fn t(&mut self) {{ self.h_world_stopped(todo!()); }} }}"
    ));
    assert!(gated.is_empty(), "{gated}");
}

#[test]
fn c12_premise_exception_table_is_read_fail_closed() {
    let read = |files: &[(&str, &str)]| {
        let sources: Vec<(String, String)> = files
            .iter()
            .map(|(file, source)| ((*file).to_owned(), (*source).to_owned()))
            .collect();
        let mut problems = Vec::new();
        let names = premise_exceptions(&sources, &mut problems);
        (names, problems.join("\n"))
    };
    let home = "neovm-value/src/tagged/gc/stopped_world_proof.rs";
    // 0. Positive control: names are read, premises are required.
    let (names, p) = read(&[(
        home,
        r#"const WORLD_STOPPED_PREMISE_EXCEPTIONS: [(&str, &str); 1] = [("a_world_stopped", "premise")];"#,
    )]);
    assert_eq!(
        (names, p.as_str()),
        (vec!["a_world_stopped".to_owned()], "")
    );
    // 1. Missing, or defined twice.
    let (_, p) = read(&[(home, "const OTHER: u8 = 0;")]);
    assert!(
        p.contains("must be defined exactly once at a module's top level, found 0"),
        "{p}"
    );
    let one = r#"const WORLD_STOPPED_PREMISE_EXCEPTIONS: [(&str, &str); 0] = [];"#;
    let (_, p) = read(&[(home, one), ("neovm-value/src/other.rs", one)]);
    assert!(p.contains("found 2"), "{p}");
    // 2. Defined in another file.
    let (_, p) = read(&[("neovm-value/src/other.rs", one)]);
    assert!(
        p.contains("is defined in neovm-value/src/other.rs, not"),
        "{p}"
    );
    // 3. Not an array literal, or an entry of another shape.
    let (_, p) = read(&[(
        home,
        "const WORLD_STOPPED_PREMISE_EXCEPTIONS: [(&str, &str); 0] = make();",
    )]);
    assert!(p.contains("is not an array literal"), "{p}");
    let (_, p) = read(&[(
        home,
        r#"const WORLD_STOPPED_PREMISE_EXCEPTIONS: [&str; 1] = ["a_world_stopped"];"#,
    )]);
    assert!(p.contains("entry is not a 2-tuple"), "{p}");
    let (_, p) = read(&[(
        home,
        r#"const WORLD_STOPPED_PREMISE_EXCEPTIONS: [(&str, &str); 1] = [(NAME, "premise")];"#,
    )]);
    assert!(
        p.contains("entry is not (name, non-empty premise) string literals"),
        "{p}"
    );
    // 4. An entry without its premise.
    let (_, p) = read(&[(
        home,
        r#"const WORLD_STOPPED_PREMISE_EXCEPTIONS: [(&str, &str); 1] = [("a_world_stopped", " ")];"#,
    )]);
    assert!(
        p.contains("entry is not (name, non-empty premise) string literals"),
        "{p}"
    );
}
