use super::super::error::Flow;
use super::super::eval::Context;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use super::super::value::{Value, ValueKind};
use crate::test_utils::runtime_startup_context;
use malachite::integer::Integer;
use std::fs;
use std::path::PathBuf;

/// Helper: create an evaluator, insert text, and position point.
fn eval_with_text(text: &str) -> Context {
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert(text);
        // Point is now at the end. Reset to beginning.
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev
}

fn eval_with_unibyte_bytes(bytes: &[u8]) -> Context {
    let mut ev = Context::new();
    let storage = crate::emacs_core::string_escape::bytes_to_unibyte_storage_string(bytes);
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.set_multibyte_value(false);
        buf.insert(&storage);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev
}

fn gnu_simple_line_eval_with_unibyte_bytes(bytes: &[u8]) -> Context {
    let mut ev = gnu_simple_line_eval();
    let storage = crate::emacs_core::string_escape::bytes_to_unibyte_storage_string(bytes);
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.set_multibyte_value(false);
        buf.insert(&storage);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev
}

fn bootstrap_eval_with_text(text: &str) -> Context {
    let mut ev = runtime_startup_context();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert(text);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev
}

fn eval_first_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU simple.el marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU simple.el from {marker} failed: {:?}", err))
        .unwrap_or_else(|| panic!("no GNU simple.el form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU simple.el form {marker} failed: {:?}", err));
}

/// Install minimal `defun`/`defmacro`/`when`/`unless` shims so a bare
/// evaluator can evaluate forms extracted from GNU `.el` source files.
fn install_bare_elisp_shims(ev: &mut Context) {
    let shims = r#"
;; `subr.el' creates these five with `defalias'; GNU has no C version of any
;; of them (lisp/subr.el:71 and :2277-2280), so a bare evaluator standing in
;; for a loaded one has to be given the same aliases subr.el gives it.
;; DIVERGENCES.md 148.
(defalias 'not #'null)
(defalias 'string= #'string-equal)
(defalias 'string< #'string-lessp)
(defalias 'string> #'string-greaterp)
(defalias 'move-marker #'set-marker)
(defalias 'defun (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name) (cons 'function (list (cons 'lambda (cons arglist body))))))))
(defalias 'defmacro (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name)
        (list 'cons ''macro (cons 'function (list (cons 'lambda (cons arglist body)))))))))
(defalias 'when (cons 'macro #'(lambda (cond &rest body)
  (list 'if cond (cons 'progn body)))))
(defalias 'unless (cons 'macro #'(lambda (cond &rest body)
  (cons 'if (cons cond (cons nil body))))))
"#;
    ev.eval_str(shims).expect("install bare elisp shims");
}

fn gnu_simple_line_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let subr_path = project_root.join("lisp/subr.el");
    let simple_source = fs::read_to_string(&simple_path)
        .expect("read GNU simple.el")
        .replace(
            "(with-suppressed-warnings ((obsolete inhibit-point-motion-hooks))",
            "(progn",
        )
        .replace("(called-interactively-p 'interactive)", "nil");
    let subr_source = fs::read_to_string(&subr_path)
        .expect("read GNU subr.el")
        .replace(
            "(defsubst buffer-narrowed-p ()",
            "(defun buffer-narrowed-p ()",
        );

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.set_lexical_binding(true);
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun zerop (number)");
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun buffer-narrowed-p ()");
    for marker in [
        "(defun count-lines (start end &optional ignore-invisible-lines)",
        "(defun beginning-of-buffer (&optional arg)",
        "(defun end-of-buffer (&optional arg)",
        "(defun goto-line (line &optional buffer relative interactive)",
        "(defun next-line (&optional arg try-vscroll)",
        "(defun previous-line (&optional arg try-vscroll)",
        "(defun line-move (arg &optional noerror _to-end try-vscroll)",
        "(defun line-move-1 (arg &optional noerror _to-end)",
        "(defun line-move-finish (column opoint forward &optional not-ipmh)",
        "(defun line-move-to-column (col)",
    ] {
        eval_first_form_after_marker(&mut ev, &simple_source, marker);
    }
    eval_str(
        &mut ev,
        "(setq next-line-add-newlines nil
               track-eol nil
               goal-column nil
               temporary-goal-column 0
               selective-display nil
               widen-automatically nil
               line-move-ignore-invisible t
               line-move-visual t)",
    );
    ev
}

/// Evaluate an Elisp string and return the result Value.
fn eval_str(ev: &mut Context, src: &str) -> Value {
    ev.eval_str(src).unwrap()
}

/// Evaluate and expect an integer result.
fn eval_int(ev: &mut Context, src: &str) -> i64 {
    match eval_str(ev, src).kind() {
        ValueKind::Fixnum(n) => n,
        _other => panic!("expected Int, got {:?}", eval_str(ev, src)),
    }
}

// -----------------------------------------------------------------------
// Position predicates
// -----------------------------------------------------------------------

#[test]
fn test_bobp_at_beginning() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    let val = eval_str(&mut ev, "(bobp)");
    assert!(val.is_truthy());
}

#[test]
fn test_bobp_not_at_beginning() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    eval_str(&mut ev, "(forward-char 2)");
    let val = eval_str(&mut ev, "(bobp)");
    assert!(val.is_nil());
}

#[test]
fn test_eobp_at_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    eval_str(&mut ev, "(goto-char 6)"); // past last char (1-based)
    let val = eval_str(&mut ev, "(eobp)");
    assert!(val.is_truthy());
}

#[test]
fn test_eobp_not_at_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    let val = eval_str(&mut ev, "(eobp)");
    assert!(val.is_nil());
}

