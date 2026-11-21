//! GNU's macOS backend binds Cmd-Q to `save-buffers-kill-emacs'
//! (`lisp/term/ns-win.el:141') and leaves `s-q' unbound elsewhere.  The
//! AppKit Quit item produces the separate `ns-power-off' non-key event
//! (`ns-win.el:177,195'); rebinding one route must preserve the other.

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn super_q_binding_matches_gnu_per_platform() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(eq (key-binding [?\s-q])
               (when (eq system-type 'darwin) #'save-buffers-kill-emacs))"#,
        expect_test::expect![[r#""OK t""#]],
    );
}

#[test]
fn ns_power_off_binding_and_non_key_mark_match_gnu_per_platform() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((original-q (lookup-key global-map [?\s-q])))
             (unwind-protect
                 (progn
                   (global-set-key [?\s-q] #'ignore)
                   (list
                    (eq (key-binding [ns-power-off])
                        (when (eq system-type 'darwin) #'save-buffers-kill-emacs))
                    (eq (get 'ns-power-off 'non-key-event) (eq system-type 'darwin))
                    (eq (key-binding [?\s-q]) #'ignore)))
               (define-key global-map [?\s-q] original-q)))"#,
        expect_test::expect![[r#""OK (t t t)""#]],
    );
}
