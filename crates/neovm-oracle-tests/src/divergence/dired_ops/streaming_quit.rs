use crate::common::oracle_sandbox::OracleSandbox;
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};
#[test]
fn oracle_directory_files_count_filters_before_limiting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    for name in ["a", "b", "c"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let form = format!(
        r#"(let ((dir {:?}))
      (let ((one (directory-files dir nil "\\`[abc]\\'" nil 1))
            (two (directory-files dir nil "\\`[abc]\\'" nil 2)))
        (list (length one) (length two)
          (equal one (mapcar #'car (directory-files-and-attributes dir nil "\\`[abc]\\'" nil nil 1)))
          (directory-files dir nil "never-matches" nil 0))))"#,
        dir.path().to_str().unwrap()
    );
    assert_oracle_parity_under_envs_expect(
        &form,
        &[&[("NEOVM_JIT", "0")], &[]],
        expect_test::expect![[r#""OK (1 2 t nil)""#]],
    );
}
