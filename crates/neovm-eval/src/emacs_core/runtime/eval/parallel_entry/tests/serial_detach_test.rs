//! L6.4's suspend half: a detached owner leaves no heap on its OS thread,
//! and reattaching it restores everything its next serial turn reads.

use super::ReattachRefusal;
use crate::emacs_core::eval::parallel_entry::serial_turn::{OwnerSerialState, TurnRefusal};
use crate::emacs_core::eval::{Context, SpecBinding};
use crate::emacs_core::print::print_value;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// State only this owner has, held where a serial turn reads it through the
/// carrier TLS the detach takes down: its buffer's case table maps ?Q down to
/// ?a, its syntax table makes ?a punctuation, and its match data comes from
/// one match made before the detach.
const OWNER_STATE: &str = "(progn
  (let ((down (copy-sequence (standard-case-table))))
    (aset down ?Q ?a)
    (set-case-table down))
  (let ((table (copy-syntax-table)))
    (modify-syntax-entry ?a \".\" table)
    (set-syntax-table table))
  (string-match \"b+\" \"abbbc\"))";

/// Reads the owner state back without changing it: no form here matches a
/// regexp, so `(match-end 0)` is still the pre-detach match. Also an
/// ordinary global and the signal path.
const PROBE: &str = "(list sd-global (downcase ?Q) (char-syntax ?a) (match-end 0)
                           (condition-case err (signal 'sd-error '(1 2))
                             (sd-error (cdr err))))";

/// The probe's answer for the owner state, not the standard tables':
/// (downcase ?Q) is ?a (97), not ?q, and (char-syntax ?a) is ?. (46), not ?w.
const OWNER_ANSWER: &str = "(42 97 46 4 (1 2))";

fn owner() -> Context {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    eval(
        &mut ev,
        "(progn (defvar sd-global 41) (setq sd-global 42)
                (put 'sd-error 'error-conditions '(sd-error error))
                (get-buffer-create \" sd-other\"))",
    );
    ev
}

fn reattach(ev: &mut Context, detached: super::DetachedOwner) {
    if ev.reattach_serial_owner(detached).is_err() {
        panic!("no other heap is installed on this thread");
    }
    assert!(
        neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap),
        "reattach installs the owner's own heap"
    );
}

/// Inside a turn: the thread allocates into this owner's heap.
fn owns_the_thread(ev: &mut Context) -> bool {
    neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap)
}

#[test]
fn l64_detach_leaves_no_heap_on_this_thread_and_reattach_restores_the_turn() {
    let mut ev = owner();
    assert_eq!(eval(&mut ev, OWNER_STATE), "1", "the match starts at 1");
    assert_eq!(eval(&mut ev, PROBE), OWNER_ANSWER, "the owner state is set");
    let detached = ev.detach_serial_owner();
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "a detached owner's thread names no heap"
    );
    super::super::super::green::carrier_tls::carrier_tls_idle_state_and_check();
    reattach(&mut ev, detached);
    assert_eq!(
        eval(&mut ev, PROBE),
        OWNER_ANSWER,
        "the next turn reads the owner's own tables and match data"
    );
}

#[test]
fn l64_an_in_place_binding_swaps_out_on_detach_and_back_on_reattach() {
    // `buffer-undo-list` always binds in place: its storage is the buffer's
    // undo state, never an owner cell, so its `let` is a LetLocal entry that
    // the detach swaps with the buffer.
    let mut ev = owner();
    let id: SymId = intern("buffer-undo-list");
    eval(
        &mut ev,
        "(save-current-buffer (set-buffer \" sd-other\") (setq buffer-undo-list nil))",
    );
    let home = eval(&mut ev, "(buffer-name)");
    eval(&mut ev, "(set-buffer \" sd-other\")");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::T).expect("bind");
    assert_eq!(eval(&mut ev, "buffer-undo-list"), "t");
    eval(&mut ev, &format!("(set-buffer {home})"));
    // The value the binding's LetLocal entry saves.
    let saved = |ev: &Context| {
        ev.specpdl
            .iter()
            .skip(count)
            .find_map(|entry| match entry {
                SpecBinding::LetLocal {
                    sym_id, old_value, ..
                } if *sym_id == id => Some(*old_value),
                _ => None,
            })
            .expect("the binding is a buffer-local let")
    };
    assert_eq!(saved(&ev), Value::NIL, "the let saves the value it hid");

    let detached = ev.detach_serial_owner();
    assert_eq!(
        saved(&ev),
        Value::T,
        "while detached the let saves the owner's binding and the buffer holds nil"
    );
    reattach(&mut ev, detached);
    assert_eq!(saved(&ev), Value::NIL, "the binding is back in the buffer");

    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") buffer-undo-list)"
        ),
        "t",
        "the owner's binding is back in the buffer's storage"
    );
    ev.unbind_to(count);
    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") buffer-undo-list)"
        ),
        "nil",
        "unbinding restores the value the binding hid"
    );
}

#[test]
fn l64_two_turns_in_a_row_detach_and_reattach_the_same_owner() {
    let mut ev = owner();
    eval(&mut ev, OWNER_STATE);
    for turn in 0..2 {
        let detached = ev.detach_serial_owner();
        reattach(&mut ev, detached);
        assert_eq!(eval(&mut ev, PROBE), OWNER_ANSWER, "turn {turn}");
    }
}

type State = OwnerSerialState<Context, (u64, &'static str)>;

/// An owner built on this thread and kept dormant: its heap leaves the
/// thread as the state is built.
fn dormant(owner_id: u64) -> State {
    let mut ev = owner();
    eval(&mut ev, OWNER_STATE);
    OwnerSerialState::new(owner_id, Box::new(ev))
}

#[test]
fn l64_turns_through_the_owner_state_reattach_and_detach_its_context() {
    let mut state = dormant(7);
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "a dormant owner's thread names no heap"
    );
    for ticket in ["ticket-1", "ticket-2"] {
        let mut turn = state.begin((7, ticket)).ok().expect("own ticket");
        assert!(
            turn.with_world(owns_the_thread),
            "begin installs the owner's heap"
        );
        assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
        assert_eq!(turn.end(), (7, ticket));
        assert!(
            neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
            "an ended turn leaves no heap on this thread"
        );
    }
}

#[test]
fn l64_a_second_owner_cannot_begin_while_another_owners_turn_is_live() {
    let mut first = dormant(7);
    let mut second = dormant(8);
    let mut turn = first.begin((7, "ticket-1")).ok().expect("own ticket");
    let refused = second.begin((8, "ticket-2")).err().expect("heap installed");
    assert_eq!(
        refused,
        (
            (8, "ticket-2"),
            TurnRefusal::Host(ReattachRefusal::HeapInstalled)
        ),
        "the second owner's ticket comes back and its Context stays dormant"
    );
    assert!(
        turn.with_world(owns_the_thread),
        "the refusal left the live turn's TLS alone"
    );
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(turn.end(), (7, "ticket-1"));
    let mut turn = second.begin((8, "ticket-2")).ok().expect("thread is free");
    assert!(
        turn.with_world(owns_the_thread),
        "the second owner's own heap"
    );
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(turn.end(), (8, "ticket-2"));
}

#[test]
fn l64_dropping_a_dormant_owner_is_ordinary_teardown() {
    let state = dormant(7);
    drop(state);
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "teardown of a dormant owner leaves no heap on this thread"
    );
    let mut next = dormant(8);
    let mut turn = next.begin((8, "ticket-1")).ok().expect("own ticket");
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(turn.end(), (8, "ticket-1"));
}
