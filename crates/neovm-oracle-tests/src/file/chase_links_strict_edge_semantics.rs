//! Oracle parity tests for GNU `file-chase-links` semantics.
//!
//! GNU implements this in `lisp/files.el`.  Unlike `file-truename`, it chases
//! only the final symlink chain and does not canonicalize symlinked parent
//! directories.  The optional numeric LIMIT caps how many final links are
//! followed.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_chase_links_limit_parent_symlink_and_cycle_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-chase-links-" t))
       (real-dir (expand-file-name "real" dir))
       (parent-link (expand-file-name "parent-link" dir))
       (plain (expand-file-name "plain.txt" real-dir))
       (target (expand-file-name "target.txt" dir))
       (link1 (expand-file-name "link1" dir))
       (link2 (expand-file-name "link2" dir))
       (up-link (expand-file-name "up-link" real-dir))
       (cycle-a (expand-file-name "cycle-a" dir))
       (cycle-b (expand-file-name "cycle-b" dir)))
  (unwind-protect
      (progn
        (make-directory real-dir)
        (write-region "plain" nil plain nil 'silent)
        (write-region "target" nil target nil 'silent)
        (make-symbolic-link "real" parent-link)
        (make-symbolic-link "target.txt" link1)
        (make-symbolic-link "link1" link2)
        (make-symbolic-link "../target.txt" up-link)
        (make-symbolic-link "cycle-b" cycle-a)
        (make-symbolic-link "cycle-a" cycle-b)
        (list
         ;; Full chase and explicit link-count limits.
         (file-relative-name (file-chase-links link2) dir)
         (file-relative-name (file-chase-links link2 1) dir)
         (file-relative-name (file-chase-links link2 0) dir)
         ;; Parent symlinks are not canonicalized when the leaf is not a
         ;; symlink.
         (file-relative-name
          (file-chase-links (expand-file-name "plain.txt" parent-link))
          dir)
         ;; A `..' target is handled by chasing the symlink's containing
         ;; directory, not by blindly using `expand-file-name`.
         (file-relative-name (file-chase-links up-link) dir)
         ;; LIMIT still prevents cycle errors.
         (file-relative-name (file-chase-links cycle-a 1) dir)
         (file-relative-name (file-chase-links cycle-a 2) dir)
         (condition-case err
             (file-chase-links cycle-a)
           (error (list (car err)
                        (and (stringp (cadr err))
                             (string-match-p
                              "\\`Apparent cycle of symbolic links for "
                              (cadr err))))))
         (condition-case err
             (file-chase-links 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-chase-links link1 "bad-limit")
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file cycle-b))
    (ignore-errors (delete-file cycle-a))
    (ignore-errors (delete-file up-link))
    (ignore-errors (delete-file link2))
    (ignore-errors (delete-file link1))
    (ignore-errors (delete-file target))
    (ignore-errors (delete-file plain))
    (ignore-errors (delete-file parent-link))
    (ignore-errors (delete-directory real-dir))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
