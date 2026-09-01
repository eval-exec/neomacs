//! Oracle parity tests for GNU external debugging output redirection.
//!
//! GNU implements `redirect-debugging-output` in `src/print.c` by redirecting
//! stderr, and `external-debugging-output` writes a character to that stream.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_redirect_debugging_output_captures_external_debug_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file (make-temp-file "neomacs-redirect-debugging-output-oracle")))
  (unwind-protect
      (progn
        (redirect-debugging-output file)
        (external-debugging-output ?A)
        (external-debugging-output ?B)
        (redirect-debugging-output nil)
        (with-temp-buffer
          (insert-file-contents-literally file)
          (buffer-string)))
    (ignore-errors (redirect-debugging-output nil))
    (ignore-errors (delete-file file))))
"#;

    let expect = expect_test::expect![[r#""OK \"AB\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
