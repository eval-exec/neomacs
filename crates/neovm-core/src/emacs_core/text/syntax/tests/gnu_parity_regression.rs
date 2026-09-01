//! Regression tests pinning behavior through the syntax-table GNU-parity
//! refactor (`docs/superpowers/plans/2026-04-17-syntax-table-gnu-parity.md`).
//!
//! These tests drive word motion, skip-syntax, parse-partial-sexp,
//! char-syntax, and modify-syntax-entry through the builtin layer and
//! direct buffer manipulation. They pass today (against the HashMap
//! compiled form) and must continue passing after the compiled form is
//! deleted and runtime reads go directly through the chartable Value.
//!
//! The existing `syntax_test.rs` suite covers the lower-level Rust API
//! (`forward_word(&buf, &table, ...)`) whose signature *will* change in
//! T2. These regression tests target stable public entry points
//! (builtins + buffer-manager helpers) that survive the refactor.

use crate::buffer::BufferId;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::tagged::value::ValueKind;

fn ctx_with_buffer(text: &str) -> (Context, BufferId) {
    let mut ctx = Context::new();
    let id = ctx.buffer_manager_mut().create_buffer("t");
    {
        let buf = ctx.buffer_manager_mut().get_mut(id).expect("buffer");
        buf.insert(text);
        buf.widen();
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let _ = ctx.switch_current_buffer(id);
    (ctx, id)
}

fn call(ctx: &mut Context, name: &str, args: Vec<Value>) -> Value {
    ctx.funcall_general(Value::symbol(name), args)
        .expect("funcall")
}

fn fixnum(n: i64) -> Value {
    Value::fixnum(n)
}

fn as_int(v: Value) -> i64 {
    match v.kind() {
        ValueKind::Fixnum(n) => n,
        other => panic!("expected fixnum, got {:?}", other),
    }
}

fn point(ctx: &mut Context) -> i64 {
    as_int(call(ctx, "point", vec![]))
}

fn goto(ctx: &mut Context, pos: i64) {
    call(ctx, "goto-char", vec![fixnum(pos)]);
}

#[test]
fn forward_sexp_syntax_propertize_reads_buffer_local_lookup_flag() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (progn
          (setq-local parse-sexp-lookup-properties t)
          (setq-local syntax-propertize-function (lambda (_start _end)))
          (insert "(foo bar|baz quux)")
          (put-text-property 9 10 'syntax-table '(1))
          (goto-char 2)
          (forward-sexp 1)
          (list parse-sexp-lookup-properties
                (fboundp 'internal--syntax-propertize)
                syntax-propertize--done
                (text-properties-at 9)
                (buffer-string)))
        "#,
    );
    assert_eq!(result, "OK (t t 19 nil \"(foo bar|baz quux)\")");
}

#[test]
fn forward_comment_prepares_and_reads_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules
                       ("\\(A\\)" (1 "<"))
                       ("\\(Z\\)" (1 ">"))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "A body Z after")
          (goto-char (point-min))
          (list (forward-comment 1)
                (point)
                syntax-propertize--done
                (get-text-property 1 'syntax-table)
                (get-text-property 8 'syntax-table)))
        "#,
    );
    assert_eq!(result, "OK (t 9 15 (11) (12))");
}

#[test]
fn backward_comment_prepares_and_reads_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules
                       ("\\(A\\)" (1 "<"))
                       ("\\(Z\\)" (1 ">"))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "before A body Z")
          (goto-char (point-max))
          (list (forward-comment -1)
                (point)
                syntax-propertize--done
                (get-text-property 8 'syntax-table)
                (get-text-property 15 'syntax-table)))
        "#,
    );
    assert_eq!(result, "OK (t 8 16 (11) (12))");
}

#[test]
fn forward_comment_snapshots_escape_policy_after_syntax_propertize() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r##"
        (with-temp-buffer
          (set-syntax-table (make-syntax-table))
          (modify-syntax-entry ?# "<")
          (modify-syntax-entry ?\n ">")
          (insert (concat "# x " (string ?\\ ?\n)))
          (setq-local parse-sexp-lookup-properties t)
          (setq-local syntax-propertize-function
                      (lambda (_start _end)
                        (setq-local comment-end-can-be-escaped t)))
          (goto-char (point-max))
          (list (forward-comment -1)
                (point)
                comment-end-can-be-escaped))
        "##,
    );
    assert_eq!(result, "OK (nil 6 t)");
}

