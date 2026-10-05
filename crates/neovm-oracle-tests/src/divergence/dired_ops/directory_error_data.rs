use crate::common::oracle_sandbox::OracleSandbox;
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};
const ENVS: &[&[(&str, &str)]] = &[&[("NEOVM_JIT", "0")], &[]];
#[test]
fn oracle_directory_stream_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"").unwrap();
    let form = format!(
        r#"(let ((dir {:?}) (out nil))
      (dolist (name '("missing" "file"))
        (let ((path (expand-file-name name dir)))
          (dolist (op '(directory-files directory-files-and-attributes file-name-completion file-name-all-completions))
            (push (condition-case err
                (if (memq op '(directory-files directory-files-and-attributes)) (funcall op path) (funcall op "" path))
              (error (list (car err) (nth 1 err) (nth 2 err) (equal (nth 3 err) path) (length err)))) out))))
      (nreverse out))"#,
        dir.path().to_str().unwrap()
    );
    assert_oracle_parity_under_envs_expect(
        &form,
        ENVS,
        expect_test::expect![[
            r#""OK ((file-missing \"Opening directory\" \"No such file or directory\" t 4) (file-missing \"Opening directory\" \"No such file or directory\" t 4) (file-missing \"Opening directory\" \"No such file or directory\" t 4) (file-missing \"Opening directory\" \"No such file or directory\" t 4) (file-error \"Opening directory\" \"Not a directory\" t 4) (file-error \"Opening directory\" \"Not a directory\" t 4) (file-error \"Opening directory\" \"Not a directory\" t 4) (file-error \"Opening directory\" \"Not a directory\" t 4))""#
        ]],
    );
}
