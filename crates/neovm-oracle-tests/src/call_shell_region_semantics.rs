//! Oracle parity tests for GNU `subr.el` `call-shell-region`.
//!
//! GNU implements `call-shell-region` as an Elisp wrapper around
//! `call-process-region`, passing dynamically visible `shell-file-name` and
//! `shell-command-switch`, optionally deleting the input region, and routing
//! stdout/stderr through the same BUFFER contract as `call-process`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_call_shell_region_delete_replaces_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "xabcY")
  (goto-char 2)
  (list
   (call-shell-region 2 5 "tr a-z A-Z" t t)
   (buffer-string)
   (point)))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"xABCY\" 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_call_shell_region_uses_dynamic_shell_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((shell-file-name "printf")
      (shell-command-switch "csr:%s"))
  (with-temp-buffer
    (insert "input")
    (goto-char 3)
    (list
     (call-shell-region 1 6 "cmd" nil t)
     (buffer-string)
     (point))))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"incsr:cmdput\" 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_call_shell_region_mixes_stderr_when_requested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (list
   (call-shell-region 1 4 "printf out; printf err >&2" nil (list t t))
   (buffer-string)))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"abcouterr\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
