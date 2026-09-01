use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::builtins::search::{
    builtin_looking_at, builtin_match_data, builtin_match_data_translate, builtin_match_string,
    builtin_re_search_forward, builtin_replace_match, builtin_set_match_data, builtin_string_match,
};
use crate::emacs_core::marker;
use crate::emacs_core::search::builtin_replace_regexp_in_string;
use crate::emacs_core::value::{ValueKind, list_to_vec};
use crate::heap_types::LispString;

// Test helpers that keep the Context alive across the returned
// Value. Previously these created a bare Context inside a block
// and returned the result; the Context was dropped at end of
// block, destroying the tagged heap and leaving the returned
// Value pointing at freed memory. A thread_local Box<Context>
// holds it alive until the next call replaces it.
use std::cell::RefCell;
thread_local! {
    static SEARCH_TEST_CTX: RefCell<Option<Box<crate::emacs_core::eval::Context>>> =
        const { RefCell::new(None) };
}

fn call_string_match(args: Vec<Value>) -> EvalResult {
    SEARCH_TEST_CTX.with(|slot| {
        let mut new_ctx = Box::new(crate::emacs_core::eval::Context::new());
        let result = builtin_string_match(&mut new_ctx, args);
        *slot.borrow_mut() = Some(new_ctx);
        result
    })
}

fn call_replace_regexp_in_string(args: Vec<Value>) -> EvalResult {
    SEARCH_TEST_CTX.with(|slot| {
        let mut new_ctx = Box::new(crate::emacs_core::eval::Context::new());
        let result = builtin_replace_regexp_in_string(&mut new_ctx, args);
        *slot.borrow_mut() = Some(new_ctx);
        result
    })
}

#[test]
fn buffer_replace_match_backref_does_not_copy_source_properties() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert "fooXXXbar")
  (put-text-property 1 10 'group 'first)
  (goto-char 1)
  (re-search-forward "foo\\([A-Z]+\\)bar" nil t)
  (replace-match "qux\\1baz")
  (list (buffer-string)
        (get-text-property 1 'group)
        (get-text-property 4 'group)))"#,
        )
        .expect("replace-match form should evaluate");
    let items = list_to_vec(&result).expect("expected list result");
    assert_eq!(items.len(), 3);
    assert_str(items[0], "quxXXXbaz");
    assert!(items[1].is_nil());
    assert!(items[2].is_nil());
}

/// GNU `Freplace_match` (search.c) treats the buffer and string replacement
/// paths differently for `\?`: the string path (search.c:2567,
/// `else if (c != '?')`) tolerates it as a literal because `\?' is reserved for
/// `query-replace-regexp`, but the buffer path (search.c:2694) rejects it like
/// any other invalid escape with "Invalid use of `\\' in replacement text".
/// neomacs previously whitelisted `\?' in both paths; this asserts the buffer
/// path now errors while the string path (replace-regexp-in-string) still
/// passes it through literally, and that the other escapes (\& \N \\) keep
/// working in the buffer path.  Verified against `/usr/bin/emacs --batch`.
#[test]
fn buffer_replace_match_rejects_backslash_question_but_string_path_allows_it() {
    crate::test_utils::init_test_tracing();

    // Buffer path: `\?' must signal `error' "Invalid use of `\\' in replacement text".
    let mut ev = crate::emacs_core::eval::Context::new();
    let err = ev
        .eval_str(
            r#"(progn
  (insert "abc")
  (goto-char 1)
  (re-search-forward "b")
  (condition-case e (replace-match "\\?") (error e)))"#,
        )
        .expect("condition-case form should evaluate to the captured error");
    let items = list_to_vec(&err).expect("captured error should be a list");
    assert_eq!(items.len(), 2);
    assert!(
        items[0].is_symbol_named("error"),
        "expected `error' symbol, got {:?}",
        items[0]
    );
    // The C-level message `Invalid use of `\\' in replacement text` is requoted
    // through `text-quoting-style` (curly by default), matching GNU's default
    // `--batch` output of U+2018/U+2019 around the backslash.
    assert_str(
        items[1],
        "Invalid use of \u{2018}\\\u{2019} in replacement text",
    );

    // Buffer path: other escapes still resolve normally.
    let mut ev = crate::emacs_core::eval::Context::new();
    let ok = ev
        .eval_str(
            r#"(progn
  (insert "abc")
  (goto-char 1)
  (re-search-forward "\\(b\\)")
  (replace-match "[\\&-\\1-\\\\]")
  (buffer-string))"#,
        )
        .expect("valid backslash escapes should evaluate");
    assert_str(ok, "a[b-b-\\]c");

    // String path (replace-regexp-in-string): `\?' is kept literally.
    let result = call_replace_regexp_in_string(vec![
        Value::string("b"),
        Value::string("\\?"),
        Value::string("abc"),
    ])
    .expect("string replacement with `\\?' should succeed");
    assert_str(result, "a\\?c");
}