#[test]
fn forward_comment_delegates_before_validating_count() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((forward-comment-function
               (lambda (count) (list :delegated count))))
          (forward-comment 'not-an-integer))
        "#,
    );
    assert_eq!(result, "OK (:delegated not-an-integer)");
}

#[test]
fn regexp_syntax_class_search_prepares_syntax_properties_before_matching() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules ("x" (0 (ignore)))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "a b\n")
          (goto-char (point-min))
          (list (re-search-forward "\\s-" nil t)
                syntax-propertize--done))
        "#,
    );
    assert_eq!(result, "OK (3 5)");
}

#[test]
fn regexp_syntax_class_search_reads_syntax_table_text_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local parse-sexp-lookup-properties t)
          (insert "x")
          (put-text-property 1 2 'syntax-table (string-to-syntax " "))
          (goto-char (point-min))
          (list (re-search-forward "\\s-" nil t)
                (point)))
        "#,
    );
    assert_eq!(result, "OK (2 2)");
}

#[test]
fn regexp_syntax_class_search_ignores_properties_when_lookup_is_disabled() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local parse-sexp-lookup-properties nil)
          (insert "x")
          (put-text-property 1 2 'syntax-table (string-to-syntax " "))
          (goto-char (point-min))
          (list (re-search-forward "\\s-" nil t)
                (point)))
        "#,
    );
    assert_eq!(result, "OK (nil 1)");
}

#[test]
fn regexp_word_boundaries_separate_word_constituents_from_different_scripts() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (list (string-match "\\<my" "変数名myVariable")
              (string-match "名\\>" "変数名myVariable")
              (string-match "\\bmy" "変数名myVariable")
              (string-match "\\Bmy" "変数名myVariable")
              (string-match "\\<字" "漢字")
              (string-match "\\<my" "abcmy"))
        "#,
    );
    assert_eq!(result, "OK (3 2 3 nil nil nil)");
}

#[test]
fn buffer_regexp_word_boundaries_separate_word_constituents_from_different_scripts() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (insert "変数名myVariable")
          (goto-char (point-min))
          (list (re-search-forward "\\<my" nil t)
                (progn
                  (goto-char (point-min))
                  (re-search-forward "名\\>" nil t))
                (progn
                  (goto-char 4)
                  (looking-at "\\bmy"))))
        "#,
    );
    assert_eq!(result, "OK (6 4 t)");
}

#[test]
fn regexp_word_boundaries_honor_script_and_category_overrides() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (list
         (string-match "\\<カ" "あカ")
         (let ((word-separating-categories nil))
           (string-match "\\<カ" "あカ"))
         (let ((word-combining-categories
                (cons (cons ?C ?a) word-combining-categories)))
           (string-match "\\<my" "名my"))
         (let ((char-script-table (copy-sequence char-script-table)))
           (set-char-table-range char-script-table ?m 'han)
           (string-match "\\<my" "名my")))
        "#,
    );
    assert_eq!(result, "OK (1 nil nil nil)");
}

