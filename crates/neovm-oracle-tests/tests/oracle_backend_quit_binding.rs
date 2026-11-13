//! GNU's macOS backend binds Cmd-Q to `save-buffers-kill-emacs'
//! (`lisp/term/ns-win.el:141') and leaves `s-q' unbound elsewhere.  The
//! AppKit Quit item is retargeted to deliver this key event, so the binding
//! must exist exactly where GNU installs it.

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