#[test]
fn test_bolp_at_beginning_of_buffer() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    let val = eval_str(&mut ev, "(bolp)");
    assert!(val.is_truthy());
}

#[test]
fn test_bolp_after_newline() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef");
    eval_str(&mut ev, "(goto-char 5)"); // right after newline
    let val = eval_str(&mut ev, "(bolp)");
    assert!(val.is_truthy());
}

#[test]
fn test_bolp_not_at_bol() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    eval_str(&mut ev, "(forward-char 2)");
    let val = eval_str(&mut ev, "(bolp)");
    assert!(val.is_nil());
}

#[test]
fn test_eolp_at_newline() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef");
    eval_str(&mut ev, "(goto-char 4)"); // at newline
    let val = eval_str(&mut ev, "(eolp)");
    assert!(val.is_truthy());
}

#[test]
fn test_eolp_at_eob() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    eval_str(&mut ev, "(goto-char 6)");
    let val = eval_str(&mut ev, "(eolp)");
    assert!(val.is_truthy());
}

#[test]
fn test_eolp_not_at_eol() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    eval_str(&mut ev, "(goto-char 2)");
    let val = eval_str(&mut ev, "(eolp)");
    assert!(val.is_nil());
}

// -----------------------------------------------------------------------
// Line operations
// -----------------------------------------------------------------------

#[test]
fn test_line_beginning_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    eval_str(&mut ev, "(goto-char 6)"); // middle of "def"
    let pos = eval_int(&mut ev, "(line-beginning-position)");
    assert_eq!(pos, 5); // start of "def" line
}

#[test]
fn test_line_end_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    eval_str(&mut ev, "(goto-char 6)"); // middle of "def"
    let pos = eval_int(&mut ev, "(line-end-position)");
    assert_eq!(pos, 8); // end of "def" (position of newline)
}

#[test]
fn test_line_positions_on_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_unibyte_bytes(&[0xFF, b'\n', 0x80, b'A']);
    eval_str(&mut ev, "(goto-char 4)");
    assert_eq!(eval_int(&mut ev, "(line-beginning-position)"), 3);
    assert_eq!(eval_int(&mut ev, "(line-end-position)"), 5);
}

