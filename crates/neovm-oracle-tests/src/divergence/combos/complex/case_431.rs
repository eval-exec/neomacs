//! Complex combo batch 431 — 18 probes into tab-bar, frame display,
//! window state/swap, select-window, with-selected-window/frame,
//! window combination constraints, tty-type, frame-geometry,
//! frame-list-z-order-delete, and pixel-resolution.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// tab-bar-mode / tab-bar-new-tab / tab-bar-close-tab.
#[test]
fn div_cx431_tab_bar_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'tab-bar)
  (list (boundp 'tab-bar-mode)
        (fboundp 'tab-bar-new-tab)
        (fboundp 'tab-bar-close-tab)))
"##,
        expect,
    );
}

/// tab-line-mode / global-tab-line-mode.
#[test]
fn div_cx431_tab_line_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'tab-line)
  (list (boundp 'global-tab-line-mode)
        (boundp 'tab-line-tab-name-format-function)))
"##,
        expect,
    );
}

/// window-swap-states / window-state-buffers: state exchange.
#[test]
fn div_cx431_window_swap_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<buffer *scratch*> #<buffer *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "buffer a")
  (let ((state (window-state-get (selected-window))))
    (condition-case e
        (window-state-buffers state)
      (error (car e)))))
"##,
        expect,
    );
}

/// select-window with norecord flag.
#[test]
fn div_cx431_select_window_norecord() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<window 1 on *scratch*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (select-window w 'norecord)
  (selected-window))
"##,
        expect,
    );
}

/// with-selected-window / with-selected-frame.
#[test]
fn div_cx431_with_selected_window_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"*scratch*\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (with-selected-window w
    (with-selected-frame (selected-frame)
      (buffer-name))))
"##,
        expect,
    );
}

/// window-combination-resize / window-combination-limit.
#[test]
fn div_cx431_window_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-combination-resize)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-combination-resize w)
        (window-combination-limit w)))
"##,
        expect,
    );
}

/// window-splits / window-combination-p.
#[test]
fn div_cx431_window_splits_comb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-splits)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-splits w)
        (window-combined-p w t)))
"##,
        expect,
    );
}

/// tty-type: terminal type as a string.
#[test]
fn div_cx431_tty_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (tty-type) (error (car e))))
"##,
        expect,
    );
}

/// frame-geometry: frame position and size.
#[test]
fn div_cx431_frame_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((g (frame-geometry (selected-frame))))
      (list (assq 'outer-position g)
            (assq 'outer-size g)))
  (error (car e)))
"##,
        expect,
    );
}

/// frame-list-z-order-delete: frame stacking order.
#[test]
fn div_cx431_frame_list_z_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (length (frame-list-z-order-delete 1))
  (error (car e)))
"##,
        expect,
    );
}

/// frame-restack: restacking frames (may be stubbed).
#[test]
fn div_cx431_frame_restack() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (frame-restack (selected-frame) (selected-frame) nil)
  (error (car e)))
"##,
        expect,
    );
}

/// frame-monitor-attributes / frame-attribute.
#[test]
fn div_cx431_frame_monitor_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((m (frame-monitor-attributes)))
      (list (listp m) (> (length m) 0)))
  (error (car e)))
"##,
        expect,
    );
}

/// pixel-resolution-width / pixel-resolution-height.
#[test]
fn div_cx431_pixel_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"pixel-resolution\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'pixel-resolution)
  (list (condition-case e (pixel-resolution-width) (error (car e)))
        (condition-case e (pixel-resolution-height) (error (car e)))))
"##,
        expect,
    );
}

/// buffer-local-set-state / buffer-local-restore-state.
#[test]
fn div_cx431_buffer_local_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 'neo-cx431-var)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (setq-local neo-cx431-var 'original)
  (let ((state (buffer-local-set-state 'neo-cx431-var 'modified)))
    (list neo-cx431-var
          (progn (buffer-local-restore-state state)
                 neo-cx431-var))))
"##,
        expect,
    );
}

/// process-get / process-put with large plist.
#[test]
fn div_cx431_process_plist_large() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx431-pl"
                          :command '("echo" "done")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (process-put proc 'key1 'val1)
  (process-put proc 'key2 'val2)
  (process-put proc 'key3 'val3)
  (prog1 (list (process-get proc 'key2)
               (process-get proc 'key4 'default))
    (delete-process proc)))
"##,
        expect,
    );
}

/// delete-process on already-exited process.
#[test]
fn div_cx431_delete_process_exited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ok""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx431-dp"
                          :command '("echo" "done")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (delete-process proc)
  (delete-process proc)
  'ok)
"##,
        expect,
    );
}

/// window-pixel-width-before/after-size-change.
#[test]
fn div_cx431_window_pixel_size_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-pixel-width w)
        (window-pixel-height w)))
"##,
        expect,
    );
}