#[test]
fn replace_match_buffer_replacement_leaves_point_at_replacement_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert "&amp;")
  (goto-char 1)
  (looking-at "&amp;")
  (replace-match "&#38;" t t)
  (let ((after-first (point)))
    (goto-char 1)
    (looking-at "&#38;")
    (replace-match "&" t t)
    (list after-first (point) (buffer-string))))"#,
        )
        .expect("replace-match point form should evaluate");
    let items = list_to_vec(&result).expect("expected list result");
    assert_eq!(items.len(), 3);
    assert_int(items[0], 6);
    assert_int(items[1], 2);
    assert_str(items[2], "&");
}

#[test]
fn replace_match_preserves_raw_nil_boundaries_for_later_properties() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert (concat (make-string 46 ?x) "abc" (make-string 31 ?x)))
  (put-text-property 1 25 'face 'org-table)
  (goto-char 47)
  (looking-at "abc")
  (replace-match "abcde" t t)
  (put-text-property 43 54 'face 'org-table)
  (prin1-to-string (buffer-string)))"#,
        )
        .expect("replace-match interval shape form should evaluate");
    let printed = result.as_utf8_str().unwrap().to_owned();
    assert!(
        printed.contains("42 46 (face org-table) 46 51 (face org-table) 51 53 (face org-table)"),
        "raw nil boundaries inside later property range should be preserved: {printed}"
    );
}

#[test]
fn replace_match_after_change_end_uses_replacement_length() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert "* TODO Wed-task
SCHEDULED: <2026-05-27 Wed>
")
  (setq hook-log nil)
  (setq after-change-functions
        (list (lambda (beg end old-len)
                (setq hook-log (list beg end old-len)))))
  (goto-char 1)
  (search-forward "TODO Wed-task")
  (replace-match "DONE Wed-task" t t)
  (list hook-log (buffer-string)))"#,
        )
        .expect("replace-match after-change form should evaluate");
    let items = list_to_vec(&result).expect("expected list result");
    assert_eq!(items.len(), 2);
    let hook = list_to_vec(&items[0]).expect("expected hook list");
    assert_eq!(hook.len(), 3);
    assert_int(hook[0], 3);
    assert_int(hook[1], 16);
    assert_int(hook[2], 13);
    assert_str(items[1], "* DONE Wed-task\nSCHEDULED: <2026-05-27 Wed>\n");
}

#[test]
fn replace_match_after_change_uses_restored_integer_match_positions() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert "alpha one")
  (goto-char 1)
  (search-forward "alpha")
  (let ((saved (match-data t))
        hook-log)
    (set-match-data saved)
    (setq after-change-functions
          (list (lambda (beg end old-len)
                  (setq hook-log (list beg end old-len)))))
    (replace-match "omega" t t)
    (list hook-log (buffer-string))))"#,
        )
        .expect("replace-match restored match data form should evaluate");
    let items = list_to_vec(&result).expect("expected list result");
    assert_eq!(items.len(), 2);
    let hook = list_to_vec(&items[0]).expect("expected hook list");
    assert_eq!(hook.len(), 3);
    assert_int(hook[0], 1);
    assert_int(hook[1], 6);
    assert_int(hook[2], 5);
    assert_str(items[1], "omega one");
}

