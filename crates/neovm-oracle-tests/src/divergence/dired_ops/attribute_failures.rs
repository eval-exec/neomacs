use crate::common::oracle_sandbox::OracleSandbox;
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};
#[test]
fn oracle_file_attribute_failures_and_dangling_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    std::os::unix::fs::symlink("loop", dir.path().join("loop")).unwrap();
    std::os::unix::fs::symlink("missing-target", dir.path().join("dangling")).unwrap();
    let form = format!(
        r#"(let* ((dir {:?}) (loop (expand-file-name "loop/entry" dir)))
      (list (file-attributes (expand-file-name "missing" dir))
        (condition-case err (file-attributes loop)
          (error (list (car err) (nth 1 err) (nth 2 err) (equal (nth 3 err) loop))))
        (mapcar (lambda (entry) (list (car entry) (car (cdr entry))))
          (directory-files-and-attributes dir nil "\\`dangling\\'" nil nil 1))))"#,
        dir.path().to_str().unwrap()
    );
    assert_oracle_parity_under_envs_expect(
        &form,
        &[&[("NEOVM_JIT", "0")], &[]],
        expect_test::expect![[
            r#""OK (nil (file-error \"Getting attributes\" \"Too many levels of symbolic links\" t) ((\"dangling\" \"missing-target\")))""#
        ]],
    );
}