#[test]
fn test_line_beginning_position_with_offset() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaa\nbbb\nccc");
    eval_str(&mut ev, "(goto-char 1)"); // beginning of first line
    let pos = eval_int(&mut ev, "(line-beginning-position 2)");
    assert_eq!(pos, 5); // beginning of second line
}

#[test]
fn test_pos_bol_forward_past_final_unterminated_line_returns_point_max() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("one\ntwo\nthree");
    eval_str(&mut ev, "(goto-char 9)"); // beginning of final line
    assert_eq!(eval_int(&mut ev, "(pos-bol 2)"), 14);

    let mut single = eval_with_text("single");
    eval_str(&mut single, "(goto-char 1)");
    assert_eq!(eval_int(&mut single, "(pos-bol 2)"), 7);
}

#[test]
fn test_line_end_position_with_offset() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaa\nbbb\nccc");
    eval_str(&mut ev, "(goto-char 1)");
    let pos = eval_int(&mut ev, "(line-end-position 2)");
    assert_eq!(pos, 8); // end of second line (position of newline)
}

#[test]
fn test_line_positions_with_zero_offset() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello world\nfoo bar\nbaz qux\n");
    eval_str(&mut ev, "(goto-char 14)");
    assert_eq!(eval_int(&mut ev, "(line-beginning-position 0)"), 1);
    assert_eq!(eval_int(&mut ev, "(line-end-position 0)"), 12);
}

#[test]
fn test_line_end_position_zero_offset_clips_to_point_min() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello world\nfoo bar\n");
    eval_str(&mut ev, "(goto-char 5)");
    assert_eq!(eval_int(&mut ev, "(line-end-position 0)"), 1);
}

#[test]
fn test_line_number_at_pos() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    let n = eval_int(&mut ev, "(line-number-at-pos 6)");
    assert_eq!(n, 2); // "def" is line 2
}

#[test]
fn test_line_number_at_pos_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    // Point is at 1 (first char)
    let n = eval_int(&mut ev, "(line-number-at-pos)");
    assert_eq!(n, 1);
}

#[test]
fn line_number_at_pos_clips_nonabsolute_positions_to_narrowing_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let actual = eval_str(
        &mut ev,
        r#"(progn
  (erase-buffer)
  (insert "zero\none\ntwo\nthree\nfour\n")
  (let ((before (copy-marker 1))
        (after (copy-marker 20)))
    (narrow-to-region 6 14)
    (list (line-number-at-pos before)
          (line-number-at-pos after)
          (condition-case nil (line-number-at-pos 1)
            (args-out-of-range :rejected))
          (condition-case nil (line-number-at-pos 20)
            (args-out-of-range :rejected))
          (line-number-at-pos before t)
          (condition-case nil (line-number-at-pos 1 t)
            (args-out-of-range :rejected)))))"#,
    );
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&actual, &ev.buffers),
        "(1 3 1 3 1 1)"
    );
}

#[test]
fn test_line_counting_on_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_line_eval_with_unibyte_bytes(&[0xFF, b'\n', 0x80, b'A']);
    assert_eq!(
        eval_str(&mut ev, "(subrp (symbol-function 'count-lines))"),
        Value::NIL
    );
    assert_eq!(eval_int(&mut ev, "(line-number-at-pos 4)"), 2);
    assert_eq!(eval_int(&mut ev, "(count-lines 1 5)"), 2);
    assert_eq!(eval_int(&mut ev, "(forward-line 1)"), 0);
    assert_eq!(eval_int(&mut ev, "(point)"), 3);
}

#[test]
fn count_lines_accepts_marker_bounds_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("a\nb");
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut ev.buffers,
        buffer_id,
        crate::buffer::LispCharPos1::new(3),
        false,
    );
    assert_eq!(
        super::builtin_count_lines(&mut ev, vec![Value::fixnum(1), marker]).unwrap(),
        Value::fixnum(1)
    );
}

