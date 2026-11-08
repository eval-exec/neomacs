//! T-A1 (design `p1-3-per-symbol-versions` §11), core half: every Lisp-level
//! function-binding producer stamps exactly its symbol, and the validity test
//! answers per symbol. The pure publication tests stay in the value crate.

use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::symbol::{FunctionEpochBump, Obarray};
use crate::emacs_core::value::Value;
use neovm_value::test_support::authority::force_fn_stamps_for_test;

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
        .find(|id| id.to_raw() as usize >= end)
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
    let before = ev.obarray().function_epoch();
    assert!(ev.obarray().fn_unchanged_since(target, before));
    ev.eval_str(src).expect(src);
    let after = ev.obarray().function_epoch();
    assert_ne!(before, after, "{src}: the clock moved");
    assert!(
        !ev.obarray().fn_unchanged_since(target, before),
        "{src}: {changed} changed since {before}"
    );
    assert!(
        ev.obarray().fn_unchanged_since(target, after),
        "{src}: {changed} is unchanged since the clock it produced"
    );
    assert!(
        ev.obarray().fn_unchanged_since(other, before),
        "{src}: an unrelated binding stays proven"
    );
}

#[test]
fn each_lisp_producer_stamps_exactly_its_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
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
    let before = ev.obarray().function_epoch();
    assert_eq!(
        ev.eval_str("(neovm--fs-letf)").expect("runs"),
        Value::NIL,
        "the body called the let-bound #'ignore"
    );
    assert_eq!(
        ev.obarray().function_epoch(),
        before + 2,
        "install and restore each moved the clock"
    );
    assert!(!ev.obarray().fn_unchanged_since(target, before));
    assert!(!ev.obarray().fn_unchanged_since(target, before + 1));
    assert!(ev.obarray().fn_unchanged_since(target, before + 2));
    assert!(ev.obarray().fn_unchanged_since(other, before));
}

/// The internal writers (`clear_function_silent_id`, the runtime installers'
/// `set_symbol_function_id`) stamp too.
#[test]
fn internal_cell_writers_stamp_their_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    let other = bystander(&mut ev);
    let id = intern("neovm--fs-internal");
    let e0 = ev.obarray().function_epoch();
    ev.obarray_mut()
        .set_symbol_function_id(id, Value::fixnum(1));
    let e1 = ev.obarray().function_epoch();
    assert!(!ev.obarray().fn_unchanged_since(id, e0));
    assert!(ev.obarray().fn_unchanged_since(id, e1));
    ev.obarray_mut().clear_function_silent_id(id);
    let e2 = ev.obarray().function_epoch();
    assert_ne!(e1, e2);
    assert!(!ev.obarray().fn_unchanged_since(id, e1));
    assert!(ev.obarray().fn_unchanged_since(id, e2));
    assert!(ev.obarray().fn_unchanged_since(other, e0));
}

/// GNU `define-abbrev` (`lisp/abbrev.el`) fsets the hook of the symbol in
/// TABLE, which can have a different id from the global symbol with that name.
#[test]
fn an_abbrev_hook_stamps_its_table_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    let other = bystander(&mut ev);
    let table = crate::emacs_core::abbrev::builtin_make_abbrev_table(&mut ev, vec![])
        .expect("abbrev table");
    let before = ev.obarray().function_epoch();
    crate::emacs_core::abbrev::builtin_define_abbrev(
        &mut ev,
        vec![
            table,
            Value::string("neovm--fs-abbrev"),
            Value::string("expanded"),
            Value::symbol("car"),
        ],
    )
    .expect("abbrev hook");
    let sym = crate::emacs_core::abbrev::builtin_abbrev_symbol(
        &mut ev,
        vec![Value::string("neovm--fs-abbrev"), table],
    )
    .expect("abbrev symbol")
    .as_bare_symbol_id()
    .expect("symbol id");
    let after = ev.obarray().function_epoch();
    assert_eq!(after, before + 1);
    assert!(!ev.obarray().fn_unchanged_since(sym, before));
    assert!(ev.obarray().fn_unchanged_since(sym, after));
    assert!(ev.obarray().fn_unchanged_since(other, before));
}

