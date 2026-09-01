//! Advanced oracle parity tests for `save-excursion` patterns.
//!
//! Tests deeply nested save-excursion, interaction with buffer changes
//! (insertions/deletions shifting markers), save-excursion with goto-char
//! and insert/delete in loops, unwind behavior on error, combined with
//! save-restriction, and multi-buffer save-excursion scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Deeply nested save-excursion preserving point at each level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_deeply_nested_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Five levels of nested save-excursion, each moving point to a
    // different position. After unwinding, each level should restore
    // the point it saved. We collect results at each level.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz0123456789")
      (goto-char 5)
      (let ((results nil))
        (push (point) results)
        (save-excursion
          (goto-char 10)
          (push (point) results)
          (save-excursion
            (goto-char 20)
            (push (point) results)
            (save-excursion
              (goto-char 30)
              (push (point) results)
              (save-excursion
                (goto-char (point-max))
                (push (point) results)
                (save-excursion
                  (goto-char 1)
                  (push (point) results))
                ;; back from level 5
                (push (point) results))
              ;; back from level 4
              (push (point) results))
            ;; back from level 3
            (push (point) results))
          ;; back from level 2
          (push (point) results))
        ;; back from level 1
        (push (point) results)
        (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (5 10 20 30 37 1 37 30 20 10 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with interleaved insertions and deletions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_interleaved_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Marker-based point restoration interacts with buffer modifications.
    // Insert before saved point (shifts marker right), delete around saved
    // point, insert after saved point (no shift). Track the marker
    // adjustment behavior precisely.
    let form = r#"(with-temp-buffer
      (insert "0123456789ABCDEF")
      (goto-char 9)  ;; point at '8'
      (let ((saved-char (char-after))
            (results nil))
        ;; First save-excursion: insert before point
        (save-excursion
          (goto-char 3)
          (insert "XXX")     ;; shifts saved marker from 9 to 12
          (push (buffer-string) results))
        (push (point) results)
        (push (char-after) results)

        ;; Second save-excursion: delete text that includes the saved point area
        (goto-char 6)
        (save-excursion
          (goto-char 4)
          (delete-region 4 8)  ;; removes 4 chars before point at 6
          (push (buffer-string) results))
        (push (point) results)

        ;; Third: insert after saved point
        (save-excursion
          (goto-char (point-max))
          (insert "YYYYY")
          (push (buffer-string) results))
        (push (point) results)

        (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"01XXX23456789ABCDEF\" 12 56 \"01X456789ABCDEF\" 4 \"01X456789ABCDEFYYYYY\" 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion in a loop: repeatedly scan from beginning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_loop_scan_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use save-excursion in a dotimes loop to collect the character at
    // successive line beginnings, always returning to the original point.
    // Also mutate the buffer inside the loop to test marker stability.
    let form = r#"(with-temp-buffer
      (insert "line-A first\nline-B second\nline-C third\nline-D fourth\nline-E fifth\n")
      (goto-char 15)
      (let ((line-starts nil)
            (line-count 0)
            (orig (point)))
        ;; Count lines first
        (save-excursion
          (goto-char (point-min))
          (while (not (eobp))
            (setq line-count (1+ line-count))
            (forward-line 1)))
        ;; Collect first word of each line
        (let ((i 0))
          (while (< i line-count)
            (save-excursion
              (goto-char (point-min))
              (forward-line i)
              (let ((start (point)))
                (re-search-forward "[^ \n]+" nil t)
                (push (match-string 0) line-starts)))
            (setq i (1+ i))))
        ;; Modify buffer between save-excursion calls
        (save-excursion
          (goto-char (point-min))
          (insert "PREPENDED\n"))
        ;; Point should still be restored (shifted by insertion)
        (list (nreverse line-starts)
              line-count
              orig
              (point)
              (> (point) orig)  ;; should have shifted right
              (buffer-substring (point-min) (+ (point-min) 9)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"line-A\" \"line-B\" \"line-C\" \"line-D\" \"line-E\") 5 15 25 t \"PREPENDED\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with error: unwind behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_error_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion must restore point even when an error is signaled
    // inside. We use condition-case to catch the error and verify point
    // restoration afterward.
    let form = r#"(with-temp-buffer
      (insert "hello world of save-excursion error handling test")
      (goto-char 7)
      (let ((pre-point (point))
            (error-caught nil)
            (inner-point nil))
        (condition-case err
            (save-excursion
              (goto-char 20)
              (setq inner-point (point))
              (insert "INSERTED")
              ;; signal an error after modification
              (error "deliberate error at pos %d" (point)))
          (error
           (setq error-caught (error-message-string err))))
        ;; Point should be restored despite the error.
        ;; The buffer modification inside save-excursion persists.
        (list pre-point
              (point)
              inner-point
              error-caught
              (buffer-string)
              (= (point) pre-point))))"#;
    let expect = expect_test::expect![[
        r#""OK (7 7 20 \"deliberate error at pos 28\" \"hello world of saveINSERTED-excursion error handling test\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion combined with save-restriction: nested widen/narrow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_restriction_nested_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple interleaved save-excursion and save-restriction forms.
    // Widen inside an inner save-restriction, narrow differently, and
    // verify that each level restores independently.
    let form = r#"(with-temp-buffer
      (insert "AAAAABBBBBCCCCCDDDDDEEEEE")
      (narrow-to-region 6 20)
      (goto-char 10)
      (let ((outer-min (point-min))
            (outer-max (point-max))
            (outer-point (point))
            (results nil))
        ;; Level 1: save-excursion + save-restriction
        (save-excursion
          (save-restriction
            (widen)
            (push (list 'widened (point-min) (point-max) (point)) results)
            (goto-char 3)
            ;; Level 2: nested save-restriction re-narrows
            (save-restriction
              (narrow-to-region 1 10)
              (goto-char (point-max))
              (push (list 'inner-narrow (point-min) (point-max) (point)) results))
            ;; After level 2 restore: widened again
            (push (list 'after-inner (point-min) (point-max) (point)) results)))
        ;; After all restores: original narrowing and point
        (push (list 'restored
                    (point-min) (= (point-min) outer-min)
                    (point-max) (= (point-max) outer-max)
                    (point) (= (point) outer-point))
              results)
        (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((widened 1 26 10) (inner-narrow 1 10 10) (after-inner 1 26 10) (restored 6 t 20 t 10 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-buffer save-excursion: switch buffers inside, verify both restore
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_multi_buffer_switching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create three temp buffers, set different points in each, then use
    // save-excursion to switch between them. Verify that each buffer's
    // point is correctly restored when returning.
    let form = r#"(let ((buf1 (generate-new-buffer "neovm--se-pat-1"))
                        (buf2 (generate-new-buffer "neovm--se-pat-2"))
                        (buf3 (generate-new-buffer "neovm--se-pat-3")))
      (unwind-protect
          (progn
            ;; Setup each buffer with content and a specific point
            (with-current-buffer buf1
              (insert "Buffer-One-Content-Here")
              (goto-char 8))
            (with-current-buffer buf2
              (insert "Buffer-Two-Content-Here-Extended")
              (goto-char 15))
            (with-current-buffer buf3
              (insert "Buffer-Three-Here")
              (goto-char 5))

            ;; From buf1, use nested save-excursion to visit buf2 and buf3
            (with-current-buffer buf1
              (let ((results nil))
                (push (list 'buf1-before (point) (buffer-name)) results)
                (save-excursion
                  (set-buffer buf2)
                  (push (list 'in-buf2 (point) (buffer-name)) results)
                  (goto-char (point-max))
                  (insert " MODIFIED-B2")
                  (save-excursion
                    (set-buffer buf3)
                    (push (list 'in-buf3 (point) (buffer-name)) results)
                    (goto-char 1)
                    (insert "PREFIX-")
                    (push (list 'buf3-after-insert (point) (buffer-string)) results))
                  ;; Back to buf2 after inner restore
                  (push (list 'back-to-buf2 (point) (buffer-name)) results))
                ;; Back to buf1 after outer restore
                (push (list 'back-to-buf1 (point) (buffer-name)) results)
                ;; Verify modifications persisted
                (push (list 'buf2-content
                            (with-current-buffer buf2 (buffer-string)))
                      results)
                (push (list 'buf3-content
                            (with-current-buffer buf3 (buffer-string)))
                      results)
                (nreverse results))))
        (kill-buffer buf1)
        (kill-buffer buf2)
        (kill-buffer buf3)))"#;
    let expect = expect_test::expect![[
        r#""OK ((buf1-before 8 \"neovm--se-pat-1\") (in-buf2 15 \"neovm--se-pat-2\") (in-buf3 5 \"neovm--se-pat-3\") (buf3-after-insert 8 \"PREFIX-Buffer-Three-Here\") (back-to-buf2 45 \"neovm--se-pat-2\") (back-to-buf1 8 \"neovm--se-pat-1\") (buf2-content \"Buffer-Two-Content-Here-Extended MODIFIED-B2\") (buf3-content \"PREFIX-Buffer-Three-Here\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion as a building block for a multi-pass text transformer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_multi_pass_transformer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline of buffer transformations, each wrapped in
    // save-excursion to preserve state. Transformations: (1) uppercase
    // all words matching a pattern, (2) wrap matches in brackets,
    // (3) collect statistics. Point must survive all passes.
    let form = r#"(with-temp-buffer
      (insert "the cat sat on the mat and the cat ate the rat")
      (goto-char 12)
      (let ((orig-point (point))
            (stats nil))
        ;; Pass 1: count word occurrences
        (let ((word-counts nil))
          (save-excursion
            (goto-char (point-min))
            (while (re-search-forward "\\b\\([a-z]+\\)\\b" nil t)
              (let* ((w (match-string 1))
                     (entry (assoc w word-counts)))
                (if entry
                    (setcdr entry (1+ (cdr entry)))
                  (setq word-counts (cons (cons w 1) word-counts))))))
          (setq stats word-counts))

        ;; Pass 2: upcase all occurrences of "the"
        (save-excursion
          (goto-char (point-min))
          (while (re-search-forward "\\bthe\\b" nil t)
            (replace-match "THE")))

        ;; Pass 3: wrap all occurrences of "cat" in brackets
        (save-excursion
          (goto-char (point-min))
          (while (re-search-forward "\\bcat\\b" nil t)
            (replace-match "[cat]")))

        ;; Pass 4: count total words in final buffer
        (let ((total-words 0))
          (save-excursion
            (goto-char (point-min))
            (while (re-search-forward "\\b[A-Za-z\\[\\]]+\\b" nil t)
              (setq total-words (1+ total-words))))

          ;; Point should be restored after all passes
          ;; (marker shifts due to replacements)
          (list (buffer-string)
                total-words
                (sort (copy-sequence stats)
                      (lambda (a b) (string-lessp (car a) (car b))))
                (point)
                (= (point) orig-point)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"THE [cat] sat on THE mat and THE [cat] ate THE rat\" 0 ((\"and\" . 1) (\"ate\" . 1) (\"cat\" . 2) (\"mat\" . 1) (\"on\" . 1) (\"rat\" . 1) (\"sat\" . 1) (\"the\" . 4)) 14 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with recursive function that modifies buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_recursive_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A recursive function uses save-excursion at each level to process
    // numbered markers in a buffer: find "{{N}}", replace with the result
    // of recursively processing the text, tracking depth.
    let form = r#"(progn
  (fset 'neovm--test-se-process
    (lambda (depth max-depth)
      "Process numbered markers {{N}} in current buffer up to max-depth."
      (when (< depth max-depth)
        (save-excursion
          (goto-char (point-min))
          (while (re-search-forward "{{\\([0-9]+\\)}}" nil t)
            (let ((n (string-to-number (match-string 1))))
              (replace-match (format "[d%d:n%d]" depth n))
              ;; Recurse to handle any new markers (none in this test,
              ;; but the recursion + save-excursion nesting is the point)
              (funcall 'neovm--test-se-process (1+ depth) max-depth)))))))
  (unwind-protect
      (with-temp-buffer
        (insert "start {{1}} middle {{2}} end {{3}} tail")
        (goto-char 7)
        (let ((orig-point (point)))
          (funcall 'neovm--test-se-process 0 3)
          (list (buffer-string)
                (point)
                orig-point)))
    (fmakunbound 'neovm--test-se-process)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"start [d0:n1] middle [d1:n2] end [d2:n3] tail\" 7 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
