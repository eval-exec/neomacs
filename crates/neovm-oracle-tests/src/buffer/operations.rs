//! Oracle parity tests for buffer operations: `insert`, `delete-char`,
//! `buffer-substring-no-properties`, `erase-buffer`, `buffer-size`,
//! `point`, `goto-char`, `bobp`, `eobp`, `bolp`, `eolp`,
//! `char-before`, `following-char`, `preceding-char`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// bobp / eobp / bolp / eolp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bobp_eobp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (list (bobp) (eobp)))"#;
    let expect = expect_test::expect![r#""OK (t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bobp_eobp_with_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello")
                    (let ((at-end (list (bobp) (eobp))))
                      (goto-char (point-min))
                      (let ((at-start (list (bobp) (eobp))))
                        (list at-start at-end))))"#;
    let expect = expect_test::expect![r#""OK ((t nil) (nil t))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bolp_eolp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3")
                    (goto-char (point-min))
                    (let ((results nil))
                      ;; At beginning of line 1
                      (setq results (cons (list (bolp) (eolp)) results))
                      ;; Move to end of line 1
                      (end-of-line)
                      (setq results (cons (list (bolp) (eolp)) results))
                      ;; Move to beginning of line 2
                      (forward-line 1)
                      (beginning-of-line)
                      (setq results (cons (list (bolp) (eolp)) results))
                      ;; Move to middle of line 2
                      (forward-char 2)
                      (setq results (cons (list (bolp) (eolp)) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![r#""OK ((t nil) (nil t) (t nil) (nil nil))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before / following-char / preceding-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "abcdef")
                    (goto-char 4)
                    (list (char-before)
                          (char-before 1)
                          (char-before 3)
                          (char-before 7)))"#;
    let expect = expect_test::expect![r#""OK (99 nil 98 102)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_following_preceding_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello")
                    (goto-char 3)
                    (list (following-char)
                          (preceding-char)))"#;
    let expect = expect_test::expect![r#""OK (108 101)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_following_char_at_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "ab")
                    (goto-char (point-min))
                    (let ((at-start (following-char)))
                      (goto-char (point-max))
                      (let ((at-end (following-char)))
                        (list at-start at-end))))"#;
    let expect = expect_test::expect![r#""OK (97 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-size
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((empty-size (buffer-size)))
                      (insert "hello world")
                      (let ((with-content (buffer-size)))
                        (list empty-size with-content))))"#;
    let expect = expect_test::expect![r#""OK (0 11)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-substring-no-properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert (propertize "hello" 'face 'bold))
                    (insert " world")
                    (list (buffer-substring 1 6)
                          (buffer-substring-no-properties 1 6)
                          (buffer-substring-no-properties
                           (point-min) (point-max))))"#;
    let expect =
        expect_test::expect![[r#""OK (#(\"hello\" 0 5 (face bold)) \"hello\" \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "content here")
                    (let ((before (buffer-size)))
                      (erase-buffer)
                      (list before (buffer-size)
                            (point) (point-min) (point-max))))"#;
    let expect = expect_test::expect![r#""OK (12 0 1 1 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: text manipulation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_word_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract words between markers
    let form = r#"(with-temp-buffer
                    (insert "BEGIN alpha beta gamma END other stuff")
                    (goto-char (point-min))
                    (let ((words nil))
                      (when (re-search-forward "BEGIN " nil t)
                        (let ((start (point)))
                          (when (re-search-forward " END" nil t)
                            (let ((region (buffer-substring
                                           start (match-beginning 0))))
                              (setq words (split-string region))))))
                      words))"#;
    let expect = expect_test::expect![[r#""OK (\"alpha\" \"beta\" \"gamma\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_buffer_line_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex line-by-line buffer manipulation
    let form = r#"(with-temp-buffer
                    (insert "  line 1  \n\n  line 3  \n  line 4  \n")
                    (goto-char (point-min))
                    (let ((lines nil) (count 0))
                      (while (not (eobp))
                        (let ((line-start (point)))
                          (end-of-line)
                          (let ((line (buffer-substring
                                       line-start (point))))
                            ;; Only collect non-empty trimmed lines
                            (let ((trimmed
                                   (replace-regexp-in-string
                                    "\\`[ \t\n]+" ""
                                    (replace-regexp-in-string
                                     "[ \t\n]+\\'" "" line))))
                              (when (> (length trimmed) 0)
                                (setq lines (cons trimmed lines)
                                      count (1+ count))))))
                        (forward-line 1))
                      (list (nreverse lines) count)))"#;
    let expect = expect_test::expect![[r#""OK ((\"line 1\" \"line 3\" \"line 4\") 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_buffer_insert_and_navigate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build structured content then navigate it
    let form = r#"(with-temp-buffer
                    ;; Insert a simple key-value block
                    (let ((pairs '(("name" . "Alice")
                                   ("age" . "30")
                                   ("role" . "engineer"))))
                      (dolist (p pairs)
                        (insert (car p) ": " (cdr p) "\n")))
                    ;; Now extract values by searching for keys
                    (let ((extract-value
                           (lambda (key)
                             (goto-char (point-min))
                             (when (re-search-forward
                                    (concat "^" (regexp-quote key)
                                            ": \\(.+\\)$")
                                    nil t)
                               (match-string 1)))))
                      (list (funcall extract-value "name")
                            (funcall extract-value "age")
                            (funcall extract-value "role")
                            (funcall extract-value "missing"))))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" \"30\" \"engineer\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_buffer_search_replace_multipass() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-pass search and replace with state tracking
    let form = r#"(with-temp-buffer
                    (insert "TODO: fix bug\nDONE: write tests\n")
                    (insert "TODO: refactor\nDONE: deploy\n")
                    (insert "TODO: review\n")
                    (goto-char (point-min))
                    (let ((todos 0) (dones 0))
                      ;; Count TODOs
                      (while (re-search-forward "^TODO:" nil t)
                        (setq todos (1+ todos)))
                      ;; Count DONEs
                      (goto-char (point-min))
                      (while (re-search-forward "^DONE:" nil t)
                        (setq dones (1+ dones)))
                      ;; Replace all TODOs with IN-PROGRESS
                      (goto-char (point-min))
                      (while (re-search-forward "^TODO:" nil t)
                        (replace-match "IN-PROGRESS:"))
                      (list todos dones (buffer-string))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 2 \"IN-PROGRESS: fix bug\\nDONE: write tests\\nIN-PROGRESS: refactor\\nDONE: deploy\\nIN-PROGRESS: review\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
