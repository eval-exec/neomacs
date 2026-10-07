//! The per-Context attention word (`attention.rs`): every writer of one of
//! its inputs keeps it derived, and a safe point reads it the way the old
//! per-field test answered.
//!
//! One test per writer class of the inputs: the cached-binding funnel
//! (`setq`/`set`, a bytecode `varset`, `let` and its unwind, a Rust
//! `specbind`/`unbind_to`), `set_quit_flag_value` (the quit machinery's own
//! writes: `process_quit_flag`, keyboard quit requests, the read-key clear,
//! `throw-on-input` promotion), and the constructors. `attention_clear`
//! re-derives the word at every read in debug builds, so every test in the
//! crate also checks it at every poll; these pin the classes explicitly.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::value::LambdaParams;

/// The stored word equals the derived one, and the poll's fast test agrees
/// with the canonical fields (`quit-flag` nil, `throw-on-input` nil, nothing
/// asynchronous pending).
fn assert_derived(ev: &Context, where_: &str) {
    let (stored, derived) = ev.attention_words_for_test();
    assert_eq!(stored, derived, "{where_}: stale attention word");
    let canonical_clear = ev.quit_flag.is_nil()
        && ev.throw_on_input.is_nil()
        && super::attention::ASYNC_ATTENTION.load() == 0;
    assert_eq!(
        ev.maybe_quit_hot_ok(),
        canonical_clear,
        "{where_}: the fast test disagrees with the canonical fields"
    );
}

fn quit_bit(ev: &Context) -> bool {
    ev.attention_words_for_test().0 & super::attention::AttentionBit::QuitFlag as u32 != 0
}

fn throw_bit(ev: &Context) -> bool {
    ev.attention_words_for_test().0 & super::attention::AttentionBit::ThrowOnInput as u32 != 0
}

/// `(lambda () (setq VAR VALUE))` as hand-assembled lexical bytecode: the
/// VM's `varset` publication path, which the tree-walker's `setq` does not
/// take.
fn bytecode_varset(var: &str, value: Value) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::VarSet(1), Op::Constant(0), Op::Return];
    f.constants = vec![value, Value::symbol(var)].into();
    f.max_stack = 4;
    f.seal_hand_assembled_ops();
    Value::make_bytecode(f)
}

#[test]
fn a_fresh_context_starts_derived_and_clear() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    assert_derived(&ev, "Context::new");
    assert_eq!(ev.attention_words_for_test().0, 0);
}

#[test]
fn attention_tracks_every_quit_flag_write_path() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Keep a set `quit-flag` from signalling while the tests look at it:
    // `inhibit-quit` gates only the slow path (GNU lisp.h:3899), not the word.
    ev.eval_str("(setq inhibit-quit t)").expect("inhibit-quit");
    assert_derived(&ev, "start");

    // The tree-walker's `setq` and the `set` builtin.
    ev.eval_str("(setq quit-flag t)").expect("setq");
    assert!(quit_bit(&ev), "setq quit-flag raises the bit");
    assert!(!ev.maybe_quit_hot_ok(), "a set quit-flag is due");
    assert_derived(&ev, "after setq t");
    ev.eval_str("(set 'quit-flag nil)").expect("set");
    assert!(!quit_bit(&ev));
    assert_derived(&ev, "after set nil");

    // The bytecode `varset`.
    let raise = bytecode_varset("quit-flag", Value::T);
    ev.funcall_general_untraced(raise, Vec::<Value>::new())
        .expect("varset t");
    assert!(quit_bit(&ev), "a bytecode varset raises the bit");
    assert_derived(&ev, "after varset t");
    let lower = bytecode_varset("quit-flag", Value::NIL);
    ev.funcall_general_untraced(lower, Vec::<Value>::new())
        .expect("varset nil");
    assert!(!quit_bit(&ev));
    assert_derived(&ev, "after varset nil");

    // A dynamic `let` and its unwind, seen from inside the binding.
    let count = ev.specpdl.len();
    ev.try_specbind_or_unwind_to(count, intern("quit-flag"), Value::T)
        .expect("specbind");
    assert!(quit_bit(&ev), "binding quit-flag raises the bit");
    assert_derived(&ev, "inside the binding");
    // GNU's `unbind_to` saves a pending quit across the unwind and puts it
    // back when the restored value is nil (eval.c `unbind_to`: `quitf`), so
    // the quit survives its own binding's unwind and the bit with it.
    ev.unbind_to(count);
    assert!(quit_bit(&ev), "the pending quit outlives the binding");
    assert_eq!(ev.quit_flag_value(), Value::T);
    assert_derived(&ev, "after the unwind");
    ev.set_quit_flag_value(Value::NIL);
    ev.eval_str("(let ((quit-flag t)) (setq quit-flag 'inner))")
        .expect("let");
    assert_eq!(
        ev.quit_flag_value(),
        Value::symbol("inner"),
        "the unwind kept the pending quit"
    );
    assert!(quit_bit(&ev));
    assert_derived(&ev, "after a Lisp let");
    ev.eval_str("(setq quit-flag nil)").expect("setq nil");
    assert!(!quit_bit(&ev));
    assert_derived(&ev, "after setq nil");
    // A binding of nil over a pending quit: inside, the flag reads nil.
    ev.set_quit_flag_value(Value::T);
    let count = ev.specpdl.len();
    ev.try_specbind_or_unwind_to(count, intern("quit-flag"), Value::NIL)
        .expect("specbind nil");
    assert!(!quit_bit(&ev), "binding quit-flag to nil lowers the bit");
    assert_derived(&ev, "inside a nil binding");
    ev.unbind_to(count);
    assert!(quit_bit(&ev), "the unwind restores the outer t");
    assert_derived(&ev, "after the nil binding");
    ev.set_quit_flag_value(Value::NIL);

    // `set_quit_flag_value`, the quit machinery's own writer.
    ev.set_quit_flag_value(Value::T);
    assert!(quit_bit(&ev));
    assert_derived(&ev, "set_quit_flag_value t");
    ev.set_quit_flag_value(Value::NIL);
    assert!(!quit_bit(&ev));
    assert_derived(&ev, "set_quit_flag_value nil");
    ev.request_quit_from_keyboard_input();
    assert!(quit_bit(&ev), "a keyboard quit request raises the bit");
    assert_derived(&ev, "request_quit_from_keyboard_input");
    let quit_char = Value::fixnum(ev.quit_char());
    ev.clear_quit_flag_after_read_key_sequence_event(&quit_char);
    assert!(!quit_bit(&ev), "reading C-g as a key lowers it");
    assert_derived(&ev, "clear_quit_flag_after_read_key_sequence_event");

    // `process_quit_flag` consumes the flag as it signals.
    ev.eval_str("(setq inhibit-quit nil)")
        .expect("inhibit-quit nil");
    ev.set_quit_flag_value(Value::T);
    let err = ev.maybe_quit().expect_err("the pending quit signals");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
    assert!(!quit_bit(&ev), "processing the quit lowered the bit");
    assert_derived(&ev, "after process_quit_flag");
}

