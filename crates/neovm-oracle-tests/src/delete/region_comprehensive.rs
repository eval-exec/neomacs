//! Comprehensive oracle parity tests for delete-region and related operations:
//! delete-region with various bounds, delete-char with positive/negative COUNT
//! and KILLP, delete-and-extract-region returning deleted text, deletion
//! interaction with markers, narrowed buffers, text properties, buffer size
//! tracking, and deletion at buffer boundaries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// delete-region: exhaustive boundary combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_boundary_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test delete-region with reversed args (END < START should swap),
    // equal args, single char, full buffer, and from various positions.
    let form = r#"(let ((results nil))
  ;; Reversed arguments (Emacs auto-swaps START and END)
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 8 4)
    (setq results (cons (list 'reversed (buffer-string) (buffer-size)) results)))
  ;; Delete single character at start
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 1 2)
    (setq results (cons (list 'single-start (buffer-string)) results)))
  ;; Delete single character at end
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 10 11)
    (setq results (cons (list 'single-end (buffer-string)) results)))
  ;; Delete single character in middle
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 5 6)
    (setq results (cons (list 'single-mid (buffer-string)) results)))
  ;; point-min and point-max
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 5)
    (delete-region (point-min) (point-max))
    (setq results (cons (list 'full (buffer-string) (point) (buffer-size)) results)))
  ;; Equal args (no-op)
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 5 5)
    (setq results (cons (list 'equal (buffer-string) (buffer-size)) results)))
  ;; Point adjustment: point inside deleted region
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 5)
    (delete-region 3 8)
    (setq results (cons (list 'point-inside (buffer-string) (point)) results)))
  ;; Point adjustment: point after deleted region
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 10)
    (delete-region 3 6)
    (setq results (cons (list 'point-after (buffer-string) (point)) results)))
  ;; Point adjustment: point before deleted region
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 2)
    (delete-region 5 9)
    (setq results (cons (list 'point-before (buffer-string) (point)) results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((reversed \"ABCHIJ\" 6) (single-start \"BCDEFGHIJ\") (single-end \"ABCDEFGHI\") (single-mid \"ABCDFGHIJ\") (full \"\" 1 0) (equal \"ABCDEFGHIJ\" 10) (point-inside \"ABHIJ\" 3) (point-after \"ABFGHIJ\" 7) (point-before \"ABCDIJ\" 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char: positive/negative COUNT and KILLP
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_delete_char_count_killp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // delete-char with positive count deletes forward, negative deletes backward.
    // KILLP non-nil means save to kill ring.
    let form = r#"(let ((results nil))
  ;; Positive count: delete 3 chars forward from point
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 4)
    (delete-char 3)
    (setq results (cons (list 'fwd-3 (buffer-string) (point)) results)))
  ;; Negative count: delete 3 chars backward from point
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 7)
    (delete-char -3)
    (setq results (cons (list 'bwd-3 (buffer-string) (point)) results)))
  ;; Delete 1 char forward
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 1)
    (delete-char 1)
    (setq results (cons (list 'fwd-1 (buffer-string) (point)) results)))
  ;; Delete 1 char backward
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 11)
    (delete-char -1)
    (setq results (cons (list 'bwd-1 (buffer-string) (point)) results)))
  ;; Delete 0 chars (no-op)
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 5)
    (delete-char 0)
    (setq results (cons (list 'zero (buffer-string) (point)) results)))
  ;; Delete all chars forward from start
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 1)
    (delete-char 10)
    (setq results (cons (list 'all-fwd (buffer-string) (point) (buffer-size)) results)))
  ;; Delete all chars backward from end
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 11)
    (delete-char -10)
    (setq results (cons (list 'all-bwd (buffer-string) (point) (buffer-size)) results)))
  ;; With KILLP (second argument non-nil)
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 4)
    (delete-char 3 t)
    (setq results (cons (list 'killp-fwd (buffer-string) (point)
                              (car kill-ring)) results)))
  ;; Backward with KILLP
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 7)
    (delete-char -3 t)
    (setq results (cons (list 'killp-bwd (buffer-string) (point)
                              (car kill-ring)) results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-3 \"ABCGHIJ\" 4) (bwd-3 \"ABCGHIJ\" 4) (fwd-1 \"BCDEFGHIJ\" 1) (bwd-1 \"ABCDEFGHI\" 10) (zero \"ABCDEFGHIJ\" 5) (all-fwd \"\" 1 0) (all-bwd \"\" 1 0) (killp-fwd \"ABCGHIJ\" 4 \"DEF\") (killp-bwd \"ABCGHIJ\" 4 \"DEF\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region: returning deleted text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_extract_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test delete-and-extract-region with various ranges,
    // verifying both the returned text and the buffer state.
    let form = r#"(let ((results nil))
  ;; Extract from beginning
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 1 4)))
      (setq results (cons (list 'begin extracted (buffer-string) (buffer-size)) results))))
  ;; Extract from middle
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 4 8)))
      (setq results (cons (list 'middle extracted (buffer-string)) results))))
  ;; Extract from end
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 8 11)))
      (setq results (cons (list 'end extracted (buffer-string)) results))))
  ;; Extract entire buffer
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 1 11)))
      (setq results (cons (list 'all extracted (buffer-string) (buffer-size)) results))))
  ;; Extract empty range
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 5 5)))
      (setq results (cons (list 'empty extracted (buffer-string)) results))))
  ;; Extract reversed args
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((extracted (delete-and-extract-region 8 3)))
      (setq results (cons (list 'reversed extracted (buffer-string)) results))))
  ;; Sequential extractions from same buffer
  (with-temp-buffer
    (insert "AAABBBCCCDDDEEE")
    (let* ((e1 (delete-and-extract-region 1 4))
           (e2 (delete-and-extract-region 1 4))
           (e3 (delete-and-extract-region 1 4)))
      (setq results (cons (list 'sequential e1 e2 e3 (buffer-string)) results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((begin \"ABC\" \"DEFGHIJ\" 7) (middle \"DEFG\" \"ABCHIJ\") (end \"HIJ\" \"ABCDEFG\") (all \"ABCDEFGHIJ\" \"\" 0) (empty \"\" \"ABCDEFGHIJ\") (reversed \"CDEFG\" \"ABHIJ\") (sequential \"AAA\" \"BBB\" \"CCC\" \"DDDEEE\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deletion interaction with markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_marker_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify marker behavior: before, inside (start/middle/end), and after deletion.
    // Also test insertion-type markers.
    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((m-before (copy-marker 2))
        (m-at-start (copy-marker 5))
        (m-inside (copy-marker 8))
        (m-at-end (copy-marker 12))
        (m-after (copy-marker 14))
        ;; Insertion-type marker (advances on insert at its position)
        (m-insert-type (copy-marker 5 t)))
    ;; Delete region [5, 12)
    (delete-region 5 12)
    (let ((result-1 (list
                     (marker-position m-before)      ;; 2 (before: unchanged)
                     (marker-position m-at-start)    ;; 5 (at start: moved to start)
                     (marker-position m-inside)      ;; 5 (inside: moved to start)
                     (marker-position m-at-end)      ;; 5 (at end: moved to start)
                     (marker-position m-after)       ;; 7 (after: shifted by 7)
                     (marker-position m-insert-type) ;; 5
                     (buffer-string))))
      ;; Now insert at position 5 to test insertion-type difference
      (goto-char 5)
      (insert "XYZ")
      (let ((result-2 (list
                       (marker-position m-at-start)    ;; 5 (stays, non-insert-type)
                       (marker-position m-insert-type) ;; 8 (advances, insert-type)
                       (marker-position m-after)       ;; 10 (shifted by 3)
                       (buffer-string))))
        ;; Cleanup markers
        (set-marker m-before nil)
        (set-marker m-at-start nil)
        (set-marker m-inside nil)
        (set-marker m-at-end nil)
        (set-marker m-after nil)
        (set-marker m-insert-type nil)
        (list result-1 result-2)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 5 5 5 7 5 \"0123BCDEF\") (5 8 10 \"0123XYZBCDEF\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deletion in narrowed buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_narrowed_buffer_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test deletion within narrowed region, at narrowed boundaries,
    // and verify full buffer state after widening.
    let form = r#"(with-temp-buffer
  (insert "HEADER__middle_content__FOOTER")
  (let ((full-before (buffer-string)))
    ;; Narrow to just the middle content (positions 9-24)
    (narrow-to-region 9 24)
    (let ((narrowed-str (buffer-string))
          (np-min (point-min))
          (np-max (point-max)))
      ;; Delete from narrowed start
      (delete-region (point-min) (+ (point-min) 3))
      (let ((after-del-start (buffer-string))
            (np-max-2 (point-max)))
        ;; Delete from narrowed end
        (delete-region (- (point-max) 3) (point-max))
        (let ((after-del-end (buffer-string))
              (np-max-3 (point-max)))
          ;; Delete from narrowed middle
          (delete-region 3 6)
          (let ((after-del-mid (buffer-string)))
            ;; Widen and verify full buffer
            (widen)
            (let ((full-after (buffer-string)))
              (list
               full-before
               narrowed-str
               np-min np-max
               after-del-start np-max-2
               after-del-end np-max-3
               after-del-mid
               full-after
               (buffer-size)))))))))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 3 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deletion with text properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that deletion properly handles propertized text:
    // remaining text retains properties, extracted text retains properties.
    let form = r#"(with-temp-buffer
  ;; Insert propertized text
  (insert (propertize "AAA" 'face 'bold))
  (insert (propertize "BBB" 'face 'italic))
  (insert (propertize "CCC" 'face 'underline))
  (insert (propertize "DDD" 'face 'bold))
  ;; Before deletion: check property boundaries
  (let ((faces-before (list
                       (get-text-property 1 'face)
                       (get-text-property 4 'face)
                       (get-text-property 7 'face)
                       (get-text-property 10 'face))))
    ;; Delete the middle part (BBB at 4-7)
    (let ((extracted (delete-and-extract-region 4 7)))
      (let ((buf-after (buffer-string))
            ;; Remaining text properties
            (faces-after (list
                          (get-text-property 1 'face)
                          (get-text-property 3 'face)
                          (get-text-property 4 'face)
                          (get-text-property 6 'face)))
            ;; Extracted text properties
            (extracted-face (get-text-property 0 'face extracted))
            (extracted-len (length extracted)))
        ;; Delete from start (propertized)
        (let ((e2 (delete-and-extract-region 1 4)))
          (let ((buf-after-2 (buffer-string))
                (e2-face (get-text-property 0 'face e2))
                ;; Properties on remaining
                (remaining-face (get-text-property 1 'face)))
            (list
             faces-before
             extracted extracted-face extracted-len
             buf-after faces-after
             e2 e2-face
             buf-after-2 remaining-face)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((bold italic underline bold) #(\"BBB\" 0 3 (face italic)) italic 3 #(\"AAACCCDDD\" 0 3 (face bold) 3 6 (face underline) 6 9 (face bold)) (bold bold underline underline) #(\"AAA\" 0 3 (face bold)) bold #(\"CCCDDD\" 0 3 (face underline) 3 6 (face bold)) underline)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer size tracking through multiple operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_size_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track buffer-size, point-min, point-max through a series of
    // insertions and deletions.
    let form = r#"(with-temp-buffer
  (let ((log nil))
    ;; Initial state
    (setq log (cons (list 'init (buffer-size) (point-min) (point-max) (point)) log))
    ;; Insert 10 chars
    (insert "ABCDEFGHIJ")
    (setq log (cons (list 'after-insert (buffer-size) (point-min) (point-max) (point)) log))
    ;; Delete 3 from start
    (delete-region 1 4)
    (setq log (cons (list 'del-3-start (buffer-size) (point-min) (point-max) (point) (buffer-string)) log))
    ;; Insert 5 in middle
    (goto-char 4)
    (insert "XXXXX")
    (setq log (cons (list 'ins-5-mid (buffer-size) (point-min) (point-max) (point) (buffer-string)) log))
    ;; Delete 2 from end
    (delete-region (- (point-max) 2) (point-max))
    (setq log (cons (list 'del-2-end (buffer-size) (point-min) (point-max) (point) (buffer-string)) log))
    ;; Narrow
    (narrow-to-region 3 8)
    (setq log (cons (list 'narrowed (buffer-size) (point-min) (point-max) (point)) log))
    ;; Delete in narrowed
    (delete-region (point-min) (+ (point-min) 2))
    (setq log (cons (list 'del-narrowed (buffer-size) (point-min) (point-max) (point) (buffer-string)) log))
    ;; Widen
    (widen)
    (setq log (cons (list 'widened (buffer-size) (point-min) (point-max) (point) (buffer-string)) log))
    (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((init 0 1 1 1) (after-insert 10 1 11 11) (del-3-start 7 1 8 8 \"DEFGHIJ\") (ins-5-mid 12 1 13 9 \"DEFXXXXXGHIJ\") (del-2-end 10 1 11 9 \"DEFXXXXXGH\") (narrowed 10 3 8 8) (del-narrowed 8 3 6 6 \"XXX\") (widened 8 1 9 6 \"DEXXXXGH\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deletion at buffer boundaries: edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_comp_boundary_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test deletion at the very start and end of buffer, single-char buffer,
    // and repeated boundary deletions.
    let form = r#"(let ((results nil))
  ;; Single character buffer: delete it
  (with-temp-buffer
    (insert "X")
    (delete-region 1 2)
    (setq results (cons (list 'single-char (buffer-string) (point) (buffer-size)) results)))
  ;; Delete first char repeatedly
  (with-temp-buffer
    (insert "ABCDE")
    (let ((chars nil))
      (dotimes (i 5)
        (let ((ch (char-after 1)))
          (delete-region 1 2)
          (setq chars (cons ch chars))))
      (setq results (cons (list 'first-char-repeat (nreverse chars) (buffer-string) (buffer-size)) results))))
  ;; Delete last char repeatedly
  (with-temp-buffer
    (insert "ABCDE")
    (let ((chars nil))
      (dotimes (i 5)
        (let ((ch (char-before (point-max))))
          (delete-region (1- (point-max)) (point-max))
          (setq chars (cons ch chars))))
      (setq results (cons (list 'last-char-repeat (nreverse chars) (buffer-string) (buffer-size)) results))))
  ;; Alternating delete from start and end
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (let ((snapshots nil))
      (dotimes (i 5)
        (if (= (mod i 2) 0)
            (delete-region 1 2)
          (delete-region (1- (point-max)) (point-max)))
        (setq snapshots (cons (buffer-string) snapshots)))
      (setq results (cons (list 'alternating (nreverse snapshots)) results))))
  ;; Delete-char at buffer boundary (should signal error)
  (with-temp-buffer
    (insert "AB")
    (goto-char 1)
    (condition-case err
        (progn (delete-char -1) 'no-error)
      (error (setq results (cons (list 'del-char-before-start (car err)) results))))
    (goto-char (point-max))
    (condition-case err
        (progn (delete-char 1) 'no-error)
      (error (setq results (cons (list 'del-char-after-end (car err)) results)))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((single-char \"\" 1 0) (first-char-repeat (65 66 67 68 69) \"\" 0) (last-char-repeat (69 68 67 66 65) \"\" 0) (alternating (\"BCDEFGHIJ\" \"BCDEFGHI\" \"CDEFGHI\" \"CDEFGH\" \"DEFGH\")) (del-char-before-start beginning-of-buffer) (del-char-after-end end-of-buffer))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
