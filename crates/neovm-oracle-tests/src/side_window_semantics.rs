//! Oracle parity tests for side-window primitives.
//!
//! Covers: window-main-window, display-buffer-in-side-window,
//! window-toggle-side-windows, window-side/window-slot parameters,
//! window-sides-vertical, window-sides-slots, window-sides-reversed,
//! and edge/error cases.
//!
//! Known-divergent geometry tests remain in the default oracle suite.
//! They differ because GNU's *batch* frame geometry is internally
//! inconsistent for the menu-bar top margin (`FRAME_TOP_MARGIN`), and
//! matching it would mean reproducing those artifacts rather than a clean
//! shape:
//!
//! - The internal root node always reports `top_line = FRAME_TOP_MARGIN`
//!   (e.g. `(0 1 80 25)`), but its *horizontal* children are laid out at
//!   `top = 0` — the parent's reported edges do not contain its children.
//! - A *vertical* root combination shifts the content and the minibuffer
//!   down by the margin, placing the minibuffer at row 25 of a 25-row
//!   frame (`frame-total-lines` stays 25) — i.e. GNU's batch geometry
//!   overflows the frame.
//!
//! neomacs reports consistent, non-overflowing window-edge coordinates
//! (menu/tab-bar lines are overlaid chrome that do not reduce the window
//! text area; verified against GNU Emacs 31.0.90, frame.h
//! `FRAME_TOP_MARGIN`).  The structural and kill-buffer side-window
//! behaviour does match GNU; see the window-tree sibling model in
//! `crates/neovm-core/src/window/mod.rs`. Resolving the surfaced geometry failures as
//! GNU parity requires implementing GNU's `adjust_frame_size`
//! margin-realization (per-window `top_line` decoupled from layout +
//! vertical-root overflow).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// window-main-window
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_main_window_no_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((main (window-main-window))
       (root (frame-root-window))
       (main-is-root (eq main root)))
  (list main-is-root))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_main_window_after_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((side (display-buffer-in-side-window
              (get-buffer-create "*side-main-test*")
              '((side . left))))
       (main (window-main-window))
       (root (frame-root-window))
       (side-window (window-with-parameter 'window-side 'left)))
  (list (not (eq main root))
        (eq main (window-parent side-window))
        (numberp (window-parent side-window))))
"#;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// display-buffer-in-side-window — basic placement
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_display_buffer_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*side-left*"))
       (side (display-buffer-in-side-window
              buf
              '((side . left))))
       (edges (window-edges side))
       (side-param (window-parameter side 'window-side))
       (slot-param (window-parameter side 'window-slot))
       (dedicated (window-dedicated-p side))
       (buf-in-side (window-buffer side)))
  (list (eq side-param 'left)
        (eq buf-in-side buf)
        dedicated
        (numberp slot-param)
        (list (car edges) (cadr edges))))
"#;
    let expect = expect_test::expect![[r#""OK (t t side t (0 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_display_buffer_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*side-right*"))
       (side (display-buffer-in-side-window
              buf
              '((side . right))))
       (edges (window-edges side))
       (side-param (window-parameter side 'window-side))
       (slot-param (window-parameter side 'window-slot)))
  (list (eq side-param 'right)
        (numberp slot-param)
        (list (car edges) (cadr edges))))
"#;
    let expect = expect_test::expect![[r#""OK (t t (60 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_display_buffer_top() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*side-top*"))
       (side (display-buffer-in-side-window
              buf
              '((side . top))))
       (edges (window-edges side))
       (side-param (window-parameter side 'window-side))
       (slot-param (window-parameter side 'window-slot)))
  (list (eq side-param 'top)
        (numberp slot-param)
        (list (car edges) (cadr edges))))
"#;
    let expect = expect_test::expect![[r#""OK (t t (0 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_display_buffer_bottom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*side-bottom*"))
       (side (display-buffer-in-side-window
              buf
              '((side . bottom))))
       (edges (window-edges side))
       (side-param (window-parameter side 'window-side))
       (slot-param (window-parameter side 'window-slot)))
  (list (eq side-param 'bottom)
        (numberp slot-param)
        (list (car edges) (cadr edges))))
"#;
    let expect = expect_test::expect![[r#""OK (t t (0 19))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// display-buffer-in-side-window — error cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_invalid_side_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An invalid side should signal an error
    let form = r#"
(condition-case err
    (progn
      (display-buffer-in-side-window
       (get-buffer-create "*bad-side*")
       '((side . front)))
      (list 'no-error))
  (error (list 'error-caught (car (cdr err)))))
"#;
    let expect = expect_test::expect![[r#""OK (error-caught \"Invalid side front specified\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_invalid_slot_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(condition-case err
    (progn
      (display-buffer-in-side-window
       (get-buffer-create "*bad-slot*")
       '((side . left) (slot . "not-a-number")))
      (list 'no-error))
  (error (list 'error-caught (car (cdr err)))))
"#;
    let expect =
        expect_test::expect![[r#""OK (error-caught \"Invalid slot not-a-number specified\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window slot management — multiple windows on same side
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_multiple_slots_same_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf0 (get-buffer-create "*slot-0*"))
       (buf1 (get-buffer-create "*slot-1*"))
       (buf2 (get-buffer-create "*slot-2*"))
       (w0 (display-buffer-in-side-window buf0 '((side . left) (slot . 0))))
       (w1 (display-buffer-in-side-window buf1 '((side . left) (slot . 1))))
       (w2 (display-buffer-in-side-window buf2 '((side . left) (slot . -1))))
       (slot0 (window-parameter w0 'window-slot))
       (slot1 (window-parameter w1 'window-slot))
       (slot2 (window-parameter w2 'window-slot)))
  (list (numberp slot0)
        (numberp slot1)
        (numberp slot2)
        ;; Each window should have the same side
        (eq (window-parameter w0 'window-side) 'left)
        (eq (window-parameter w1 'window-side) 'left)
        (eq (window-parameter w2 'window-side) 'left)
        ;; The windows should be distinct
        (not (eq w0 w1))
        (not (eq w1 w2))
        (not (eq w0 w2))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_slot_reuse_same_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-a (get-buffer-create "*reuse-a*"))
       (buf-b (get-buffer-create "*reuse-b*"))
       (w-a (display-buffer-in-side-window buf-a '((side . right) (slot . 2))))
       (w-b (display-buffer-in-side-window buf-b '((side . right) (slot . 2))))
       (buf-a-after (window-buffer w-a))
       (buf-b-after (window-buffer w-b)))
  (list (eq w-a w-b)
        (eq buf-b-after buf-b)))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window dedication
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_dedicated_by_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*dedicated-side*"))
       (side (display-buffer-in-side-window
              buf
              '((side . right))))
       (dedicated (window-dedicated-p side)))
  (list dedicated))
"#;
    let expect = expect_test::expect![[r#""OK (side)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_dedicated_explicit_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*side-dedicated-nil*"))
       (side (display-buffer-in-side-window
              buf
              '((side . bottom) (dedicated . nil)))))
  (list (window-dedicated-p side)))
"#;
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-toggle-side-windows
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_toggle_no_side_windows_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(condition-case err
    (progn
      (window-toggle-side-windows)
      (list 'no-error))
  (error (list 'error-caught (car (cdr err)))))
"#;
    let expect = expect_test::expect![[r#""OK (error-caught \"No side windows state found\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_toggle_after_create_deletes_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*toggle-test*"))
       (_side (display-buffer-in-side-window
               buf
               '((side . left))))
       (_toggle (window-toggle-side-windows))
       (any-side-left (window-with-parameter 'window-side 'left))
       (any-side-right (window-with-parameter 'window-side 'right))
       (any-side-top (window-with-parameter 'window-side 'top))
       (any-side-bottom (window-with-parameter 'window-side 'bottom)))
  (list any-side-left any-side-right any-side-top any-side-bottom))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-sides-vertical effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_sides_vertical_left_occupies_full_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-vertical t)
       (buf (get-buffer-create "*vert-left*"))
       (side (display-buffer-in-side-window
              buf
              '((side . left))))
       (edges (window-edges side))
       (root (frame-root-window))
       (root-edges (window-edges root))
       (height (- (nth 3 edges) (nth 1 edges)))
       (root-height (- (nth 3 root-edges) (nth 1 root-edges))))
  (list (= height root-height)))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_sides_vertical_right_occupies_full_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-vertical t)
       (buf (get-buffer-create "*vert-right*"))
       (side (display-buffer-in-side-window
              buf
              '((side . right))))
       (edges (window-edges side))
       (root (frame-root-window))
       (root-edges (window-edges root))
       (height (- (nth 3 edges) (nth 1 edges)))
       (root-height (- (nth 3 root-edges) (nth 1 root-edges))))
  (list (= height root-height)))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window deletion
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_delete_side_window_removes_it() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*delete-side-test*"))
       (side (display-buffer-in-side-window
              buf
              '((side . bottom))))
       (side-existed (window-live-p side))
       (_deleted (delete-window side))
       (side-gone (not (window-live-p side)))
       (still-has-side (window-with-parameter 'window-side 'bottom)))
  (list side-existed side-gone (not still-has-side)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window--sides-shown buffer-local variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_sides_shown_set_on_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sides-shown-test*"))
       (_side (display-buffer-in-side-window
               buf
               '((side . left))))
       (shown (buffer-local-value 'window--sides-shown buf)))
  (list shown))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window with explicit window-width/window-height
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_explicit_width_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*explicit-width*"))
       (side (display-buffer-in-side-window
              buf
              '((side . left) (window-width . 25))))
       (edges (window-edges side)))
  (list (nth 2 edges)))
"#;
    let expect = expect_test::expect![[r#""OK (25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_explicit_height_top() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*explicit-height*"))
       (side (display-buffer-in-side-window
              buf
              '((side . top) (window-height . 5))))
       (edges (window-edges side)))
  (list (nth 3 edges)))
"#;
    let expect = expect_test::expect![[r#""OK (6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-sides-slots — limiting number of slots
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_sides_slots_zero_prevents_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-slots '(0 0 0 0))
       (result (display-buffer-in-side-window
                (get-buffer-create "*slot-zero*")
                '((side . left)))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side windows and split-window interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_split_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*split-side*"))
       (side (display-buffer-in-side-window
              buf
              '((side . left))))
       (child (condition-case err
                  (split-window side)
                (error (list 'split-failed (car (cdr err)))))))
  (list (if (windowp child)
            (list (window-parameter side 'window-side)
                  (window-parameter child 'window-side))
          child)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((split-failed \"Cannot split side window or parent of side window\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side windows and other-window / select-window interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_other_window_skips_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((main (selected-window))
       (side-buf (get-buffer-create "*other-win-side*"))
       (side (display-buffer-in-side-window
              side-buf
              '((side . left))))
       ;; Try selecting the side window explicitly, then use other-window
       (_ (select-window side))
       (selected-after-side-select (selected-window))
       (other (other-window-for-scrolling))
       (other-is-side (window-parameter other 'window-side)))
  (list (eq selected-after-side-select side)
        (window-parameter other 'window-side)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window frame parameter persistence
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_window_state_after_toggle_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*state-restore*"))
       (_side (display-buffer-in-side-window
               buf
               '((side . right) (slot . 0))))
       (before-toggle (window-with-parameter 'window-side 'right))
       (had-side-before (and before-toggle t))
       (_toggle (window-toggle-side-windows))
       (after-toggle-gone (window-with-parameter 'window-side 'right))
       (_restore (window-toggle-side-windows))
       (after-restore (window-with-parameter 'window-side 'right)))
  (list had-side-before
        (not after-toggle-gone)
        (and after-restore t)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side windows on all four sides simultaneously
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_all_four_sides() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-l (get-buffer-create "*all-l*"))
       (buf-r (get-buffer-create "*all-r*"))
       (buf-t (get-buffer-create "*all-t*"))
       (buf-b (get-buffer-create "*all-b*"))
       (wl (display-buffer-in-side-window buf-l '((side . left))))
       (wr (display-buffer-in-side-window buf-r '((side . right))))
       (wt (display-buffer-in-side-window buf-t '((side . top))))
       (wb (display-buffer-in-side-window buf-b '((side . bottom))))
       (main-w (window-main-window))
       (side-windows (list wl wr wt wb)))
  (list (every 'window-live-p side-windows)
        (not (memq main-w side-windows))
        (eq (window-parameter wl 'window-side) 'left)
        (eq (window-parameter wr 'window-side) 'right)
        (eq (window-parameter wt 'window-side) 'top)
        (eq (window-parameter wb 'window-side) 'bottom)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window default side (bottom when not specified)
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_default_side_is_bottom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*default-side*"))
       (side (display-buffer-in-side-window
              buf
              '()))
       (side-param (window-parameter side 'window-side)))
  (list (eq side-param 'bottom)))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window persistent parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_parameters_are_persistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*persist-params*"))
       (side (display-buffer-in-side-window
              buf
              '((side . left))))
       (side-entry (assq 'window-side window-persistent-parameters))
       (slot-entry (assq 'window-slot window-persistent-parameters)))
  (list (not (null side-entry))
        (not (null slot-entry))
        (eq (cdr side-entry) 'writable)
        (eq (cdr slot-entry) 'writable)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// side window: same buffer in a side window is reused
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_same_buffer_same_side_reuses() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*reuse-buf*"))
       (w1 (display-buffer-in-side-window buf '((side . left) (slot . 0))))
       (w2 (display-buffer buf))
       (w2-is-w1 (eq w1 w2)))
  (list w2-is-w1))
"#;
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window--sides-check integrity after multi-side creation
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_sides_check_no_crash_after_all_sides() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-l (get-buffer-create "*check-l*"))
       (buf-r (get-buffer-create "*check-r*"))
       (buf-t (get-buffer-create "*check-t*"))
       (buf-b (get-buffer-create "*check-b*"))
       (_wl (display-buffer-in-side-window buf-l '((side . left))))
       (_wr (display-buffer-in-side-window buf-r '((side . right))))
       (_wt (display-buffer-in-side-window buf-t '((side . top))))
       (_wb (display-buffer-in-side-window buf-b '((side . bottom))))
       ;; window--sides-check should not error
       (result (condition-case err
                   (progn
                     (window--sides-check (selected-frame))
                     'ok)
                 (error (list 'check-failed (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK (ok)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Combo tests: side windows + complex interactions
// ===========================================================================

// ---------------------------------------------------------------------------
// Combo: side windows + quit-restore (window-prev-buffers / window-next-buffers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_quit_restore_prev_next_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-a (get-buffer-create "*sw-qr-a*"))
       (buf-b (get-buffer-create "*sw-qr-b*"))
       (sw (display-buffer-in-side-window buf-a '((side . left) (slot . 0))))
       (_ (display-buffer-in-side-window buf-b '((side . left) (slot . 0))))
       ;; buf-b replaced buf-a; window-prev-buffers should list buf-a
       (prev (window-prev-buffers sw))
       (next (window-next-buffers sw))
       (quit-restore (window-parameter sw 'quit-restore)))
  (list (not (null prev))
        (null next)
        (not (null quit-restore))
        (eq (car (car prev)) buf-a)
        ;; quit-restore should indicate this is a side window
        (eq (nth 0 quit-restore) 'window)
        (eq (nth 1 quit-restore) sw)
        (eq (nth 2 quit-restore) buf-b)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + delete-other-windows preserves main window
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_delete_other_windows_preserves_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-del-other*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (main (window-main-window))
       (_ (select-window main))
       (_ (delete-other-windows main))
       (sw-still-alive (window-live-p sw))
       (side-remains (window-with-parameter 'window-side 'left)))
  (list sw-still-alive
        (and side-remains t)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + buffer kill → window behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_buffer_kill_window_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-kill-buf*"))
       (sw (display-buffer-in-side-window buf '((side . right))))
       (was-live (window-live-p sw))
       (_ (kill-buffer buf))
       (still-live (window-live-p sw)))
  (list was-live still-live))
"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + switch-to-buffer vs dedicated
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_switch_to_buffer_dedicated_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-switch-1*"))
       (buf2 (get-buffer-create "*sw-switch-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       (selected-before (selected-window))
       (_ (select-window sw))
       ;; Try switching buffer in dedicated side window
       (result (condition-case err
                   (progn
                     (switch-to-buffer buf2)
                     (list 'switched (eq (current-buffer) buf2)))
                 (error (list 'cannot-switch (car (cdr err)))))))
  (list result
        (window-parameter sw 'window-side)
        (window-dedicated-p sw)))
"#;
    let expect = expect_test::expect![[r#""OK ((switched t) left nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + resize + re-split
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_resize_and_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-resize*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . left) (window-width . 15))))
       (orig-width (window-total-width sw))
       ;; Now try splitting the side window
       (child (condition-case err
                  (split-window sw 5 'below)
                (error (list 'split-err (car (cdr err))))))
       (after-width (window-total-width sw)))
  (list orig-width
        after-width
        (if (windowp child)
            (list 'child-created
                  (window-total-width child)
                  (window-parameter child 'window-side)
                  (window-parameter child 'window-slot))
          child)))
"#;
    let expect = expect_test::expect![[
        r#""OK (15 15 (split-err \"Cannot split side window or parent of side window\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + balance-windows
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_balance_windows_ignores_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-balance*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (main (window-main-window))
       ;; Split main window
       (_ (select-window main))
       (lower (split-window main nil 'below))
       (widths-before (list (window-total-width sw)
                            (window-total-width main)
                            (window-total-width lower)))
       (_balance (balance-windows))
       (widths-after (list (window-total-width sw)
                           (window-total-width main)
                           (window-total-width lower))))
  (list (equal widths-before widths-after)
        widths-before
        widths-after))
"#;
    let expect = expect_test::expect![[r#""OK (nil (20 60 60) (40 40 40))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + window-state-get/put round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_window_state_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-state-round*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . left) (slot . 0))))
       ;; Save state of the entire frame
       (state (window-state-get (frame-root-window) t))
       (side-before (window-parameter sw 'window-side))
       (slot-before (window-parameter sw 'window-slot))
       (buf-before (window-buffer sw))
       ;; Delete all windows and restore
       (_ (delete-other-windows (window-main-window)))
       (_ (window-state-put state (frame-root-window) 'safe))
       ;; Find the side window again
       (sw-after (window-with-parameter 'window-side 'left))
       (side-after (and sw-after (window-parameter sw-after 'window-side)))
       (slot-after (and sw-after (window-parameter sw-after 'window-slot)))
       (buf-after (and sw-after (window-buffer sw-after))))
  (list side-before slot-before
        side-after slot-after
        (eq buf-before buf-after)))
"#;
    let expect = expect_test::expect![[r#""OK (left 0 left 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + narrow + widen in side window buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_narrow_widen_in_side_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-narrow*"))
       (_ (with-current-buffer buf
            (insert "line one\nline two\nline three\nline four\n")))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (total-lines-before (count-lines (point-min) (point-max)))
       ;; Narrow in the side window's buffer
       (_ (with-current-buffer buf
            (narrow-to-region 10 25)))
       (narrow-start (point-min))
       (narrow-end (point-max))
       (_ (widen))
       (total-lines-after (count-lines (point-min) (point-max))))
  (list total-lines-before
        narrow-start narrow-end
        (= total-lines-before total-lines-after)))
"#;
    let expect = expect_test::expect![[r#""OK (0 1 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + window-configuration-change-hook
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_config_change_hook_fires() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((hook-fired nil))
  (let* ((buf (get-buffer-create "*sw-hook*"))
         (_add (add-hook 'window-configuration-change-hook
                         (lambda () (setq hook-fired t))))
         (sw (display-buffer-in-side-window buf '((side . left)))))
    (list hook-fired
          (window-parameter sw 'window-side))))
"#;
    let expect = expect_test::expect![[r#""OK (nil left)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + fit-window-to-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_fit_window_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-fit*"))
       (_ (with-current-buffer buf
            (insert "short\n")))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (height-before (window-total-height sw))
       (_fit (condition-case err
                 (fit-window-to-buffer sw)
               (error (list 'fit-err (car (cdr err))))))
       (height-after (window-total-height sw)))
  (list height-before height-after
        (< height-after height-before)))
"#;
    let expect = expect_test::expect![[r#""OK (6 4 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + replace-buffer-in-windows
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_replace_buffer_in_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-replace-1*"))
       (buf2 (get-buffer-create "*sw-replace-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       (_replace (replace-buffer-in-windows buf1 buf2))
       (buf-in-sw (window-buffer sw)))
  (list (eq buf-in-sw buf2)
        (window-parameter sw 'window-side)))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + set-window-dedicated-p change
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_change_dedication_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-ded-change*"))
       (sw (display-buffer-in-side-window buf '((side . top))))
       (ded-before (window-dedicated-p sw))
       (_ (set-window-dedicated-p sw 'direct))
       (ded-after (window-dedicated-p sw)))
  (list ded-before ded-after))
"#;
    let expect = expect_test::expect![[r#""OK (side direct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + other-window (not for-scrolling) cycles main only
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_other_window_cycle_skips_sides() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-l (get-buffer-create "*sw-ow-l*"))
       (buf-r (get-buffer-create "*sw-ow-r*"))
       (wl (display-buffer-in-side-window buf-l '((side . left))))
       (wr (display-buffer-in-side-window buf-r '((side . right))))
       (main (window-main-window))
       ;; Split main to have multiple non-side windows
       (_ (select-window main))
       (lower (split-window main nil 'below))
       ;; Now cycle: from main, call other-window twice
       ;; Should visit only main-area windows, not side windows
       (_ (select-window main))
       (w1 (selected-window))
       (_ (other-window 1))
       (w2 (selected-window))
       (_ (other-window 1))
       (w3 (selected-window))
       (w1-side (window-parameter w1 'window-side))
       (w2-side (window-parameter w2 'window-side))
       (w3-side (window-parameter w3 'window-side)))
  (list w1-side w2-side w3-side
        (eq w1 w3)  ;; should cycle back to start
        (not (eq w1 w2))))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil right nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side window + minibuffer interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_combo_minibuffer_window_never_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((mini (minibuffer-window))
       (mini-side (window-parameter mini 'window-side))
       (buf (get-buffer-create "*sw-mini*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (mini-still (minibuffer-window))
       (mini-side-after (window-parameter mini-still 'window-side)))
  (list mini-side mini-side-after
        (not (eq sw mini-still))))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combo: side windows + window-resize (shrink/enlarge)
// ---------------------------------------------------------------------------

// ===========================================================================
// Deep probes: y-offset-1 divergence investigation
// ===========================================================================
fn oracle_side_window_deep_mode_line_height_effect_on_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-offset*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (edges (window-edges sw))
       (pixel-edges (window-pixel-edges sw))
       (root-edges (window-edges (frame-root-window)))
       (mode-line-h (window-mode-line-height sw))
       (header-line-h (window-header-line-height sw))
       (tab-line-h (window-tab-line-height sw))
       (top-line (window-top-line sw))
       (pixel-top (window-pixel-top sw)))
  (list (nth 0 edges) (nth 1 edges) (nth 2 edges) (nth 3 edges)
        mode-line-h header-line-h tab-line-h
        top-line pixel-top))
"#;
    let expect = expect_test::expect![[r#""OK (0 7 20 25 1 0 0 7 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_all_four_sides_edges_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((bl (get-buffer-create "*edges-l*"))
       (br (get-buffer-create "*edges-r*"))
       (bt (get-buffer-create "*edges-t*"))
       (bb (get-buffer-create "*edges-b*"))
       (wl (display-buffer-in-side-window bl '((side . left))))
       (wr (display-buffer-in-side-window br '((side . right))))
       (wt (display-buffer-in-side-window bt '((side . top))))
       (wb (display-buffer-in-side-window bb '((side . bottom))))
       (root (frame-root-window))
       (root-edges (window-edges root))
       (l-edges (window-edges wl))
       (r-edges (window-edges wr))
       (t-edges (window-edges wt))
       (b-edges (window-edges wb)))
  (list l-edges r-edges t-edges b-edges root-edges))
"#;
    let expect = expect_test::expect![[
        r#""OK ((0 7 20 19) (60 7 80 19) (0 1 80 7) (0 19 80 25) (0 1 80 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_minibuffer_window_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((mini (minibuffer-window))
       (mini-edges (window-edges mini))
       (root (frame-root-window))
       (root-edges (window-edges root))
       (frame-edges (window-edges (frame-root-window) t))
       (buf (get-buffer-create "*sw-miniedge*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (sw-edges (window-edges sw))
       (mini-edges-after (window-edges mini)))
  (list mini-edges root-edges sw-edges mini-edges-after))
"#;
    let expect =
        expect_test::expect![[r#""OK ((0 24 80 25) (0 0 80 24) (0 0 20 24) (0 24 80 25))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Deep probes: dedication divergence investigation
// ===========================================================================

#[test]
fn oracle_side_window_deep_display_buffer_mark_dedicated_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((val display-buffer-mark-dedicated)
       (buf (get-buffer-create "*sw-ded-mark*"))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (ded (window-dedicated-p sw)))
  (list val ded))
"#;
    let expect = expect_test::expect![[r#""OK (nil side)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_dedicated_explicit_soft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-ded-soft*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . left) (dedicated . soft)))))
  (list (window-dedicated-p sw)))
"#;
    let expect = expect_test::expect![[r#""OK (soft)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_dedicated_blocks_switch_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-ded-block-1*"))
       (buf2 (get-buffer-create "*sw-ded-block-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       (_ (select-window sw))
       (result (condition-case err
                   (progn
                     (switch-to-buffer buf2 'norecord)
                     (list 'ok (eq (current-buffer) buf2)
                           (eq (window-buffer sw) buf2)))
                 (error (list 'blocked (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK ((ok t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Deep probes: kill-buffer lifecycle divergence investigation
// ===========================================================================

#[test]
fn oracle_side_window_deep_bury_buffer_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-bury-1*"))
       (buf2 (get-buffer-create "*sw-bury-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       (_ (display-buffer-in-side-window buf2 '((side . left) (slot . 0))))
       (_ (bury-buffer buf2))
       (buf-in-sw (window-buffer sw))
       (prev (window-prev-buffers sw))
       (next (window-next-buffers sw)))
  (list buf-in-sw
        (length prev)
        (length next)))
"#;
    let expect = expect_test::expect![[r#""OK (#<buffer *sw-bury-2*> 1 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_switch_to_prev_buffer_after_bury() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-a (get-buffer-create "*sw-spb-a*"))
       (buf-b (get-buffer-create "*sw-spb-b*"))
       (buf-c (get-buffer-create "*sw-spb-c*"))
       (sw (display-buffer-in-side-window buf-a '((side . right))))
       (_ (display-buffer-in-side-window buf-b '((side . right) (slot . 0))))
       (_ (display-buffer-in-side-window buf-c '((side . right) (slot . 0))))
       (_ (select-window sw))
       (_ (switch-to-prev-buffer sw))
       (buf-after-spb (window-buffer sw))
       (_ (switch-to-next-buffer sw))
       (buf-after-snb (window-buffer sw)))
  (list (eq buf-after-spb buf-b)
        (eq buf-after-snb buf-c)))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Deep probes: window-configuration-change-hook investigation
// ===========================================================================

#[test]
fn oracle_side_window_deep_config_change_hook_delete_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((hook-fired 0))
  (let* ((buf (get-buffer-create "*sw-hook-del*"))
         (_add (add-hook 'window-configuration-change-hook
                         (lambda () (setq hook-fired (1+ hook-fired)))))
         (sw (display-buffer-in-side-window buf '((side . left))))
         (count-after-create hook-fired)
         (_del (delete-window sw))
         (count-after-delete hook-fired))
    (list count-after-create count-after-delete)))
"#;
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_config_change_hook_toggle_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((hook-count 0))
  (let* ((buf (get-buffer-create "*sw-hook-tog*"))
         (_add (add-hook 'window-configuration-change-hook
                         (lambda () (setq hook-count (1+ hook-count)))))
         (_ (display-buffer-in-side-window buf '((side . bottom))))
         (count-after-create hook-count)
         (_ (window-toggle-side-windows))
         (count-after-toggle1 hook-count)
         (_ (window-toggle-side-windows))
         (count-after-toggle2 hook-count))
    (list count-after-create count-after-toggle1 count-after-toggle2)))
"#;
    let expect = expect_test::expect![[r#""OK (0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Deep probes: window-sides-vertical investigation
// ===========================================================================

#[test]
fn oracle_side_window_deep_sides_vertical_vs_horizontal_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-vertical t)
       (buf-l (get-buffer-create "*sw-dim-l*"))
       (buf-r (get-buffer-create "*sw-dim-r*"))
       (wl (display-buffer-in-side-window buf-l '((side . left) (window-width . 20))))
       (wr (display-buffer-in-side-window buf-r '((side . right) (window-width . 20))))
       (l-size (window-total-size wl))
       (r-size (window-total-size wr))
       (l-h-size (window-total-size wl t))
       (r-h-size (window-total-size wr t))
       (l-pixel (window-pixel-width wl))
       (r-pixel (window-pixel-width wr))
       (l-body (list (window-body-width wl) (window-body-height wl))))
  (list l-size r-size l-h-size r-h-size l-pixel r-pixel l-body))
"#;
    let expect = expect_test::expect![[r#""OK (24 24 20 20 20 20 (19 23))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_sides_vertical_nil_left_not_full_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-vertical nil)
       (buf (get-buffer-create "*sw-vert-nil*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (sw-edges (window-edges sw))
       (root-edges (window-edges (frame-root-window)))
       (sw-bottom (nth 3 sw-edges))
       (root-bottom (nth 3 root-edges)))
  (list (= sw-bottom root-bottom)
        sw-bottom root-bottom))
"#;
    let expect = expect_test::expect![[r#""OK (nil 24 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// New territory: functions not yet tested
// ===========================================================================

#[test]
fn oracle_side_window_get_buffer_window_returns_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-gbw*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (found (get-buffer-window buf))
       (found-all (get-buffer-window buf 'all-frames)))
  (list (eq found sw)
        (eq found-all sw)))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_walk_windows_filters_with_side_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf-l (get-buffer-create "*sw-walk-l*"))
       (buf-r (get-buffer-create "*sw-walk-r*"))
       (wl (display-buffer-in-side-window buf-l '((side . left))))
       (wr (display-buffer-in-side-window buf-r '((side . right))))
       (all-windows 0)
       (side-windows 0)
       (main-windows 0))
  (walk-windows (lambda (w)
                  (setq all-windows (1+ all-windows))
                  (if (window-parameter w 'window-side)
                      (setq side-windows (1+ side-windows))
                    (setq main-windows (1+ main-windows))))
                'nominibuf)
  (list all-windows side-windows main-windows
        (> all-windows 0)))
"#;
    let expect = expect_test::expect![[r#""OK (3 2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_delete_other_windows_from_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-del-other-from-side*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (_ (select-window sw))
       (result (condition-case err
                   (progn
                     (delete-other-windows sw)
                     (list 'ok
                           (window-live-p sw)
                           (length (window-list nil 'nominibuf))))
                 (error (list 'error (car (cdr err)))))))
  (list result))
"#;
    let expect =
        expect_test::expect![[r#""OK ((error \"Cannot make side window the only window\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_set_window_buffer_directly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-setbuf-1*"))
       (buf2 (get-buffer-create "*sw-setbuf-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . right))))
       (ded-before (window-dedicated-p sw))
       (_ (set-window-buffer sw buf2))
       (buf-now (window-buffer sw))
       (side-still (window-parameter sw 'window-side))
       (ded-after (window-dedicated-p sw)))
  (list (eq buf-now buf2)
        (eq side-still 'right)
        ded-before ded-after))
"#;
    let expect = expect_test::expect![[r#""OK (t t side nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_display_buffer_reuse_window_over_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-reuse-ovr-1*"))
       (buf2 (get-buffer-create "*sw-reuse-ovr-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       ;; Now use plain display-buffer for buf2 - should it reuse the side window?
       ;; display-buffer-reuse-window says no (side windows are dedicated)
       (result (condition-case err
                   (let ((w2 (display-buffer buf2)))
                     (list 'got-window
                           (eq w2 sw)
                           (window-parameter w2 'window-side)))
                 (error (list 'err (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK ((got-window nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_scroll_bars_fringes_margins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-scroll*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (scroll-bars (window-scroll-bars sw))
       (fringes (window-fringes sw))
       (margins (window-margins sw)))
  (list scroll-bars fringes margins))
"#;
    let expect = expect_test::expect![[r#""OK ((nil 0 t nil 0 t nil) (0 0 nil nil) (nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_split_with_explicit_side_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((main (window-main-window))
       (_ (select-window main))
       (sw (split-window main nil 'right))
       (side-param (window-parameter sw 'window-side)))
  (list side-param
        (window-parameter sw 'window-slot)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_set_window_parameter_side_manually() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((main (window-main-window))
       (orig-side (window-parameter main 'window-side))
       (_ (set-window-parameter main 'window-side 'left))
       (after-set (window-parameter main 'window-side))
       (main-now (window-main-window))
       (_cleanup (set-window-parameter main 'window-side nil))
       (after-cleanup (window-parameter main 'window-side)))
  (list orig-side after-set main-now after-cleanup))
"#;
    let expect = expect_test::expect![[r#""OK (nil left #<window 1 on *scratch*> nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_window_list_with_different_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-wl*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (all-no-mini (window-list nil 'nominibuf))
       (all-with-mini (window-list nil 'nominibuf nil))
       (has-side (memq sw all-no-mini))
       (count (length all-no-mini)))
  (list (and has-side t)
        count
        (= (length all-no-mini) (length all-with-mini))))
"#;
    let expect = expect_test::expect![[r#""OK (t 2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_display_buffer_in_side_window_with_extra_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-extra-params*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . bottom)
              (window-parameters (no-other-window . t)
                                 (modeline . nil)))))
       (side (window-parameter sw 'window-side))
       (slot (window-parameter sw 'window-slot))
       (no-other (window-parameter sw 'no-other-window))
       (modeline (window-parameter sw 'modeline)))
  (list (eq side 'bottom)
        (numberp slot)
        no-other
        modeline))
"#;
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// More deep probes: dedication blocking, atom, delete-main, use-time
// ===========================================================================

#[test]
fn oracle_side_window_deep_set_window_buffer_on_dedicated_nil_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-setbuf-nil-1*"))
       (buf2 (get-buffer-create "*sw-setbuf-nil-2*"))
       (sw (display-buffer-in-side-window
            buf1
            '((side . left) (dedicated . nil))))
       (_ (set-window-buffer sw buf2))
       (buf-now (window-buffer sw)))
  (list (eq buf-now buf2)
        (window-dedicated-p sw)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_display_buffer_reuse_window_with_side_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-reuse-alist-1*"))
       (buf2 (get-buffer-create "*sw-reuse-alist-2*"))
       (_ (display-buffer-in-side-window buf1 '((side . left))))
       (result (condition-case err
                   (let ((w (display-buffer
                             buf2
                             '((display-buffer-reuse-window
                                display-buffer-in-side-window)
                               (side . left)))))
                     (list 'got-window
                           (eq (window-buffer w) buf2)
                           (window-parameter w 'window-side)
                           (window-dedicated-p w)))
                 (error (list 'err (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK ((got-window t left side))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_atom_root_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-atom*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (atom-root (window-atom-root sw)))
  (list (eq atom-root sw)
        (window-parameter sw 'window-atom)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_delete_main_window_with_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-del-main*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (main (window-main-window))
       (result (condition-case err
                   (progn
                     (select-window main)
                     (delete-window main)
                     (list 'after-delete
                           (window-live-p sw)
                           (length (window-list nil 'nominibuf))))
                 (error (list 'delete-err (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[
        r#""OK ((delete-err \"Attempt to delete main window of frame #<frame F1 0xADDR>\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_use_time_side_vs_main() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-use-time*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (main (window-main-window))
       (sw-time (window-use-time sw))
       (main-time (window-use-time main)))
  (list (numberp sw-time)
        (numberp main-time)
        (>= sw-time main-time)))
"#;
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_resize_with_side_window_present() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-resize-main*"))
       (sw (display-buffer-in-side-window buf '((side . right) (window-width . 25))))
       (main (window-main-window))
       (main-width-before (window-total-width main))
       (_ (select-window main))
       (_enlarge (condition-case err
                     (enlarge-window 2 t)
                   (error (list 'enlarge-err (car (cdr err))))))
       (main-width-after (window-total-width main))
       (sw-width-after (window-total-width sw)))
  (list main-width-before main-width-after sw-width-after))
"#;
    let expect = expect_test::expect![[r#""OK (55 57 23)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_set_window_start_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-start*"))
       (_ (with-current-buffer buf
            (insert "line 1\nline 2\nline 3\nline 4\nline 5\n"
                    "line 6\nline 7\nline 8\nline 9\nline 10\n")))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (start-before (window-start sw))
       (_ (set-window-start sw 10))
       (start-after (window-start sw))
       (point-at (window-point sw)))
  (list start-before start-after point-at))
"#;
    let expect = expect_test::expect![[r#""OK (1 10 72)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_split_window_below_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-split-below*"))
       (sw (display-buffer-in-side-window buf '((side . top))))
       (sw-height-before (window-total-height sw))
       (result (condition-case err
                   (let ((lower (split-window sw nil 'below)))
                     (list 'split-ok
                           (window-parameter lower 'window-side)
                           (window-parameter lower 'window-slot)
                           (window-total-height sw)
                           (window-total-height lower)))
                 (error (list 'split-err (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[
        r#""OK ((split-err \"Cannot split side window or parent of side window\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_body_vs_total_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-body*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (total-w (window-total-width sw))
       (body-w (window-body-width sw))
       (total-h (window-total-height sw))
       (body-h (window-body-height sw))
       (margins (window-margins sw))
       (fringes (window-fringes sw))
       (scroll-bar-w (car (window-scroll-bars sw)))
       (right-div (window-right-divider-width sw)))
  (list total-w body-w total-h body-h margins fringes scroll-bar-w right-div))
"#;
    let expect = expect_test::expect![[r#""OK (20 19 24 23 (nil) (0 0 nil nil) nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_two_side_windows_opposite_sides_vertical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((window-sides-vertical t)
       (buf-l (get-buffer-create "*sw-opp-l*"))
       (buf-r (get-buffer-create "*sw-opp-r*"))
       (wl (display-buffer-in-side-window buf-l '((side . left) (window-width . 15))))
       (wr (display-buffer-in-side-window buf-r '((side . right) (window-width . 15))))
       (l-edges (window-edges wl))
       (r-edges (window-edges wr))
       (main-edges (window-edges (window-main-window)))
       (l-height (nth 3 l-edges))
       (r-height (nth 3 r-edges))
       (main-height (nth 3 main-edges)))
  (list l-edges r-edges main-edges
        (= l-height r-height)
        (= l-height main-height)))
"#;
    let expect = expect_test::expect![[r#""OK ((0 0 15 24) (65 0 80 24) (15 0 65 24) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Downstream probes: y-offset-1 effects on scrolling, point, visible-p
// ===========================================================================

#[test]
fn oracle_side_window_deep_pos_visible_in_window_p_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-visible*"))
       (_ (with-current-buffer buf
            (insert "line 1\nline 2\nline 3\nline 4\nline 5\n"
                    "line 6\nline 7\nline 8\nline 9\nline 10\n")))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (_ (select-window sw))
       ;; Go to point-min and check visibility
       (_ (goto-char (point-min)))
       (visible-1 (pos-visible-in-window-p (point-min) sw))
       ;; Scroll to bottom
       (_ (goto-char (point-max)))
       (visible-end (pos-visible-in-window-p (point-max) sw)))
  (list visible-1 visible-end
        (window-start sw)
        (window-end sw)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil 1 72)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_recenter_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-recenter*"))
       (_ (with-current-buffer buf
            (dotimes (i 50)
              (insert (format "line number %d\n" i)))))
       (sw (display-buffer-in-side-window buf '((side . right))))
       (_ (select-window sw))
       (_ (goto-char (point-max)))
       (_ (recenter 0))
       (start-after-recenter (window-start sw))
       (_ (goto-char (point-min)))
       (_ (recenter -1))
       (start-after-negative (window-start sw)))
  (list (numberp start-after-recenter)
        (> start-after-recenter 1)
        (= start-after-negative 1)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_scroll_up_down_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-scroll*"))
       (_ (with-current-buffer buf
            (dotimes (i 50)
              (insert (format "scroll line %d content here\n" i)))))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (_ (select-window sw))
       (start-before (window-start sw))
       (_ (scroll-up 3))
       (start-after-up (window-start sw))
       (_ (scroll-down 3))
       (start-after-down (window-start sw)))
  (list start-before start-after-up start-after-down
        (= start-before start-after-down)))
"#;
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_end_on_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-end*"))
       (_ (with-current-buffer buf
            (insert "line A\nline B\nline C\nline D\nline E\n"
                    "line F\nline G\nline H\n")))
       (sw (display-buffer-in-side-window buf '((side . top))))
       (wend (window-end sw))
       (wstart (window-start sw)))
  (list wstart wend
        (> wend wstart)
        (= wstart 1)))
"#;
    let expect = expect_test::expect![[r#""OK (1 57 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_absolute_pixel_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-abs-pixel*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (abs-edges (window-absolute-pixel-edges sw))
       (pixel-edges (window-pixel-edges sw)))
  (list abs-edges pixel-edges))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_move_to_window_line_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-move-line*"))
       (_ (with-current-buffer buf
            (dotimes (i 20)
              (insert (format "content for line %d here\n" i)))))
       (sw (display-buffer-in-side-window buf '((side . right))))
       (_ (select-window sw))
       (point-before (point))
       (_ (move-to-window-line 2))
       (point-after (point)))
  (list point-before point-after
        (not (= point-before point-after))))
"#;
    let expect = expect_test::expect![[r#""OK (491 49 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// New territory: quit-window, display-buffer-same-window, window-state writable
// ===========================================================================

#[test]
fn oracle_side_window_deep_quit_window_on_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-quit*"))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (buf-before (window-buffer sw))
       (_ (condition-case err
              (quit-window nil sw)
            (error (list 'quit-err (car (cdr err))))))
       (sw-after (and (window-live-p sw) sw))
       (buf-after (and sw-after (window-buffer sw))))
  (list (window-live-p sw)
        (and sw-after
             (window-parameter sw-after 'window-side))
        (eq buf-before buf-after)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_display_buffer_same_window_with_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-same-1*"))
       (buf2 (get-buffer-create "*sw-same-2*"))
       (main (window-main-window))
       (_ (display-buffer-in-side-window buf1 '((side . left))))
       ;; display-buffer-same-window on main should not use side window
       (_ (select-window main))
       (result (display-buffer buf2 '((display-buffer-same-window))))
       (result-side (window-parameter result 'window-side)))
  (list (eq result main)
        (not result-side)))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_same_buffer_two_different_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-same-buf*"))
       (wl (display-buffer-in-side-window buf '((side . left) (slot . 0))))
       (wr (display-buffer-in-side-window buf '((side . right) (slot . 0))))
       (l-buf (window-buffer wl))
       (r-buf (window-buffer wr)))
  (list (eq l-buf buf)
        (eq r-buf buf)
        (eq wl wr)))
"#;
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_state_get_with_writable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-state-wr*"))
       (sw (display-buffer-in-side-window buf '((side . left) (slot . 3))))
       (state (window-state-get sw t))
       (state-side (cdr (assq 'window-side (nth 1 state))))
       (state-slot (cdr (assq 'window-slot (nth 1 state)))))
  (list state-side state-slot
        (eq state-side 'left)
        (numberp state-slot)))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp leaf)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_frame_parameter_window_state_after_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-fpws*"))
       (_sw (display-buffer-in-side-window buf '((side . left) (slot . 0))))
       (state-before (frame-parameter nil 'window-state))
       (_toggle (window-toggle-side-windows))
       (state-after (frame-parameter nil 'window-state)))
  (list (null state-before)
        (not (null state-after))))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_unbury_buffer_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-unbury-1*"))
       (buf2 (get-buffer-create "*sw-unbury-2*"))
       (sw (display-buffer-in-side-window buf1 '((side . left))))
       (_ (display-buffer-in-side-window buf2 '((side . left) (slot . 0))))
       ;; buf1 is now buried behind buf2. Unbury it.
       (_ (bury-buffer buf2))
       (buf-in-sw (window-buffer sw))
       (prev (window-prev-buffers sw)))
  (list (eq buf-in-sw buf1)
        (length prev)))
"#;
    let expect = expect_test::expect![[r#""OK (nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_parameters_in_alist_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-alist-params*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . left)
              (window-parameters
               (no-delete-other-windows . t)
               (window-preserved-size . (t . nil))))))
       (ndow (window-parameter sw 'no-delete-other-windows))
       (wps (window-parameter sw 'window-preserved-size))
       (side (window-parameter sw 'window-side))
       (slot (window-parameter sw 'window-slot)))
  (list ndow wps side (numberp slot)))
"#;
    let expect = expect_test::expect![[r#""OK (t (t) left t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_display_buffer_no_window_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-no-win*"))
       (result (display-buffer
                buf
                '((display-buffer-no-window)
                  (allow-no-window . t))))
       (got-window (and result (window-parameter result 'window-side))))
  (list result got-window))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Window-tree structure probes
// ===========================================================================

#[test]
fn oracle_side_window_deep_window_tree_parent_child_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-tree*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (parent (window-parent sw))
       (root (frame-root-window))
       (is-child-of-root (eq parent root))
       (has-sibling (and (window-next-sibling sw)
                         (window-prev-sibling sw)))
       ;; The side window should have siblings
       (next (window-next-sibling sw))
       (prev (window-prev-sibling sw))
       (next-live (and next (window-live-p next)))
       (prev-live (and prev (window-live-p prev)))
       (next-side (and next (window-parameter next 'window-side)))
       (prev-side (and prev (window-parameter prev 'window-side))))
  (list is-child-of-root
        (not (null next))
        (null prev)
        next-live prev-live
        next-side prev-side))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_tree_after_multi_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((bl (get-buffer-create "*sw-tree-l*"))
       (br (get-buffer-create "*sw-tree-r*"))
       (wl (display-buffer-in-side-window bl '((side . left))))
       (wr (display-buffer-in-side-window br '((side . right))))
       (l-parent (window-parent wl))
       (r-parent (window-parent wr))
       (same-parent (eq l-parent r-parent))
       (root (frame-root-window))
       (both-children-of-root (and (eq l-parent root) (eq r-parent root)))
       ;; walk children of root
       (children (let ((kids nil)
                       (child (window-child root)))
                   (while child
                     (push (list (window-parameter child 'window-side)
                                 (window-live-p child))
                           kids)
                     (setq child (window-next-sibling child)))
                   (nreverse kids))))
  (list same-parent both-children-of-root children))
"#;
    let expect = expect_test::expect![[r#""OK (t t ((left t) (nil t) (right t)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_resizable_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-resizable*"))
       (sw (display-buffer-in-side-window buf '((side . left) (window-width . 20))))
       (resizable-h (window-resizable sw 5 t))
       (resizable-v (window-resizable sw 2 nil))
       (resizable-h-neg (window-resizable sw -5 t)))
  (list resizable-h resizable-v resizable-h-neg))
"#;
    let expect = expect_test::expect![[r#""OK (5 0 -5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_set_window_configuration_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-config*"))
       (sw (display-buffer-in-side-window buf '((side . left) (slot . 1))))
       ;; save configuration
       (config (current-window-configuration))
       (side-before (window-parameter sw 'window-side))
       (slot-before (window-parameter sw 'window-slot))
       ;; delete all windows
       (_ (delete-other-windows (window-main-window)))
       ;; restore configuration
       (_ (set-window-configuration config))
       ;; find side window again
       (sw-after (window-with-parameter 'window-side 'left))
       (slot-after (and sw-after (window-parameter sw-after 'window-slot))))
  (list side-before slot-before
        (and sw-after t)
        slot-after))
"#;
    let expect = expect_test::expect![[r#""OK (left 1 t 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_minimum_size_enforcement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-minsize*"))
       (sw (display-buffer-in-side-window
            buf
            '((side . left) (window-width . 2))))
       (width (window-total-width sw))
       (min-width (window-min-size sw t))
       (min-height (window-min-size sw nil)))
  (list width min-width min-height))
"#;
    let expect = expect_test::expect![[r#""OK (2 10 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_combination_limit_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-comb-limit*"))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (parent (window-parent sw))
       (combination-limit (window-combination-limit parent)))
  (list (windowp parent)
        combination-limit))
"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Pixel-level geometry probes (y-offset-1 deep dive)
// ===========================================================================

#[test]
fn oracle_side_window_deep_pixel_level_positioning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-pixel*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (pixel-left (window-pixel-left sw))
       (pixel-top (window-pixel-top sw))
       (top-line (window-top-line sw))
       (left-col (window-left-column sw))
       (inside-edges (window-inside-pixel-edges sw))
       (pixel-edges (window-pixel-edges sw)))
  (list pixel-left pixel-top top-line left-col
        inside-edges pixel-edges))
"#;
    let expect = expect_test::expect![[r#""OK (0 0 0 0 (0 0 19 23) (0 0 20 24))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_all_four_sides_pixel_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((bl (get-buffer-create "*sw-pix-l*"))
       (br (get-buffer-create "*sw-pix-r*"))
       (bt (get-buffer-create "*sw-pix-t*"))
       (bb (get-buffer-create "*sw-pix-b*"))
       (wl (display-buffer-in-side-window bl '((side . left) (window-width . 15))))
       (wr (display-buffer-in-side-window br '((side . right) (window-width . 15))))
       (wt (display-buffer-in-side-window bt '((side . top) (window-height . 5))))
       (wb (display-buffer-in-side-window bb '((side . bottom) (window-height . 5))))
       (l-top (window-top-line wl))
       (r-top (window-top-line wr))
       (t-top (window-top-line wt))
       (b-top (window-top-line wb))
       (l-pix-top (window-pixel-top wl))
       (r-pix-top (window-pixel-top wr))
       (t-pix-top (window-pixel-top wt))
       (b-pix-top (window-pixel-top wb)))
  (list l-top r-top t-top b-top
        l-pix-top r-pix-top t-pix-top b-pix-top))
"#;
    let expect = expect_test::expect![[r#""OK (6 6 1 20 5 5 0 19)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// More tree-structure and lifecycle probes
// ===========================================================================

#[test]
fn oracle_side_window_deep_walk_window_tree_with_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-walktree*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (tree (let ((nodes nil))
               (walk-window-tree (lambda (w) (push w nodes))
                                 nil 'nominibuf)
               nodes))
       (tree-count (length tree))
       (side-count (let ((n 0))
                     (walk-window-tree
                      (lambda (w)
                        (when (window-parameter w 'window-side)
                          (setq n (1+ n))))
                      nil 'nominibuf)
                     n))
       (main-count (- tree-count side-count)))
  (list tree-count side-count main-count))
"#;
    let expect = expect_test::expect![[r#""OK (3 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_delete_and_recreate_side_window_on_same_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-recreate-1*"))
       (buf2 (get-buffer-create "*sw-recreate-2*"))
       (sw1 (display-buffer-in-side-window buf1 '((side . left) (slot . 0))))
       (_ (delete-window sw1))
       (sw2 (display-buffer-in-side-window buf2 '((side . left) (slot . 0))))
       (slot2 (window-parameter sw2 'window-slot))
       (side2 (window-parameter sw2 'window-side)))
  (list (eq sw1 sw2)
        (eq side2 'left)
        (numberp slot2)))
"#;
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_normal_size_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-normal*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (normal-h (window-normal-size sw t))
       (normal-v (window-normal-size sw nil))
       (main (window-main-window))
       (main-normal-h (window-normal-size main t)))
  (list normal-h normal-v main-normal-h))
"#;
    let expect = expect_test::expect![[r#""OK (0.25 1.0 0.75)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_display_buffer_in_previous_window_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf1 (get-buffer-create "*sw-prev-1*"))
       (buf2 (get-buffer-create "*sw-prev-2*"))
       (main (window-main-window))
       (_ (select-window main))
       ;; Create a side window, then switch back to main
       (sw (display-buffer-in-side-window buf1 '((side . bottom))))
       (_ (select-window main))
       ;; display-buffer-in-previous-window should NOT use the side window
       (result (condition-case err
                   (let ((w (display-buffer-in-previous-window
                             buf2 '((reusable-frames . visible)))))
                     (list 'got-window (eq w main)))
                 (error (list 'err (car (cdr err)))))))
  (list result))
"#;
    let expect = expect_test::expect![[r#""OK ((got-window nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_parameters_all_after_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-params-all*"))
       (sw (display-buffer-in-side-window buf '((side . right) (slot . 5))))
       (params (window-parameters sw))
       (side-entry (assq 'window-side params))
       (slot-entry (assq 'window-slot params))
       (quit-restore-entry (assq 'quit-restore params)))
  (list (not (null side-entry))
        (not (null slot-entry))
        (cdr side-entry)
        (cdr slot-entry)
        (not (null quit-restore-entry))))
"#;
    let expect = expect_test::expect![[r#""OK (t t right 5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_inside_edges_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-inside*"))
       (sw (display-buffer-in-side-window buf '((side . left) (window-width . 18))))
       (inside-pixel (window-inside-pixel-edges sw))
       (inside-char (window-inside-edges sw))
       (body-pixel (list (window-body-width sw t)
                         (window-body-height sw t)))
       (total-pixel (list (window-total-width sw t)
                          (window-total-height sw t))))
  (list inside-pixel inside-char body-pixel total-pixel))
"#;
    let expect = expect_test::expect![[r#""OK ((0 0 17 23) (0 0 17 23) (17 23) (18 24))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// get-lru-window, get-largest-window, count-windows probes
// ===========================================================================

#[test]
fn oracle_side_window_deep_get_lru_window_with_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-lru*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (lru-all (get-lru-window nil nil 'visible))
       (lru-nomini (get-lru-window nil 'nominibuf 'visible))
       (lru-all-side (window-parameter lru-all 'window-side))
       (lru-nomini-side (window-parameter lru-nomini 'window-side)))
  (list lru-all-side lru-nomini-side))
"#;
    let expect = expect_test::expect![[r#""OK (nil left)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_get_largest_window_with_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-largest*"))
       (sw (display-buffer-in-side-window buf '((side . bottom))))
       (largest-all (get-largest-window nil nil 'visible))
       (largest-nomini (get-largest-window nil 'nominibuf 'visible))
       (main (window-main-window))
       (largest-is-main (eq largest-nomini main))
       (largest-side (window-parameter largest-all 'window-side)))
  (list largest-is-main largest-side))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_count_windows_with_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-count*"))
       (count-before (count-windows nil 'nominibuf))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (count-after (count-windows nil 'nominibuf))
       (count-all (count-windows))
       (diff (- count-after count-before)))
  (list count-before count-after count-all diff))
"#;
    let expect = expect_test::expect![[r#""OK (1 2 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full lifecycle: create → toggle → create → toggle → delete
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_deep_full_toggle_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-lifecycle*"))
       ;; Phase 1: create side window
       (sw1 (display-buffer-in-side-window buf '((side . left) (slot . 0))))
       (side-after-create (window-parameter sw1 'window-side))
       ;; Phase 2: toggle off
       (_ (window-toggle-side-windows))
       (after-toggle-off (window-with-parameter 'window-side 'left))
       ;; Phase 3: toggle on (restore)
       (_ (window-toggle-side-windows))
       (after-toggle-on (window-with-parameter 'window-side 'left))
       ;; Phase 4: delete the restored window
       (_ (when after-toggle-on (delete-window after-toggle-on)))
       (after-delete (window-with-parameter 'window-side 'left))
       ;; Phase 5: toggle again - should error since all were deleted
       (toggle-after-delete (condition-case err
                                (progn
                                  (window-toggle-side-windows)
                                  'succeeded)
                              (error 'no-saved-state))))
  (list (eq side-after-create 'left)
        (null after-toggle-off)
        (and after-toggle-on t)
        (null after-delete)
        toggle-after-delete))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t succeeded)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-first-child / window-last-child on root with side windows
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_deep_window_first_last_child_with_sides() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((bl (get-buffer-create "*sw-fl-child-l*"))
       (br (get-buffer-create "*sw-fl-child-r*"))
       (wl (display-buffer-in-side-window bl '((side . left))))
       (wr (display-buffer-in-side-window br '((side . right))))
       (root (frame-root-window))
       (first (window-child root))
       (last (window-last-child root))
       (first-side (window-parameter first 'window-side))
       (last-side (window-parameter last 'window-side)))
  (list first-side last-side
        (window-live-p first)
        (window-live-p last)))
"#;
    let expect = expect_test::expect![[r#""OK (left right t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-other-windows FROM a side window (should be allowed)
// ---------------------------------------------------------------------------

#[test]
fn oracle_side_window_deep_delete_other_windows_from_side_window_in_detail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-del-other-side*"))
       (sw (display-buffer-in-side-window buf '((side . left))))
       (main (window-main-window))
       ;; Also create a non-side window below main
       (_ (select-window main))
       (lower (split-window main nil 'below))
       (windows-before (window-list nil 'nominibuf))
       (count-before (length windows-before))
       ;; Now from the side window, delete all other windows
       (_ (select-window sw))
       (_ (condition-case err
              (delete-other-windows sw)
            (error (list 'delete-err (car (cdr err))))))
       (windows-after (window-list nil 'nominibuf))
       (count-after (length windows-after)))
  (list count-before count-after
        (eq (selected-window) sw)
        (window-live-p main)))
"#;
    let expect = expect_test::expect![[r#""OK (3 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ===========================================================================
// Resize-boundary probes: main ↔ side window interaction
// ===========================================================================

#[test]
fn oracle_side_window_deep_enlarge_main_window_into_side_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-enlarge-main*"))
       (sw (display-buffer-in-side-window buf '((side . left) (window-width . 20))))
       (main (window-main-window))
       (sw-width-before (window-total-width sw))
       (main-width-before (window-total-width main))
       (_ (select-window main))
       ;; Try to enlarge main horizontally (should shrink left side window)
       (_enlarge (condition-case err
                     (enlarge-window 3 t)
                   (error (list 'err (car (cdr err))))))
       (sw-width-after (window-total-width sw))
       (main-width-after (window-total-width main)))
  (list sw-width-before main-width-before
        sw-width-after main-width-after
        (< sw-width-after sw-width-before)
        (> main-width-after main-width-before)))
"#;
    let expect = expect_test::expect![[r#""OK (20 60 17 63 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_shrink_main_window_side_window_grows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-shrink-main*"))
       (sw (display-buffer-in-side-window buf '((side . right) (window-width . 20))))
       (main (window-main-window))
       (sw-width-before (window-total-width sw))
       (_ (select-window main))
       (_shrink (condition-case err
                    (shrink-window 3 t)
                  (error (list 'err (car (cdr err))))))
       (sw-width-after (window-total-width sw)))
  (list sw-width-before sw-width-after
        (> sw-width-after sw-width-before)))
"#;
    let expect = expect_test::expect![[r#""OK (20 23 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_enlarge_window_horizontally_in_side_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-enlarge-self*"))
       (sw (display-buffer-in-side-window buf '((side . right) (window-width . 15))))
       (width-before (window-total-width sw))
       (_ (select-window sw))
       (_enlarge (condition-case err
                     (enlarge-window 3 t)
                   (error (list 'err (car (cdr err))))))
       (width-after (window-total-width sw)))
  (list width-before width-after
        (> width-after width-before)))
"#;
    let expect = expect_test::expect![[r#""OK (15 18 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_side_window_deep_window_pixel_width_after_resize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((buf (get-buffer-create "*sw-pix-resize*"))
       (sw (display-buffer-in-side-window buf '((side . left) (window-width . 12))))
       (pix-w-before (window-pixel-width sw))
       (pix-total-w-before (window-total-width sw t))
       (_ (select-window sw))
       (_ (enlarge-window 5 t))
       (pix-w-after (window-pixel-width sw))
       (pix-total-w-after (window-total-width sw t)))
  (list pix-w-before pix-total-w-before
        pix-w-after pix-total-w-after
        (> pix-w-after pix-w-before)))
"#;
    let expect = expect_test::expect![[r#""OK (12 12 17 17 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