#[test]
fn count_lines_reports_narrow_to_region_range_errors_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc");

    let err = super::builtin_count_lines(&mut ev, vec![Value::fixnum(0), Value::fixnum(2)])
        .expect_err("out-of-range start should signal");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(0), Value::fixnum(2)]);
        }
        other => panic!("expected signal, got {other:?}"),
    }

    let big = Value::make_integer(Integer::from(1u64) << 100u32);
    let err = super::builtin_count_lines(&mut ev, vec![Value::fixnum(1), big])
        .expect_err("out-of-range bignum end should signal");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data[0], Value::fixnum(1));
            assert!(matches!(sig.data[1].kind(), ValueKind::Veclike(_)));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn test_forward_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    let remainder = eval_int(&mut ev, "(forward-line 1)");
    assert_eq!(remainder, 0);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 5); // beginning of "def" line
}

#[test]
fn test_forward_line_past_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef");
    let remainder = eval_int(&mut ev, "(forward-line 5)");
    assert!(remainder > 0);
}

#[test]
fn test_forward_line_negative_from_middle_of_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaa\nbbb\nccc");
    eval_str(&mut ev, "(goto-char 6)");
    let remainder = eval_int(&mut ev, "(forward-line -1)");
    assert_eq!(remainder, 0);
    assert_eq!(eval_int(&mut ev, "(point)"), 1);
}

#[test]
fn bootstrap_next_and_previous_line_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_line_eval();
    let ownership = eval_str(
        &mut ev,
        "(list (subrp (symbol-function 'next-line))
               (subrp (symbol-function 'previous-line)))",
    );
    assert_eq!(ownership, Value::list(vec![Value::NIL, Value::NIL]));

    let next_line_pos = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\ndef\")
           (goto-char 1)
           (next-line)
           (point))",
    );
    assert_eq!(next_line_pos, 5);

    let next_line_err = eval_str(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\")
           (goto-char 1)
           (condition-case err (next-line) (error (car err))))",
    );
    assert_eq!(next_line_err.as_symbol_name(), Some("end-of-buffer"));

    let previous_line_pos = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\ndef\")
           (goto-char 5)
           (previous-line)
           (point))",
    );
    assert_eq!(previous_line_pos, 1);

    let previous_line_err = eval_str(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\")
           (goto-char 1)
           (condition-case err (previous-line) (error (car err))))",
    );
    assert_eq!(
        previous_line_err.as_symbol_name(),
        Some("beginning-of-buffer")
    );

    let previous_line_mid_err = eval_str(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\")
           (goto-char 2)
           (condition-case err (previous-line) (error (car err))))",
    );
    assert_eq!(
        previous_line_mid_err.as_symbol_name(),
        Some("beginning-of-buffer")
    );
}

#[test]
fn test_beginning_of_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef");
    eval_str(&mut ev, "(goto-char 6)");
    eval_str(&mut ev, "(beginning-of-line)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 5);
}

#[test]
fn test_end_of_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef");
    eval_str(&mut ev, "(goto-char 1)");
    eval_str(&mut ev, "(end-of-line)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 4); // position of '\n'
}

#[test]
fn bootstrap_beginning_and_end_of_buffer_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_line_eval();
    let buf = ev.buffers.current_buffer_id().expect("current buffer");
    ev.frames.create_frame("F1", 800, 600, buf);

    let ownership = eval_str(
        &mut ev,
        "(list (subrp (symbol-function 'beginning-of-buffer))
               (subrp (symbol-function 'end-of-buffer)))",
    );
    assert_eq!(ownership, Value::list(vec![Value::NIL, Value::NIL]));
    eval_str(&mut ev, "(fset 'push-mark (lambda (&rest _args) nil))");
    eval_str(&mut ev, "(fset 'region-active-p (lambda () nil))");

    let beginning_default = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\ndef\")
           (goto-char 5)
           (beginning-of-buffer)
           (point))",
    );
    assert_eq!(beginning_default, 1);

    let beginning_numeric = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\ndef\")
           (goto-char 2)
           (beginning-of-buffer 1)
           (point))",
    );
    assert_eq!(beginning_numeric, 5);

    let end_default = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"abc\ndef\")
           (goto-char 2)
           (end-of-buffer)
           (point))",
    );
    assert_eq!(end_default, 8);

    let beginning_err = eval_str(
        &mut ev,
        "(condition-case err (beginning-of-buffer nil nil) (error (car err)))",
    );
    assert_eq!(
        beginning_err.as_symbol_name(),
        Some("wrong-number-of-arguments")
    );

    let end_err = eval_str(
        &mut ev,
        "(condition-case err (end-of-buffer nil nil) (error (car err)))",
    );
    assert_eq!(end_err.as_symbol_name(), Some("wrong-number-of-arguments"));
}