#[test]
fn syntax_table_property_positions_survive_insertion() {
    // Regression pin (GNU-verified): the property stays glued to its shifted
    // text after an insertion before it; the parse state is byte-identical
    // to GNU's (34 = inside a `"`-delimited string at end of buffer).
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (insert "aa" (string 34) "x" (string 34) "bb")
          (put-text-property 3 4 'syntax-table '(1))
          (setq-local parse-sexp-lookup-properties t)
          (parse-partial-sexp (point-min) (point-max))
          (goto-char (point-min))
          (insert "ZZZZ")
          (nth 3 (parse-partial-sexp (point-min) (point-max))))
        "#,
    );
    assert_eq!(result, "OK 34");
}

#[test]
fn syntax_table_property_positions_survive_deletion() {
    // Regression (found by the interactive sim, DIVERGED from GNU before the
    // fix): the sparse syntax-prop range cache held char positions, and a
    // deletion shifted the property LEFT without touching the cache — the
    // override was applied at its stale position and the parse state came
    // out nil where GNU reports 34.
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (insert "ZZZZaa" (string 34) "x" (string 34) "bb")
          (put-text-property 7 8 'syntax-table '(1))
          (setq-local parse-sexp-lookup-properties t)
          (parse-partial-sexp (point-min) (point-max))
          (delete-region 1 5)
          (nth 3 (parse-partial-sexp (point-min) (point-max))))
        "#,
    );
    assert_eq!(result, "OK 34");
}

#[test]
fn syntax_independent_regexp_does_not_trigger_propertization() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules ("x" (0 (ignore)))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "abc")
          (goto-char (point-min))
          (list (re-search-forward "a" nil t)
                syntax-propertize--done))
        "#,
    );
    assert_eq!(result, "OK (2 -1)");
}

#[test]
fn looking_at_prepares_and_reads_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules ("x" (0 " "))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "x")
          (goto-char (point-min))
          (list (looking-at "\\s-")
                syntax-propertize--done))
        "#,
    );
    assert_eq!(result, "OK (t 2)");
}

#[test]
fn regexp_syntax_preparation_preserves_later_string_match_data() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules ("x" (0 (ignore)))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "a b c d e f g h i j k l m n o p q r s t u v w x y z\n")
          (goto-char (point-min))
          (re-search-forward "\\s-" nil t)
          (let ((subject "'zeta'"))
            (string-match "'[^']+'" subject 0)
            (syntax-ppss)
            (match-string 0 subject)))
        "#,
    );
    assert_eq!(result, "OK \"'zeta'\"");
}

// --- char-syntax --------------------------------------------------------

#[test]
fn char_syntax_ascii_word() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("");
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum('a' as i64)])),
        'w' as i64
    );
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum('Z' as i64)])),
        'w' as i64
    );
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum('5' as i64)])),
        'w' as i64
    );
}

#[test]
fn char_syntax_ascii_whitespace() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("");
    // GNU's canonical whitespace syntax-class designator is SPACE (32).
    // Both SPACE and '-' (45) parse to SyntaxClass::Whitespace via
    // `string-to-syntax`, but `char-syntax` returns the first form.
    let space = ' ' as i64;
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum(' ' as i64)])),
        space
    );
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum('\t' as i64)])),
        space
    );
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum('\n' as i64)])),
        space
    );
}

#[test]
fn char_syntax_cjk_is_word() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("");
    // U+4E2D 中 — GNU's standard-syntax-table ranges 0x80..=0x3FFFFF as Word.
    assert_eq!(
        as_int(call(&mut ctx, "char-syntax", vec![fixnum(0x4e2d)])),
        'w' as i64
    );
}

// --- forward-word / backward-word ---------------------------------------

#[test]
fn forward_word_crosses_whitespace() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("hello world foo");
    goto(&mut ctx, 1);
    call(&mut ctx, "forward-word", vec![fixnum(1)]);
    assert_eq!(point(&mut ctx), 6, "after 'hello', point is 6");
}

#[test]
fn forward_word_count_two_skips_second_word() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("aa bb cc");
    goto(&mut ctx, 1);
    call(&mut ctx, "forward-word", vec![fixnum(2)]);
    assert_eq!(point(&mut ctx), 6, "after 'aa bb', point is 6");
}

