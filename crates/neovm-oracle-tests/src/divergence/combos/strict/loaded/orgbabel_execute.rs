//! Strict combo oracle probes, batch 79: org-babel (source code block
//! execution — elisp and shell blocks).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p3_org_babel_elisp_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_load(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "* Test\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
  (org-babel-execute-src-block))
"##,
        &["org/org.el", "org/ob.el", "org/ob-emacs-lisp.el"],
    );
}

#[test]
fn div_p3_org_babel_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_load(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "* Test\n#+begin_src emacs-lisp\n(list 1 2 3)\n#+end_src\n")
  (org-babel-execute-src-block)
  (buffer-substring-no-properties (point-min) (point-max)))
"##,
        &["org/org.el", "org/ob.el", "org/ob-emacs-lisp.el"],
    );
}

#[test]
fn div_p3_org_babel_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_load(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "* Test\n#+begin_src emacs-lisp :var x=5 :var y=10\n(+ x y)\n#+end_src\n")
  (org-babel-execute-src-block))
"##,
        &["org/org.el", "org/ob.el", "org/ob-emacs-lisp.el"],
    );
}
