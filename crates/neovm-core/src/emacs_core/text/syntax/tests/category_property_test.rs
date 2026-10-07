//! GNU parity for the syntax scanner's `syntax-table` property lookup.
//!
//! GNU's scanner does not read the `syntax-table` text property directly: it
//! calls `textget` (`src/syntax.c` `update_syntax_table` ->
//! `src/intervals.c` `lookup_char_property`), the same resolver
//! `get-char-property` uses for text properties. That resolver follows a
//! `category` symbol's plist, `char-property-alias-alist`, and
//! `default-text-properties`. Reading the property raw makes the scanner
//! disagree with `syntax-after` on the same character -- the invariant these
//! tests pin. Every expectation here was measured on GNU Emacs 31.0.90.
//!
//! Two boundaries GNU draws that these tests also pin:
//!   - overlays never reach the scanner, only text properties do
//!     (`get-char-property` sees them; `forward-sexp` must not);
//!   - the fallbacks apply only where an interval exists, because
//!     `update_syntax_table` returns early when `interval_of` finds none --
//!     so `default-text-properties` reaches a character with some other
//!     property on it and not one with a bare plist.

fn eval(src: &str) -> String {
    crate::test_utils::init_test_tracing();
    crate::test_utils::runtime_startup_eval_one(src)
}

/// Preamble defining the CC Mode `c-use-category` shape: a symbol carrying the
/// paren syntax on its own plist, applied to text through `category`.
const CATEGORY_SYMBOLS: &str = r#"
  (put 'p12-open 'syntax-table '(4 . ?>))
  (put 'p12-close 'syntax-table '(5 . ?<))
"#;

#[test]
fn forward_sexp_resolves_syntax_through_a_category_property() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "<()>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 4 5 'category 'p12-close)
              (goto-char (point-min))
              (forward-sexp)
              (point))))
        "#
    ));
    assert_eq!(result, "OK 5");
}

#[test]
fn the_scanner_and_syntax_after_agree_on_a_category_supplied_syntax() {
    // The invariant whose violation is the bug: `syntax-after` resolves the
    // category through `get-char-property`, so a scanner reading the property
    // raw contradicts it about the very same character.
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "<()>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 4 5 'category 'p12-close)
              (list (syntax-after 1)
                    (progn (goto-char 1) (forward-sexp) (point))
                    (nth 0 (parse-partial-sexp 1 3))))))
        "#
    ));
    assert_eq!(result, "OK ((4 . 62) 5 2)");
}

#[test]
fn regexp_syntax_classes_resolve_a_category_property() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "a<b")
              (put-text-property 2 3 'category 'p12-open)
              (goto-char (point-min))
              (re-search-forward "\\s(" nil t))))
        "#
    ));
    assert_eq!(result, "OK 3");
}

#[test]
fn a_direct_syntax_table_property_still_wins_over_the_category() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "<()>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 1 2 'syntax-table '(1))
              (goto-char 1)
              (forward-sexp)
              (point))))
        "#
    ));
    assert_eq!(result, "OK 4");
}

#[test]
fn a_category_symbol_without_the_property_leaves_the_table_syntax() {
    let result = eval(
        r#"
        (with-temp-buffer
          (let ((parse-sexp-lookup-properties t))
            (set-syntax-table (make-syntax-table))
            (insert "<()>")
            (put-text-property 1 2 'category 'p12-no-such-prop)
            (goto-char 1)
            (forward-sexp)
            (point)))
        "#,
    );
    assert_eq!(result, "OK 2");
}