#[test]
fn forward_word_drops_syntax_property_cache_before_boundary_callback() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (insert "aa bb cc")
          (setq-local parse-sexp-lookup-properties t)
          (setq-local syntax-propertize--done (point-max))
          (goto-char (point-min))
          (let ((table (make-char-table nil)))
            (setq syntax-test--boundary-calls 0)
            (fset 'syntax-test--boundary
                  (lambda (pos limit)
                    (setq syntax-test--boundary-calls
                          (1+ syntax-test--boundary-calls))
                    (when (= syntax-test--boundary-calls 1)
                      ;; The next word becomes punctuation while the boundary
                      ;; callback runs.  A scan cache retained across this Lisp
                      ;; call would incorrectly continue to see the old nil run.
                      (put-text-property 4 6 'syntax-table '(1)))
                    (min (+ pos 2) limit)))
            (set-char-table-range table t 'syntax-test--boundary)
            (setq-local find-word-boundary-function-table table)
            (forward-word 2)
            (list (point) syntax-test--boundary-calls)))
        "#,
    );

    assert_eq!(result, "OK (9 2)");
}

#[test]
fn forward_word_negative_goes_backward() {
    crate::test_utils::init_test_tracing();
    // `backward-word` is defined in lisp/simple.el; the underlying primitive
    // is `forward-word` with a negative count. Exercise that directly.
    let (mut ctx, _) = ctx_with_buffer("hello world");
    goto(&mut ctx, 12); // point-max
    call(&mut ctx, "forward-word", vec![fixnum(-1)]);
    assert_eq!(
        point(&mut ctx),
        7,
        "forward-word -1 from end lands at 'w' of 'world' = 7"
    );
}

// --- skip-syntax-forward / skip-syntax-backward ------------------------

#[test]
fn skip_syntax_forward_word_returns_count() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("abc def");
    goto(&mut ctx, 1);
    let n = as_int(call(
        &mut ctx,
        "skip-syntax-forward",
        vec![Value::string("w")],
    ));
    assert_eq!(n, 3, "skip \"w\" over 'abc' returns 3");
}

#[test]
fn skip_syntax_forward_whitespace() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("   abc");
    goto(&mut ctx, 1);
    let n = as_int(call(
        &mut ctx,
        "skip-syntax-forward",
        vec![Value::string(" ")],
    ));
    assert_eq!(n, 3);
}

#[test]
fn skip_syntax_forward_prepares_and_reads_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules
                       ("\\(X+\\)" (1 " "))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "XXXa")
          (goto-char (point-min))
          (list (skip-syntax-forward " ")
                (point)
                syntax-propertize--done
                (get-text-property 1 'syntax-table)))
        "#,
    );
    assert_eq!(result, "OK (3 4 5 (0))");
}

#[test]
fn skip_syntax_backward_prepares_and_reads_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local syntax-propertize-function
                      (syntax-propertize-rules
                       ("\\(X+\\)" (1 " "))))
          (setq-local parse-sexp-lookup-properties t)
          (insert "aXXX")
          (goto-char (point-max))
          (list (skip-syntax-backward " ")
                (point)
                syntax-propertize--done
                (get-text-property 2 'syntax-table)))
        "#,
    );
    assert_eq!(result, "OK (-3 2 5 (0))");
}

// --- modify-syntax-entry changes motion --------------------------------

#[test]
fn modify_syntax_entry_adds_underscore_to_word_class() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("foo_bar baz");
    // modify-syntax-entry ?_ "w" — upgrades underscore to word class.
    call(
        &mut ctx,
        "modify-syntax-entry",
        vec![fixnum('_' as i64), Value::string("w")],
    );
    goto(&mut ctx, 1);
    call(&mut ctx, "forward-word", vec![fixnum(1)]);
    assert_eq!(
        point(&mut ctx),
        8,
        "with _ as word, forward-word consumes 'foo_bar'"
    );
}

