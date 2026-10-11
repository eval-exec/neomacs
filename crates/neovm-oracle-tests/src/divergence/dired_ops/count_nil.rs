use crate::common::oracle_sandbox::OracleSandbox;
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};
const ENVS: &[&[(&str, &str)]] = &[&[("NEOVM_JIT", "0")], &[]];
#[test]
fn oracle_directory_files_nil_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    std::fs::write(dir.path().join("entry"), b"").unwrap();
    let form = format!(
        r#"(let ((dir {:?}))
      (list (equal (directory-files dir) (directory-files dir nil nil nil nil))
        (equal (directory-files dir)
          (funcall (lambda (d &optional n) (directory-files d nil nil nil n)) dir))
        (equal (directory-files dir)
          (funcall (byte-compile (lambda (d &optional n) (directory-files d nil nil nil n))) dir))
        (condition-case err (directory-files dir nil nil nil 'invalid-count) (error err))))"#,
        dir.path().to_str().unwrap()
    );
    assert_oracle_parity_under_envs_expect(
        &form,
        ENVS,
        expect_test::expect![[r#""OK (t t t (wrong-type-argument wholenump invalid-count))""#]],
    );
}