#[test]
fn char_property_alias_alist_reaches_the_scanner() {
    let result = eval(
        r#"
        (with-temp-buffer
          (let ((parse-sexp-lookup-properties t)
                (char-property-alias-alist '((syntax-table p12-alias))))
            (set-syntax-table (make-syntax-table))
            (insert "<()>")
            (put-text-property 1 2 'p12-alias '(4 . ?>))
            (put-text-property 4 5 'p12-alias '(5 . ?<))
            (goto-char 1)
            (list (syntax-after 1)
                  (progn (forward-sexp) (point)))))
        "#,
    );
    assert_eq!(result, "OK ((4 . 62) 5)");
}

#[test]
fn default_text_properties_reach_the_scanner_only_inside_an_interval() {
    // GNU `update_syntax_table` returns early when no interval covers the
    // position, so the `default-text-properties` fallback never runs there.
    let with_interval = eval(
        r#"
        (with-temp-buffer
          (let ((parse-sexp-lookup-properties t)
                (default-text-properties '(syntax-table (1))))
            (set-syntax-table (make-syntax-table))
            (insert "(a)b")
            (put-text-property 1 2 'face 'default)
            (list (syntax-after 1)
                  (progn (goto-char 1) (forward-sexp) (point)))))
        "#,
    );
    assert_eq!(with_interval, "OK ((1) 5)");

    let without_interval = eval(
        r#"
        (with-temp-buffer
          (let ((parse-sexp-lookup-properties t)
                (default-text-properties '(syntax-table (1))))
            (set-syntax-table (make-syntax-table))
            (insert "(a)b")
            (list (syntax-after 1)
                  (progn (goto-char 1) (forward-sexp) (point)))))
        "#,
    );
    assert_eq!(without_interval, "OK ((1) 4)");
}

#[test]
fn overlay_syntax_table_properties_do_not_reach_the_scanner() {
    // `get-char-property` and `syntax-after` see the overlay; the scanner reads
    // text-property intervals only, so `forward-sexp` must ignore it.
    let result = eval(
        r#"
        (with-temp-buffer
          (let ((parse-sexp-lookup-properties t))
            (set-syntax-table (make-syntax-table))
            (insert "<()>")
            (let ((o (make-overlay 1 2)))
              (overlay-put o 'syntax-table '(4 . ?>)))
            (goto-char 1)
            (list (get-char-property 1 'syntax-table)
                  (syntax-after 1)
                  (progn (forward-sexp) (point)))))
        "#,
    );
    assert_eq!(result, "OK ((4 . 62) (4 . 62) 2)");
}

#[test]
fn category_resolution_survives_the_run_cache_boundaries() {
    // Plain text on both sides of the category runs, so the scan crosses three
    // intervals and the per-scan property-run cache has to refill at each edge.
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "aaaa<()>bbbb")
              (put-text-property 5 6 'category 'p12-open)
              (put-text-property 8 9 'category 'p12-close)
              (goto-char 5)
              (forward-sexp)
              (point))))
        "#
    ));
    assert_eq!(result, "OK 9");
}

#[test]
fn scan_lists_skip_syntax_and_backward_motion_resolve_a_category() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "<()>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 4 5 'category 'p12-close)
              (list (scan-lists 1 1 0)
                    (progn (goto-char 1) (skip-syntax-forward "(") (point))
                    (progn (goto-char (point-max)) (backward-sexp) (point))))))
        "#
    ));
    assert_eq!(result, "OK (5 3 1)");
}

#[test]
fn forward_comment_resolves_a_category_supplied_comment_syntax() {
    let result = eval(
        r#"
        (progn
          (put 'p12-cstart 'syntax-table '(11))
          (put 'p12-cend 'syntax-table '(12))
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "A body Z after")
              (put-text-property 1 2 'category 'p12-cstart)
              (put-text-property 8 9 'category 'p12-cend)
              (goto-char (point-min))
              (list (forward-comment 1) (point)))))
        "#,
    );
    assert_eq!(result, "OK (t 9)");
}

#[test]
fn word_motion_resolves_a_category_supplied_word_syntax() {
    let result = eval(
        r#"
        (progn
          (put 'p12-word 'syntax-table '(2))
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "ab-cd")
              (put-text-property 3 4 'category 'p12-word)
              (goto-char 1)
              (forward-word)
              (point))))
        "#,
    );
    assert_eq!(result, "OK 6");
}

#[test]
fn syntax_ppss_depth_resolves_a_category() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties t))
              (set-syntax-table (make-syntax-table))
              (insert "<ab>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 4 5 'category 'p12-close)
              (list (nth 0 (syntax-ppss 3)) (nth 0 (syntax-ppss 5))))))
        "#
    ));
    assert_eq!(result, "OK (1 0)");
}

#[test]
fn properties_are_ignored_when_parse_sexp_lookup_properties_is_nil() {
    let result = eval(&format!(
        r#"
        (progn
          {CATEGORY_SYMBOLS}
          (with-temp-buffer
            (let ((parse-sexp-lookup-properties nil))
              (set-syntax-table (make-syntax-table))
              (insert "<()>")
              (put-text-property 1 2 'category 'p12-open)
              (put-text-property 4 5 'category 'p12-close)
              (goto-char 1)
              (forward-sexp)
              (point))))
        "#
    ));
    assert_eq!(result, "OK 2");
}