/// Naming a recorded macro installs it into the named function cell (GNU's
/// `kmacro-name-last-macro`, `lisp/kmacro.el`, calls `fset`).
#[test]
fn naming_a_keyboard_macro_stamps_its_symbol() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    let other = bystander(&mut ev);
    crate::emacs_core::kmacro::builtin_start_kbd_macro(&mut ev, vec![]).expect("start");
    crate::emacs_core::kmacro::builtin_store_kbd_macro_event(
        &mut ev,
        vec![Value::symbol("forward-char")],
    )
    .expect("record");
    ev.finalize_kbd_macro_runtime_chars();
    crate::emacs_core::kmacro::builtin_end_kbd_macro(&mut ev, vec![]).expect("end");
    let sym = intern("neovm--fs-kmacro");
    let before = ev.obarray().function_epoch();
    crate::emacs_core::kmacro::builtin_name_last_kbd_macro(&mut ev, vec![Value::from_sym_id(sym)])
        .expect("name macro");
    let after = ev.obarray().function_epoch();
    assert_eq!(after, before + 1);
    assert!(!ev.obarray().fn_unchanged_since(sym, before));
    assert!(ev.obarray().fn_unchanged_since(sym, after));
    assert!(ev.obarray().fn_unchanged_since(other, before));
}

/// Storing the value the cell already holds redefines nothing: no clock
/// move, no stamp.
#[test]
fn an_unchanged_write_does_not_stamp() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    ev.eval_str("(fset 'neovm--fs-same #'car)").expect("fset");
    let id = intern("neovm--fs-same");
    let before = ev.obarray().function_epoch();
    ev.eval_str("(fset 'neovm--fs-same #'car)").expect("fset");
    ev.eval_str("(defalias 'neovm--fs-same #'car)")
        .expect("defalias");
    assert_eq!(ev.obarray().function_epoch(), before);
    assert!(ev.obarray().fn_unchanged_since(id, before));
}

/// A subr-table rewrite and the compiler-overrides toggle change what every
/// symbol resolves to: the floor rises, so nothing validated before it is
/// proven, while later validations are.
#[test]
fn whole_table_changes_raise_the_floor() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    let id = bystander(&mut ev);
    let before = ev.obarray().function_epoch();
    neovm_eval::test_support::authority::invalidate_all_function_bindings_for_test(
        &mut ev,
        FunctionEpochBump::SubrRewrite,
    );
    let after = ev.obarray().function_epoch();
    assert!(!ev.obarray().fn_unchanged_since(id, before));
    assert!(ev.obarray().fn_unchanged_since(id, after));
    // A symbol in a chunk created after the raise inherits the floor.
    let far = symbol_past_the_store(ev.obarray(), "floor");
    ev.with_obarray_mut(|mut ob| {
        ob.ensure_symbol_id(far);
    });
    assert!(!ev.obarray().fn_unchanged_since(far, before));
    assert!(ev.obarray().fn_unchanged_since(far, after));

    // Same Lisp symbol the evaluator keys on (the source constant is
    // eval-private: crate::emacs_core::eval::INTERNAL_COMPILER_FUNCTION_OVERRIDES
    // = "internal--compiler-function-overrides"). Other core and eval tests
    // set this variable by the same name.
    let overrides = intern("internal--compiler-function-overrides");
    let before = ev.obarray().function_epoch();
    ev.sync_cached_runtime_binding_by_id(overrides, Value::cons(Value::NIL, Value::NIL));
    let toggled = ev.obarray().function_epoch();
    assert_ne!(before, toggled, "the toggle moved the clock");
    assert!(!ev.obarray().fn_unchanged_since(id, before));
    assert!(ev.obarray().fn_unchanged_since(id, toggled));
    ev.sync_cached_runtime_binding_by_id(overrides, Value::NIL);
    assert!(!ev.obarray().fn_unchanged_since(id, toggled));

    // `install_subr` rewrites the static subr entries in place.
    let before = ev.obarray().function_epoch();
    crate::emacs_core::eval::register_public_subrs(&mut ev);
    let installed = ev.obarray().function_epoch();
    assert!(!ev.obarray().fn_unchanged_since(id, before));
    assert!(ev.obarray().fn_unchanged_since(id, installed));
}

/// `u64::MAX` is every cache's EMPTY / DISARMED sentinel: it never
/// validates, not even for `nil`.
#[test]
fn the_empty_sentinel_never_validates() {
    crate::test_utils::init_test_tracing();
    let _on = StampsOn::new();
    let mut ev = crate::test_utils::new_context();
    let id = bystander(&mut ev);
    for sym in [intern("nil"), intern("t"), intern("car"), id] {
        assert!(!ev.obarray().fn_unchanged_since(sym, u64::MAX));
    }
}

#[test]
fn the_knob_off_makes_the_predicate_false() {
    crate::test_utils::init_test_tracing();
    force_fn_stamps_for_test(Some(false));
    let mut ev = crate::test_utils::new_context();
    let id = bystander(&mut ev);
    let now = ev.obarray().function_epoch();
    assert!(!ev.obarray().fn_unchanged_since(id, now));
    force_fn_stamps_for_test(Some(true));
    assert!(ev.obarray().fn_unchanged_since(id, now));
    force_fn_stamps_for_test(None);
}