// --- parse-partial-sexp ------------------------------------------------

#[test]
fn parse_partial_sexp_tracks_depth() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("((a b) (c");
    let state = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(10)]);
    // "((a b) (c" — three opens ( ( (, one close ) → depth 2.
    let depth = as_int(call(&mut ctx, "car", vec![state]));
    assert_eq!(depth, 2, "three opens, one close => depth 2");
}

#[test]
fn parse_partial_sexp_string_state() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("foo \"bar");
    let state = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(9)]);
    // state's 4th elt (index 3) is non-nil if inside a string.
    let in_string = call(&mut ctx, "nth", vec![fixnum(3), state]);
    assert!(!in_string.is_nil(), "unterminated string => elt 3 non-nil");
}

#[test]
fn parse_partial_sexp_tracks_last_complete_sexp_start() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) =
        ctx_with_buffer("(defun sample ()\n(message \"alpha\")\n(when t\n(message \"beta\")))\n");

    let after_defun_signature = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(18)]);
    assert_eq!(
        as_int(call(
            &mut ctx,
            "nth",
            vec![fixnum(2), after_defun_signature]
        )),
        15,
        "last complete sexp before function body should be the empty arg list"
    );

    let after_message_form = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(36)]);
    assert_eq!(
        as_int(call(&mut ctx, "nth", vec![fixnum(2), after_message_form])),
        18,
        "last complete sexp at function body level should be the message form"
    );
}

#[test]
fn parse_partial_sexp_tracks_later_top_level_atom_after_list() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("(a) b");

    let state = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(6)]);
    assert_eq!(
        as_int(call(&mut ctx, "nth", vec![fixnum(2), state])),
        5,
        "last complete sexp at top level should advance from the list to the later atom"
    );
}

#[test]
fn parse_partial_sexp_stopbefore_stops_at_sexp_start() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("(a) x");

    let state = call(
        &mut ctx,
        "parse-partial-sexp",
        vec![fixnum(1), fixnum(6), Value::NIL, Value::T],
    );

    assert_eq!(point(&mut ctx), 1, "STOPBEFORE leaves point at sexp start");
    assert!(
        call(&mut ctx, "nth", vec![fixnum(2), state]).is_nil(),
        "no sexp has been consumed when stopping before the opening paren"
    );
}

#[test]
fn parse_partial_sexp_targetdepth_stops_after_depth_reached() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("(a) x");

    let state = call(
        &mut ctx,
        "parse-partial-sexp",
        vec![fixnum(1), fixnum(6), fixnum(0), Value::NIL],
    );

    assert_eq!(point(&mut ctx), 4, "TARGETDEPTH stops after closing paren");
    assert_eq!(as_int(call(&mut ctx, "car", vec![state])), 0);
}

#[test]
fn parse_partial_sexp_oldstate_matches_full_scan_from_nonzero_position() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("λ (alpha \"β\") (gamma)");

    let point_max = call(&mut ctx, "point-max", vec![]);
    let full = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), point_max]);
    let first = call(&mut ctx, "parse-partial-sexp", vec![fixnum(1), fixnum(14)]);
    let segmented = call(
        &mut ctx,
        "parse-partial-sexp",
        vec![fixnum(14), point_max, Value::NIL, Value::NIL, first],
    );

    assert!(
        call(&mut ctx, "equal", vec![full, segmented]).is_truthy(),
        "OLDSTATE scan from a nonzero position should match a full scan"
    );
}

#[test]
fn parse_partial_sexp_uses_absolute_positions_under_narrowing() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = ctx_with_buffer("xx (α) yy");
    call(&mut ctx, "narrow-to-region", vec![fixnum(4), fixnum(7)]);

    let state = call(&mut ctx, "parse-partial-sexp", vec![fixnum(4), fixnum(7)]);

    assert_eq!(point(&mut ctx), 7, "scan should stop at absolute TO");
    assert_eq!(as_int(call(&mut ctx, "car", vec![state])), 0);
    assert_eq!(
        as_int(call(&mut ctx, "nth", vec![fixnum(2), state])),
        4,
        "last complete sexp should keep its absolute buffer position"
    );
}

