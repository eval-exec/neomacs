//! Issue #470: the mode-line result's multibyte identity must come from the
//! FORMAT INPUTS, never from whether the accumulated characters happen to fit
//! in one byte.
//!
//! `into_display_output' used to derive `multibyte` as "any code > 0xFF", so
//! a mode line whose only non-ASCII character is a Latin-1-supplement
//! character (U+0080..U+00FF, e.g. U+00B7 MIDDLE DOT) was re-encoded as a
//! UNIBYTE string of raw bytes -- and a raw byte displays as GNU's octal
//! escape (`\267`, src/xdisp.c:8649-8662 `"%03o"` of CHAR_TO_BYTE8).  That is
//! exactly the reported "dot renders, then turns into a literal `\267`".
//! GNU never re-encodes: a string's multibyte flag follows its inputs
//! (the `concat' rule -- multibyte iff any argument is multibyte), so U+00B7
//! stays U+00B7 across every mode-line re-evaluation.

use super::*;
use crate::emacs_core::Context;

fn interactive() -> Context {
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    eval
}

fn render(eval: &mut Context, elements: Vec<Value>) -> ModeLineDisplayOutput {
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("mode-line-multibyte-identity", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    format_mode_line_for_display_with_sources(
        eval,
        Value::list(elements),
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        80,
    )
}

/// `(multibyte, bytes)` of the rendered output string.
fn output_identity(output: &ModeLineDisplayOutput) -> (bool, Vec<u8>) {
    let value = output.value();
    let string = value
        .as_lisp_string()
        .expect("mode-line output is a string");
    (string.is_multibyte(), string.as_bytes().to_vec())
}

#[test]
fn mode_line_middle_dot_stays_multibyte_like_its_input() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive();
    let output = render(
        &mut eval,
        vec![Value::string("A"), Value::string("·"), Value::string("B")],
    );
    let (multibyte, bytes) = output_identity(&output);
    assert!(
        multibyte,
        "a mode line built from multibyte inputs must stay multibyte; \
         got unibyte bytes {bytes:?} -- U+00B7 degrades to a raw byte and \
         displays as \\267 (issue #470)"
    );
    assert_eq!(bytes, b"A\xC2\xB7B".to_vec());
}

#[test]
fn mode_line_latin1_supplement_band_is_multibyte_not_raw_bytes() {
    // The whole degrading band U+0080..U+00FF: every character in it has a
    // code that fits in one byte, which is exactly what the old content
    // heuristic got wrong.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive();
    let band: String = (0x80u32..=0xFF).flat_map(char::from_u32).collect();
    let output = render(&mut eval, vec![Value::string(&band)]);
    let (multibyte, bytes) = output_identity(&output);
    assert!(multibyte);
    assert_eq!(bytes, band.as_bytes().to_vec());
}

#[test]
fn mode_line_re_derivation_keeps_the_multibyte_identity() {
    // The issue's observed transition: a later mode-line re-evaluation must
    // not change the identity of the first result.  Feed the rendered output
    // back through the walker and require byte-identical, still-multibyte
    // output.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive();
    let first = render(
        &mut eval,
        vec![Value::string("A"), Value::string("·"), Value::string("B")],
    );
    let second = render(&mut eval, vec![first.value()]);
    let (multibyte, bytes) = output_identity(&second);
    assert!(multibyte, "re-derived mode line lost multibyteness");
    assert_eq!(bytes, b"A\xC2\xB7B".to_vec());
}

#[test]
fn mode_line_unibyte_raw_byte_input_stays_unibyte_like_gnu() {
    // GNU parity control: a UNIBYTE input with a raw byte keeps unibyte
    // identity (and therefore displays as the raw-byte escape `\267`,
    // exactly as GNU displays a unibyte string's raw byte).  The fix must
    // not "repair" raw bytes that GNU also leaves raw.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xB7]));
    let output = render(&mut eval, vec![Value::string("A"), raw, Value::string("B")]);
    let (multibyte, bytes) = output_identity(&output);
    assert!(!multibyte, "unibyte input must not become multibyte");
    assert_eq!(bytes, b"A\xB7B".to_vec());
}

#[test]
fn mode_line_multibyte_input_with_high_codepoints_stays_multibyte() {
    // Regression guard above the degrading band: U+2014 EM DASH already
    // forced multibyteness under the old content heuristic (> 0xFF); the
    // input-driven rule keeps it.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive();
    let output = render(
        &mut eval,
        vec![Value::string("A"), Value::string("—"), Value::string("B")],
    );
    let (multibyte, bytes) = output_identity(&output);
    assert!(multibyte);
    assert_eq!(bytes, "A—B".as_bytes().to_vec());
}
