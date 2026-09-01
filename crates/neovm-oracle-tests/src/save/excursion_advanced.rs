//! Advanced oracle parity tests for `save-excursion` combined with
//! narrowing, mark, buffer modifications, loops, and multi-buffer
//! scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// save-excursion restoring point after complex movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_complex_movement_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple forward/backward movements, searches, and insertions
    // inside save-excursion; point must be restored precisely.
    let form = r#"(with-temp-buffer
      (insert "line one\nline two\nline three\nline four\nline five\n")
      (goto-char 5)
      (let ((original-point (point)))
        (save-excursion
          (goto-char (point-min))
          (forward-line 2)
          (let ((mid (point)))
            (goto-char (point-max))
            (forward-line -1)
            (let ((near-end (point)))
              (goto-char mid)
              (re-search-forward "three" nil t)
              (list mid near-end (point) (match-string 0)))))
        (list (point) (= (point) original-point))))"#;
    let expect = expect_test::expect![[r#""OK (5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion restoring mark
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_restores_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion saves and restores the mark as well as point.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz")
      (goto-char 10)
      (push-mark 20 t)
      (let ((mark-before (mark t)))
        (save-excursion
          (goto-char 5)
          (push-mark 15 t)
          (list (point) (mark t)))
        (list mark-before (mark t) (= mark-before (mark t)) (point))))"#;
    let expect = expect_test::expect![[r#""OK (20 15 nil 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested save-excursion with different buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_nested_different_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer save-excursion in buffer A, inner switches to buffer B,
    // modifies it, then both restore properly.
    let form = r#"(let ((buf-a (generate-new-buffer "neovm--test-se-A"))
                        (buf-b (generate-new-buffer "neovm--test-se-B")))
      (unwind-protect
          (progn
            (with-current-buffer buf-a
              (insert "AAAA")
              (goto-char 3))
            (with-current-buffer buf-b
              (insert "BBBB")
              (goto-char 2))
            (with-current-buffer buf-a
              (save-excursion
                (goto-char (point-max))
                (save-excursion
                  (set-buffer buf-b)
                  (goto-char (point-max))
                  (insert "XX")
                  (point))
                ;; Still in buf-a after inner restore
                (list (buffer-name (current-buffer)) (point)))
              ;; After outer restore
              (list (buffer-name (current-buffer))
                    (point)
                    (with-current-buffer buf-b
                      (buffer-string)))))
        (kill-buffer buf-a)
        (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[r#""OK (\"neovm--test-se-A\" 3 \"BBBBXX\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with buffer modifications between save and restore
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_with_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insertions/deletions shift point; save-excursion uses markers,
    // so the restored position adjusts to the modified buffer.
    let form = r#"(with-temp-buffer
      (insert "0123456789")
      (goto-char 6)
      (let ((before-point (point)))
        (save-excursion
          ;; Insert text BEFORE saved point — marker shifts right
          (goto-char 3)
          (insert "XXX")
          ;; Delete text AFTER saved point — marker unaffected
          (goto-char 12)
          (delete-char 2)
          (buffer-string))
        ;; Point should have shifted by the insertion length
        (list before-point (point) (buffer-string))))"#;
    let expect = expect_test::expect![[r#""OK (6 9 \"01XXX234567\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion combined with narrow-to-region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion inside a narrowed region; operations respect
    // narrowing bounds, but point is still restored.
    let form = r#"(with-temp-buffer
      (insert "aaa-bbb-ccc-ddd-eee")
      (narrow-to-region 5 15)
      (goto-char (point-min))
      (let ((narrow-min (point-min))
            (narrow-max (point-max)))
        (save-excursion
          (goto-char (point-max))
          (let ((at-max (point)))
            ;; Search backward within narrowed region
            (goto-char (point-min))
            (re-search-forward "ccc" nil t)
            (let ((found (match-string 0))
                  (found-pos (match-beginning 0)))
              (list at-max found found-pos))))
        (list (point) narrow-min narrow-max
              (point-min) (point-max))))"#;
    let expect = expect_test::expect![[r#""OK (5 5 15 5 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion + save-restriction combination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_save_restriction_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine both: save-excursion saves point/mark/buffer,
    // save-restriction saves narrowing state. Interleave them.
    let form = r#"(with-temp-buffer
      (insert "line-1\nline-2\nline-3\nline-4\nline-5\n")
      (goto-char 8)
      (narrow-to-region 8 22)
      (let ((orig-point (point))
            (orig-min (point-min))
            (orig-max (point-max)))
        (save-excursion
          (save-restriction
            (widen)
            (let ((wide-min (point-min))
                  (wide-max (point-max)))
              (goto-char (point-min))
              (narrow-to-region 1 15)
              (let ((inner-min (point-min))
                    (inner-max (point-max)))
                (list wide-min wide-max inner-min inner-max)))))
        ;; After both restores: point, narrowing all back
        (list (point) (= (point) orig-point)
              (point-min) (= (point-min) orig-min)
              (point-max) (= (point-max) orig-max))))"#;
    let expect = expect_test::expect![[r#""OK (8 t 8 t 22 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-pass buffer processing using save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_multi_pass_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three passes over a buffer:
    // Pass 1: Count occurrences of a pattern
    // Pass 2: Collect all matches with positions
    // Pass 3: Replace matches
    // Each pass uses save-excursion to preserve state for the next.
    let form = r#"(with-temp-buffer
      (insert "val=10 val=20 val=30 val=40")
      (goto-char 5)
      (let (count matches replaced)
        ;; Pass 1: count
        (setq count
              (save-excursion
                (goto-char (point-min))
                (let ((n 0))
                  (while (re-search-forward "val=\\([0-9]+\\)" nil t)
                    (setq n (1+ n)))
                  n)))
        ;; Pass 2: collect positions and values
        (setq matches
              (save-excursion
                (goto-char (point-min))
                (let ((acc nil))
                  (while (re-search-forward "val=\\([0-9]+\\)" nil t)
                    (setq acc (cons (list (match-beginning 0)
                                         (match-string 1))
                                   acc)))
                  (nreverse acc))))
        ;; Pass 3: replace all val=N with result=N*2
        (save-excursion
          (goto-char (point-min))
          (while (re-search-forward "val=\\([0-9]+\\)" nil t)
            (let ((n (string-to-number (match-string 1))))
              (replace-match (format "result=%d" (* n 2))))))
        (setq replaced (buffer-string))
        ;; Point should be restored to 5
        (list (point) count matches replaced)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 4 ((1 \"10\") (8 \"20\") (15 \"30\") (22 \"40\")) \"result=20 result=40 result=60 result=80\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion within dolist/while loops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_in_loops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use save-excursion inside a loop to repeatedly scan from point-min
    // while maintaining an accumulator outside the save-excursion.
    let form = r#"(with-temp-buffer
      (insert "apple:3 banana:7 cherry:2 date:9 elderberry:1")
      (goto-char 10)
      (let ((patterns '("apple" "cherry" "elderberry" "fig"))
            (found nil))
        (dolist (pat patterns)
          (save-excursion
            (goto-char (point-min))
            (if (re-search-forward
                 (concat (regexp-quote pat) ":\\([0-9]+\\)") nil t)
                (setq found (cons (list pat (string-to-number
                                             (match-string 1)))
                                  found))
              (setq found (cons (list pat nil) found)))))
        ;; Point unchanged after all loop iterations
        (list (point) (nreverse found))))"#;
    let expect = expect_test::expect![[
        r#""OK (10 ((\"apple\" 3) (\"cherry\" 2) (\"elderberry\" 1) (\"fig\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