#[test]
fn bootstrap_goto_line_matches_simple_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_line_eval();

    let ownership = eval_str(&mut ev, "(subrp (symbol-function 'goto-line))");
    assert_eq!(ownership, Value::NIL);

    let default_pos = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"aaa\nbbb\nccc\")
           (goto-line 3)
           (point))",
    );
    assert_eq!(default_pos, 9);

    let relative_pos = eval_int(
        &mut ev,
        "(progn
           (erase-buffer)
           (insert \"a\nb\nc\nd\")
           (narrow-to-region 3 7)
           (goto-line 2 nil t nil)
           (point))",
    );
    assert_eq!(relative_pos, 5);

    let arity_err = eval_str(
        &mut ev,
        "(condition-case err (goto-line 1 nil nil nil nil) (error (car err)))",
    );
    assert_eq!(
        arity_err.as_symbol_name(),
        Some("wrong-number-of-arguments")
    );
}

// -----------------------------------------------------------------------
// Character movement
// -----------------------------------------------------------------------

#[test]
fn test_forward_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdef");
    eval_str(&mut ev, "(forward-char 3)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 4); // 1-based
}

#[test]
fn test_backward_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdef");
    eval_str(&mut ev, "(goto-char 5)");
    eval_str(&mut ev, "(backward-char 2)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 3);
}

#[test]
fn test_forward_char_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdef");
    eval_str(&mut ev, "(forward-char)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 2);
}

#[test]
fn skip_char_class_codes_match_gnu_re_wctype_t() {
    crate::test_utils::init_test_tracing();
    let expected = [
        (super::SkipCharClass::Alnum, "alnum", 1),
        (super::SkipCharClass::Alpha, "alpha", 2),
        (super::SkipCharClass::Word, "word", 3),
        (super::SkipCharClass::Graph, "graph", 4),
        (super::SkipCharClass::Print, "print", 5),
        (super::SkipCharClass::Lower, "lower", 6),
        (super::SkipCharClass::Upper, "upper", 7),
        (super::SkipCharClass::Punct, "punct", 8),
        (super::SkipCharClass::Cntrl, "cntrl", 9),
        (super::SkipCharClass::Digit, "digit", 10),
        (super::SkipCharClass::Xdigit, "xdigit", 11),
        (super::SkipCharClass::Blank, "blank", 12),
        (super::SkipCharClass::Space, "space", 13),
        (super::SkipCharClass::Multibyte, "multibyte", 14),
        (super::SkipCharClass::Nonascii, "nonascii", 15),
        (super::SkipCharClass::Ascii, "ascii", 16),
        (super::SkipCharClass::Unibyte, "unibyte", 17),
    ];

    for (class, name, gnu_code) in expected {
        assert_eq!(u8::from(class), gnu_code);
        assert_eq!(name.parse::<super::SkipCharClass>(), Ok(class));
    }
    assert!("Alpha".parse::<super::SkipCharClass>().is_err());
}

#[test]
fn test_skip_chars_forward() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaabbbccc");
    let moved = eval_int(&mut ev, "(skip-chars-forward \"a\")");
    assert_eq!(moved, 3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 4);
}

