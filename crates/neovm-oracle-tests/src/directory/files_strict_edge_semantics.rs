//! Oracle parity tests for GNU `directory-files` edge semantics.
//!
//! GNU implements `directory-files` in `src/dired.c`: it expands the input
//! directory, filters decoded basenames with MATCH, applies COUNT while
//! scanning, includes dot entries from the directory stream, and sorts the
//! final list with `string-lessp` unless NOSORT is non-nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_files_listing_filter_full_count_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir (make-temp-file "neomacs-oracle-dirfiles-" t)))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "sub" dir))
        (dolist (name '("alpha.txt" "beta.el" "zeta.txt" ".hidden"))
          (with-temp-file (expand-file-name name dir)
            (insert name)))
        (let* ((dirslash (file-name-as-directory dir))
               (full-txt (directory-files dir t "\\.txt\\'" nil))
               (nosort (directory-files dir nil nil t)))
          (list
           (directory-files dir nil nil nil)
           (directory-files dir nil "\\.txt\\'" nil)
           (directory-files dir nil "^\\." nil)
           (mapcar #'file-name-nondirectory full-txt)
           (mapcar (lambda (name) (string-prefix-p dirslash name)) full-txt)
           (sort (copy-sequence nosort) #'string-lessp)
           (directory-files dir nil nil nil 0)
           (length (directory-files dir nil "\\.txt\\'" nil 1))
           (condition-case err
               (directory-files dir nil 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files dir nil nil nil -1)
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files 42)
             (error (list (car err) (cdr err)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\".\" \"..\" \".hidden\" \"alpha.txt\" \"beta.el\" \"sub\" \"zeta.txt\") (\"alpha.txt\" \"zeta.txt\") (\".\" \"..\" \".hidden\") (\"alpha.txt\" \"zeta.txt\") (t t) (\".\" \"..\" \".hidden\" \"alpha.txt\" \"beta.el\" \"sub\" \"zeta.txt\") nil 1 (wrong-type-argument (stringp 42)) (wrong-type-argument (wholenump -1)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
