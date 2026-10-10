//! GNU full-code syntax, category and script lookups for raw-byte regexp input.

#[test]
fn regexp_raw_byte_word_preserves_byte8_syntax_key() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
  (set-buffer-multibyte nil) (insert #xc3 #xa9) (goto-char 2)
  (list (looking-at "\\w") (point) (char-after)))"#,
    );
    // GNU regex-emacs.c:79-80,92-94 and syntax.h:98-103.
    assert_eq!(observed, "OK (t 2 169)");
}

#[test]
fn regexp_raw_byte_backward_posix_boundary_preserves_script_key() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
  (set-buffer-multibyte nil) (insert "\351\200y") (goto-char (point-max))
  (let ((found (posix-search-backward "\\b\\(\\w\\)" nil t)))
    (list found (and found (match-beginning 1)) (and found (match-end 1))
          (and found (string-to-list (match-string 1))))))"#,
    );
    // GNU regex-emacs.c:4964-5005 and category.h:116-118.
    assert_eq!(observed, "OK (3 3 4 (121))");
}

#[test]
fn regexp_raw_byte_syntax_property_table_preserves_full_key_in_both_encodings() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let ((base (copy-syntax-table)) (property (copy-syntax-table)) out)
  (modify-syntax-entry #xa9 "." base)
  (modify-syntax-entry #x3fffa9 "w" base)
  (modify-syntax-entry #xa9 "w" property)
  (modify-syntax-entry #x3fffa9 "." property)
  (dolist (multibyte '(nil t))
    (with-temp-buffer
      (set-buffer-multibyte multibyte) (set-syntax-table base)
      (insert (unibyte-string #xa9))
      (put-text-property 1 2 'syntax-table property)
      (dolist (parse-sexp-lookup-properties '(nil t))
        (dolist (pattern '("\\w" "[[:word:]]" "\\(?:\\w\\|\\s_\\)"))
          (goto-char 1)
          (push (list (looking-at pattern)
                      (and (string-match pattern (buffer-string)) t)) out)))))
  (nreverse out))"#,
    );
    // Base and positional property tables disagree deliberately at the byte8
    // and Latin-1 keys. Cover matcher, dynamic charset and fused syntax tests.
    // GNU execute_charset:3773-3783 sends byte8 to its bitmap arm, so
    // [[:word:]] remains false even when the full-code \w test succeeds.
    assert_eq!(
        observed,
        "OK ((t t) (nil nil) (t t) (nil nil) (nil nil) (nil nil) (t t) (nil nil) (t t) (nil nil) (nil nil) (nil nil))"
    );
}
