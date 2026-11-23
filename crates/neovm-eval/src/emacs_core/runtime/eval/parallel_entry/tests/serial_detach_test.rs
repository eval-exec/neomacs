//! L6.4's suspend half: a detached owner leaves no heap on its OS thread,
//! and reattaching it restores everything its next serial turn reads.

use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// What a serial turn reads through the carrier TLS the detach takes down:
/// the case table, the syntax table, the regex cache, the in-flight roots,
/// and an ordinary global.
const PROBE: &str = "(list sd-global (upcase \"abc\") (char-syntax ?a)
                           (string-match \"b+\" \"abbbc\") (match-end 0)
                           (condition-case err (signal 'sd-error '(1 2))
                             (sd-error (cdr err))))";

fn owner() -> Context {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    eval(
        &mut ev,
        "(progn (defvar sd-global 41) (setq sd-global 42)
                (put 'sd-error 'error-conditions '(sd-error error))
                (defvar sd-auto 5) (make-variable-buffer-local 'sd-auto)
                (get-buffer-create \" sd-other\"))",
    );
    ev.project_thread_variable_for_test(intern("sd-auto"))
        .expect("sd-auto projects");
    ev
}

#[test]
fn l64_detach_leaves_no_heap_on_this_thread_and_reattach_restores_the_turn() {
    let mut ev = owner();
    let before = eval(&mut ev, PROBE);
    assert_eq!(
        before, "(42 \"ABC\" 119 1 4 (1 2))",
        "the probe itself works"
    );
    let detached = ev.detach_serial_owner();
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "a detached owner's thread names no heap"
    );
    super::super::super::green::carrier_tls::carrier_tls_idle_state_and_check();
    ev.reattach_serial_owner(detached);
    assert_eq!(eval(&mut ev, PROBE), before, "the next turn reads the same");
}

#[test]
fn l64_an_in_place_binding_swaps_out_on_detach_and_back_on_reattach() {
    let mut ev = owner();
    let id: SymId = intern("sd-auto");
    eval(
        &mut ev,
        "(save-current-buffer (set-buffer \" sd-other\") (setq sd-auto 6))",
    );
    let home = eval(&mut ev, "(buffer-name)");
    eval(&mut ev, "(set-buffer \" sd-other\")");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::fixnum(9)).expect("bind");
    assert_eq!(eval(&mut ev, "sd-auto"), "9");
    eval(&mut ev, &format!("(set-buffer {home})"));

    let detached = ev.detach_serial_owner();
    ev.reattach_serial_owner(detached);

    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") sd-auto)"
        ),
        "9",
        "the owner's binding is back in the buffer's storage"
    );
    ev.unbind_to(count);
    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") sd-auto)"
        ),
        "6",
        "unbinding restores the value the binding hid"
    );
}

#[test]
fn l64_two_turns_in_a_row_detach_and_reattach_the_same_owner() {
    let mut ev = owner();
    let before = eval(&mut ev, PROBE);
    assert_eq!(
        before, "(42 \"ABC\" 119 1 4 (1 2))",
        "the probe itself works"
    );
    for turn in 0..2 {
        let detached = ev.detach_serial_owner();
        ev.reattach_serial_owner(detached);
        assert_eq!(eval(&mut ev, PROBE), before, "turn {turn}");
    }
}
