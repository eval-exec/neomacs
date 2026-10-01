//! T-A1 (design `p1-3-per-symbol-versions` §11): every function-binding
//! producer stamps exactly its symbol, every change of all symbols'
//! resolution raises the floor, and the validity test answers per symbol.

use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::symbol::{FunctionEpochBump, Obarray, force_fn_stamps_for_test};
use crate::emacs_core::value::Value;

/// Turns `NEOVM_FN_STAMPS` on for the test's thread, and back to the
/// environment when dropped.
struct StampsOn;

impl StampsOn {
    fn new() -> Self {
        force_fn_stamps_for_test(Some(true));
        Self
    }
}

impl Drop for StampsOn {
    fn drop(&mut self) {
        force_fn_stamps_for_test(None);
    }
}

/// A fresh interned symbol whose slot lies in a chunk `ob` does not have
/// yet (ids are process-wide, so intern until one lands past the store).
fn symbol_past_the_store(ob: &Obarray, tag: &str) -> SymId {
    let end = ob.symbol_count_upper_bound_for_test();
    (0..)
        .map(|i| intern(&format!("neovm--fs-{tag}-{i}")))
        .find(|id| id.0 as usize >= end)
        .expect("ids grow without bound")
}

/// An unrelated symbol whose binding nothing in these tests changes.
fn bystander(ev: &mut Context) -> SymId {
    ev.eval_str("(defalias 'neovm--fs-bystander #'car)")
        .expect("defalias");
    intern("neovm--fs-bystander")
}

/// Run `src` and assert that it changed `changed`'s binding (and only that
/// one, as seen by `bystander`), moving the clock once.
fn assert_stamps_only(ev: &mut Context, src: &str, changed: &str) {
    let other = bystander(ev);
    let target = intern(changed);
    let before = ev.obarray.function_epoch();
    assert!(ev.obarray.fn_unchanged_since(target, before));
    ev.eval_str(src).expect(src);
    let after = ev.obarray.function_epoch();
    assert_ne!(before, after, "{src}: the clock moved");
    assert!(
        !ev.obarray.fn_unchanged_since(target, before),
        "{src}: {changed} changed since {before}"
    );
    assert!(
        ev.obarray.fn_unchanged_since(target, after),
        "{src}: {changed} is unchanged since the clock it produced"
    );
    assert!(
        ev.obarray.fn_unchanged_since(other, before),
        "{src}: an unrelated binding stays proven"
    );
}

#[test]
fn each_lisp_producer_stamps_exactly_its_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = Context::new();
    assert_stamps_only(&mut ev, "(fset 'neovm--fs-a #'car)", "neovm--fs-a");
    assert_stamps_only(&mut ev, "(defalias 'neovm--fs-a #'cdr)", "neovm--fs-a");
    assert_stamps_only(&mut ev, "(fmakunbound 'neovm--fs-a)", "neovm--fs-a");
    assert_stamps_only(&mut ev, "(fset 'neovm--fs-b #'car)", "neovm--fs-b");
    assert_stamps_only(&mut ev, "(unintern \"neovm--fs-b\" obarray)", "neovm--fs-b");
}

/// `cl-letf` of a symbol's function compiles to the `Bfset` opcode, which
/// never goes through `fset`'s function cell (GNU `bytecode.c`, `CASE
/// (Bfset)`): the install and the restore stamp the symbol.
#[test]
fn bfset_from_a_compiled_cl_letf_stamps_its_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        "(progn (require 'cl-lib)
           (defun neovm--fs-target () 1)
           (defun neovm--fs-letf ()
             (cl-letf (((symbol-function 'neovm--fs-target) #'ignore)) (neovm--fs-target)))
           (byte-compile 'neovm--fs-letf))",
    )
    .expect("defined");
    let other = bystander(&mut ev);
    let target = intern("neovm--fs-target");
    let before = ev.obarray.function_epoch();
    assert_eq!(
        ev.eval_str("(neovm--fs-letf)").expect("runs"),
        Value::NIL,
        "the body called the let-bound #'ignore"
    );
    assert_eq!(
        ev.obarray.function_epoch(),
        before + 2,
        "install and restore each moved the clock"
    );
    assert!(!ev.obarray.fn_unchanged_since(target, before));
    assert!(!ev.obarray.fn_unchanged_since(target, before + 1));
    assert!(ev.obarray.fn_unchanged_since(target, before + 2));
    assert!(ev.obarray.fn_unchanged_since(other, before));
}

/// The internal writers (`clear_function_silent_id`, the runtime installers'
/// `set_symbol_function_id`) stamp too.
#[test]
fn internal_cell_writers_stamp_their_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = Context::new();
    let other = bystander(&mut ev);
    let id = intern("neovm--fs-internal");
    let e0 = ev.obarray.function_epoch();
    ev.obarray.set_symbol_function_id(id, Value::fixnum(1));
    let e1 = ev.obarray.function_epoch();
    assert!(!ev.obarray.fn_unchanged_since(id, e0));
    assert!(ev.obarray.fn_unchanged_since(id, e1));
    ev.obarray.clear_function_silent_id(id);
    let e2 = ev.obarray.function_epoch();
    assert_ne!(e1, e2);
    assert!(!ev.obarray.fn_unchanged_since(id, e1));
    assert!(ev.obarray.fn_unchanged_since(id, e2));
    assert!(ev.obarray.fn_unchanged_since(other, e0));
}