#[test]
fn set_match_data_without_buffer_restores_string_match_positions() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (set-match-data '(3 4))
  (replace-match "X" t t "ab红阵营cd"))"#,
        )
        .expect("replace-match should use restored string character positions");
    assert_str(result, "ab红X营cd");
}

#[test]
fn save_match_data_preserves_string_match_positions_for_replace_match() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (string-match "阵" "ab红阵营cd")
  (let ((saved-match-data (match-data)))
    (unwind-protect
        (string-match "x" "x")
      (set-match-data saved-match-data t)))
  (replace-match "X" t t "ab红阵营cd"))"#,
        )
        .expect("replace-match should use saved string character positions");
    assert_str(result, "ab红X营cd");
}

#[test]
fn replace_match_after_change_reports_multibyte_char_positions() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev
        .eval_str(
            r#"(progn
  (insert "ab")
  (setq hook-log nil)
  (setq after-change-functions
        (list (lambda (beg end old-len)
                (setq hook-log (list beg end old-len)))))
  (goto-char 1)
  (looking-at "a")
  (replace-match "éé" t t)
  (list hook-log (buffer-string)))"#,
        )
        .expect("replace-match multibyte after-change form should evaluate");
    let items = list_to_vec(&result).expect("expected list result");
    assert_eq!(items.len(), 2);
    let hook = list_to_vec(&items[0]).expect("expected hook list");
    assert_eq!(hook.len(), 3);
    assert_int(hook[0], 1);
    assert_int(hook[1], 3);
    assert_int(hook[2], 1);
    assert_str(items[1], "ééb");
}

fn call_looking_at_in_buffer(pattern: Value, buffer_text: &str) -> EvalResult {
    SEARCH_TEST_CTX.with(|slot| {
        let mut new_ctx = Box::new(crate::emacs_core::eval::Context::new());
        {
            let buf = new_ctx
                .buffers
                .current_buffer_mut()
                .expect("current buffer");
            buf.insert(buffer_text);
            buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
        }
        let result = builtin_looking_at(&mut new_ctx, vec![pattern]);
        *slot.borrow_mut() = Some(new_ctx);
        result
    })
}

fn assert_int(val: Value, expected: i64) {
    match val.kind() {
        ValueKind::Fixnum(n) => assert_eq!(n, expected),
        _other => panic!("Expected Int({}), got {:?}", expected, val),
    }
}

fn assert_nil(val: Value) {
    assert!(val.is_nil(), "Expected nil, got {:?}", val);
}

fn assert_true(val: Value) {
    assert!(val.is_truthy(), "Expected true, got {:?}", val);
}

fn assert_str(val: Value, expected: &str) {
    match val.kind() {
        ValueKind::String => {
            let s = val.as_utf8_str().unwrap().to_owned();
            assert_eq!(&*s, expected);
        }
        _other => panic!("Expected string {:?}, got {:?}", expected, val),
    }
}

fn assert_lisp_string_bytes(val: Value, expected: &[u8], multibyte: bool) {
    match val.kind() {
        ValueKind::String => {
            let s = val
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            assert_eq!(s.as_bytes(), expected);
            assert_eq!(s.is_multibyte(), multibyte);
        }
        _other => panic!("Expected string bytes {:?}, got {:?}", expected, val),
    }
}

#[test]
fn string_match_basic() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![Value::string("he..o"), Value::string("hello world")]);
    assert_int(result.unwrap(), 0);
}

#[test]
fn string_match_with_start() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![
        Value::string("world"),
        Value::string("hello world"),
        Value::fixnum(6),
    ]);
    assert_int(result.unwrap(), 6);
}

#[test]
fn string_match_no_match() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![Value::string("xyz"), Value::string("hello world")]);
    assert_nil(result.unwrap());
}

#[test]
fn string_match_defaults_to_case_fold() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![Value::string("a"), Value::string("A")]);
    assert_int(result.unwrap(), 0);
}

