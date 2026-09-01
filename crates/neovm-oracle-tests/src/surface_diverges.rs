//! Oracle divergence surface tests for text properties.
//!
//! Each test targets a *suspected divergence* between NeoMacs and GNU Emacs.
//! Tests that FAIL expose an active divergence; tests that PASS serve as
//! regression guards verifying parity.
//!
//! ## Previously confirmed divergence (now passing)
//!
//! - **D4** property mutations must fire buffer change hooks.
//!   GNU Emacs calls `prepare_to_modify_buffer_1` before buffer text-property
//!   changes and `signal_after_change` after mutations in `src/textprop.c`.
//!   These tests now pass and remain as regression guards.
//!
//! ## Audit corrections (tests PASS — audit was wrong)
//!
//! - **D1** Audit said NeoMacs lacks sticky inheritance. Wrong: plain `insert`
//!   doesn't inherit in GNU either. NeoMacs implements `insert-and-inherit`
//!   correctly via `apply_inherited_text_properties` (buffers.rs:2941).
//! - **D3** Audit said undo isn't recorded. Wrong: `put_buffer_text_property`
//!   (buffer.rs:3615) calls `undo_list_record_property_change` before each
//!   mutation. The audit only saw the tick-bumper function.
//! - **D5** Audit said overlays use raw positions that become stale. Wrong:
//!   `adjust_for_insert` (overlay.rs:370) explicitly shifts all overlay
//!   positions on every edit. Equivalent behavior to GNU markers.
//! - **D6** Audit said adjacent equal intervals aren't merged. True internally,
//!   but `next_property_change` (text_props.rs:574) skips equal neighbors.
//!   Observable behavior is identical.
//! - **D7** Audit said evaporate isn't handled on delete. Wrong:
//!   `adjust_for_delete` (overlay.rs:405) checks evaporate and removes
//!   zero-width overlays.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
//  D1 · Sticky property inheritance via insert-and-inherit
//
//  Plain `insert` does NOT inherit in either GNU or NeoMacs.  Only
//  `insert-and-inherit` triggers merge_properties_sticky.  NeoMacs
//  implements this via apply_inherited_text_properties (buffers.rs:2941).
//  These tests verify that sticky inheritance works correctly.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d1_insert_and_inherit_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK bold""#]];
    // insert-and-inherit into a uniformly-propertied region: the new text
    // should inherit face='bold via default rear-stickiness.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 11 'face 'bold)
  (goto-char 5)
  (insert-and-inherit "XXX")
  (get-text-property 6 'face))
"#,
        expect,
    );
}

#[test]
fn surface_d1_insert_and_inherit_front_sticky_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK bold""#]];
    // Explicit front-sticky: text inserted at the right boundary inherits.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "aaaa")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 1 5 'front-sticky t)
  (goto-char 5)
  (insert-and-inherit "bbbb")
  (get-text-property 5 'face))
"#,
        expect,
    );
}

#[test]
fn surface_d1_insert_and_inherit_rear_nonsticky_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // rear-nonsticky=t: text inserted at the right boundary should NOT
    // inherit the property.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "aaaa")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 1 5 'rear-nonsticky t)
  (goto-char 5)
  (insert-and-inherit "bbbb")
  (get-text-property 5 'face))
"#,
        expect,
    );
}

#[test]
fn surface_d1_insert_and_inherit_before_front_sticky() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK bold""#]];
    // Insert at position 1 of a front-sticky interval: the inserted text
    // should inherit because front-sticky means "text at my front gets
    // my properties."
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "aaaa")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 1 5 'front-sticky t)
  (goto-char 1)
  (insert-and-inherit "bbbb")
  (get-text-property 1 'face))
"#,
        expect,
    );
}

