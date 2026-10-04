//! Compiled BLV fallback bookkeeping must not invent Lisp reads.
//!
//! These are work-contract tests for the first cold gap miss, not regressions
//! claimed red on the older returning-refinement path. Every owner is an
//! actual cached cell of a real localized symbol; no BLV field is fabricated.

use super::*;
use crate::emacs_core::error::EvalResult;
use crate::emacs_core::eval::{SubrEntry, register_global_subr_entry};
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test, is_observed,
};
use crate::tagged::header::{ConsCell, SubrDispatchKind, SubrFn};
use crate::tagged::mutate::LispCollectionRevision;
use crate::tagged::value::TAG_MASK;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn cached_cell(context: &Context, name: &str, local: bool) -> Value {
    let symbol = context.obarray().get_by_id(intern(name)).expect("symbol");
    assert_eq!(symbol.redirect(), SymbolRedirect::Localized);
    // SAFETY: the localized symbol retains its BLV and the fixture loaded
    // this buffer's actual cache, without changing any production fields.
    let blv = unsafe { &*symbol.val.blv };
    assert_eq!(blv.found, local);
    if !local {
        assert_eq!(blv.valcell, blv.defcell);
    }
    blv.valcell
}

fn raw_cdr(owner: Value) -> Value {
    assert!(owner.is_cons());
    // SAFETY: fixture roots retain this actual BLV Cons. This inspection is
    // test bookkeeping, with no allocation or safe point; a Lisp read would
    // intentionally mark the target and defeat the setter-only assertion.
    unsafe { (*((owner.bits() & !TAG_MASK) as *const ConsCell)).load_cdr() }
}

fn fixture(local: bool) -> (Context, &'static str, Value, Value, Value, Value) {
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let mut context = context(false);
    context.specpdl.reserve(32);
    context.jit_bind_stack.reserve(16);
    let names = [
        "fx1-cold-blv-a",
        "fx1-cold-blv-b",
        "fx1-cold-blv-c",
        "fx1-cold-blv-d",
    ];
    let definitions = names
        .iter()
        .enumerate()
        .map(|(index, name)| format!("(defvar {name} {})", 43 + index))
        .collect::<Vec<_>>()
        .join(" ");
    let localizations = names
        .iter()
        .map(|name| format!("(make-local-variable '{name})"))
        .collect::<Vec<_>>()
        .join(" ");
    let setup = if local {
        format!(
            "(progn {definitions} {localizations} (list {}))",
            names.join(" ")
        )
    } else {
        format!(
            "(progn {definitions} (save-current-buffer (set-buffer (get-buffer-create \" fx1-cold-blv-other\")) {localizations}) (list {}))",
            names.join(" ")
        )
    };
    context.eval_str(&setup).expect("real localized cells");
    let mut owners = names.map(|name| (name, cached_cell(&context, name, local)));
    owners.sort_unstable_by_key(|(_, owner)| owner.bits() & !TAG_MASK);
    let [(_, lower), (name, target), (_, new_anchor), (_, upper)] = owners;
    assert!((lower.bits() & !TAG_MASK) < (target.bits() & !TAG_MASK));
    assert!((target.bits() & !TAG_MASK) < (new_anchor.bits() & !TAG_MASK));
    assert!((new_anchor.bits() & !TAG_MASK) < (upper.bits() & !TAG_MASK));
    for owner in [lower, target, new_anchor, upper] {
        context.push_specpdl_root(owner);
        assert!(!is_observed(owner.bits()));
    }
    let (_, reads) = capture(|| {
        lower.cons_cdr();
        upper.cons_cdr();
    });
    assert!(reads.expect("captured endpoints").unchanged());
    assert!(
        context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(target.bits() & !TAG_MASK)
    );
    (context, name, target, lower, new_anchor, upper)
}

fn check_setter(local: bool) {
    let (mut context, name, target, _, _, _) = fixture(local);
    let _mode = ObservedMode;
    super::super::inline_vars::reset_inline_var_sites();
    let leaf = compile_blv(
        &context,
        &[Op::StackRef(0), Op::VarSet(0), Op::StackRef(0), Op::Return],
        &[Value::symbol(name)],
        1,
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Set),
        1
    );
    let shims = super::super::shims::VARSET_SHIM_CALLS.with(|count| count.get());
    let revision = LispCollectionRevision::current();
    let value = Value::make_int(71);
    let (result, reads) = capture(|| native(&mut context, &leaf, &[value]));
    assert_eq!(result, value);
    assert_eq!(raw_cdr(target), value);
    assert_eq!(
        super::super::shims::VARSET_SHIM_CALLS.with(|count| count.get()) - shims,
        1,
        "first in-window refusal uses the compiled cached setter"
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        0
    );
    assert!(!is_observed(target.bits()));
    let reads = reads.expect("a setter returning its supplied value is coherent");
    let shims = super::super::shims::VARSET_SHIM_CALLS.with(|count| count.get());
    assert_eq!(
        native(&mut context, &leaf, &[Value::make_int(73)]),
        Value::make_int(73)
    );
    assert_eq!(
        super::super::shims::VARSET_SHIM_CALLS.with(|count| count.get()) - shims,
        0,
        "the proved empty gap keeps the next store inline"
    );
    assert!(reads.unchanged(), "the setter created no owner dependency");
    let (_, dependency) = capture(|| target.cons_cdr());
    let dependency = dependency.expect("real owner read");
    let revision = LispCollectionRevision::current();
    assert_eq!(
        native(&mut context, &leaf, &[Value::make_int(79)]),
        Value::make_int(79)
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );
    assert!(!dependency.unchanged());
}