#[test]
fn parse_partial_sexp_honors_syntax_table_string_fence_property() {
    crate::test_utils::init_test_tracing();
    // A `syntax-table' text property of generic-string-fence syntax (class 15)
    // must put parse-partial-sexp inside a string between the two fences, and
    // back outside once past the closing fence.  This drives the per-char
    // syntax-table property read -- and the SyntaxPropRange run cache that
    // serves it -- through a NON-nil value, which the common all-nil run never
    // exercises.  `syntax-propertize--done' is pinned past point-max so the
    // lazy propertizer does not clear the manually applied properties.
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (progn
          (setq-local parse-sexp-lookup-properties t)
          (insert "a|b|c")
          (put-text-property 2 3 'syntax-table '(15))
          (put-text-property 4 5 'syntax-table '(15))
          (setq-local syntax-propertize--done (point-max))
          (list (and (nth 3 (parse-partial-sexp 1 3)) t)
                (and (nth 3 (parse-partial-sexp 1 5)) t)))
        "#,
    );
    assert_eq!(result, "OK (t nil)");
}

#[test]
fn c_mode_font_lock_ends_leading_comments_before_preprocessor_directives() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r##"
        (with-temp-buffer
          (insert "// SPDX-License-Identifier: GPL-2.0-only\n")
          (insert "/*\n * Header comment.\n */\n\n")
          (insert "#define pr_fmt(fmt) KBUILD_MODNAME \": \" fmt\n")
          (insert "#include <linux/kernel.h>\n")
          (c-mode)
          (font-lock-mode 1)
          (font-lock-ensure)
          (goto-char (point-min))
          (search-forward "#define")
          (let ((define-face
                 (get-char-property (match-beginning 0) 'face)))
            (search-forward "#include")
            (list define-face
                  (get-char-property (match-beginning 0) 'face))))
        "##,
    );
    assert_eq!(
        result,
        "OK (font-lock-preprocessor-face font-lock-preprocessor-face)"
    );
}

/// Ledger entry 53: a string object's own `syntax-table` properties reach the
/// regexp matcher, exactly as they do for a buffer.
///
/// GNU sets up syntax state for the searched object, not only for buffers
/// (`RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`, src/syntax.c:277), so `string-match`
/// over a propertized string resolves each character's syntax through the same
/// `textget` fallbacks. Every expectation below is the value GNU Emacs 31.0.90
/// prints for the same form.
#[test]
fn string_match_reads_the_strings_own_syntax_table_property() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t)
              (s (concat "a" (propertize "<" 'syntax-table '(4 . ?>)) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn string_match_resolves_a_strings_syntax_through_a_category_symbol() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (progn
          (put 'p13-open 'syntax-table '(4 . ?>))
          (let ((parse-sexp-lookup-properties t)
                (s (concat "a" (propertize "<" 'category 'p13-open) "b")))
            (string-match "\\s(" s)))
        "#,
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn string_match_resolves_a_strings_syntax_through_char_property_alias_alist() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let* ((parse-sexp-lookup-properties t)
               (char-property-alias-alist '((syntax-table p13-syn)))
               (s (concat "a" (propertize "<" 'p13-syn '(4 . ?>)) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn a_strings_syntax_table_property_can_also_take_syntax_away() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t)
              (s (concat "a" (propertize "(" 'syntax-table '(1)) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK nil");
}

#[test]
fn string_match_ignores_string_properties_when_lookup_is_disabled() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties nil)
              (s (concat "a" (propertize "<" 'syntax-table '(4 . ?>)) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK nil");
}

/// The gate is the CURRENT BUFFER's `parse-sexp-lookup-properties`, whatever
/// buffer the string came from: GNU reads the C variable that mirrors the
/// buffer-local binding at the `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` site.
#[test]
fn the_current_buffers_lookup_flag_governs_a_string_match() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((s (concat "a" (propertize "<" 'syntax-table '(4 . ?>)) "b")))
          (list (with-temp-buffer
                  (setq-local parse-sexp-lookup-properties t)
                  (string-match "\\s(" s))
                (with-temp-buffer
                  (setq-local parse-sexp-lookup-properties nil)
                  (string-match "\\s(" s))))
        "#,
    );
    assert_eq!(result, "OK (1 nil)");
}

#[test]
fn a_propertyless_string_still_matches_by_the_syntax_table_alone() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t))
          (list (string-match "\\s(" "a(b")
                (string-match "\\s(" "abc")
                ;; A string with no intervals at all gets no property, not even
                ;; the `default-text-properties' fallback -- GNU's
                ;; `update_syntax_table' returns early when `interval_of' finds
                ;; no interval.
                (let ((default-text-properties '(syntax-table (4 . ?>))))
                  (string-match "\\s(" "a<b"))))
        "#,
    );
    assert_eq!(result, "OK (1 nil nil)");
}

/// The matcher addresses the string by BYTE, the intervals by CHARACTER, so a
/// multibyte character before the propertized one is where a missing
/// conversion would show up.
#[test]
fn string_match_maps_matcher_bytes_to_interval_characters() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t)
              (s (concat "é" (propertize "<" 'syntax-table '(4 . ?>)) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn string_match_p_and_posix_string_match_read_string_syntax_properties_too() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t)
              (s (concat "a" (propertize "<" 'syntax-table '(4 . ?>)) "b")))
          (list (string-match-p "\\s(" s)
                (posix-string-match "\\s(" s)))
        "#,
    );
    assert_eq!(result, "OK (1 1)");
}

/// Buffer matching keeps reading the BUFFER's properties -- the string object
/// arriving at the same seam must not redirect it.
#[test]
fn buffer_regexp_matching_is_unaffected_by_the_string_object_path() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (setq-local parse-sexp-lookup-properties t)
          (insert "a<b")
          (put-text-property 2 3 'syntax-table '(4 . ?>))
          (setq-local syntax-propertize--done (point-max))
          (goto-char (point-min))
          (list (re-search-forward "\\s(" nil t)
                (string-match "\\s(" "a<b")))
        "#,
    );
    assert_eq!(result, "OK (3 nil)");
}