#[test]
fn surface_d1_plain_insert_does_not_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Verify plain insert does NOT inherit — both GNU and NeoMacs agree.
    // The audit incorrectly expected GNU's plain `insert` to inherit.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 11 'face 'bold)
  (goto-char 5)
  (insert "XXX")
  (get-text-property 6 'face))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D2 · Multibyte position correctness (parity guard)
//
//  TextPropertyTable stores character positions.  The builtin layer
//  converts between byte/char at the API boundary.  These tests verify
//  no byte/char confusion leaks through with multibyte text.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d2_multibyte_put_get_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (front front back back)""#]];
    // Greek letters (2 bytes each).  Property boundaries on char positions.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "αβγδεζηθ")
  (put-text-property 1 5 'test 'front)
  (put-text-property 5 9 'test 'back)
  (list (get-text-property 1 'test)
        (get-text-property 4 'test)
        (get-text-property 5 'test)
        (get-text-property 8 'test)))
"#,
        expect,
    );
}

#[test]
fn surface_d2_multibyte_boundary_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a a nil nil)""#]];
    // Property on chars 1-3 of multibyte text.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "αβγδε")
  (put-text-property 1 3 'marker 'a)
  (list (get-text-property 1 'marker)
        (get-text-property 2 'marker)
        (get-text-property 3 'marker)
        (get-text-property 4 'marker)))
"#,
        expect,
    );
}

#[test]
fn surface_d2_multibyte_next_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    // Boundary between two property regions in multibyte text.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "αβγδεζηθ")
  (put-text-property 1 5 'zone 'left)
  (put-text-property 5 9 'zone 'right)
  (next-single-property-change 1 'zone))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D3 · Undo recording for property changes (parity guard)
//
//  AUDIT SAID: only tick incremented, no undo entries.
//  REALITY: put_buffer_text_property (buffer.rs:3615) calls
//  undo_list_record_property_change before each mutation.  The audit
//  only saw the tick-bumper and missed the lower-level undo recording.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d3_undo_list_populated_after_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    // buffer-undo-list should be non-nil after put-text-property.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (setq buffer-undo-list nil)
  (put-text-property 1 12 'face 'bold)
  (not (null buffer-undo-list)))
"#,
        expect,
    );
}

#[test]
fn surface_d3_undo_restores_previous_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    // Set face=bold, change to italic, undo → should get bold back.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 12 'face 'bold)
  (setq buffer-undo-list nil)
  (put-text-property 1 12 'face 'italic)
  (undo)
  (get-text-property 1 'face))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D4 · signal_after_change for property mutations (regression guard)
//
//  GNU: all property mutation functions call signal_after_change after
//       modifying intervals (textprop.c).  This fires after-change-functions
//       used by font-lock, jit-lock, etc.
//  NeoMacs: the text property path mirrors this hook behavior; keep these
//           oracle cases strict so regressions stay visible.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d4_after_change_fired_on_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 12 11))""#]];
    // after-change-functions should fire when put-text-property mutates.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (let (calls)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len) calls))
              nil t)
    (put-text-property 1 12 'face 'bold)
    (nreverse calls)))
"#,
        expect,
    );
}

#[test]
fn surface_d4_after_change_fired_on_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 6 5))""#]];
    // Same for add-text-properties.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (let (calls)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len) calls))
              nil t)
    (add-text-properties 1 6 '(face bold test t))
    (nreverse calls)))
"#,
        expect,
    );
}

#[test]
fn surface_d4_after_change_fired_on_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 12 11))""#]];
    // Same for remove-text-properties.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 12 'face 'bold)
  (let (calls)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len) calls))
              nil t)
    (remove-text-properties 1 12 '(face))
    (nreverse calls)))
"#,
        expect,
    );
}

#[test]
fn surface_d4_before_change_fired_on_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 6))""#]];
    // GNU calls prepare_to_modify_buffer_1 before property mutations,
    // which runs before-change-functions.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (let (calls)
    (add-hook 'before-change-functions
              (lambda (beg end)
                (push (list beg end) calls))
              nil t)
    (set-text-properties 1 6 '(face bold))
    (nreverse calls)))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D5 · Overlay position tracking (parity guard)
//
//  AUDIT SAID: OverlayData uses raw usize, overlays become stale.
//  REALITY: adjust_for_insert (overlay.rs:370) and adjust_for_delete
//  (overlay.rs:405) explicitly shift all overlay positions on every edit.
//  Raw usize internally, but adjusted to match GNU marker behavior.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d5_overlay_start_end_track_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 12)""#]];
    // Insert text before an overlay: positions should advance.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'test 'marked)
    (goto-char 2)
    (insert "xxx")
    (list (overlay-start ov) (overlay-end ov))))
"#,
        expect,
    );
}

#[test]
fn surface_d5_overlay_property_after_insertion_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK bold""#]];
    // After inserting 3 chars at position 1, overlay at 5-9 should be at
    // 8-12.  get-char-property at 8 should find it.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'face 'bold)
    (goto-char 1)
    (insert "XXX")
    (get-char-property 8 'face)))
"#,
        expect,
    );
}

#[test]
fn surface_d5_overlay_start_end_track_deletion_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 7)""#]];
    // Delete text before an overlay: positions should retreat.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'test 'marked)
    (delete-region 1 3)
    (list (overlay-start ov) (overlay-end ov))))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D6 · Adjacent equal-property intervals (parity guard)