fn inspect_bound_and_close_gap(context: &mut Context) -> EvalResult {
    let target = global(context, "fx1-cold-blv-target");
    assert_eq!(raw_cdr(target), Value::make_int(83));
    let anchor = global(context, "fx1-cold-blv-new-anchor");
    anchor.cons_cdr();
    assert!(
        context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(target.bits() & !TAG_MASK),
        "a new real dependency inside the first gap closes it before restoration"
    );
    Ok(Value::make_int(83))
}

fn check_binding(local: bool) {
    let (mut context, name, target, _, new_anchor, _) = fixture(local);
    let _mode = ObservedMode;
    context.assign("fx1-cold-blv-target", target);
    context.assign("fx1-cold-blv-new-anchor", new_anchor);
    let observer = intern("fx1-cold-blv-inspect-bound");
    register_global_subr_entry(
        observer,
        SubrEntry {
            function: Some(SubrFn::A0(inspect_bound_and_close_gap)),
            min_args: 0,
            max_args: Some(0),
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
        },
    );
    context.set_function(
        "fx1-cold-blv-inspect-bound",
        Value::subr_from_sym_id(observer),
    );
    super::super::inline_vars::reset_inline_var_sites();
    let leaf = compile_blv(
        &context,
        &[
            Op::Constant(1),
            Op::VarBind(0),
            Op::Constant(2),
            Op::Call(0),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            Value::symbol(name),
            Value::make_int(83),
            Value::from_sym_id(observer),
        ],
        0,
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Bind),
        1
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Unbind),
        1
    );
    let old = raw_cdr(target);
    let depths = (context.specpdl.len(), context.jit_bind_stack.len());
    let binds = super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get());
    let unbinds = super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get());
    let revision = LispCollectionRevision::current();
    let (result, reads) = capture(|| native(&mut context, &leaf, &[]));
    assert_eq!(result, Value::make_int(83));
    assert_eq!(raw_cdr(target), old);
    assert_eq!(
        (context.specpdl.len(), context.jit_bind_stack.len()),
        depths
    );
    assert_eq!(
        super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get()) - binds,
        1
    );
    assert_eq!(
        super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get()) - unbinds,
        1
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        0
    );
    assert!(
        !is_observed(target.bits()),
        "saving the old binding is implementation bookkeeping"
    );
    let reads = reads.expect("only the body anchor was observed");
    let setter = compile_blv(
        &context,
        &[Op::StackRef(0), Op::VarSet(0), Op::StackRef(0), Op::Return],
        &[Value::symbol(name)],
        1,
    );
    assert_eq!(
        native(&mut context, &setter, &[Value::make_int(89)]),
        Value::make_int(89)
    );
    assert!(
        reads.unchanged(),
        "later stores to the unobserved binding cell cannot stale its independent body read"
    );
    let (_, dependency) = capture(|| target.cons_cdr());
    let dependency = dependency.expect("real owner read after the implementation-only saved load");
    let revision = LispCollectionRevision::current();
    assert_eq!(native(&mut context, &leaf, &[]), Value::make_int(83));
    assert_eq!(raw_cdr(target), Value::make_int(89));
    assert_eq!(
        (context.specpdl.len(), context.jit_bind_stack.len()),
        depths
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        2
    );
    assert!(!dependency.unchanged());
}

#[test]
fn gen0_unobserved_local_blv_first_cold_setter_does_not_capture_owner() {
    check_setter(true);
}

#[test]
fn gen0_unobserved_default_blv_first_cold_setter_does_not_capture_owner() {
    check_setter(false);
}

#[test]
fn gen0_unobserved_local_blv_cold_bind_and_restore_do_not_capture_saved_owner() {
    check_binding(true);
}

#[test]
fn gen0_unobserved_default_blv_cold_bind_and_restore_do_not_capture_saved_owner() {
    check_binding(false);
}
