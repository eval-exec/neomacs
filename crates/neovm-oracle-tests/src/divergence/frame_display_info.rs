//! Divergence tests: frame parameters, multi-monitor, display info deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_frame_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-parameters)
  (fboundp 'frame-parameter)
  (fboundp 'set-frame-parameter)
  (fboundp 'modify-frame-parameters))"#,
        expect,
    );
}

#[test]
fn divergence_frame_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-list)
  (fboundp 'selected-frame)
  (fboundp 'next-frame)
  (fboundp 'delete-frame)
  (fboundp 'make-frame))"#,
        expect,
    );
}

#[test]
fn divergence_frame_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil \"F1\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-frame-name)
  (frame-parameter (selected-frame) 'title)
  (frame-parameter (selected-frame) 'name)
  (stringp (frame-parameter (selected-frame) 'name)))"#,
        expect,
    );
}

#[test]
fn divergence_frame_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-visible-p)
  (fboundp 'iconify-frame)
  (fboundp 'make-frame-visible)
  (fboundp 'make-frame-invisible))"#,
        expect,
    );
}

#[test]
fn divergence_multi_monitor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'display-monitor-attributes-list)
  (fboundp 'frame-monitor-attributes)
  (fboundp 'display-screens))"#,
        expect,
    );
}

#[test]
fn divergence_display_color_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'display-color-cells)
  (fboundp 'display-color-p)
  (fboundp 'display-grayscale-p)
  (fboundp 'color-values))"#,
        expect,
    );
}

#[test]
fn divergence_frame_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-font)
  (fboundp 'set-frame-font)
  (fboundp 'font-get)
  (fboundp 'font-put)
  (featurep 'font))"#,
        expect,
    );
}

#[test]
fn divergence_frame_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-position)
  (fboundp 'set-frame-position)
  (fboundp 'set-frame-size))"#,
        expect,
    );
}

#[test]
fn divergence_frame_child_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'frame-parent)
  (fboundp 'frame-ancestor-p)
  (fboundp 'make-frame-on-monitor))"#,
        expect,
    );
}

/// `frame-initial-p` answers about a TERMINAL as readily as about a FRAME.
///
/// GNU's `Fframe_initial_p` (src/terminal.c:482-500) tests `FRAMEP` first and
/// falls through to `decode_terminal` otherwise; its doc string says so outright
/// ("If FRAME is a terminal object, return non-nil if it holds the initial
/// frame").  `turn-on-xterm-mouse-tracking-on-terminal` (lisp/xt-mouse.el:512)
/// is the caller that relies on it, and on a TERM in
/// `xterm--auto-xt-mouse-allowed-types` (lisp/term/xterm.el:134-140 -- alacritty,
/// contour) it runs during startup, so a raise here costs `-l` and `--eval`.
#[test]
fn divergence_frame_initial_p_terminal_designator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (frame-initial-p)
  (frame-initial-p (selected-frame))
  (frame-initial-p (frame-terminal))
  (frame-initial-p (car (terminal-list))))"#,
        expect,
    );
}

/// `decode_terminal` (src/terminal.c:223-233) returns NULL rather than raising,
/// so every shape that is neither a frame nor a live terminal answers nil.  A
/// fixnum is one of them: GNU's `FRAMEP` is false for 0, and 0 is not a terminal.
#[test]
fn divergence_frame_initial_p_rejects_without_signalling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (frame-initial-p "junk")
  (frame-initial-p 'sym)
  (frame-initial-p 42)
  (frame-initial-p 0)
  (frame-initial-p (list 1 2)))"#,
        expect,
    );
}

/// The guard `turn-on-xterm-mouse-tracking-on-terminal` pairs with the
/// `frame-initial-p` test: `(eq t (terminal-live-p terminal))`.  GNU reports
/// both `output_initial` and `output_termcap` as `t` (src/terminal.c:452-478),
/// so the guard passes and the `frame-initial-p` answer is what decides.
#[test]
fn divergence_terminal_live_p_reports_t_for_the_initial_terminal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"initial_terminal\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (eq t (terminal-live-p (car (terminal-list))))
  (eq t (terminal-live-p (frame-terminal)))
  (terminal-name (car (terminal-list))))"#,
        expect,
    );
}

#[test]
fn divergence_x_display_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'x-display-screens)
  (fboundp 'x-server-version)
  (fboundp 'x-server-vendor)
  (fboundp 'x-display-pixel-width)
  (fboundp 'x-display-pixel-height))"#,
        expect,
    );
}