#[test]
fn test_skip_chars_forward_range() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdef123");
    let moved = eval_int(&mut ev, "(skip-chars-forward \"a-f\")");
    assert_eq!(moved, 6);
}

#[test]
fn test_skip_chars_backward() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaabbbccc");
    eval_str(&mut ev, "(goto-char 10)"); // end
    let moved = eval_int(&mut ev, "(skip-chars-backward \"c\")");
    assert_eq!(moved, -3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 7);
}

#[test]
fn test_skip_chars_forward_negate() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaabbbccc");
    let moved = eval_int(&mut ev, "(skip-chars-forward \"^b\")");
    assert_eq!(moved, 3);
}

#[test]
fn skip_chars_forward_line_scans_do_not_rescan_from_buffer_start() {
    crate::test_utils::init_test_tracing();
    let line_count = 6_000;
    let mut text = String::new();
    for _ in 0..line_count {
        text.push_str("abc def\n");
    }
    let mut ev = eval_with_text(&text);

    let start = std::time::Instant::now();
    let visited = eval_int(
        &mut ev,
        r#"(let ((n 0))
             ;; `not' is lisp/subr.el:71 and does not exist on a bare
             ;; evaluator (DIVERGENCES.md 148); `null' is the C subr it
             ;; aliases (src/data.c:177).
             (while (null (eobp))
               (skip-chars-forward "^ \t")
               (skip-chars-forward " \t")
               (skip-chars-forward "^\r\n")
               (forward-line 1)
               (setq n (1+ n)))
             n)"#,
    );
    let elapsed = start.elapsed();

    assert_eq!(visited, line_count);
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "skip-chars-forward should count scanned chars directly like GNU, elapsed={elapsed:?}"
    );
}

// -----------------------------------------------------------------------
// Mark and region
// -----------------------------------------------------------------------

#[test]
fn test_push_mark_and_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello world");
    eval_str(&mut ev, "(push-mark 3)");
    let m = eval_int(&mut ev, "(mark t)");
    assert_eq!(m, 3);
}

#[test]
fn test_push_mark_default_pos() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    eval_str(&mut ev, "(goto-char 3)");
    eval_str(&mut ev, "(push-mark)");
    let m = eval_int(&mut ev, "(mark t)");
    assert_eq!(m, 3);
}

#[test]
fn test_pop_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello world");
    eval_str(&mut ev, "(push-mark 3)");
    eval_str(&mut ev, "(push-mark 5)");
    // Mark is now at 5, ring has [3]
    let m = eval_int(&mut ev, "(mark t)");
    assert_eq!(m, 5);
    eval_str(&mut ev, "(pop-mark)");
    let m2 = eval_int(&mut ev, "(mark t)");
    assert_eq!(m2, 3);
}

#[test]
fn test_region_beginning_and_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello world");
    eval_str(&mut ev, "(goto-char 8)");
    eval_str(&mut ev, "(push-mark 3 nil t)");
    let beg = eval_int(&mut ev, "(region-beginning)");
    let end = eval_int(&mut ev, "(region-end)");
    assert_eq!(beg, 3);
    assert_eq!(end, 8);
}

#[test]
fn test_region_beginning_and_end_clip_mark_to_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("0123456789");
    let bounds = eval_str(
        &mut ev,
        r#"(let ((transient-mark-mode nil))
             (goto-char 6)
             (set-marker (mark-marker) 2 (current-buffer))
             (narrow-to-region 4 8)
             (list (region-beginning) (region-end)))"#,
    );
    assert_eq!(bounds, eval_str(&mut ev, "'(4 6)"));
}

#[test]
fn test_set_mark_nil_clears_region() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("abc");
    let result = eval_str(
        &mut ev,
        r#"(progn
             (set-mark 2)
             (set-mark nil)
             (condition-case err
                 (region-beginning)
               (error (list (car err) (cdr err)))))"#,
    );
    assert_eq!(
        result,
        eval_str(
            &mut ev,
            r#"'(error ("The mark is not set now, so there is no region"))"#,
        )
    );
}

