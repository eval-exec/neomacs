//! Strict combo oracle probes, batch 182: file IO round-trip (shared tempdir).
//! write-region + insert-file-contents content round-trip incl multibyte,
//! file-attributes type/size, file-readable-p/exists-p, and delete-file +
//! re-check. Uses the shared-tempdir harness so both engines use one path.
//! Uses assert_oracle_parity_with_shared_tempdir_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_file_write_read_roundtrip_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (file (expand-file-name "probe_io_rt.txt" dir)))
  (when (file-exists-p file) (delete-file file))
  (with-temp-buffer
    (insert "line one\nline two\n日本語 café\n")
    (write-region (point-min) (point-max) file nil 'silent))
  (let ((attrs (file-attributes file)))
    (with-temp-buffer
      (insert-file-contents file)
      (list (buffer-string)
            (count-lines (point-min) (point-max))
            (nth 7 attrs)
            (null (file-attribute-type attrs))
            (file-readable-p file)
            (file-exists-p file)
            (progn (delete-file file) (file-exists-p file))))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"line one\\nline two\\n日本語 café\\n\" 3 34 t t t nil)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_file_append_mode_size_growth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (file (expand-file-name "probe_io_app.txt" dir)))
  (when (file-exists-p file) (delete-file file))
  (write-region "first\n" nil file nil 'silent)
  (let ((s1 (nth 7 (file-attributes file))))
    (write-region "second\n" nil file 'append 'silent)
    (let ((s2 (nth 7 (file-attributes file))))
      (with-temp-buffer
        (insert-file-contents file)
        (list s1
              s2
              (buffer-string)
              (count-lines (point-min) (point-max))
              (progn (delete-file file) (file-exists-p file)))))))
"##;
    let expect = expect_test::expect![[r#""OK (6 13 \"first\\nsecond\\n\" 2 nil)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_directory_files_file_attributes_listing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (probe-dir (expand-file-name "probe_dir/" dir)))
  (make-directory probe-dir t)
  (write-region "a\n" nil (expand-file-name "one.txt" probe-dir) nil 'silent)
  (write-region "b\n" nil (expand-file-name "two.txt" probe-dir) nil 'silent)
  (let ((listing (directory-files probe-dir nil "\\.txt\\'")))
    (prog1
        (list (sort listing #'string<)
              (length listing)
              (file-directory-p probe-dir)
              (file-attribute-type (file-attributes probe-dir))
              (eq (file-attribute-type (file-attributes probe-dir)) t))
      (delete-directory probe-dir t)))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}
