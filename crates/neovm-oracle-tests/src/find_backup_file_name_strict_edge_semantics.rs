//! Oracle parity tests for GNU numbered backup selection semantics.
//!
//! GNU implements `find-backup-file-name` and `backup-extract-version` in
//! `lisp/files.el`.  The numbered backup path depends on existing filename
//! completions, `version-control`, `kept-old-versions`, and
//! `kept-new-versions`; the new backup itself counts as one kept new version.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_find_backup_file_name_numbered_version_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-find-backup-name-" t))
       (file (expand-file-name "tracked.txt" root))
       (other (expand-file-name "other.txt" root)))
  (cl-labels
      ((rel (name)
            (and (stringp name)
                 (file-relative-name name root)))
       (rel-result (result)
            (and result
                 (cons (rel (car result))
                       (mapcar #'rel (cdr result))))))
    (unwind-protect
        (progn
          (write-region "tracked" nil file nil 'silent)
          (write-region "other" nil other nil 'silent)
          (write-region "v1" nil (expand-file-name "tracked.txt.~1~" root) nil 'silent)
          (write-region "v2" nil (expand-file-name "tracked.txt.~2~" root) nil 'silent)
          (write-region "v4" nil (expand-file-name "tracked.txt.~4~" root) nil 'silent)
          (write-region "bad" nil (expand-file-name "tracked.txt.~bad~" root) nil 'silent)
          (list
           ;; Existing numbered backups force numbered mode even when
           ;; `version-control' is nil.
           (rel-result
            (let ((version-control nil)
                  (kept-old-versions 1)
                  (kept-new-versions 1)
                  (backup-directory-alist nil))
              (find-backup-file-name file)))
           ;; Forced numbered mode starts at .~1~ with no deletion suggestions
           ;; when no existing numbered backup exists.
           (rel-result
            (let ((version-control t)
                  (kept-old-versions 1)
                  (kept-new-versions 1)
                  (backup-directory-alist nil))
              (find-backup-file-name other)))
           ;; `never' bypasses numbered backups and returns the nonnumeric
           ;; backup name from `make-backup-file-name'.
           (rel-result
            (let ((version-control 'never)
                  (backup-directory-alist nil))
              (find-backup-file-name file)))
           ;; `backup-extract-version' uses the dynamically bound start index:
           ;; exact numeric backup, malformed suffix, and too-early match.
           (let ((backup-extract-version-start (length "tracked.txt.~")))
             (mapcar #'backup-extract-version
                     '("tracked.txt.~123~" "tracked.txt.~bad~" "xtracked.txt.~5~")))
           (condition-case err
               (find-backup-file-name)
             (error (list (car err) (cdr err))))
           (condition-case err
               (find-backup-file-name file 'extra)
             (error (list (car err) (cdr err))))
           (condition-case err
               (backup-extract-version)
             (error (list (car err) (cdr err)))))))
      (ignore-errors (delete-directory root t)))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