#[test]
fn test_use_region_p_is_available_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    // use-region-p is a defun in simple.el, not autoloaded in GNU Emacs.
    let ev = runtime_startup_context();
    let function = ev
        .obarray
        .symbol_function("use-region-p")
        .expect("missing use-region-p startup function cell");
    assert!(
        !crate::emacs_core::autoload::is_autoload_value(&function),
        "expected use-region-p to be a resolved function, not an autoload"
    );
}

#[test]
fn test_use_region_p() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let active = eval_str(
        &mut ev,
        "(let ((transient-mark-mode t))
           (push-mark 3 nil t)
           (use-region-p))",
    );
    assert!(active.is_truthy());
}

#[test]
fn test_use_region_p_inactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    eval_str(&mut ev, "(push-mark 3)"); // not activated
    let active = eval_str(&mut ev, "(use-region-p)");
    assert!(active.is_nil());
}

#[test]
fn test_region_active_p_true_for_active_empty_region() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let active = eval_str(
        &mut ev,
        "(let ((transient-mark-mode t))
           (push-mark (point) nil t)
           (region-active-p))",
    );
    assert!(active.is_truthy());
}

#[test]
fn test_region_active_p_requires_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let active = eval_str(
        &mut ev,
        "(condition-case err
             (let ((transient-mark-mode t)
                   (mark-active t))
               (region-active-p))
           (error (list (car err) (cdr err))))",
    );
    assert_eq!(
        active,
        eval_str(&mut ev, "'(cl-assertion-failed ((mark)))",)
    );
}

#[test]
fn test_region_active_p_over_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let result = eval_str(
        &mut ev,
        "(condition-case err (region-active-p nil) (error (car err)))",
    );
    assert_eq!(result, Value::symbol("wrong-number-of-arguments"));
}

#[test]
fn test_deactivate_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    eval_str(&mut ev, "(push-mark 3 nil t)");
    eval_str(&mut ev, "(deactivate-mark)");
    let active = eval_str(&mut ev, "(use-region-p)");
    assert!(active.is_nil());
}

#[test]
fn test_exchange_point_and_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello world");
    eval_str(&mut ev, "(goto-char 3)");
    eval_str(&mut ev, "(push-mark 8 nil t)");
    eval_str(&mut ev, "(exchange-point-and-mark)");
    let pt = eval_int(&mut ev, "(point)");
    let mk = eval_int(&mut ev, "(mark t)");
    assert_eq!(pt, 8);
    assert_eq!(mk, 3);
}

#[test]
fn transient_mark_mode_the_variable_is_c_and_the_command_is_not() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("hello");
    // DEFVAR_LISP ("transient-mark-mode", ...) is src/buffer.c:5835 and is
    // here; the `define-minor-mode' command at lisp/simple.el:7614 is not, and
    // its arms are measured against GNU in
    // `builtins::lisp_only_misc_names_test::misc_name_arms_match_gnu'
    // (DIVERGENCES.md 152).
    assert_eq!(eval_str(&mut ev, "(boundp 'transient-mark-mode)"), Value::T);
    assert_eq!(
        eval_str(&mut ev, "(fboundp 'transient-mark-mode)"),
        Value::NIL
    );
}

#[test]
fn test_mark_marker() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    eval_str(&mut ev, "(push-mark 4)");
    let pos = eval_int(&mut ev, "(marker-position (mark-marker))");
    assert_eq!(pos, 4);
}

#[test]
fn test_set_mark_activates() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let active = eval_str(
        &mut ev,
        "(let ((transient-mark-mode t))
           (set-mark 3)
           (use-region-p))",
    );
    assert!(active.is_truthy());
}

