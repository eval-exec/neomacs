//! Oracle parity tests for GNU `find-buffer`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_find_buffer_uses_equal_for_buffer_local_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/buffer.c:Ffind_buffer walks live buffers and compares VALUE with
    // (buffer-local-value VARIABLE BUF) using `equal`, not object identity.
    let form = r#"
(let ((buf (get-buffer-create " *oracle-find-buffer-equal*"))
      (var 'neomacs--oracle-find-buffer-value))
  (unwind-protect
      (progn
        (set-default var nil)
        (with-current-buffer buf
          (set (make-local-variable var) (copy-sequence "payload")))
        (let ((found (find-buffer var (copy-sequence "payload"))))
          (list (bufferp found)
                (and found (buffer-name found))
                (eq found buf))))
    (when (buffer-live-p buf)
      (kill-buffer buf))
    (makunbound var)))
"#;
    let expect = expect_test::expect![[r#""OK (t \" *oracle-find-buffer-equal*\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
