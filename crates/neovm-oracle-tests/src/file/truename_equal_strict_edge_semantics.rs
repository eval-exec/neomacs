//! Oracle parity tests for GNU `file-truename` and `file-equal-p` semantics.
//!
//! GNU implements `file-truename` and `file-equal-p` in `lisp/files.el`.
//! `file-truename` recursively resolves parent directory symlinks before the
//! leaf, preserves missing suffixes after the last existing component, and
//! treats `.` / `..` after resolving the parent directory.  `file-equal-p`
//! compares `file-attributes` after resolving truenames.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_truename_parent_symlink_missing_tail_and_file_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-truename-" t))
       (real-dir (expand-file-name "real" dir))
       (nested-dir (expand-file-name "nested" real-dir))
       (target (expand-file-name "target.txt" nested-dir))
       (link-dir (expand-file-name "link-dir" dir))
       (link-file (expand-file-name "link-file" dir))
       (dangling-link (expand-file-name "dangling-link" dir)))
  (unwind-protect
      (progn
        (make-directory nested-dir t)
        (write-region "target" nil target nil 'silent)
        (make-symbolic-link "real" link-dir)
        (make-symbolic-link "real/nested/target.txt" link-file)
        (make-symbolic-link "real/nested/missing.txt" dangling-link)
        (list
         ;; Direct symlink leaf resolution.
         (file-relative-name (file-truename link-file) dir)
         ;; Parent-directory symlink resolution before the leaf.
         (file-relative-name
          (file-truename (expand-file-name "nested/target.txt" link-dir))
          dir)
         ;; Missing suffixes are preserved after resolving existing symlinked
         ;; parents.
         (file-relative-name
          (file-truename (expand-file-name "nested/missing-tail.txt" link-dir))
          dir)
         ;; `.' and `..' are interpreted after parent truename resolution.
         (file-relative-name
          (directory-file-name (file-truename (expand-file-name "." link-dir)))
          dir)
         (file-relative-name
          (directory-file-name (file-truename (expand-file-name ".." link-dir)))
          dir)
         ;; `file-equal-p' resolves truenames before comparing attributes.
         (file-equal-p target link-file)
         (file-equal-p target (expand-file-name "nested/target.txt" link-dir))
         (file-equal-p target dangling-link)
         (file-equal-p target (expand-file-name "missing.txt" dir))
         (condition-case err
             (file-truename 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-equal-p target 42)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file dangling-link))
    (ignore-errors (delete-file link-file))
    (ignore-errors (delete-file link-dir))
    (ignore-errors (delete-file target))
    (ignore-errors (delete-directory nested-dir))
    (ignore-errors (delete-directory real-dir))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"real/nested/target.txt\" \"real/nested/target.txt\" \"real/nested/missing-tail.txt\" \"real\" \".\" t t nil nil (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_truename_empty_dot_and_symlink_cycle_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-truename-cycle-" t))
       (default-directory (file-name-as-directory dir))
       (sub (expand-file-name "sub" dir))
       (self (expand-file-name "self" dir))
       (cycle-a (expand-file-name "cycle-a" dir))
       (cycle-b (expand-file-name "cycle-b" dir)))
  (unwind-protect
      (progn
        (make-directory sub)
        (make-symbolic-link "self" self)
        (make-symbolic-link "cycle-b" cycle-a)
        (make-symbolic-link "cycle-a" cycle-b)
        (list
         ;; GNU's `file-truename` first expands empty and dot names against
         ;; `default-directory`; the directory spelling is still observable
         ;; via `file-relative-name`.
         (mapcar (lambda (name)
                   (file-relative-name (file-truename name) dir))
                 '("" "." "./" "sub/.." "sub/."))
         ;; Both self-links and multi-link cycles signal `error`.  The final
         ;; pathname in the message depends on where the cycle is detected, so
         ;; assert the GNU message shape instead of hard-coding /tmp names.
         (condition-case err
             (file-truename self)
           (error (list (car err)
                        (and (stringp (cadr err))
                             (string-match-p
                              "\\`Apparent cycle of symbolic links for "
                              (cadr err))))))
         (condition-case err
             (file-truename cycle-a)
           (error (list (car err)
                        (and (stringp (cadr err))
                             (string-match-p
                              "\\`Apparent cycle of symbolic links for "
                              (cadr err)))))))))
    (ignore-errors (delete-file self))
    (ignore-errors (delete-file cycle-a))
    (ignore-errors (delete-file cycle-b))
    (ignore-errors (delete-directory sub))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