/// `string-match-p' has no subr, here or in GNU: it is a `defsubst' at
/// lisp/subr.el:5941 whose whole body is `(string-match REGEXP STRING START t)',
/// so a caller inlines the C primitive with its INHIBIT-MODIFY argument set and
/// never looks the name up (DIVERGENCES.md 152).  What a bare evaluator can
/// measure is that argument, on `string-match' (src/search.c:442) -- which is
/// what the three `string_match_p_*' tests that used to sit here were about:
/// the answer, the miss, the case fold, and the match data left alone.
///
/// Measured on GNU 31.0.90 `-Q --batch' first.
#[test]
fn string_match_with_inhibit_modify_answers_without_touching_match_data() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let result = ev.eval_str(
        r#"(list (string-match "x\\([0-9]+\\)" "x42")
                     (match-data)
                     (string-match "[0-9]+" "abc 123 def" nil t)
                     (string-match "[0-9]+" "no digits here" nil t)
                     (string-match "a" "A" nil t)
                     (match-data))"#,
    );
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (0 (0 3 1 3) 4 nil 0 (0 3 1 3))",
    );
}

#[test]
fn string_match_accepts_raw_unibyte_regexp_pattern() {
    crate::test_utils::init_test_tracing();
    let pattern = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let haystack = Value::heap_string(LispString::from_unibyte(vec![0x80, 0xFF, 0x81]));
    let result = call_string_match(vec![pattern, haystack]).unwrap();
    assert_int(result, 1);
}

#[test]
fn string_match_accepts_start_on_raw_unibyte_haystack() {
    crate::test_utils::init_test_tracing();
    let pattern = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let haystack = Value::heap_string(LispString::from_unibyte(vec![0x80, 0xFF, 0x81]));
    let result = call_string_match(vec![pattern, haystack, Value::fixnum(1)]).unwrap();
    assert_int(result, 1);
}

#[test]
fn match_string_preserves_raw_unibyte_bytes_with_explicit_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let pattern = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let haystack = Value::heap_string(LispString::from_unibyte(vec![0x80, 0xFF, 0x81]));
    let matched = builtin_string_match(&mut eval, vec![pattern, haystack]).unwrap();
    assert_int(matched, 1);

    let slice = builtin_match_string(&mut eval, vec![Value::fixnum(0), haystack]).unwrap();
    let string = slice
        .as_lisp_string()
        .expect("match-string should return string");
    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xFF]);
}

#[test]
fn explicit_string_match_uses_public_register_positions_after_buffer_search() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("explicit-string-buffer-match");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "abcdef")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("rewind point");

    builtin_re_search_forward(&mut eval, vec![Value::string("bc")]).expect("regexp should match");

    assert_eq!(
        builtin_match_string(&mut eval, vec![Value::fixnum(0), Value::string("abcdef")])
            .expect("match-string should succeed"),
        Value::string("cd")
    );
    assert_eq!(
        builtin_replace_match(
            &mut eval,
            vec![
                Value::string("X"),
                Value::NIL,
                Value::NIL,
                Value::string("abcdef"),
            ],
        )
        .expect("replace-match should succeed"),
        Value::string("abXef")
    );
}

#[test]
fn translated_buffer_match_data_expands_backrefs_in_explicit_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval
        .buffers
        .create_buffer("translated-explicit-string-backrefs");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "a1b2c3")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("rewind point");

    builtin_re_search_forward(&mut eval, vec![Value::string("\\(.\\)\\(.\\)")])
        .expect("regexp should match");
    builtin_match_data_translate(&mut eval, vec![Value::fixnum(-1)])
        .expect("match data translation should succeed");

    assert_eq!(
        builtin_match_string(&mut eval, vec![Value::fixnum(0), Value::string("a1")])
            .expect("match-string should succeed"),
        Value::string("a1")
    );
    assert_eq!(
        builtin_replace_match(
            &mut eval,
            vec![
                Value::string("[\\1\\2]"),
                Value::NIL,
                Value::NIL,
                Value::string("a1"),
            ],
        )
        .expect("replace-match should succeed"),
        Value::string("[a1]")
    );
}

#[test]
fn looking_at_accepts_raw_unibyte_regexp_pattern_in_buffer() {
    crate::test_utils::init_test_tracing();
    let pattern = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let result = call_looking_at_in_buffer(pattern, "tool-bar").unwrap();
    assert_nil(result);
}

