//! GNU's global modes set frame parameters; redisplay reads those requests.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn frame_bar_parameters_are_independent_of_global_mode_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
             (require 'menu-bar) (require 'tool-bar)
             (let ((menu-bar-mode t) (tool-bar-mode t)
                   (default-frame-alist nil) (result nil))
               (set-frame-parameter nil 'menu-bar-lines 0)
               (set-frame-parameter nil 'tool-bar-lines 0)
               (push (list (frame-parameter nil 'menu-bar-lines)
                           (frame-parameter nil 'tool-bar-lines)
                           menu-bar-mode tool-bar-mode) result)
               (menu-bar-mode -1) (tool-bar-mode -1)
               (set-frame-parameter nil 'menu-bar-lines 1)
               (set-frame-parameter nil 'tool-bar-lines 2)
               (push (list (frame-parameter nil 'menu-bar-lines)
                           (frame-parameter nil 'tool-bar-lines)
                           menu-bar-mode tool-bar-mode) result)
               (menu-bar-mode 1) (tool-bar-mode 1)
               (push (list (frame-parameter nil 'menu-bar-lines)
                           (frame-parameter nil 'tool-bar-lines)
                           menu-bar-mode tool-bar-mode) result)
               (menu-bar-mode -1) (tool-bar-mode -1)
               (push (list (frame-parameter nil 'menu-bar-lines)
                           (frame-parameter nil 'tool-bar-lines)
                           menu-bar-mode tool-bar-mode) result)
               (nreverse result)))"#,
        expect_test::expect![[r#""OK ((0 0 t t) (1 2 nil nil) (1 1 t t) (0 0 nil nil))""#]],
    );
}
