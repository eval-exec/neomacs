use super::*;

use neovm_edit::emacs_core::case_encoding::CaseEncoding;
use neovm_value::error::LispCondition;
use neovm_value::error::expect_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::LispString;
use neovm_value::value::ValueKind;

use crate::emacs_core::casefiddle_entries::builtin_capitalize_word;
use crate::emacs_core::casefiddle_entries::builtin_upcase_region;
use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::casefiddle::*;
use neovm_value::value::Value;

#[test]
fn capitalize_string_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("hello world")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn capitalize_honors_case_symbols_as_words() {
    // GNU `casefiddle.c` `case_ch_is_word`: with `case-symbols-as-words` set,
    // symbol-constituent characters (`_`, `-`) count as word constituents, so
    // `foo_bar` is a single word.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    // Default (nil): `_` is a word boundary.
    let r = ev.eval_str(r#"(capitalize "foo_bar baz")"#).unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_Bar Baz"));

    // With the flag: `_` (Ssymbol) is part of the word.
    let r = ev
        .eval_str(r#"(let ((case-symbols-as-words t)) (capitalize "foo_bar baz"))"#)
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));

    // upcase-initials honors it too.
    let r = ev
        .eval_str(r#"(let ((case-symbols-as-words t)) (upcase-initials "foo_bar baz"))"#)
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));

    // capitalize-region in a buffer.
    let r = ev
        .eval_str(
            r#"(let ((case-symbols-as-words t)) (with-temp-buffer (insert "foo_bar baz") (capitalize-region (point-min) (point-max)) (buffer-string)))"#,
        )
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));
}

#[test]
fn capitalize_string_mixed() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("hELLO wORLD")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn upcase_initials_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_upcase_initials(vec![Value::string("hello world")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn eval_upcase_region_noncontiguous_uses_region_extract_bounds() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let buffer_id = ev.edit.buffers.current_buffer_id().expect("current buffer");
    ev.eval_str("(setq region-extract-function (lambda (_op) (list (cons 2 4))))")
        .expect("region-extract-function setup");
    ev.edit.buffers.insert_into_buffer(buffer_id, "abcde");

    super::builtin_upcase_region(&mut ev, vec![Value::fixnum(1), Value::fixnum(5), Value::T])
        .expect("upcase-region");

    let buffer = ev.edit.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "aBCde");
}

#[test]
fn eval_upcase_region_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer_id = ev.edit.buffers.current_buffer_id().expect("current buffer");
    ev.edit
        .buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.edit.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &neovm_value::heap_types::LispString::from_unibyte(vec![0xFF, b'a', b'b']),
    );

    super::builtin_upcase_region(&mut ev, vec![Value::fixnum(1), Value::fixnum(4)])
        .expect("upcase-region");

    let buffer = ev.edit.buffers.get(buffer_id).expect("buffer");
    let text = buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range());
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'A', b'B']);
}

#[test]
fn eval_capitalize_word_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer_id = ev.edit.buffers.current_buffer_id().expect("current buffer");
    ev.edit
        .buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.edit.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &neovm_value::heap_types::LispString::from_unibyte(vec![0xFF, b'a', b'b']),
    );
    ev.edit
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(0));

    super::builtin_capitalize_word(&mut ev, vec![Value::fixnum(1)]).expect("capitalize-word");

    let buffer = ev.edit.buffers.get(buffer_id).expect("buffer");
    let text = buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range());
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'a', b'b']);
}

#[test]
fn eval_capitalize_word_updates_buffer_text() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer_id = ev.edit.buffers.current_buffer_id().expect("current buffer");
    ev.edit.buffers.insert_into_buffer(buffer_id, "hELLO world");
    ev.edit
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(0));

    super::builtin_capitalize_word(&mut ev, vec![Value::fixnum(1)]).expect("capitalize-word");

    let buffer = ev.edit.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "Hello world");
}

/// GNU 31.1: the region and word case commands leave text properties where
/// they were -- `casify_region` overwrites a same-size character in place, so
/// every interval still spans the same characters. They used to be stripped
/// from the whole region (and from the word, through a generic replace).
#[test]
fn case_commands_keep_text_properties_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (insert "hello Wörld foo éclair")
          (put-text-property 1 6 'face 'bold)
          (put-text-property 7 12 'mouse-face 'highlight)
          (put-text-property 3 9 'help-echo "x")
          (put-text-property 17 23 'face 'italic)
          (upcase-region 1 6)
          (capitalize-region 7 16)
          (downcase-region 1 3)
          (upcase-initials-region 13 16)
          (goto-char 17)
          (capitalize-word 1)
          (list (buffer-string) (point) (point-max)))
        "#,
    );
    assert_eq!(
        result,
        "OK (#(\"heLLO Wörld Foo Éclair\" 0 2 (face bold) 2 5 (help-echo \"x\" face bold) 5 6 (help-echo \"x\") 6 8 (help-echo \"x\" mouse-face highlight) 8 11 (mouse-face highlight) 16 22 (face italic)) 23 23)"
    );
}

/// GNU 31.1: `capitalize-word` & co. are `casify_region (PT, farend)` -- a
/// read-only buffer signals even when nothing would change, the undo record
/// is `((START . END) (ORIGINAL . START) ...)`, a read-only character
/// signals `text-read-only`, and a negative ARG cases the words before point
/// without moving it.
#[test]
fn case_words_follow_casify_region_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (list
         (with-temp-buffer (insert "Abc def") (goto-char 1) (setq buffer-read-only t)
           (condition-case e (progn (capitalize-word 1) (point)) (error (car e))))
         (with-temp-buffer (buffer-enable-undo) (insert "abc def") (undo-boundary)
           (set-buffer-modified-p nil) (goto-char 2)
           (capitalize-word 1) (list (point) (buffer-modified-p) (seq-take buffer-undo-list 2)))
         (with-temp-buffer (insert "abc déf ghi") (goto-char 9) (upcase-word -2)
           (list (buffer-string) (point)))
         (with-temp-buffer (insert "abc") (put-text-property 1 3 'read-only t) (goto-char 1)
           (condition-case e (progn (upcase-word 1) (buffer-string)) (error (car e))))
         (with-temp-buffer (insert "straße x") (goto-char 1) (upcase-word 1)
           (list (buffer-string) (point) (point-max))))
        "#,
    );
    assert_eq!(
        result,
        "OK (buffer-read-only (4 t ((2 . 4) (\"bc\" . 2))) (\"ABC DÉF ghi\" 9) text-read-only (\"STRASSE x\" 8 10))"
    );
}