#[test]
fn regexp_quote_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("foo.bar*baz+qux")]);
    assert_str(result.unwrap(), "foo\\.bar\\*baz\\+qux");
}

#[test]
fn regexp_quote_no_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("hello")]);
    assert_str(result.unwrap(), "hello");
}

#[test]
fn regexp_quote_all_specials() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string(".*+?[]^$\\")]);
    // GNU regexp-quote does NOT escape ']' — only '[' is special.
    assert_str(result.unwrap(), "\\.\\*\\+\\?\\[]\\^\\$\\\\");
}

#[test]
fn regexp_quote_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(LispString::from_unibyte(vec![b'a', 0xFF, b'*', b'[', b'z']));
    let result = builtin_regexp_quote(vec![input]).unwrap();
    let string = result
        .as_lisp_string()
        .expect("regexp-quote should return string");
    assert!(!string.is_multibyte());
    assert_eq!(
        string.as_bytes(),
        &[b'a', 0xFF, b'\\', b'*', b'\\', b'[', b'z']
    );
    assert!(string.as_utf8_str().is_none());
}

#[test]
fn match_data_nil_without_match_data() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_set_match_data(&mut eval, vec![Value::NIL]).unwrap();
    let result = builtin_match_data(&mut eval, vec![]);
    assert_nil(result.unwrap());
}

#[test]
fn set_match_data_nil_clears_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_set_match_data(
        &mut eval,
        vec![Value::list(vec![Value::fixnum(1), Value::fixnum(2)])],
    )
    .unwrap();
    let result = builtin_set_match_data(&mut eval, vec![Value::NIL]);
    assert_nil(result.unwrap());
    let md = builtin_match_data(&mut eval, vec![]).unwrap();
    assert_nil(md);
}

#[test]
fn set_match_data_round_trip() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_set_match_data(
        &mut eval,
        vec![Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::fixnum(5),
            Value::fixnum(7),
        ])],
    )
    .unwrap();
    let md = builtin_match_data(&mut eval, vec![]).unwrap();
    assert_eq!(
        md,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::fixnum(5),
            Value::fixnum(7)
        ])
    );
}

#[test]
fn killed_buffer_match_data_uses_detached_markers_and_restores_zero() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("dead-core-match-data");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::ZERO)
        .expect("rewind point");
    builtin_re_search_forward(&mut eval, vec![Value::string("\\(abc\\)")])
        .expect("seed buffer match data");
    assert!(eval.buffers.kill_buffer(buffer_id));

    let saved = builtin_match_data(&mut eval, vec![]).expect("materialize dead-buffer match data");
    let items = list_to_vec(&saved).expect("match-data returns a proper list");
    assert_eq!(items.len(), 4);
    for marker in &items {
        let data = marker
            .as_marker_data()
            .expect("dead buffer positions materialize as markers");
        assert_eq!(data.buffer, None);
        assert_eq!(data.charpos, 0);
        assert!(!data.last_position_valid);
    }

    builtin_set_match_data(&mut eval, vec![saved]).expect("detached markers coerce to zero");
    let restored = builtin_match_data(&mut eval, vec![Value::T]).expect("read restored registers");
    assert_eq!(
        list_to_vec(&restored),
        Some(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ])
    );
}

#[test]
fn set_match_data_reseat_detaches_buffer_markers() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("match-data-reseat");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "alpha beta")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("rewind point");

    assert_true(builtin_looking_at(&mut eval, vec![Value::string("alpha")]).unwrap());
    let saved = builtin_match_data(&mut eval, vec![]).unwrap();
    let start_marker = saved.cons_car();
    let end_marker = saved.cons_cdr().cons_car();
    assert!(start_marker.as_marker_data().unwrap().buffer.is_some());
    assert!(end_marker.as_marker_data().unwrap().buffer.is_some());

    builtin_set_match_data(&mut eval, vec![saved, Value::T]).unwrap();

    assert!(start_marker.as_marker_data().unwrap().buffer.is_none());
    assert!(end_marker.as_marker_data().unwrap().buffer.is_none());
    assert_nil(saved.cons_car());
    assert_nil(saved.cons_cdr().cons_car());
    let buffer = eval.buffers.get(buffer_id).expect("test buffer");
    assert_eq!(buffer.marker_chain_len(), 0);
}