/// Once ANY text property exists anywhere in an object, GNU's interval tree
/// partitions the WHOLE object, so `textget`'s `default-text-properties`
/// fallback reaches every position in it -- not only the positions the property
/// was actually put on. Both halves of that rule are pinned here, for a buffer
/// and for a string:
///
/// * a position whose own interval carries an unrelated property, one before it,
///   one after it, and one nowhere near it all take syntax from the fallback;
/// * an object with NO property anywhere takes nothing, because
///   `update_syntax_table` returns early when `interval_of` finds no interval.
///
/// Every expectation below was read off GNU 31.0.90. The second half is the
/// boundary entry 44 pins and this must never cross: coverage of an existing
/// partition, never synthesis of intervals for propertyless text.
#[test]
fn an_existing_partition_carries_default_text_properties_to_every_position() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((probe (lambda (put)
                       (with-temp-buffer
                         (setq-local parse-sexp-lookup-properties t)
                         (insert "a<b")
                         (funcall put)
                         (let ((default-text-properties '(syntax-table (4 . ?>))))
                           (goto-char (point-min))
                           (re-search-forward "\\s(" nil t))))))
          (list
           ;; Buffer: the interval on the character itself, on its neighbours,
           ;; and one far away all reach the fallback; none at all does not.
           (funcall probe (lambda () (put-text-property 2 3 'p15-unrelated 'x)))
           (funcall probe (lambda () (put-text-property 1 2 'p15-unrelated 'x)))
           (funcall probe (lambda () (put-text-property 3 4 'p15-unrelated 'x)))
           (funcall probe (lambda () (insert "cccc")
                            (put-text-property 5 6 'p15-unrelated 'x)))
           (funcall probe (lambda () nil))
           ;; String: the same rule, same boundary.
           (let ((parse-sexp-lookup-properties t)
                 (default-text-properties '(syntax-table (4 . ?>))))
             (list (string-match "\\s(" (concat "a" (propertize "<" 'p15-unrelated 'x) "b"))
                   (string-match "\\s(" (concat (propertize "a" 'p15-unrelated 'x) "<b"))
                   (string-match "\\s(" (concat "a<" (propertize "b" 'p15-unrelated 'x)))
                   (string-match "\\s(" "a<b")))))
        "#,
    );
    assert_eq!(result, "OK (2 2 2 2 nil (0 0 0 nil))");
}

/// GNU consumes a whole nested comment inside one `forw_comment` call
/// (`scan_sexps_forward`, src/syntax.c:3352), so a `'syntax-table` commentstop
/// parse stops exactly twice per comment -- once at its start, once at its real
/// end -- and never at the ender that only pops a nesting level.  This is the
/// loop `font-lock-fontify-syntactically-region` runs, so stopping early cut a
/// Scala block comment's `font-lock-comment-face` run at the inner `*/`.
///
/// The syntax table is scala-mode's: `?/` is ". 124b" and `?*` is ". 23n"
/// (scala-mode-syntax.el:498-499), the `n` making block comments nestable.
#[test]
fn commentstop_parse_ignores_an_ender_that_only_pops_a_nesting_level() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (let ((table (make-syntax-table)))
            (modify-syntax-entry ?\/ ". 124b" table)
            (modify-syntax-entry ?\* ". 23n" table)
            (modify-syntax-entry ?\n "> b" table)
            (set-syntax-table table))
          (insert "  /* outer /* nested */ done */\nafter\n")
          (goto-char (point-min))
          (let ((state nil) (steps nil) (end (point-max)))
            (while (< (point) end)
              (setq state (parse-partial-sexp (point) end nil nil state 'syntax-table))
              (push (list (point) (nth 4 state) (nth 8 state)) steps)
              (when (> (length steps) 12) (goto-char end)))
            (nreverse steps)))
        "#,
    );
    assert_eq!(result, "OK ((5 1 3) (32 nil nil) (39 nil nil))");
}