#[test]
fn test_use_region_p_honors_buffer_local_mark_active_when_global_is_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_eval_with_text("hello");
    let active = eval_str(
        &mut ev,
        "(with-temp-buffer
           (let ((transient-mark-mode t))
             (insert \"abc\")
             (goto-char (point-max))
             (set-mark (point-min))
             (setq mark-active t)
             (use-region-p)))",
    );
    assert!(active.is_truthy());
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn test_empty_buffer_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let val = eval_str(&mut ev, "(bobp)");
    assert!(val.is_truthy());
    let val = eval_str(&mut ev, "(eobp)");
    assert!(val.is_truthy());
    let val = eval_str(&mut ev, "(bolp)");
    assert!(val.is_truthy());
    let val = eval_str(&mut ev, "(eolp)");
    assert!(val.is_truthy());
}

#[test]
fn test_forward_line_negative() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    eval_str(&mut ev, "(goto-char 9)"); // on "ghi" line
    eval_str(&mut ev, "(forward-line -1)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 5); // beginning of "def"
}

#[test]
fn test_line_number_at_pos_last_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abc\ndef\nghi");
    let n = eval_int(&mut ev, "(line-number-at-pos 10)");
    assert_eq!(n, 3);
}

#[test]
fn test_skip_chars_forward_with_limit() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaaaaaa");
    let moved = eval_int(&mut ev, "(skip-chars-forward \"a\" 4)");
    assert_eq!(moved, 3); // limited to position 4 (1-based = 3 chars from pos 1)
}

#[test]
fn skip_chars_accepts_marker_limit() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaaaaaa");
    let moved = eval_int(&mut ev, "(skip-chars-forward \"a\" (copy-marker 4))");
    assert_eq!(moved, 3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 4);

    eval_str(&mut ev, "(goto-char 8)");
    let moved = eval_int(&mut ev, "(skip-chars-backward \"a\" (copy-marker 5))");
    assert_eq!(moved, -3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 5);
}

#[test]
fn skip_chars_bignum_limit_clamps_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("aaa");
    let moved = eval_int(
        &mut ev,
        "(skip-chars-forward \"a\" 1000000000000000000000000000000000000)",
    );
    assert_eq!(moved, 3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 4);

    eval_str(&mut ev, "(goto-char 4)");
    let moved = eval_int(
        &mut ev,
        "(skip-chars-backward \"a\" -1000000000000000000000000000000000000)",
    );
    assert_eq!(moved, -3);
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 1);
}

#[test]
fn test_forward_char_negative() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdef");
    eval_str(&mut ev, "(goto-char 4)");
    eval_str(&mut ev, "(forward-char -2)");
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 2);
}

/// Regression for audit §7.1: `forward-char` must clamp against the
/// narrowed region (BEGV/ZV), matching GNU `move_point` in
/// `src/cmds.c:36`. Previously it clamped against the absolute buffer
/// extents, which let point silently slip outside the accessible
/// portion.
#[test]
fn test_forward_char_honors_narrowing_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdefghij");
    // Narrow to "cdef" (positions 3..7).
    eval_str(&mut ev, "(narrow-to-region 3 7)");
    eval_str(&mut ev, "(goto-char 5)");
    // Try to walk past ZV — must signal end-of-buffer and stop at ZV (= 7).
    let result = eval_str(
        &mut ev,
        "(condition-case err (forward-char 10) (end-of-buffer 'caught))",
    );
    assert_eq!(result.as_symbol_name(), Some("caught"));
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 7);
}

#[test]
fn test_forward_char_honors_narrowing_beginning() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_text("abcdefghij");
    eval_str(&mut ev, "(narrow-to-region 3 7)");
    eval_str(&mut ev, "(goto-char 5)");
    // Walking back past BEGV must signal beginning-of-buffer and clamp.
    let result = eval_str(
        &mut ev,
        "(condition-case err (forward-char -10) (beginning-of-buffer 'caught))",
    );
    assert_eq!(result.as_symbol_name(), Some("caught"));
    let pos = eval_int(&mut ev, "(point)");
    assert_eq!(pos, 3);
}
