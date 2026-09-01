//! Default key-binding divergence probes.
//!
//! Probes whether Neomacs' default global-map key bindings match GNU's.
//! Each test lists lookup-key results for a group of common default bindings;
//! differences surface which default bindings diverge (missing, or bound to a
//! different command).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_db_cursor_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (move-beginning-of-line move-end-of-line forward-char backward-char next-line previous-line forward-word backward-word)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "\C-a")
      (lookup-key global-map "\C-e")
      (lookup-key global-map "\C-f")
      (lookup-key global-map "\C-b")
      (lookup-key global-map "\C-n")
      (lookup-key global-map "\C-p")
      (lookup-key global-map "\M-f")
      (lookup-key global-map "\M-b"))
"##,
        expect,
    );
}

#[test]
fn div_db_editing_kill_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (delete-char kill-line yank kill-region kill-ring-save kill-word undo)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "\C-d")
      (lookup-key global-map "\C-k")
      (lookup-key global-map "\C-y")
      (lookup-key global-map "\C-w")
      (lookup-key global-map "\M-w")
      (lookup-key global-map "\M-d")
      (lookup-key (current-global-map) "\C-_"))
"##,
        expect,
    );
}

#[test]
fn div_db_search_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (isearch-forward isearch-backward query-replace query-replace)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "\C-s")
      (lookup-key global-map "\C-r")
      (lookup-key global-map "\M-%")
      (lookup-key global-map [?\M-%]))
"##,
        expect,
    );
}

#[test]
fn div_db_scroll_buffer_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (scroll-up-command scroll-down-command beginning-of-buffer end-of-buffer switch-to-buffer kill-buffer find-file save-buffer)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "\C-v")
      (lookup-key global-map "\M-v")
      (lookup-key global-map "\M-<")
      (lookup-key global-map "\M->")
      (lookup-key global-map "\C-xb")
      (lookup-key global-map "\C-xk")
      (lookup-key global-map "\C-x\C-f")
      (lookup-key global-map "\C-x\C-s"))
"##,
        expect,
    );
}

#[test]
fn div_db_prefix_and_misc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (Control-X-prefix mode-specific-command-prefix help-command execute-extended-command keyboard-quit universal-argument save-buffers-kill-terminal)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "\C-x")
      (lookup-key global-map "\C-c")
      (lookup-key global-map "\C-h")
      (lookup-key global-map "\M-x")
      (lookup-key global-map "\C-g")
      (lookup-key global-map "\C-u")
      (lookup-key global-map "\C-x\C-c"))
"##,
        expect,
    );
}

#[test]
fn div_db_self_insert_and_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (self-insert-command self-insert-command newline indent-for-tab-command delete-backward-char ESC-prefix)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map "a")
      (lookup-key global-map " ")
      (lookup-key global-map "\r")
      (lookup-key global-map "\t")
      (lookup-key global-map "\d")
      (lookup-key global-map "\e"))
"##,
        expect,
    );
}

#[test]
fn div_db_function_and_arrow_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (left-char right-char previous-line next-line move-beginning-of-line move-end-of-line scroll-down-command scroll-up-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key global-map [left])
      (lookup-key global-map [right])
      (lookup-key global-map [up])
      (lookup-key global-map [down])
      (lookup-key global-map [home])
      (lookup-key global-map [end])
      (lookup-key global-map [prior])
      (lookup-key global-map [next]))
"##,
        expect,
    );
}

#[test]
fn div_db_ctl_x_map_common() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (other-window delete-window delete-other-windows split-window-below save-some-buffers insert-file)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ctlx (lookup-key global-map "\C-x")))
  (list (lookup-key ctlx "o")
        (lookup-key ctlx "0")
        (lookup-key ctlx "1")
        (lookup-key ctlx "2")
        (lookup-key ctlx "s")
        (lookup-key ctlx "i")))
"##,
        expect,
    );
}
