//! Advanced oracle parity tests for deletion operations:
//! `delete-char` with positive/negative count, boundary behavior,
//! `delete` (equal-based list removal), `delq` vs `delete` semantics,
//! `delete-and-extract-region`, and complex text-editor command pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// delete-char with positive COUNT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_positive_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "abcdefghij")
      (goto-char (point-min))
      ;; Delete 1 char from front
      (delete-char 1)
      (let ((after-1 (buffer-string)))
        ;; Delete 3 more from current position
        (delete-char 3)
        (let ((after-4 (buffer-string)))
          ;; Move to middle and delete 2
          (goto-char 3)
          (delete-char 2)
          (list after-1 after-4 (buffer-string) (point) (buffer-size)))))"#;
    let expect = expect_test::expect![[r#""OK (\"bcdefghij\" \"efghij\" \"efij\" 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char with negative COUNT (backward deletion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_negative_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "0123456789")
      (goto-char (point-max))
      ;; Delete 2 backward from end
      (delete-char -2)
      (let ((after-neg2 (buffer-string)))
        ;; Move to position 5 (between '3' and '4') and delete 3 backward
        (goto-char 5)
        (delete-char -3)
        (let ((after-neg3 (buffer-string)))
          (list after-neg2
                after-neg3
                (point)
                (buffer-size)))))"#;
    let expect = expect_test::expect![[r#""OK (\"01234567\" \"04567\" 2 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char at buffer boundaries (beginning / end)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_boundary_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Attempting to delete past buffer boundaries signals an error.
    // We verify both that the error is raised and that partial-success
    // semantics are correct (buffer unchanged on error).
    let form = r#"(with-temp-buffer
      (insert "abc")
      (goto-char (point-min))
      (let ((err-forward
             (condition-case err
                 (progn (delete-char 10) nil)
               (error (list 'caught (car err)))))
            (buf-after-fwd-err (buffer-string)))
        (goto-char (point-max))
        (let ((err-backward
               (condition-case err
                   (progn (delete-char -10) nil)
                 (error (list 'caught (car err)))))
              (buf-after-bwd-err (buffer-string)))
          ;; Successful delete of entire buffer
          (goto-char (point-min))
          (delete-char 3)
          (list err-forward buf-after-fwd-err
                err-backward buf-after-bwd-err
                (buffer-string) (buffer-size)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught end-of-buffer) \"abc\" (caught beginning-of-buffer) \"abc\" \"\" 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete on lists (equal comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_list_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `delete` uses `equal` for comparison, so it can remove
    // structurally matching elements including strings and sub-lists.
    let form = r#"(let ((original (list "foo" "bar" "baz" "foo" "quux")))
      (let ((after-del (delete "foo" (copy-sequence original))))
        (let ((nested-list (list '(1 2) '(3 4) '(1 2) '(5 6))))
          (let ((after-nested (delete '(1 2) (copy-sequence nested-list))))
            ;; delete with element not present
            (let ((no-match (delete "zzz" (copy-sequence original))))
              ;; delete from nil
              (let ((from-nil (delete "x" nil)))
                (list after-del
                      after-nested
                      no-match
                      from-nil
                      ;; verify original is unchanged
                      original)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"bar\" \"baz\" \"quux\") ((3 4) (5 6)) (\"foo\" \"bar\" \"baz\" \"foo\" \"quux\") nil (\"foo\" \"bar\" \"baz\" \"foo\" \"quux\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_mutates_before_improper_tail_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fdelete uses the same list traversal shape as Fdelq:
    // it removes equal elements destructively before CHECK_LIST_END validates
    // the final tail.  A later improper tail therefore preserves earlier
    // mutation even though the call signals.
    let form = r#"
(let ((x (list '(needle) '(remove-me) '(keep))))
  (setcdr (cdr x) 'tail)
  (condition-case err
      (delete '(remove-me) x)
    (error (list (car err) x))))
"#;

    let expect = expect_test::expect![[r#""OK (wrong-type-argument ((needle) . tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delq vs delete (eq vs equal semantics)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delq_vs_delete_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that delq uses eq (identity) while delete uses equal.
    // For symbols and fixnums, eq and equal agree.  For strings, they differ.
    let form = r#"(let ((str1 "hello")
                        (str2 (concat "hel" "lo")))
      ;; str1 and str2 are equal but NOT eq
      (let ((test-list (list str1 "world" str2 "foo")))
        (let ((eq-result (equal str1 str2))
              (identity-result (eq str1 str2))
              ;; delq with str1 only removes the identical object
              (delq-result (delq str1 (copy-sequence test-list)))
              ;; delete with "hello" removes ALL equal matches
              (delete-result (delete "hello" (copy-sequence test-list))))
          ;; Also test with symbols (eq and equal agree)
          (let ((sym-list (list 'a 'b 'c 'a 'd)))
            (let ((delq-sym (delq 'a (copy-sequence sym-list)))
                  (delete-sym (delete 'a (copy-sequence sym-list))))
              ;; And with integers (eq and equal agree for fixnums)
              (let ((int-list (list 1 2 3 1 4)))
                (let ((delq-int (delq 1 (copy-sequence int-list)))
                      (delete-int (delete 1 (copy-sequence int-list))))
                  (list eq-result identity-result
                        delq-result delete-result
                        delq-sym delete-sym
                        delq-int delete-int))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil (\"world\" \"hello\" \"foo\") (\"world\" \"foo\") (b c d) (b c d) (2 3 4) (2 3 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region returning deleted text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_region_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use delete-and-extract-region to implement a "cut" operation
    // that collects multiple extracted regions into a kill-ring-like list.
    let form = r#"(with-temp-buffer
      (insert "line-one\nline-two\nline-three\nline-four\nline-five\n")
      (let ((kill-ring nil))
        ;; Extract first line
        (goto-char (point-min))
        (let ((eol (line-end-position)))
          (let ((extracted (delete-and-extract-region (point) (1+ eol))))
            (setq kill-ring (cons extracted kill-ring))))
        ;; Extract what is now the second line (was third)
        (goto-char (point-min))
        (forward-line 1)
        (let ((bol (line-beginning-position))
              (eol (line-end-position)))
          (let ((extracted (delete-and-extract-region bol (min (1+ eol) (point-max)))))
            (setq kill-ring (cons extracted kill-ring))))
        ;; Extract from middle of a line
        (goto-char (point-min))
        (let ((mid-start 6))
          (let ((extracted (delete-and-extract-region mid-start (+ mid-start 3))))
            (setq kill-ring (cons extracted kill-ring))))
        (list (nreverse kill-ring)
              (buffer-string)
              (buffer-size))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"line-one\\n\" \"line-three\\n\" \"two\") \"line-\\nline-four\\nline-five\\n\" 26)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: delete-matching-lines implementation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_matching_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simplified `delete-matching-lines` (flush-lines) and
    // `keep-matching-lines` (keep-lines) from scratch.  Uses unwind-protect
    // with fmakunbound for cleanup.
    let form = r#"(progn
  (fset 'neovm--test-del-flush-lines
    (lambda (regexp)
      (goto-char (point-min))
      (let ((deleted-count 0))
        (while (not (eobp))
          (if (looking-at regexp)
              (progn
                (delete-region (line-beginning-position)
                               (min (1+ (line-end-position)) (point-max)))
                (setq deleted-count (1+ deleted-count)))
            (forward-line 1)))
        deleted-count)))

  (fset 'neovm--test-del-keep-lines
    (lambda (regexp)
      (goto-char (point-min))
      (let ((kept-count 0))
        (while (not (eobp))
          (if (looking-at regexp)
              (progn
                (setq kept-count (1+ kept-count))
                (forward-line 1))
            (delete-region (line-beginning-position)
                           (min (1+ (line-end-position)) (point-max)))))
        kept-count)))

  (unwind-protect
      (list
       ;; Test flush-lines: remove comments
       (with-temp-buffer
         (insert "code line 1\n")
         (insert ";; comment A\n")
         (insert "code line 2\n")
         (insert ";; comment B\n")
         (insert ";; comment C\n")
         (insert "code line 3\n")
         (let ((count (funcall 'neovm--test-del-flush-lines "^;;")))
           (list count (buffer-string))))
       ;; Test keep-lines: keep only lines with numbers
       (with-temp-buffer
         (insert "alpha\n")
         (insert "item 42\n")
         (insert "beta\n")
         (insert "item 99\n")
         (insert "gamma\n")
         (let ((count (funcall 'neovm--test-del-keep-lines ".*[0-9]")))
           (list count (buffer-string)))))
    (fmakunbound 'neovm--test-del-flush-lines)
    (fmakunbound 'neovm--test-del-keep-lines)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 \"code line 1\\ncode line 2\\ncode line 3\\n\") (2 \"item 42\\nitem 99\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: text editor command pipeline (delete-word, delete-line, etc.)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_editor_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a sequence of text-editor deletion commands:
    // delete-word-forward, delete-word-backward, delete-to-end-of-line,
    // delete-whole-line, and undo-all (reconstruct from kill ring).
    let form = r#"(progn
  (fset 'neovm--test-del-word-forward
    (lambda ()
      (let ((start (point)))
        (skip-chars-forward "^ \t\n")
        (skip-chars-forward " \t")
        (delete-and-extract-region start (point)))))

  (fset 'neovm--test-del-word-backward
    (lambda ()
      (let ((end (point)))
        (skip-chars-backward " \t")
        (skip-chars-backward "^ \t\n")
        (delete-and-extract-region (point) end))))

  (fset 'neovm--test-del-to-eol
    (lambda ()
      (delete-and-extract-region (point) (line-end-position))))

  (fset 'neovm--test-del-whole-line
    (lambda ()
      (let ((bol (line-beginning-position))
            (eol-plus (min (1+ (line-end-position)) (point-max))))
        (delete-and-extract-region bol eol-plus))))

  (unwind-protect
      (with-temp-buffer
        (insert "the quick brown fox\njumps over the lazy dog\nend of text\n")
        (let ((killed nil))
          ;; Delete first word forward
          (goto-char (point-min))
          (setq killed (cons (funcall 'neovm--test-del-word-forward) killed))
          (let ((after-del-word-fwd (buffer-string)))
            ;; Delete last word backward from end of first line
            (end-of-line)
            (setq killed (cons (funcall 'neovm--test-del-word-backward) killed))
            (let ((after-del-word-bwd (buffer-string)))
              ;; Delete to end of current line
              (goto-char (point-min))
              (forward-char 5)
              (setq killed (cons (funcall 'neovm--test-del-to-eol) killed))
              (let ((after-del-eol (buffer-string)))
                ;; Delete whole second line
                (goto-char (point-min))
                (forward-line 1)
                (setq killed (cons (funcall 'neovm--test-del-whole-line) killed))
                (list (nreverse killed)
                      after-del-word-fwd
                      after-del-word-bwd
                      after-del-eol
                      (buffer-string)))))))
    (fmakunbound 'neovm--test-del-word-forward)
    (fmakunbound 'neovm--test-del-word-backward)
    (fmakunbound 'neovm--test-del-to-eol)
    (fmakunbound 'neovm--test-del-whole-line)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"the \" \"fox\" \" brown \" \"jumps over the lazy dog\\n\") \"quick brown fox\\njumps over the lazy dog\\nend of text\\n\" \"quick brown \\njumps over the lazy dog\\nend of text\\n\" \"quick\\njumps over the lazy dog\\nend of text\\n\" \"quick\\nend of text\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: delete-duplicate-lines implementation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_duplicate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove duplicate lines from a buffer, keeping only the first
    // occurrence of each unique line.  Uses a hash table for O(1) lookup.
    let form = r#"(progn
  (fset 'neovm--test-del-dedup-lines
    (lambda ()
      (let ((seen (make-hash-table :test 'equal))
            (removed 0))
        (goto-char (point-min))
        (while (not (eobp))
          (let ((line (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))))
            (if (gethash line seen)
                (progn
                  (delete-region (line-beginning-position)
                                 (min (1+ (line-end-position)) (point-max)))
                  (setq removed (1+ removed)))
              (puthash line t seen)
              (forward-line 1))))
        removed)))

  (unwind-protect
      (with-temp-buffer
        (insert "apple\n")
        (insert "banana\n")
        (insert "apple\n")
        (insert "cherry\n")
        (insert "banana\n")
        (insert "date\n")
        (insert "cherry\n")
        (insert "apple\n")
        (let ((count (funcall 'neovm--test-del-dedup-lines)))
          (list count (buffer-string))))
    (fmakunbound 'neovm--test-del-dedup-lines)))"#;
    let expect = expect_test::expect![[r#""OK (4 \"apple\\nbanana\\ncherry\\ndate\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
