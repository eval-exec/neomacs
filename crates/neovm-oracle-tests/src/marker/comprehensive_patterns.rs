//! Comprehensive oracle parity tests for marker operations:
//! `make-marker`, `point-marker`, `copy-marker` with insertion-type,
//! `marker-position`, `marker-buffer`, `set-marker`,
//! `marker-insertion-type`, `set-marker-insertion-type`,
//! markers surviving buffer edits, `insert-before-markers`,
//! multiple markers tracking, and markers across narrowing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-marker: unset marker properties and set-marker lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_make_and_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that make-marker creates a marker with no position and no buffer,
    // then set-marker associates it, and set-marker nil detaches it.
    let form = r#"(with-temp-buffer
  (insert "abcdefghijklmnop")
  (let ((m (make-marker)))
    (let ((before-pos (marker-position m))
          (before-buf (marker-buffer m)))
      ;; Attach to buffer at position 5
      (set-marker m 5 (current-buffer))
      (let ((mid-pos (marker-position m))
            (mid-buf (eq (marker-buffer m) (current-buffer))))
        ;; Move to position 10
        (set-marker m 10)
        (let ((moved-pos (marker-position m)))
          ;; Detach by setting to nil
          (set-marker m nil)
          (let ((after-pos (marker-position m))
                (after-buf (marker-buffer m)))
            ;; Re-attach
            (set-marker m 3 (current-buffer))
            (list before-pos before-buf
                  mid-pos mid-buf
                  moved-pos
                  after-pos after-buf
                  (marker-position m))))))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil 5 t 10 nil nil 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-marker with insertion-type parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_copy_with_insertion_type_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-marker's second arg controls insertion type of the new marker.
    // Test all combinations: source nil/t, copy-arg nil/t/omitted.
    let form = r#"(with-temp-buffer
  (insert "0123456789")
  ;; Source marker with insertion-type nil (default)
  (let ((src-nil (copy-marker 5 nil))
        (src-t   (copy-marker 5 t)))
    ;; Copy without specifying insertion-type (inherits)
    (let ((c1 (copy-marker src-nil))
          (c2 (copy-marker src-t))
          ;; Copy with explicit override
          (c3 (copy-marker src-nil t))
          (c4 (copy-marker src-t nil))
          ;; Copy from integer position
          (c5 (copy-marker 7))
          (c6 (copy-marker 7 t)))
      ;; Insert at position 5 to test movement
      (goto-char 5)
      (insert "XX")
      (list
       ;; Positions after insert at 5
       (marker-position src-nil) ;; stays at 5 (insertion-type nil)
       (marker-position src-t)   ;; moves to 7 (insertion-type t)
       (marker-position c1)      ;; inherited nil -> stays at 5
       (marker-position c2)      ;; inherited t -> moves to 7
       (marker-position c3)      ;; overridden to t -> moves to 7
       (marker-position c4)      ;; overridden to nil -> stays at 5
       (marker-position c5)      ;; from int, nil -> stays at 7? (insert was at 5, so 7 becomes 9)
       (marker-position c6)      ;; from int, t -> 9
       ;; Insertion types
       (marker-insertion-type src-nil)
       (marker-insertion-type src-t)
       (marker-insertion-type c1)
       (marker-insertion-type c2)
       (marker-insertion-type c3)
       (marker-insertion-type c4)))))"#;
    let expect = expect_test::expect![[r#""OK (5 7 5 5 7 5 9 9 nil t nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-marker-insertion-type: dynamic changes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_set_insertion_type_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Change insertion type dynamically and observe different behavior
    // on subsequent inserts at the marker's position.
    let form = r#"(with-temp-buffer
  (insert "ABCDE")
  (let ((m (copy-marker 3)))
    ;; Initially insertion-type is nil
    (let ((type-before (marker-insertion-type m)))
      ;; Insert at marker position with type nil -> marker stays
      (goto-char 3)
      (insert "x")
      (let ((pos-after-nil-insert (marker-position m)))
        ;; Now change insertion type to t
        (set-marker-insertion-type m t)
        (let ((type-after (marker-insertion-type m)))
          ;; Insert at marker position with type t -> marker advances
          (goto-char (marker-position m))
          (insert "y")
          (let ((pos-after-t-insert (marker-position m)))
            ;; Change back to nil
            (set-marker-insertion-type m nil)
            (goto-char (marker-position m))
            (insert "z")
            (list type-before
                  pos-after-nil-insert
                  type-after
                  pos-after-t-insert
                  (marker-position m)
                  (marker-insertion-type m)
                  (buffer-string))))))))"#;
    let expect = expect_test::expect![[r#""OK (nil 3 t 4 4 nil \"AByzxCDE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-before-markers vs insert: marker movement difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_insert_before_markers_vs_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-before-markers advances ALL markers at point, regardless
    // of insertion-type. Regular insert only advances markers with
    // insertion-type t.
    let form = r#"(with-temp-buffer
  (insert "ABCDE")
  (let ((m-nil (copy-marker 3 nil))
        (m-t   (copy-marker 3 t)))
    ;; Use regular insert at position 3
    (goto-char 3)
    (insert "11")
    (let ((after-insert-nil (marker-position m-nil))
          (after-insert-t   (marker-position m-t)))
      ;; Reset markers to same position
      (set-marker m-nil 6)
      (set-marker m-t   6)
      ;; Use insert-before-markers at position 6
      (goto-char 6)
      (insert-before-markers "22")
      (let ((after-ibm-nil (marker-position m-nil))
            (after-ibm-t   (marker-position m-t)))
        (list after-insert-nil after-insert-t
              after-ibm-nil after-ibm-t
              (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK (3 5 8 8 \"AB11C22DE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple markers tracking positions through complex edits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_multiple_tracking_complex_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Place markers at various positions, perform insertions, deletions,
    // and replacements, then verify all marker positions.
    let form = r#"(with-temp-buffer
  (insert "0123456789")
  (let ((m1 (copy-marker 1))
        (m2 (copy-marker 3))
        (m3 (copy-marker 5))
        (m4 (copy-marker 7))
        (m5 (copy-marker 9)))
    ;; Record initial
    (let ((init (list (marker-position m1) (marker-position m2)
                      (marker-position m3) (marker-position m4)
                      (marker-position m5))))
      ;; Delete region [3,5) -- removes chars at positions 3,4
      (delete-region 3 5)
      (let ((after-del (list (marker-position m1) (marker-position m2)
                             (marker-position m3) (marker-position m4)
                             (marker-position m5))))
        ;; Insert "XYZ" at position 2
        (goto-char 2)
        (insert "XYZ")
        (let ((after-ins (list (marker-position m1) (marker-position m2)
                               (marker-position m3) (marker-position m4)
                               (marker-position m5))))
          ;; Delete from beginning: [1,4)
          (delete-region 1 4)
          (let ((after-del2 (list (marker-position m1) (marker-position m2)
                                  (marker-position m3) (marker-position m4)
                                  (marker-position m5))))
            (list init after-del after-ins after-del2
                  (buffer-string))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5 7 9) (1 3 3 5 7) (1 6 6 8 10) (1 3 3 5 7) \"Z1456789\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Markers surviving buffer replace operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_survive_delete_insert_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate "replace" by delete-region + insert and check markers.
    let form = r#"(with-temp-buffer
  (insert "the quick brown fox jumps over the lazy dog")
  ;; Place markers at word boundaries
  (let ((m-quick (copy-marker 5))   ;; start of "quick"
        (m-brown (copy-marker 11))  ;; start of "brown"
        (m-fox   (copy-marker 17))  ;; start of "fox"
        (m-jumps (copy-marker 21))) ;; start of "jumps"
    ;; Replace "brown" (positions 11-16) with "red"
    (goto-char 11)
    (delete-region 11 16)
    (insert "red")
    (let ((after-replace1 (list (marker-position m-quick)
                                (marker-position m-brown)
                                (marker-position m-fox)
                                (marker-position m-jumps)
                                (buffer-string))))
      ;; Replace "quick" (positions 5-10) with "slow lazy"
      (goto-char 5)
      (delete-region 5 10)
      (insert "slow lazy")
      (list after-replace1
            (list (marker-position m-quick)
                  (marker-position m-brown)
                  (marker-position m-fox)
                  (marker-position m-jumps)
                  (buffer-string))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 11 15 19 \"the quick red fox jumps over the lazy dog\") (5 15 19 23 \"the slow lazy red fox jumps over the lazy dog\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Markers across buffer narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_across_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Markers exist independent of narrowing -- they track absolute positions.
    // Verify markers outside the narrowed region still report correct positions.
    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOP")
  (let ((m1 (copy-marker 3))
        (m2 (copy-marker 8))
        (m3 (copy-marker 14)))
    ;; Narrow to [5, 12)
    (narrow-to-region 5 12)
    (let ((narrowed-positions (list (marker-position m1)
                                    (marker-position m2)
                                    (marker-position m3)))
          (narrowed-pmin (point-min))
          (narrowed-pmax (point-max)))
      ;; Insert inside narrowed region
      (goto-char 8)
      (insert "xxx")
      (let ((after-insert-narrowed (list (marker-position m1)
                                          (marker-position m2)
                                          (marker-position m3))))
        ;; Widen and check
        (widen)
        (let ((after-widen (list (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3))))
          (list narrowed-positions
                narrowed-pmin narrowed-pmax
                after-insert-narrowed
                after-widen
                (buffer-string)))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 8 14) 5 12 (3 8 17) (3 8 17) \"ABCDEFGxxxHIJKLMNOP\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Marker set-marker with clamping to buffer bounds
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_position_clamping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // set-marker clamps positions to valid buffer range.
    // Also test marker-position on various edge positions.
    let form = r#"(with-temp-buffer
  (insert "hello")
  (let ((m (make-marker)))
    ;; Set to position beyond buffer end
    (set-marker m 100 (current-buffer))
    (let ((clamped-high (marker-position m)))
      ;; Set to 0 (below point-min)
      (set-marker m 0)
      (let ((clamped-low (marker-position m)))
        ;; Set to exact boundaries
        (set-marker m 1)
        (let ((at-min (marker-position m)))
          (set-marker m 6) ;; point-max for "hello" is 6
          (let ((at-max (marker-position m)))
            ;; Negative position
            (set-marker m -5)
            (let ((neg-pos (marker-position m)))
              ;; With narrowing: still clamps to buffer, not narrowed region
              (narrow-to-region 2 4)
              (set-marker m 100)
              (let ((clamped-narrow (marker-position m)))
                (widen)
                (list clamped-high clamped-low
                      at-min at-max neg-pos
                      clamped-narrow)))))))))"#;
    let expect = expect_test::expect![[r#""OK (6 1 1 6 1 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a gap-tracking structure with markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_gap_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use markers to track "gap" positions as text is edited.
    // Simulate a simple cursor-tracking system with before/after markers.
    let form = r#"(with-temp-buffer
  (insert "line one\nline two\nline three\nline four\n")
  ;; Create marker pairs (start . end) for each line
  (goto-char (point-min))
  (let ((line-markers nil))
    (while (not (eobp))
      (let ((start (point-marker)))
        (end-of-line)
        (let ((end (point-marker)))
          (setq line-markers (cons (cons start end) line-markers)))
        (when (not (eobp)) (forward-char 1))))
    (setq line-markers (nreverse line-markers))
    ;; Record initial line spans
    (let ((initial-spans
           (mapcar (lambda (pair)
                     (list (marker-position (car pair))
                           (marker-position (cdr pair))))
                   line-markers)))
      ;; Insert text at beginning of second line
      (goto-char (marker-position (car (nth 1 line-markers))))
      (insert "INSERTED: ")
      ;; Delete third line entirely
      (let ((l3-start (marker-position (car (nth 2 line-markers))))
            (l3-end (marker-position (cdr (nth 2 line-markers)))))
        (delete-region l3-start (min (1+ l3-end) (point-max))))
      ;; Record final spans
      (let ((final-spans
             (mapcar (lambda (pair)
                       (list (marker-position (car pair))
                             (marker-position (cdr pair))))
                     line-markers)))
        (list initial-spans
              final-spans
              (buffer-string))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 9) (10 18) (19 29) (30 39)) ((1 9) (10 28) (29 29) (29 38)) \"line one\\nINSERTED: line two\\nline four\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-before-markers with multiple markers at same position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_insert_before_markers_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple markers (with different insertion types) all at the same
    // position. insert-before-markers should advance them all.
    let form = r#"(with-temp-buffer
  (insert "ABCDE")
  (let ((m1 (copy-marker 3 nil))
        (m2 (copy-marker 3 nil))
        (m3 (copy-marker 3 t))
        (m4 (copy-marker 3 t)))
    ;; Regular insert at pos 3
    (goto-char 3)
    (insert "11")
    (let ((after-regular
           (list (marker-position m1) (marker-position m2)
                 (marker-position m3) (marker-position m4))))
      ;; Move all to same position again
      (set-marker m1 5)
      (set-marker m2 5)
      (set-marker m3 5)
      (set-marker m4 5)
      ;; insert-before-markers at pos 5
      (goto-char 5)
      (insert-before-markers "ZZ")
      (let ((after-ibm
             (list (marker-position m1) (marker-position m2)
                   (marker-position m3) (marker-position m4))))
        (list after-regular after-ibm (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK ((3 3 5 5) (7 7 7 7) \"AB11ZZCDE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
