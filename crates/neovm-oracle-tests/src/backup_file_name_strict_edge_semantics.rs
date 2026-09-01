//! Oracle parity tests for GNU backup file-name semantics.
//!
//! GNU implements these helpers in `lisp/files.el`.  `make-backup-file-name`
//! delegates through `make-backup-file-name-function`; the default function
//! applies `backup-directory-alist`, creates matched backup directories, and
//! encodes absolute backup names by replacing directory separators with `!`
//! while doubling literal `!` characters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_backup_file_name_default_and_directory_alist_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'cl-lib)
(let* ((root (make-temp-file "neomacs-oracle-backup-name-" t))
       (sub (expand-file-name "sub" root))
       (plain (expand-file-name "plain.txt" sub))
       (bang (expand-file-name "bang!file.txt" sub))
       (absolute-backups (expand-file-name "abs-backups" root))
       (relative-backups nil))
  (cl-labels
      ((rel (name)
            (and (stringp name)
                 (file-relative-name name root))))
    (unwind-protect
        (progn
          (make-directory sub)
          (write-region "plain" nil plain nil 'silent)
          (write-region "bang" nil bang nil 'silent)
          (list
           ;; Plain default: append "~" in the original directory.
           (rel (let ((backup-directory-alist nil)
                      (make-backup-file-name-function nil))
                  (make-backup-file-name plain)))
           ;; Function override is called with the original FILE name.
           (rel (let ((make-backup-file-name-function
                       (lambda (file) (concat file ".bak")))
                      (backup-directory-alist nil))
                  (make-backup-file-name plain)))
           ;; Relative backup-directory-alist entries are relative to FILE's
           ;; directory, not `default-directory`, and are created on demand.
           (let* ((backup-directory-alist '(("plain" . "backups")))
                  (make-backup-file-name-function nil)
                  (name (make-backup-file-name plain))
                  (dir (file-name-directory name)))
             (setq relative-backups dir)
             (list (rel name)
                   (file-directory-p dir)))
           ;; Absolute backup directories encode the whole absolute file name.
           ;; Normalize away ROOT so random temp prefixes do not affect parity,
           ;; but keep GNU's slash-to-! and literal-!-doubling rules visible.
           (let* ((backup-directory-alist `(("bang" . ,absolute-backups)))
                  (make-backup-file-name-function nil)
                  (name (make-backup-file-name bang))
                  (relative-name (rel name))
                  (encoded (file-name-nondirectory name))
                  (root-encoded (subst-char-in-string
                                 ?/ ?! (string-replace "!" "!!" root))))
             (list (string-replace root-encoded "<root>" relative-name)
                   (file-directory-p absolute-backups)
                   (string-prefix-p root-encoded encoded)
                   (string-suffix-p "!sub!bang!!file.txt~" encoded)))
           ;; `backup-file-name-p` is just the trailing-tilde regexp; its
           ;; return value is the match index, not canonical t.
           (mapcar #'backup-file-name-p
                   '("plain.txt" "plain.txt~" "plain.txt.~1~" "tilde~/child"))
           (condition-case err
               (backup-file-name-p 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (make-backup-file-name)
             (error (list (car err) (cdr err))))
           (condition-case err
               (make-backup-file-name plain 'extra)
             (error (list (car err) (cdr err))))))
      (ignore-errors (delete-file plain))
      (ignore-errors (delete-file bang))
      (when relative-backups
        (ignore-errors (delete-directory relative-backups t)))
      (ignore-errors (delete-directory absolute-backups t))
      (ignore-errors (delete-directory sub t))
      (ignore-errors (delete-directory root t)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"sub/plain.txt~\" \"sub/plain.txt.bak\" (\"sub/backups/plain.txt~\" t) (\"abs-backups/<root>!sub!bang!!file.txt~\" t t t) (nil 9 12 nil) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((1 . 1) 0)) (wrong-number-of-arguments ((1 . 1) 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
