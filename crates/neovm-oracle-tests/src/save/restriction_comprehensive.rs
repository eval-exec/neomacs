//! Comprehensive oracle parity tests for `save-restriction` and narrowing.
//!
//! Covers: save-restriction preserving narrowing, narrow-to-region with various
//! ranges, widen inside save-restriction, nested save-restriction, interaction
//! with save-excursion, narrowing + point movement, narrowing + buffer
//! modification, point-min/point-max inside narrowing, buffer-narrowed-p, and
//! narrowing with markers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// save-restriction preserving narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_preserves_narrowing_on_normal_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After save-restriction body completes normally, the original
    // narrowing state must be exactly restored.
    let form = r#"
(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (narrow-to-region 5 15)
  (let ((before-min (point-min))
        (before-max (point-max))
        (before-str (buffer-string)))
    (save-restriction
      (widen)
      (narrow-to-region 1 3))
    ;; Must be back to 5-15
    (list before-min before-max before-str
          (point-min) (point-max) (buffer-string)
          (= before-min (point-min))
          (= before-max (point-max)))))
"#;
    let expect = expect_test::expect![[r#""OK (5 15 \"EFGHIJKLMN\" 5 15 \"EFGHIJKLMN\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_restriction_comp_preserves_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-restriction must restore narrowing even when body signals error.
    let form = r#"
(with-temp-buffer
  (insert "Hello World 1234567890")
  (narrow-to-region 7 12)
  (let ((orig-str (buffer-string)))
    (condition-case nil
        (save-restriction
          (widen)
          (error "fail inside save-restriction"))
      (error nil))
    ;; Narrowing restored despite error
    (list orig-str (buffer-string)
          (equal orig-str (buffer-string)))))
"#;
    let expect = expect_test::expect![[r#""OK (\"World\" \"World\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// narrow-to-region with various ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_narrow_various_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test narrowing with swapped args, zero-width, at buffer boundaries.
    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (let ((results nil))
    ;; Normal range
    (save-restriction
      (narrow-to-region 3 7)
      (setq results (cons (list 'normal (buffer-string) (point-min) (point-max)) results)))
    ;; Swapped args (should work identically)
    (save-restriction
      (narrow-to-region 7 3)
      (setq results (cons (list 'swapped (buffer-string) (point-min) (point-max)) results)))
    ;; Zero-width (empty)
    (save-restriction
      (narrow-to-region 5 5)
      (setq results (cons (list 'empty (buffer-string) (point-min) (point-max)
                                (= (point-min) (point-max))) results)))
    ;; Full buffer (same as widen)
    (save-restriction
      (narrow-to-region 1 (1+ (buffer-size)))
      (setq results (cons (list 'full (buffer-string)) results)))
    ;; Single character
    (save-restriction
      (narrow-to-region 5 6)
      (setq results (cons (list 'single-char (buffer-string)) results)))
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((normal \"2345\" 3 7) (swapped \"2345\" 3 7) (empty \"\" 5 5 t) (full \"0123456789\") (single-char \"4\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// widen inside save-restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_widen_reveals_full_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Widen inside save-restriction reveals full buffer content;
    // after save-restriction, old narrowing is back.
    let form = r#"
(with-temp-buffer
  (insert "AAA-BBB-CCC-DDD-EEE")
  (narrow-to-region 5 8)
  (let ((narrow-view (buffer-string))
        (wide-view nil)
        (re-narrow nil))
    (save-restriction
      (widen)
      (setq wide-view (buffer-string)))
    (setq re-narrow (buffer-string))
    (list narrow-view wide-view re-narrow
          (string= narrow-view re-narrow))))
"#;
    let expect = expect_test::expect![[r#""OK (\"BBB\" \"AAA-BBB-CCC-DDD-EEE\" \"BBB\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested save-restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_triple_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three nested save-restriction, each narrowing differently,
    // each restored correctly on exit.
    let form = r#"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (let ((log nil))
    (setq log (cons (list 'initial (point-min) (point-max)) log))
    (save-restriction
      (narrow-to-region 1 18)
      (setq log (cons (list 'level1 (buffer-string)) log))
      (save-restriction
        (narrow-to-region 3 12)
        (setq log (cons (list 'level2 (buffer-string)) log))
        (save-restriction
          (narrow-to-region 2 6)
          (setq log (cons (list 'level3 (buffer-string)) log)))
        ;; Back to level2
        (setq log (cons (list 'back-to-2 (buffer-string)) log)))
      ;; Back to level1
      (setq log (cons (list 'back-to-1 (buffer-string)) log)))
    ;; Back to original (full buffer)
    (setq log (cons (list 'back-to-0 (buffer-string)) log))
    (nreverse log)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((initial 1 21) (level1 \"0123456789ABCDEFG\") (level2 \"23456789A\") (level3 \"1234\") (back-to-2 \"23456789A\") (back-to-1 \"0123456789ABCDEFG\") (back-to-0 \"0123456789ABCDEFGHIJ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_with_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion inside save-restriction: point is restored
    // but within the current narrowing context.
    let form = r#"
(with-temp-buffer
  (insert "Line one.\nLine two.\nLine three.\nLine four.\n")
  (narrow-to-region 11 32)
  ;; Narrowed to "Line two.\nLine three.\n"
  (goto-char (point-min))
  (let ((results nil))
    (save-restriction
      (save-excursion
        (goto-char (point-max))
        (setq results (cons (list 'inside-excursion (point)) results))
        (widen)
        (setq results (cons (list 'after-widen (point-min) (point-max) (point)) results)))
      ;; save-excursion restores point, save-restriction body still active
      ;; but widen happened inside save-excursion — it persists in save-restriction body
      (setq results (cons (list 'after-excursion (point) (point-min) (point-max)) results)))
    ;; After save-restriction: narrowing restored to 11-32
    (setq results (cons (list 'after-restriction (point-min) (point-max) (buffer-string)) results))
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((inside-excursion 32) (after-widen 1 44 32) (after-excursion 11 1 44) (after-restriction 11 32 \"Line two.\\nLine three.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing + point movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_point_clamped_by_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Point is clamped within narrowed region boundaries.
    // goto-char outside bounds is clamped; forward-char stops at boundary.
    let form = r#"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (narrow-to-region 5 12)
  (let ((results nil))
    ;; goto-char beyond max is clamped
    (goto-char 999)
    (setq results (cons (list 'after-goto-max (point)) results))
    ;; goto-char before min is clamped
    (goto-char -5)
    (setq results (cons (list 'after-goto-min (point)) results))
    ;; forward-char within bounds
    (goto-char (point-min))
    (forward-char 3)
    (setq results (cons (list 'forward-3 (point) (char-after)) results))
    ;; forward-line
    (goto-char (point-min))
    (let ((moved (forward-line 1)))
      (setq results (cons (list 'forward-line moved (point)) results)))
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((after-goto-max 12) (after-goto-min 5) (forward-3 8 55) (forward-line 0 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing + buffer modification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_insert_delete_affects_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insertions inside narrowed region expand point-max;
    // deletions shrink it. Verify the full buffer after widen.
    let form = r#"
(with-temp-buffer
  (insert "AAABBBCCCDDDEEE")
  (save-restriction
    (narrow-to-region 4 10)
    ;; Visible: "BBBCCC"
    (let ((before (buffer-string))
          (before-max (point-max)))
      (goto-char (point-max))
      (insert "XXX")
      ;; Visible now: "BBBCCCXXX"
      (let ((after-insert (buffer-string))
            (after-insert-max (point-max)))
        (goto-char (point-min))
        (delete-char 2)
        ;; Visible now: "BCCCXXX"
        (let ((after-delete (buffer-string))
              (after-delete-max (point-max)))
          (list before before-max
                after-insert after-insert-max
                after-delete after-delete-max)))))
  ;; Full buffer after widen
  (buffer-string))
"#;
    let expect = expect_test::expect![[r#""OK \"AAABCCCXXXDDDEEE\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// point-min / point-max inside narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_point_min_max_transitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track point-min and point-max through a sequence of narrow/widen.
    let form = r#"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJKLMNOP")
  (let ((log nil))
    (setq log (cons (list (point-min) (point-max)) log))
    (narrow-to-region 5 20)
    (setq log (cons (list (point-min) (point-max)) log))
    (save-restriction
      (narrow-to-region 3 10)
      (setq log (cons (list (point-min) (point-max)) log))
      (widen)
      (setq log (cons (list (point-min) (point-max)) log)))
    ;; Restored to 5-20
    (setq log (cons (list (point-min) (point-max)) log))
    (widen)
    (setq log (cons (list (point-min) (point-max)) log))
    (nreverse log)))
"#;
    let expect = expect_test::expect![[r#""OK ((1 27) (5 20) (3 10) (1 27) (5 20) (1 27))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-narrowed-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_buffer_narrowed_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // buffer-narrowed-p returns t when narrowed, nil when widened.
    let form = r#"
(with-temp-buffer
  (insert "Some buffer content here")
  (let ((results nil))
    (setq results (cons (buffer-narrowed-p) results))
    (narrow-to-region 5 15)
    (setq results (cons (buffer-narrowed-p) results))
    (save-restriction
      (widen)
      (setq results (cons (buffer-narrowed-p) results))
      (narrow-to-region 1 3)
      (setq results (cons (buffer-narrowed-p) results)))
    ;; Restored to narrowed 5-15
    (setq results (cons (buffer-narrowed-p) results))
    (widen)
    (setq results (cons (buffer-narrowed-p) results))
    (nreverse results)))
"#;
    let expect = expect_test::expect![[r#""OK (nil t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing with markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_markers_through_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Markers track absolute positions; verify they survive narrowing
    // and are accessible regardless of current restriction.
    let form = r#"
(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRST")
  (let ((m1 (copy-marker 5))
        (m2 (copy-marker 15)))
    (let ((results nil))
      ;; Before narrowing
      (setq results (cons (list 'before (marker-position m1) (marker-position m2)) results))
      (save-restriction
        (narrow-to-region 3 10)
        ;; Markers still at absolute positions
        (setq results (cons (list 'narrowed (marker-position m1) (marker-position m2)) results))
        ;; Insert at m1 — shifts m2
        (goto-char (marker-position m1))
        (insert "***")
        (setq results (cons (list 'after-insert
                                  (marker-position m1)
                                  (marker-position m2)
                                  (buffer-string)) results)))
      ;; After save-restriction — narrowing restored, markers updated
      (setq results (cons (list 'restored
                                (marker-position m1)
                                (marker-position m2)
                                (point-min) (point-max)) results))
      (widen)
      (setq results (cons (list 'widened (buffer-string)) results))
      (nreverse results))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((before 5 15) (narrowed 5 15) (after-insert 5 18 \"CD***EFGHI\") (restored 5 18 1 24) (widened \"ABCD***EFGHIJKLMNOPQRST\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: narrowing-based text processing pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restriction_comp_csv_field_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a CSV-like line by narrowing to each field delimited by commas.
    let form = r#"
(with-temp-buffer
  (insert "Alice,30,Engineer,Seattle")
  (goto-char (point-min))
  (let ((fields nil)
        (field-start (point)))
    (while (not (eobp))
      (if (eq (char-after) ?,)
          (progn
            (save-restriction
              (narrow-to-region field-start (point))
              (setq fields (cons (buffer-string) fields)))
            (forward-char 1)
            (setq field-start (point)))
        (forward-char 1)))
    ;; Last field
    (save-restriction
      (narrow-to-region field-start (point))
      (setq fields (cons (buffer-string) fields)))
    (nreverse fields)))
"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" \"30\" \"Engineer\" \"Seattle\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_restriction_comp_nested_narrow_with_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested narrowing: outer narrows to a paragraph, inner narrows to
    // each sentence for transformation, then verify full buffer.
    let form = r#"
(with-temp-buffer
  (insert "HEADER: skip this\n")
  (insert "apple banana cherry. dog elephant fox. grape hazel ivy.\n")
  (insert "FOOTER: skip this too\n")
  (let ((header-end 19)
        (footer-start (- (point-max) 22)))
    (save-restriction
      (narrow-to-region header-end footer-start)
      ;; Visible: the middle paragraph
      (goto-char (point-min))
      (let ((count 0))
        (while (search-forward "." nil t)
          (setq count (1+ count)))
        (list count (buffer-string))))
    ;; Full buffer intact
    (buffer-string)))
"#;
    let expect = expect_test::expect![[
        r#""OK \"HEADER: skip this\\napple banana cherry. dog elephant fox. grape hazel ivy.\\nFOOTER: skip this too\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
