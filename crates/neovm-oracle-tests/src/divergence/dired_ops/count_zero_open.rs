use crate::common::oracle_sandbox::OracleSandbox;
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};
const ENVS: &[&[(&str, &str)]] = &[&[("NEOVM_JIT", "0")], &[]];
#[test]
fn oracle_directory_files_zero_count_opens_and_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"").unwrap();
    let form = format!(
        r#"(let ((dir {:?}) (out nil))
      (dolist (path (list dir (expand-file-name "missing" dir) (expand-file-name "file" dir)))
        (push (list
          (condition-case err (directory-files path nil nil nil 0) (error (car err)))
          (condition-case err (directory-files-and-attributes path nil nil nil nil 0) (error (car err)))) out))
      (push (condition-case err (directory-files dir nil "[" nil 0) (error (car err))) out)
      (nreverse out))"#,
        dir.path().to_str().unwrap()
    );
    assert_oracle_parity_under_envs_expect(
        &form,
        ENVS,
        expect_test::expect![[
            r#""OK ((nil nil) (file-missing file-missing) (file-error file-error) invalid-regexp)""#
        ]],
    );
}