#[test]
fn match_data_destructively_fills_reuse_list_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("match-data-reuse");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("rewind point");

    builtin_re_search_forward(&mut eval, vec![Value::string("\\(a\\)b")])
        .expect("regexp should match");
    let reuse = Value::list(vec![
        Value::symbol("a"),
        Value::symbol("b"),
        Value::symbol("c"),
        Value::symbol("d"),
        Value::symbol("e"),
    ]);

    let result = builtin_match_data(&mut eval, vec![Value::T, reuse]).unwrap();

    assert_eq!(result, reuse);
    assert_eq!(
        list_to_vec(&reuse).unwrap(),
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::make_buffer(buffer_id),
        ]
    );
}

#[test]
fn match_data_reseat_reuse_markers_before_refill_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.create_buffer("match-data-reseat-reuse");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert test text");
    eval.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("rewind point");

    builtin_re_search_forward(&mut eval, vec![Value::string("b")]).expect("regexp should match");
    let first_marker = marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(3),
        false,
    );
    let reuse = Value::list(vec![
        first_marker,
        marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(3),
            false,
        ),
        marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(3),
            false,
        ),
        marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(3),
            false,
        ),
        marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(3),
            false,
        ),
    ]);

    let result = builtin_match_data(&mut eval, vec![Value::T, reuse, Value::T]).unwrap();

    assert_eq!(result, reuse);
    assert!(first_marker.as_marker_data().unwrap().buffer.is_none());
    assert_eq!(
        list_to_vec(&reuse).unwrap(),
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::make_buffer(buffer_id),
            Value::NIL,
            Value::NIL,
        ]
    );
}

#[test]
fn string_match_start_nil_and_negative() {
    crate::test_utils::init_test_tracing();
    let with_nil =
        call_string_match(vec![Value::string("a"), Value::string("ba"), Value::NIL]).unwrap();
    assert_int(with_nil, 1);

    let with_negative = call_string_match(vec![
        Value::string("a"),
        Value::string("ba"),
        Value::fixnum(-1),
    ])
    .unwrap();
    assert_int(with_negative, 1);

    let out_of_range = call_string_match(vec![
        Value::string("a"),
        Value::string("ba"),
        Value::fixnum(3),
    ]);
    assert!(out_of_range.is_err());
}

#[test]
fn replace_regexp_basic() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("[0-9]+"),
        Value::string("NUM"),
        Value::string("abc 123 def 456"),
    ]);
    assert_str(result.unwrap(), "abc NUM def NUM");
}

#[test]
fn replace_regexp_literal() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("[0-9]+"),
        Value::string("$0"),
        Value::string("abc 123 def"),
        Value::NIL, // fixedcase
        Value::T,   // literal
    ]);
    assert_str(result.unwrap(), "abc $0 def");
}

#[test]
fn replace_regexp_with_backref() {
    crate::test_utils::init_test_tracing();
    // Use Emacs-style group: \(\w+\) and back-reference \1
    let result = call_replace_regexp_in_string(vec![
        Value::string("\\(\\w+\\)"),
        Value::string("[\\1]"),
        Value::string("hello world"),
    ]);
    assert_str(result.unwrap(), "[hello] [world]");
}

#[test]
fn replace_regexp_with_start() {
    crate::test_utils::init_test_tracing();
    // Emacs: START omits the first START chars from the result.
    let result = call_replace_regexp_in_string(vec![
        Value::string("[0-9]+"),
        Value::string("X"),
        Value::string("111 222 333"),
        Value::NIL,       // fixedcase
        Value::NIL,       // literal
        Value::NIL,       // subexp
        Value::fixnum(4), // start
    ]);
    assert_str(result.unwrap(), "X X");
}