//
//  AUDIT SAID: intervals never merged, next-property-change wrong.
//  REALITY: intervals ARE kept separate internally, but next_property_change
//  (text_props.rs:574) skips equal neighbors.  Observable behavior matches
//  GNU which does merge internally.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d6_next_property_change_adjacent_equal_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Two adjacent regions with identical face=bold.  Both return nil
    // (no property change) — GNU via merge, NeoMacs via skip.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 6 11 'face 'bold)
  (next-property-change 1))
"#,
        expect,
    );
}

#[test]
fn surface_d6_next_property_change_three_way() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Three regions, same property.  Both return nil.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghijklmno")
  (put-text-property 1 6 'test 'x)
  (put-text-property 6 11 'test 'x)
  (put-text-property 11 16 'test 'x)
  (next-property-change 1))
"#,
        expect,
    );
}

#[test]
fn surface_d6_merge_after_overlapping_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Make adjacent intervals equal by setting same property over full range.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 6 11 'face 'italic)
  (put-text-property 1 11 'face 'bold)
  (next-property-change 1))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D7 · Overlay evaporate on text deletion (parity guard)
//
//  AUDIT SAID: evaporate only checked in overlay-put and move-overlay.
//  REALITY: adjust_for_delete (overlay.rs:405-447) checks evaporate
//  and removes zero-width overlays during deletion.  Fully implemented.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d7_overlay_evaporate_on_delete_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK evaporated""#]];
    // Delete entire content of evaporate overlay → overlay removed.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (let ((ov (make-overlay 7 12)))
    (overlay-put ov 'evaporate t)
    (delete-region 7 12)
    (if (overlay-start ov) 'alive 'evaporated)))
"#,
        expect,
    );
}

#[test]
fn surface_d7_overlay_evaporate_on_partial_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK evaporated""#]];
    // Delete part of overlay content → overlay survives.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "prefix MIDDLE suffix")
  (let ((ov (make-overlay 8 14)))
    (overlay-put ov 'evaporate t)
    (delete-region 8 14)
    (if (overlay-start ov) 'alive 'evaporated)))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D8 · Read-only stickiness check for insertions (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d8_read_only_blocks_sticky_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (text-read-only)""#]];
    // Inserting at the front of a read-only, front-sticky interval.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 6 'read-only t)
  (put-text-property 1 6 'front-sticky t)
  (condition-case err
      (progn
        (goto-char 1)
        (insert "X")
        'inserted)
    (buffer-read-only 'blocked)))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D9 · Propertied string insertion merge (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d9_insert_propertied_string_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK italic""#]];
    // Insert a propertied substring into a buffer with existing properties.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "aaaa")
  (put-text-property 1 5 'face 'bold)
  (let ((str (propertize "bbbb" 'face 'italic)))
    (goto-char 3)
    (insert str)
    (get-text-property 3 'face)))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D10 · display property storage (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d10_display_property_string_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"XXXXX\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 6 'display "XXXXX")
  (get-text-property 1 'display))
"#,
        expect,
    );
}

#[test]
fn surface_d10_display_property_space_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (space :width 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 2 'display '(space :width 10))
  (get-text-property 1 'display))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D11 · composition property (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d11_find_composition_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello")
  (find-composition 1))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D12 · line-prefix / wrap-prefix storage (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d12_line_wrap_prefix_storage() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\">> \" \"   \" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 6 'line-prefix ">> ")
  (put-text-property 1 6 'wrap-prefix "   ")
  (list (get-text-property 1 'line-prefix)
        (get-text-property 1 'wrap-prefix)
        (get-text-property 6 'line-prefix)))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D13 · Combined overlay + text property boundary (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d13_next_char_property_change_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8 13)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 6 'face 'bold)
  (let ((ov (make-overlay 3 8)))
    (overlay-put ov 'face 'italic)
    (goto-char 1)
    (insert "XX")
    (list (next-char-property-change 1)
          (next-char-property-change 5)
          (next-char-property-change 10))))
"#,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  D14 · invisible property with buffer-invisibility-spec (parity guard)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn surface_d14_invisible_property_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "visible hidden visible")
  (add-text-properties 8 14 '(invisible t))
  (setq buffer-invisibility-spec '((t . t)))
  (list (get-text-property 8 'invisible)
        (get-text-property 15 'invisible)))
"#,
        expect,
    );
}

#[test]
fn surface_d14_invisible_p_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcdefghij")
  (add-text-properties 4 7 '(invisible t))
  (setq buffer-invisibility-spec '(t))
  (list (invisible-p 3)
        (invisible-p 4)
        (invisible-p 7)))
"#,
        expect,
    );
}