/// `default-text-properties` reaches a position that HAS an interval, even one
/// whose plist says nothing about syntax -- the same `textget` fallback the
/// buffer scanner applies. Contrast the propertyless string above, where there
/// is no interval and so no fallback either.
#[test]
fn a_strings_interval_takes_syntax_from_default_text_properties() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((parse-sexp-lookup-properties t)
              (default-text-properties '(syntax-table (4 . ?>)))
              (s (concat "a" (propertize "<" 'p13-unrelated 'x) "b")))
          (string-match "\\s(" s))
        "#,
    );
    assert_eq!(result, "OK 0");
}

/// The upstream report's headline case: backward sexp motion in `emacs-lisp-mode`
/// over a form whose string contains a `;`.  The closing paren sits on a later
/// line, so the backward scan crosses the newline that ends line 2, and
/// `back_comment` has to decide whether that `;` starts a comment.  It does not
/// -- it is inside a string -- and `backward-list` must reach the `(` at 2.
#[test]
fn backward_list_crosses_an_elisp_string_holding_a_comment_char() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (with-temp-buffer
          (emacs-lisp-mode)
          (insert " (progn\n  (setq a \";\")\n  nil)\n")
          (goto-char (point-max))
          (skip-chars-backward "\n")
          (backward-list)
          (point))
        "#,
    );
    assert_eq!(result, "OK 2");
}
