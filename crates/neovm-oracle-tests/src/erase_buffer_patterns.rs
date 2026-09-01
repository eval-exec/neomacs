//! Advanced oracle parity tests for `erase-buffer` and buffer reset patterns:
//! basic erase semantics, effect on point/markers/narrowing, equivalence with
//! `delete-region`, interaction with `save-excursion`, buffer recycling,
//! state diff tracking, and multi-buffer pipeline patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// erase-buffer basic: empty, non-empty, already-empty buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_basic_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test erase-buffer on: non-empty buffer, buffer with only newlines,
    // already-empty buffer, and buffer with multibyte content.
    let form = r#"(let ((results nil))
  ;; Non-empty buffer
  (with-temp-buffer
    (insert "Hello, World!")
    (let ((before (list (buffer-string) (buffer-size) (point))))
      (erase-buffer)
      (setq results
            (cons (list 'non-empty
                        before
                        (buffer-string)
                        (buffer-size)
                        (point)
                        (point-min)
                        (point-max)
                        (bobp)
                        (eobp))
                  results))))
  ;; Buffer with only newlines
  (with-temp-buffer
    (insert "\n\n\n\n\n")
    (erase-buffer)
    (setq results
          (cons (list 'newlines-only
                      (buffer-string)
                      (buffer-size)
                      (point))
                results)))
  ;; Already-empty buffer
  (with-temp-buffer
    (let ((before-size (buffer-size)))
      (erase-buffer)
      (setq results
            (cons (list 'already-empty
                        before-size
                        (buffer-size)
                        (point)
                        (bobp)
                        (eobp))
                  results))))
  ;; Multibyte content
  (with-temp-buffer
    (insert "abc")
    (let ((before-size (buffer-size)))
      (erase-buffer)
      (setq results
            (cons (list 'multibyte
                        before-size
                        (buffer-size)
                        (= (point) 1))
                  results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((non-empty (\"Hello, World!\" 13 14) \"\" 0 1 1 1 t t) (newlines-only \"\" 0 1) (already-empty 0 0 1 t t) (multibyte 3 0 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer effect on point, markers, and narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_point_markers_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that erase-buffer:
    // - moves point to 1
    // - invalidates marker positions (markers point to 1)
    // - widens any narrowing
    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (goto-char 15)
  (let ((m1 (copy-marker 5))
        (m2 (copy-marker 20))
        (m-end (copy-marker (point-max))))
    ;; Apply narrowing before erase
    (narrow-to-region 10 20)
    (let ((pre-narrow-str (buffer-string))
          (pre-narrow-pmin (point-min))
          (pre-narrow-pmax (point-max)))
      (erase-buffer)
      (let ((result (list
                     pre-narrow-str
                     pre-narrow-pmin
                     pre-narrow-pmax
                     ;; After erase:
                     (buffer-string)
                     (buffer-size)
                     (point)
                     (point-min)
                     (point-max)
                     ;; Markers after erase
                     (marker-position m1)
                     (marker-position m2)
                     (marker-position m-end)
                     ;; Narrowing should be gone
                     (= (point-min) 1)
                     (= (point-max) 1))))
        (set-marker m1 nil)
        (set-marker m2 nil)
        (set-marker m-end nil)
        result))))"#;
    let expect = expect_test::expect![[r#""OK (\"JKLMNOPQRS\" 10 20 \"\" 0 1 1 1 1 1 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer vs delete-region(point-min, point-max) equivalence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_vs_delete_region_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare the state after erase-buffer vs delete-region on identical
    // buffer contents.
    let form = r#"(let ((content "Line one\nLine two\nLine three\nLine four\n")
      (erase-result nil)
      (delete-result nil))
  ;; Method 1: erase-buffer
  (with-temp-buffer
    (insert content)
    (goto-char 10)
    (erase-buffer)
    (setq erase-result
          (list (buffer-string) (buffer-size) (point)
                (point-min) (point-max) (bobp) (eobp))))
  ;; Method 2: delete-region
  (with-temp-buffer
    (insert content)
    (goto-char 10)
    (delete-region (point-min) (point-max))
    (setq delete-result
          (list (buffer-string) (buffer-size) (point)
                (point-min) (point-max) (bobp) (eobp))))
  ;; Both should produce identical buffer state
  (list erase-result
        delete-result
        (equal erase-result delete-result)))"#;
    let expect = expect_test::expect![[r#""OK ((\"\" 0 1 1 1 t t) (\"\" 0 1 1 1 t t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer within save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_within_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion saves point and mark; after erase-buffer the saved
    // point is invalid. Verify behavior on restore.
    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  (goto-char 10)
  (let ((point-before (point))
        (size-before (buffer-size)))
    (save-excursion
      (erase-buffer)
      (insert "NEW"))
    ;; After save-excursion restores: what happens to point?
    (list point-before
          size-before
          (buffer-string)
          (buffer-size)
          (point)
          ;; Point should be clamped to valid range
          (<= (point) (point-max))
          (>= (point) (point-min)))))"#;
    let expect = expect_test::expect![[r#""OK (10 16 \"NEW\" 3 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer recycling pattern: erase + rebuild multiple times
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_recycling_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a buffer being reused: erase, populate with different data,
    // process, collect results, repeat. This is a common pattern in Emacs.
    let form = r#"(with-temp-buffer
  (let ((datasets '(("Alpha Bravo Charlie" . " ")
                    ("one:two:three:four" . ":")
                    ("X--Y--Z" . "--")
                    ("single" . ",")))
        (all-results nil))
    (dolist (dataset datasets)
      (erase-buffer)
      ;; Populate
      (insert (car dataset))
      ;; Process: split by delimiter and count parts
      (let* ((content (buffer-string))
             (parts (split-string content (regexp-quote (cdr dataset))))
             (num-parts (length parts))
             (total-len (buffer-size)))
        ;; Build a summary line in the buffer
        (erase-buffer)
        (insert (format "Parts: %d, Lengths: %s"
                        num-parts
                        (mapconcat (lambda (p)
                                     (number-to-string (length p)))
                                   parts ",")))
        (setq all-results
              (cons (list (car dataset)
                          num-parts
                          parts
                          (buffer-string))
                    all-results))))
    (nreverse all-results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alpha Bravo Charlie\" 3 (\"Alpha\" \"Bravo\" \"Charlie\") \"Parts: 3, Lengths: 5,5,7\") (\"one:two:three:four\" 4 (\"one\" \"two\" \"three\" \"four\") \"Parts: 4, Lengths: 3,3,5,4\") (\"X--Y--Z\" 3 (\"X\" \"Y\" \"Z\") \"Parts: 3, Lengths: 1,1,1\") (\"single\" 1 (\"single\") \"Parts: 1, Lengths: 6\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State diff before/after erase-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_state_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Capture comprehensive buffer state before and after erase-buffer,
    // including point, markers, match-data, and restriction info.
    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog")
  ;; Set up some state
  (goto-char 11)
  (let ((m1 (copy-marker 5))
        (m2 (copy-marker 30))
        (m3 (point-marker)))
    ;; Do a search to set match-data
    (goto-char (point-min))
    (re-search-forward "\\(quick\\) \\(brown\\)" nil t)
    ;; Capture pre-state
    (let ((pre-state
           (list (buffer-string)
                 (buffer-size)
                 (point)
                 (marker-position m1)
                 (marker-position m2)
                 (marker-position m3)
                 (match-beginning 0)
                 (match-end 0))))
      (erase-buffer)
      ;; Capture post-state
      (let ((post-state
             (list (buffer-string)
                   (buffer-size)
                   (point)
                   (marker-position m1)
                   (marker-position m2)
                   (marker-position m3)
                   (point-min)
                   (point-max))))
        (set-marker m1 nil)
        (set-marker m2 nil)
        (set-marker m3 nil)
        (list pre-state post-state)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"The quick brown fox jumps over the lazy dog\" 43 16 5 30 11 5 16) (\"\" 0 1 1 1 1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer in multi-buffer pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_multi_buffer_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: source buffer -> transform buffer -> output buffer.
    // Each stage erases and rebuilds. Simulates a data processing pipeline.
    let form = r#"(let ((src (get-buffer-create " *neovm-pipe-src*"))
                        (xform (get-buffer-create " *neovm-pipe-xform*"))
                        (out (get-buffer-create " *neovm-pipe-out*")))
  (unwind-protect
      (progn
        ;; Stage 1: Load raw data into source
        (with-current-buffer src
          (erase-buffer)
          (insert "apple 3\nbanana 7\ncherry 2\ndate 5\n"))
        ;; Stage 2: Transform -- parse and compute in xform buffer
        (with-current-buffer xform
          (erase-buffer)
          (with-current-buffer src
            (goto-char (point-min))
            (while (re-search-forward
                    "^\\([a-z]+\\) \\([0-9]+\\)$" nil t)
              (let ((name (match-string 1))
                    (count (string-to-number (match-string 2))))
                (with-current-buffer xform
                  (insert (format "%s:%d\n"
                                  (upcase name)
                                  (* count count))))))))
        ;; Stage 3: Aggregate into output
        (with-current-buffer out
          (erase-buffer)
          (insert "=== REPORT ===\n")
          (let ((total 0)
                (items 0))
            (with-current-buffer xform
              (goto-char (point-min))
              (while (re-search-forward
                      "^[A-Z]+:\\([0-9]+\\)$" nil t)
                (setq total (+ total (string-to-number
                                       (match-string 1))))
                (setq items (1+ items))))
            (with-current-buffer out
              (insert (format "Items: %d\nTotal: %d\n" items total)
                      "--- Detail ---\n")
              (insert-buffer-substring xform))))
        ;; Collect results
        (list (with-current-buffer src (buffer-string))
              (with-current-buffer xform (buffer-string))
              (with-current-buffer out (buffer-string))
              (with-current-buffer out (buffer-size))))
    (kill-buffer src)
    (kill-buffer xform)
    (kill-buffer out)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"apple 3\\nbanana 7\\ncherry 2\\ndate 5\\n\" \"APPLE:9\\nBANANA:49\\nCHERRY:4\\nDATE:25\\n\" \"=== REPORT ===\\nItems: 4\\nTotal: 87\\n--- Detail ---\\nAPPLE:9\\nBANANA:49\\nCHERRY:4\\nDATE:25\\n\" 84)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