#[test]
fn replace_regexp_with_start_no_subexp() {
    crate::test_utils::init_test_tracing();
    // In Emacs, arg 6 is SUBEXP and arg 7 is START.
    // To pass START without SUBEXP, use nil for SUBEXP.
    let result = call_replace_regexp_in_string(vec![
        Value::string("[0-9]+"),
        Value::string("X"),
        Value::string("111 222 333"),
        Value::NIL,       // fixedcase
        Value::NIL,       // literal
        Value::NIL,       // subexp (default 0)
        Value::fixnum(4), // start
    ]);
    assert_str(result.unwrap(), "X X");
}

#[test]
fn replace_regexp_subexp() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("\\([a-z]+\\)-\\([0-9]+\\)"),
        Value::string("N"),
        Value::string("aaa-111 bbb-222"),
        Value::NIL, // fixedcase
        Value::NIL, // literal
        Value::fixnum(1),
        Value::NIL, // start
    ]);
    assert_str(result.unwrap(), "N-111 N-222");
}

#[test]
fn replace_regexp_subexp_unmatched_errors() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("\\(a\\)?b"),
        Value::string("N"),
        Value::string("b"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(1),
        Value::NIL,
    ]);
    assert!(result.is_err());
}

#[test]
fn replace_regexp_subexp_past_gnu_search_regs_errors_out_of_range() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("cat"),
        Value::string("dog"),
        Value::string("Cat sat on cat mat with cat"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(10),
    ]);

    assert!(matches!(
        result,
        Err(crate::emacs_core::error::Flow::Signal(sig))
            if sig.symbol_name() == "args-out-of-range"
                && sig.data == vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(6)]
    ));
}

#[test]
fn replace_regexp_preserves_case_when_fixedcase_nil() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("a"),
        Value::string("x"),
        Value::string("A a"),
    ]);
    assert_str(result.unwrap(), "X x");
}

#[test]
fn replace_regexp_fixedcase_disables_case_preserve() {
    crate::test_utils::init_test_tracing();
    let result = call_replace_regexp_in_string(vec![
        Value::string("a"),
        Value::string("x"),
        Value::string("A a"),
        Value::T, // fixedcase
    ]);
    assert_str(result.unwrap(), "x x");
}

#[test]
fn string_match_wrong_type() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![Value::fixnum(42), Value::string("hello")]);
    assert!(result.is_err());
}

#[test]
fn string_match_too_few_args() {
    crate::test_utils::init_test_tracing();
    let result = call_string_match(vec![Value::string("foo")]);
    assert!(result.is_err());
}

#[test]
fn regexp_quote_parens_not_escaped() {
    crate::test_utils::init_test_tracing();
    // In Emacs regex, literal ( ) are NOT special, so regexp-quote
    // should NOT escape them.
    let result = builtin_regexp_quote(vec![Value::string("(foo)")]);
    assert_str(result.unwrap(), "(foo)");
}

#[test]
fn regexp_quote_right_bracket_not_escaped() {
    crate::test_utils::init_test_tracing();
    let result = builtin_regexp_quote(vec![Value::string("]")]);
    assert_str(result.unwrap(), "]");
}

#[test]
fn regexp_quote_preserves_raw_unibyte_bytes_compact_case() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'[', b'.',
    ]));
    let result = builtin_regexp_quote(vec![raw]).unwrap();
    assert_lisp_string_bytes(result, &[b'a', 0xFF, b'\\', b'[', b'\\', b'.'], false);
}

#[test]
fn string_match_emacs_groups() {
    crate::test_utils::init_test_tracing();
    // Emacs regex with groups: \(foo\|bar\) matching "test bar"
    let result = call_string_match(vec![
        Value::string("\\(foo\\|bar\\)"),
        Value::string("test bar"),
    ]);
    assert_int(result.unwrap(), 5);
}

#[test]
fn replace_regexp_preserves_raw_unibyte_source() {
    crate::test_utils::init_test_tracing();
    let source = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'b',
    ]));
    let result =
        call_replace_regexp_in_string(vec![Value::string("a"), Value::string("x"), source])
            .unwrap();
    assert_lisp_string_bytes(result, &[b'x', 0xFF, b'b'], false);
}

