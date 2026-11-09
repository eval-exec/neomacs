//! Divergence tests: accessibility, mouse, drag-n-drop, menu deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_accessibility_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'window-min-height)
  (boundp 'window-min-width)
  (boundp 'window-resize-pixelwise)
  (boundp 'frame-resize-pixelwise))"#,
        expect,
    );
}

#[test]
fn divergence_mouse_event_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'mouse-set-point)
  (fboundp 'mouse-set-region)
  (fboundp 'mouse-yank-at-click)
  (fboundp 'mouse-start-end))"#,
        expect,
    );
}

#[test]
fn divergence_drag_n_drop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Both pinned GNU reference builds bind x-begin-drag: the XOBJ DEFUN on
    // linux-gtk3 and Lisp preload on the NS build (live macro-expansion dump,
    // a360712c9d27), so the answer is attested per host even though it agrees.
    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (t t)""#,
        MacosDarwin => r#""OK (t t)""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'x-begin-drag)
  (featurep 'dnd))"#,
        expect,
    );

    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (t t)""#,
        MacosDarwin => r#""OK (t t)""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'x-dnd)
  (list
    (fboundp 'x-dnd-handle-drag-n-drop-event)
    (featurep 'x-dnd)))"#,
        expect,
    );
}

/// MAC-X11-SUBRS: the 36 X11-only subrs that Neo registers must be bound on
/// the X11 reference and void on the NS reference, exactly like GNU.  The
/// names are the live fboundp dump intersection: XOBJ DEFUNs the pinned NS
/// build leaves void, minus x-begin-drag (Lisp-bound on NS) and x-file-dialog
/// (not registered by Neo).
#[test]
fn divergence_mac_x11_only_subrs_are_void_on_ns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (36 36)""#,
        MacosDarwin => r#""OK (36 0)""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(let ((names '(font-get-system-font
                   font-get-system-normal-font
                   handle-save-session
                   tool-bar-get-system-style
                   x-backspace-delete-keys-p
                   x-change-window-property
                   x-delete-window-property
                   x-disown-selection-internal
                   x-display-monitor-attributes-list
                   x-display-set-last-user-time
                   x-double-buffered-p
                   x-export-frames
                   x-frame-edges
                   x-frame-geometry
                   x-frame-list-z-order
                   x-frame-restack
                   x-get-atom-name
                   x-get-local-selection
                   x-get-modifier-masks
                   x-get-selection-internal
                   x-internal-focus-input-context
                   x-menu-bar-open-internal
                   x-mouse-absolute-pixel-position
                   x-own-selection-internal
                   x-register-dnd-atom
                   x-selection-exists-p
                   x-selection-owner-p
                   x-send-client-message
                   x-server-input-extension-version
                   x-set-mouse-absolute-pixel-position
                   x-synchronize
                   x-translate-coordinates
                   x-uses-old-gtk-dialog
                   x-window-property
                   x-window-property-attributes
                   x-wm-set-size-hint)))
  (list (length names)
        (length (delq nil (mapcar #'fboundp names)))))"#,
        expect,
    );
}

/// MX-2: GNU binds `x-file-dialog` in every toolkit build and, where the C
/// layer has no dialog, from Lisp: the X DEFUN is registered only under
/// USE_MOTIF || USE_GTK (xfns.c:10632-10634), w32fns.c defines it for W32
/// (8252), and the NS and Haiku builds bind it from Lisp (term/ns-win.el:464
/// over ns-read-file-name, term/haiku-win.el:329).  The pinned references are
/// a GTK X build and an NS build; both answer fboundp t with the same
/// arglist:
///   (prompt dir &optional default-filename mustmatch only-dir-p)
///
/// Non-GUI semantics, live on both references: both GNU builds check the
/// window system before any argument work, but the message is
/// backend-specific -- the X DEFUN checks the selected frame
/// (src/frame.c:134-141, xfns.c:9688) and says "Window system frame should be
/// used", while the NS arm ends in ns-read-file-name's
/// check_window_system(NULL) (nsfns.m:1784) and says "Window system is not in
/// use or not initialized".  Neo's chooser reproduces the matching answer for
/// its surface; the (x-file-dialog 1 2) arm pins that the window-system check
/// precedes the argument checks.
#[test]
fn divergence_x_file_dialog_is_bound_on_every_surface() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (t (prompt dir &optional default-filename mustmatch only-dir-p) (error \"Window system frame should be used\") (error \"Window system frame should be used\"))""#,
        MacosDarwin => r#""OK (t (prompt dir &optional default-filename mustmatch only-dir-p) (error \"Window system is not in use or not initialized\") (error \"Window system is not in use or not initialized\"))""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(list (fboundp 'x-file-dialog)
  (help-function-arglist 'x-file-dialog t)
  (condition-case e (x-file-dialog "p" "/tmp") (error e))
  (condition-case e (x-file-dialog 1 2) (error e)))"#,
        expect,
    );
}

/// MX-2a: the GUI arm must not re-enter itself.  `read-file-name-default`
/// calls `x-file-dialog` when `next-read-file-uses-dialog-p` is non-nil
/// (minibuffer.el:3886, :4051), and GNU's C predicate answers t on a toolkit
/// GUI frame (fileio.c:6556-6572, via use-dialog-box / use-file-dialog).  The
/// GUI arm therefore let-binds `use-file-dialog` to nil around its inner
/// `read-file-name`; without that binding this mocked call recurses into
/// x-file-dialog until excessive-lisp-nesting.
#[test]
fn x_file_dialog_gui_arm_does_not_recurse_through_read_file_name() {
    let form = r#"(progn
  (require 'cl-lib)
  (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t))
            ((symbol-function 'next-read-file-uses-dialog-p)
             (lambda ()
               (and (or (null last-nonmenu-event) (consp last-nonmenu-event))
                    use-dialog-box
                    use-file-dialog
                    (display-graphic-p))))
            ((symbol-function 'completing-read) (lambda (&rest _) "/tmp/picked")))
    (x-file-dialog "p" "/tmp")))"#;
    let outcome = crate::common::run_neovm_eval(form).expect("Neomacs evaluates the mocked dialog");
    assert!(
        outcome.contains("/tmp/picked"),
        "the inner read-file-name must reach completing-read instead of \
         recursing into x-file-dialog: {outcome}"
    );
}

#[test]
fn divergence_menu_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'menu-bar-open)
  (fboundp 'popup-menu)
  (boundp 'menu-bar-mode)
  (boundp 'tool-bar-mode))"#,
        expect,
    );
}

#[test]
fn divergence_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'tool-bar-add-item)
  (fboundp 'tool-bar-local-item)
  (featurep 'tool-bar))"#,
        expect,
    );
}

#[test]
fn divergence_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'tab-bar-mode)
  (fboundp 'tab-new)
  (fboundp 'tab-close)
  (featurep 'tab-bar))"#,
        expect,
    );
}

#[test]
fn divergence_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'scroll-bar-mode)
  (fboundp 'scroll-bar-toolkit-scroll)
  (featurep 'scroll-bar))"#,
        expect,
    );
}

#[test]
fn divergence_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'tooltip-mode)
  (boundp 'tooltip-delay)
  (featurep 'tooltip))"#,
        expect,
    );
}

#[test]
fn divergence_notification_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'notifications-notify)
  (fboundp 'alerts-add-alert)
  (featurep 'notifications))"#,
        expect,
    );
}

#[test]
fn divergence_sound_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'play-sound)
  (boundp 'ring-bell-function)
  (featurep 'sound))"#,
        expect,
    );
}
