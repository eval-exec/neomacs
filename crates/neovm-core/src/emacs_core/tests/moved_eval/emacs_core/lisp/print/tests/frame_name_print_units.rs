//! Core-authored regression test (not a re-home): GNU prints a frame as
//! `#<frame NAME 0xADDR>` using the frame's Lisp name (`src/print.c:1912-1929`,
//! `XFRAME (obj)->name`), never an ordinal derived from its identity.
//!
//! A loadup (no-pdump) boot can allocate the live initial frame with
//! `FRAME_ID_BASE + 1` while its GNU name is `F1`.  Deriving the printed name
//! from the id ordinal then answers `#<frame F2 ...>`; the side-window
//! deep-delete oracle row compared that against GNU's `F1`.  The evaluator
//! printer must read the live frame registry.

use crate::window::DisplayFrameManagerExt;
use neovm_value::value::Value;

#[test]
fn evaluator_frame_handle_prints_the_frame_name_not_the_id_ordinal() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer = eval
        .buffer_manager()
        .current_buffer_id()
        .or_else(|| eval.buffer_manager().find_buffer_by_name("*scratch*"))
        .expect("unit runtime has a buffer");

    // The first frame consumes FRAME_ID_BASE; the live initial frame then has
    // id FRAME_ID_BASE + 1 but GNU's name F1, which is the loadup/no-pdump
    // shape that exposed the ordinal-based rendering.
    let first = eval
        .frame_manager_mut()
        .create_frame("first-bootstrap-frame", 80, 25, buffer);
    let initial = eval.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    assert_eq!(
        initial.0,
        first.0 + 1,
        "precondition: the frame id sequence has a gap"
    );
    eval.set_variable("memq-frame-print-probe", Value::make_frame(initial.0));

    let printed = eval
        .eval_str("(format \"%s\" memq-frame-print-probe)")
        .expect("format a frame handle");
    let expected = format!("#<frame F1 0x{:x}>", initial.0);
    assert_eq!(
        printed.as_utf8_str(),
        Some(expected.as_str()),
        "GNU prints the frame's name (print.c:1912-1929); an F2 here comes \
         from deriving the name from the FrameId ordinal"
    );
}
