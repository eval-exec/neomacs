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
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::gc_collect_from_current_roots_body", "StoppedWorldProof::with_stopped_world", 7),
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::start_concurrent_mark", "SingleMutatorWorld::from_heap", 1),
    ("neovm-eval/src/emacs_core/runtime/eval/command_loop.rs", "Context::terminate_concurrent_mark", "StoppedWorldProof::with_stopped_world", 1),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::finish_concurrent_mark", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::raw_concurrent_begin", "StoppedWorldProof::with_stopped_world", 1),
    ("neovm-value/src/tagged/gc/concurrent.rs", "TaggedHeap::raw_launch_concurrent_mark", "SingleMutatorWorld::from_heap", 3),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "CollectorQuiescence::check", "SingleMutatorWorld::from_heap", 1),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "TaggedHeap::drain_to_quiescent", "SingleMutatorWorld::from_heap", 2),
    ("neovm-value/src/tagged/gc/facade_mark.rs", "TaggedHeap::drain_to_quiescent", "StoppedWorldProof::with_stopped_world", 2),
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
