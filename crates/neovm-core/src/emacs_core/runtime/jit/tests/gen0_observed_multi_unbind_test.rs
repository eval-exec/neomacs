//! A refinement inside one unbind suffix must not tear its window snapshot.

use super::*;
use crate::emacs_core::error::EvalResult;
use crate::emacs_core::eval::{SubrEntry, register_global_subr_entry};
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test, is_observed,
};
use crate::tagged::header::{SubrDispatchKind, SubrFn};
use crate::tagged::mutate::LispCollectionRevision;
use crate::tagged::value::TAG_MASK;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn loaded_local(context: &Context, name: &str) -> Value {
    let symbol = context
        .obarray()
        .get_by_id(intern(name))
        .expect("localized symbol");
    assert_eq!(symbol.redirect(), SymbolRedirect::Localized);
    // SAFETY: the checked symbol retains its BLV; fixture lookup loaded this
    // buffer's actual local pair, without replacing any production fields.
    let blv = unsafe { &*symbol.localized_blv().expect("localized").as_ptr() };
    assert!(blv.found);
    blv.valcell
}

fn observe_bound_cells(context: &mut Context) -> EvalResult {
    let lower = global(context, "fx1-unbind-observed-owner");
    let target = global(context, "fx1-unbind-refine-owner");
    let upper = global(context, "fx1-unbind-upper-anchor");
    let (bound, reads) = capture(|| {
        upper.cons_car();
        lower.cons_cdr()
    });
    assert!(
        reads
            .expect("the reads after both bindings are coherent")
            .unchanged()
    );
    assert!(!is_observed(target.bits()));
    assert!(
        context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(target.bits() & !TAG_MASK)
    );
    Ok(bound)
}

fn check_multi_blv_unbind() {
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut context = context(false);
    context.specpdl.reserve(32);
    context.jit_bind_stack.reserve(16);
    context
        .eval_str(
            "(progn
           (defvar fx1-unbind-local-a 43)
           (make-local-variable 'fx1-unbind-local-a)
           (defvar fx1-unbind-local-b 47)
           (make-local-variable 'fx1-unbind-local-b)
           (list fx1-unbind-local-a fx1-unbind-local-b))",
        )
        .expect("both real BLV caches are loaded in the current buffer");
    let mut owners = [
        (
            "fx1-unbind-local-a",
            loaded_local(&context, "fx1-unbind-local-a"),
        ),
        (
            "fx1-unbind-local-b",
            loaded_local(&context, "fx1-unbind-local-b"),
        ),
    ];
    owners.sort_unstable_by_key(|(_, owner)| owner.bits() & !TAG_MASK);
    let [(observed_name, lower), (refine_name, target)] = owners;
    let upper = Value::cons(Value::make_int(97), Value::NIL);
    assert!((lower.bits() & !TAG_MASK) < (target.bits() & !TAG_MASK));
    assert!((target.bits() & !TAG_MASK) < (upper.bits() & !TAG_MASK));
    context.assign("fx1-unbind-observed-owner", lower);
    context.assign("fx1-unbind-refine-owner", target);
    context.assign("fx1-unbind-upper-anchor", upper);
    let old_lower = lower.cons_cdr();
    let old_target = target.cons_cdr();
    assert!(!is_observed(lower.bits()) && !is_observed(target.bits()));
    let observer = intern("fx1-unbind-observe-bound-cells");
    register_global_subr_entry(
        observer,
        SubrEntry {
            function: Some(SubrFn::A0(observe_bound_cells)),
            min_args: 0,
            max_args: Some(0),
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
        },
    );
    context.set_function(
        "fx1-unbind-observe-bound-cells",
        Value::subr_from_sym_id(observer),
    );
    // Bind lower first, so the suffix checks the unobserved higher cell
    // first. The callback observes lower and the retained upper anchor only
    // after both bindings; that makes higher a genuine window hit.
    super::super::inline_vars::reset_inline_var_sites();
    let leaf = compile_blv(
        &context,
        &[
            Op::Constant(2),
            Op::VarBind(0),
            Op::Constant(3),
            Op::VarBind(1),
            Op::Constant(4),
            Op::Call(0),
            Op::Unbind(2),
            Op::Return,
        ],
        &[
            Value::symbol(observed_name),
            Value::symbol(refine_name),
            Value::make_int(31),
            Value::make_int(37),
            Value::from_sym_id(observer),
        ],
        0,
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Bind),
        2
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Unbind),
        1
    );
    let binds = super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get());
    let unbinds = super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get());
    let specpdl_depth = context.specpdl.len();
    let bind_depth = context.jit_bind_stack.len();
    let revision = LispCollectionRevision::current();
    let (result, reads) = capture(|| native(&mut context, &leaf, &[]));
    assert_eq!(result, Value::make_int(31));
    assert_eq!(
        super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get()) - binds,
        0
    );
    assert_eq!(
        super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get()) - unbinds,
        1
    );
    // The observed refusal falls back before either restore is written.
    // The compiled suffix restores both owners, journaling the observed one only.
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );
    assert!(is_observed(lower.bits()));
    assert!(
        !is_observed(target.bits()),
        "an internal restore is not a read"
    );
    assert_eq!(context.specpdl.len(), specpdl_depth);
    assert_eq!(context.jit_bind_stack.len(), bind_depth);
    assert_eq!(lower.cons_cdr(), old_lower);
    assert_eq!(target.cons_cdr(), old_target);
    assert!(
        reads.is_none(),
        "the body read preceded restoration of its observed cell"
    );
}

#[test]
fn gen0_multi_blv_unbind_keeps_coherent_window_after_first_refinement() {
    check_multi_blv_unbind();
}