/// Storing the value the cell already holds redefines nothing: no clock
/// move, no stamp.
#[test]
fn an_unchanged_write_does_not_stamp() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = Context::new();
    ev.eval_str("(fset 'neovm--fs-same #'car)").expect("fset");
    let id = intern("neovm--fs-same");
    let before = ev.obarray.function_epoch();
    ev.eval_str("(fset 'neovm--fs-same #'car)").expect("fset");
    ev.eval_str("(defalias 'neovm--fs-same #'car)")
        .expect("defalias");
    assert_eq!(ev.obarray.function_epoch(), before);
    assert!(ev.obarray.fn_unchanged_since(id, before));
}

/// A subr-table rewrite and the compiler-overrides toggle change what every
/// symbol resolves to: the floor rises, so nothing validated before it is
/// proven, while later validations are.
#[test]
fn whole_table_changes_raise_the_floor() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = Context::new();
    let id = bystander(&mut ev);
    let before = ev.obarray.function_epoch();
    ev.obarray
        .invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
    let after = ev.obarray.function_epoch();
    assert!(!ev.obarray.fn_unchanged_since(id, before));
    assert!(ev.obarray.fn_unchanged_since(id, after));
    // A symbol in a chunk created after the raise inherits the floor.
    let far = symbol_past_the_store(&ev.obarray, "floor");
    ev.obarray.ensure_symbol_id(far);
    assert!(!ev.obarray.fn_unchanged_since(far, before));
    assert!(ev.obarray.fn_unchanged_since(far, after));

    let overrides = intern(crate::emacs_core::eval::INTERNAL_COMPILER_FUNCTION_OVERRIDES);
    let before = ev.obarray.function_epoch();
    ev.sync_cached_runtime_binding_by_id(overrides, Value::cons(Value::NIL, Value::NIL));
    let toggled = ev.obarray.function_epoch();
    assert_ne!(before, toggled, "the toggle moved the clock");
    assert!(!ev.obarray.fn_unchanged_since(id, before));
    assert!(ev.obarray.fn_unchanged_since(id, toggled));
    ev.sync_cached_runtime_binding_by_id(overrides, Value::NIL);
    assert!(!ev.obarray.fn_unchanged_since(id, toggled));

    // `install_subr` rewrites the static subr entries in place.
    let before = ev.obarray.function_epoch();
    crate::emacs_core::eval::register_public_subrs(&mut ev);
    let installed = ev.obarray.function_epoch();
    assert!(!ev.obarray.fn_unchanged_since(id, before));
    assert!(ev.obarray.fn_unchanged_since(id, installed));
}

/// `u64::MAX` is every cache's EMPTY / DISARMED sentinel: it never
/// validates, not even for `nil`.
#[test]
fn the_empty_sentinel_never_validates() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = Context::new();
    let id = bystander(&mut ev);
    for sym in [intern("nil"), intern("t"), intern("car"), id] {
        assert!(!ev.obarray.fn_unchanged_since(sym, u64::MAX));
    }
}

/// A symbol past the last chunk proves nothing; a stamp is kept even for a
/// symbol whose chunk the stamp itself creates.
#[test]
fn a_slot_past_the_store_proves_nothing() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let ob = Obarray::new();
    let far = symbol_past_the_store(&ob, "past");
    assert!(!ob.fn_unchanged_since(far, ob.function_epoch()));
}

/// The stamp array of a chunk is allocated only when one of its symbols
/// first changes its binding.
#[test]
fn stamp_arrays_are_allocated_on_first_change() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let far = symbol_past_the_store(&ob, "lazy");
    ob.ensure_symbol_id(far);
    assert!(!ob.chunk_has_fn_stamps_for_test(far));
    ob.set_symbol_function_id(far, Value::fixnum(1));
    assert!(ob.chunk_has_fn_stamps_for_test(far));
}

/// A cloned obarray keeps the stamps and the floor; the copy is deep.
#[test]
fn a_clone_keeps_stamps_and_floor() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ob = Obarray::new();
    let a = intern("neovm--fs-clone-a");
    let b = intern("neovm--fs-clone-b");
    ob.set_symbol_function_id(b, Value::fixnum(0));
    let e0 = ob.function_epoch();
    ob.set_symbol_function_id(a, Value::fixnum(1));
    let copy = ob.clone();
    assert!(!copy.fn_unchanged_since(a, e0));
    assert!(copy.fn_unchanged_since(b, e0));
    ob.set_symbol_function_id(b, Value::fixnum(2));
    assert!(!ob.fn_unchanged_since(b, e0));
    assert!(
        copy.fn_unchanged_since(b, e0),
        "the original's later stamp is not the copy's"
    );
    let mut floored = Obarray::new();
    floored.invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
    let raised = floored.function_epoch();
    let copy = floored.clone();
    assert!(!copy.fn_unchanged_since(a, raised - 1));
    assert!(copy.fn_unchanged_since(a, raised));
}

/// A restored image's cells were written without stamps: nothing validated
/// before the restored clock is proven through them.
#[test]
fn from_dump_raises_the_floor_to_the_restored_clock() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let id = intern("neovm--fs-dumped");
    let ob = Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 1234);
    assert_eq!(ob.function_epoch(), 1234);
    assert!(!ob.fn_unchanged_since(id, 1233));
    assert!(ob.fn_unchanged_since(id, 1234));
}

/// `NEOVM_FN_STAMPS` off: the predicate never proves anything (every
/// consumer takes today's path), though the stamps are still written.
#[test]
fn the_knob_off_makes_the_predicate_false() {
    crate::test_utils::init_test_tracing();
    force_fn_stamps_for_test(Some(false));
    let mut ev = Context::new();
    let id = bystander(&mut ev);
    let now = ev.obarray.function_epoch();
    assert!(!ev.obarray.fn_unchanged_since(id, now));
    force_fn_stamps_for_test(Some(true));
    assert!(ev.obarray.fn_unchanged_since(id, now));
    force_fn_stamps_for_test(None);
}