#[test]
fn replace_regexp_preserves_raw_unibyte_replacement() {
    crate::test_utils::init_test_tracing();
    let replacement = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFE]));
    let source = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'b',
    ]));
    let result =
        call_replace_regexp_in_string(vec![Value::string("a"), replacement, source]).unwrap();
    assert_lisp_string_bytes(result, &[0xFE, 0xFF, b'b'], false);
}

#[test]
fn replace_regexp_function_can_return_raw_unibyte_string() {
    crate::test_utils::init_test_tracing();
    let source = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'b',
    ]));
    let replacement = Value::list(vec![
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("_")]),
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFE])),
    ]);
    let result =
        call_replace_regexp_in_string(vec![Value::string("a"), replacement, source]).unwrap();
    assert_lisp_string_bytes(result, &[0xFE, 0xFF, b'b'], false);
}

fn bootstrap_eval_one(src: &str) -> String {
    crate::test_utils::runtime_startup_eval_all(src)
        .into_iter()
        .next()
        .expect("at least one form")
}

/// GNU's `Freplace_match' (search.c:2396) chooses between buffer and string
/// from its STRING argument alone and never consults `last_thing_searched'.
/// So a match installed by `set-match-data' from plain integers still replaces
/// in the current buffer, even though both editors record such match data as
/// string-sourced because no element names a buffer (search.c:2966).
///
/// shift-number, which evil-numbers drives, reaches `replace-match' exactly
/// this way.
#[test]
fn replace_match_uses_the_current_buffer_whatever_the_match_data_records() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(list
             (with-temp-buffer
               (insert "workers = 0099\n")
               (goto-char 11)
               (set-match-data (list 11 15 11 11 11 15))
               (replace-match "0102" t t nil 2)
               (buffer-string))
             (with-temp-buffer
               (insert "workers = 0099\n")
               (set-match-data (list 11 15))
               (replace-match "X" t t nil 0)
               (buffer-string))
             (with-temp-buffer
               (insert "workers = 0099\n")
               (set-match-data (list (copy-marker 11) (copy-marker 15)))
               (replace-match "X" t t nil 0)
               (buffer-string)))"#,
    );
    assert_eq!(
        result,
        "OK (\"workers = 0102\n\" \"workers = X\n\" \"workers = X\n\")"
    );
}

/// The subexpression is checked against the *accessible* portion of the
/// buffer, and both endpoints are reported (search.c:2418-2427).  A group that
/// did not participate is a different error entirely.
#[test]
fn replace_match_reports_a_subexpression_outside_the_accessible_portion() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(mapcar
             (lambda (thunk) (condition-case e (funcall thunk) (error e)))
             (list
              (lambda () (with-temp-buffer
                           (insert "abc")
                           (set-match-data (list 2 99))
                           (replace-match "X" t t nil 0)))
              (lambda () (with-temp-buffer
                           (insert "abcdef")
                           (narrow-to-region 3 6)
                           (set-match-data (list 1 2))
                           (replace-match "X" t t nil 0)))
              (lambda () (with-temp-buffer
                           (insert "abc")
                           (set-match-data (list 1 3 nil nil))
                           (replace-match "X" t t nil 1)))))"#,
    );
    assert_eq!(
        result,
        r#"OK ((args-out-of-range 2 99) (args-out-of-range 1 2) (error "replace-match subexpression does not exist" 1))"#
    );
}

/// GNU adjusts the match registers for the edit it just made
/// (`update_search_regs', search.c) whatever the match data records as its
/// origin.  Leaving them stale makes a caller that replaces in a loop -- which
/// is how shift-number drives evil-numbers -- read the same registers forever
/// and never terminate.
#[test]
fn replace_match_adjusts_the_registers_for_the_edit_it_made() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(list
             (with-temp-buffer
               (insert "aa 01 bb 02 cc")
               (set-match-data (list 4 6))
               (replace-match "999" t t nil 0)
               (list (point) (match-beginning 0) (match-end 0)))
             (with-temp-buffer
               (insert "aa 01 bb 02 cc")
               (goto-char (point-min))
               (re-search-forward "[0-9]+")
               (replace-match "999" t t nil 0)
               (list (point) (match-beginning 0) (match-end 0))))"#,
    );
    assert_eq!(result, "OK ((7 4 7) (7 4 7))");
}
