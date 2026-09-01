//! Oracle parity tests for GNU `directory-files-and-attributes` semantics.
//!
//! GNU implements this in `src/dired.c` by reusing
//! `directory_files_internal` with ATTRS enabled.  The attribute cdr is the
//! same 12-element list returned by `file-attributes`; this test normalizes
//! volatile inode/time/device values while preserving the semantic shape.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_files_and_attributes_shape_id_format_count_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir (make-temp-file "neomacs-oracle-dirattrs-" t)))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "sub" dir))
        (with-temp-file (expand-file-name "alpha.txt" dir)
          (insert "alpha"))
        (with-temp-file (expand-file-name "zeta.txt" dir)
          (insert "zeta!"))
        (with-temp-file (expand-file-name "beta.el" dir)
          (insert "beta"))
        (let* ((dirslash (file-name-as-directory dir))
               (txt (directory-files-and-attributes
                     dir nil "\\.txt\\'" nil 'integer))
               (txt-full (directory-files-and-attributes
                          dir t "\\.txt\\'" nil 'integer))
               (txt-string-id (directory-files-and-attributes
                               dir nil "\\.txt\\'" nil 'string))
               (dot-entries (directory-files-and-attributes
                             dir nil "^\\." nil 'integer)))
          (list
           (mapcar
            (lambda (cell)
              (let ((attrs (cdr cell)))
                (list (car cell)
                      (nth 0 attrs)
                      (integerp (nth 1 attrs))
                      (integerp (nth 2 attrs))
                      (integerp (nth 3 attrs))
                      (consp (nth 4 attrs))
                      (consp (nth 5 attrs))
                      (consp (nth 6 attrs))
                      (nth 7 attrs)
                      (substring (nth 8 attrs) 0 1)
                      (nth 9 attrs)
                      (integerp (nth 10 attrs))
                      (integerp (nth 11 attrs))
                      (length attrs))))
            txt)
           (mapcar #'file-name-nondirectory (mapcar #'car txt-full))
           (mapcar (lambda (cell) (string-prefix-p dirslash (car cell))) txt-full)
           (mapcar
            (lambda (cell)
              (let ((attrs (cdr cell)))
                (list (car cell)
                      (or (stringp (nth 2 attrs)) (integerp (nth 2 attrs)))
                      (or (stringp (nth 3 attrs)) (integerp (nth 3 attrs))))))
            txt-string-id)
           (mapcar (lambda (cell) (list (car cell) (nth 0 (cdr cell)))) dot-entries)
           (directory-files-and-attributes dir nil nil nil 'integer 0)
           (length (directory-files-and-attributes dir nil "\\.txt\\'" nil 'integer 1))
           (condition-case err
               (directory-files-and-attributes dir nil 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files-and-attributes dir nil nil nil 'integer -1)
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files-and-attributes 42)
             (error (list (car err) (cdr err)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((\"alpha.txt\" nil t t t t t t 5 \"-\" t t t 12) (\"zeta.txt\" nil t t t t t t 5 \"-\" t t t 12)) (\"alpha.txt\" \"zeta.txt\") (t t) ((\"alpha.txt\" t t) (\"zeta.txt\" t t)) ((\".\" t) (\"..\" t)) nil 1 (wrong-type-argument (stringp 42)) (wrong-type-argument (wholenump -1)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_attribute_timestamps_respect_current_time_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-attrs-time-" t))
       (file (expand-file-name "sample" dir)))
  (unwind-protect
      (progn
        (write-region "" nil file nil 'silent)
        (set-file-times file '(0 1000 0 0))
        (list
         (let ((current-time-list nil))
           (nth 5 (file-attributes file)))
         (let ((current-time-list nil))
           (nth 5
                (cdr (assoc "sample"
                            (directory-files-and-attributes
                             dir nil "\\`sample\\'" nil 'integer)))))
         (let ((current-time-list t))
           (nth 5 (file-attributes file)))
         (let ((current-time-list t))
           (nth 5
                (cdr (assoc "sample"
                            (directory-files-and-attributes
                             dir nil "\\`sample\\'" nil 'integer)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1000000000000 . 1000000000) (1000000000000 . 1000000000) (0 1000 0 0) (0 1000 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