#[test]
fn attention_tracks_throw_on_input_writes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    assert_derived(&ev, "start");
    ev.eval_str("(setq throw-on-input 'from-setq)")
        .expect("setq");
    assert!(throw_bit(&ev), "a bound throw-on-input raises the bit");
    assert!(!ev.maybe_quit_hot_ok());
    assert_derived(&ev, "after setq");
    ev.eval_str("(setq throw-on-input nil)").expect("setq nil");
    assert!(!throw_bit(&ev));
    assert_derived(&ev, "after setq nil");

    let count = ev.specpdl.len();
    ev.try_specbind_or_unwind_to(count, intern("throw-on-input"), Value::symbol("tag"))
        .expect("specbind");
    assert!(throw_bit(&ev), "binding throw-on-input raises the bit");
    assert_derived(&ev, "inside the binding");
    ev.unbind_to(count);
    assert!(!throw_bit(&ev), "the unwind lowers it");
    assert_derived(&ev, "after the unwind");

    let raise = bytecode_varset("throw-on-input", Value::symbol("tag"));
    ev.funcall_general_untraced(raise, Vec::<Value>::new())
        .expect("varset");
    assert!(throw_bit(&ev));
    assert_derived(&ev, "after varset");
    ev.eval_str("(set 'throw-on-input nil)").expect("set nil");
    assert!(!throw_bit(&ev));
    assert_derived(&ev, "after set nil");
}

/// `throw-on-input`'s bit is conservative: it is set in a batch session too,
/// where there is no host channel to poll. The safe point then takes its slow
/// path, which finds no channel and does nothing, so every result is the
/// same as with the old precise test; the loop below never throws.
#[test]
fn batch_throw_on_input_still_polls_correctly() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    assert!(
        !ev.has_throw_on_input_poll_source(),
        "premise: a batch Context has no input channel"
    );
    let result = ev
        .eval_str(
            "(let ((throw-on-input 'neo-attention-tag))
               (catch 'neo-attention-tag
                 (let ((i 0)) (while (< i 5000) (setq i (1+ i))) i)))",
        )
        .expect("the loop runs to completion");
    assert_eq!(result, Value::fixnum(5000));
    assert_derived(&ev, "after the binding");
    assert!(
        ev.maybe_quit_hot_ok(),
        "the fast path is back after the let"
    );
}

/// A quit raised INSIDE a `while-no-input`-style binding still reaches the
/// safe point: the word carries both bits and the slow path sorts them out.
#[test]
fn a_quit_inside_a_throw_on_input_binding_still_signals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str(
        "(let ((throw-on-input 'neo-attention-tag))
           (catch 'neo-attention-tag
             (setq quit-flag t)
             (let ((i 0)) (while (< i 5000) (setq i (1+ i))) i)))",
    );
    match result {
        Err(e) => assert!(format!("{e}").contains("quit"), "{e}"),
        Ok(v) => panic!("expected a quit, got {v:?}"),
    }
    assert_derived(&ev, "after the quit");
}

/// The dump-restored constructor (`pdump_reconstruct`) initializes the word
/// from the restored fields, like `Context::new`.
#[test]
fn a_dump_restored_context_starts_derived() {
    crate::test_utils::init_test_tracing();
    let ev = crate::test_utils::runtime_startup_context();
    assert_derived(&ev, "pdump-restored context");
}
