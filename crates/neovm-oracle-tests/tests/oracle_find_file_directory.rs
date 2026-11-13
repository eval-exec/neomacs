//! `find-file-noselect` on a directory must run `find-directory-functions`
//! and open dired, for a plain directory, a trailing-slash name, a symlink to
//! a directory, and with `find-file-visit-truename` (GNU `lisp/files.el:2561`).
//! The hook argument is checked too: the plain name normally, the truename
//! when `find-file-visit-truename` is set. Reported as a failure in #657 and
//! #666; current main matches GNU, so this pins the contract.

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn find_file_noselect_on_a_directory_opens_dired_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let* ((dir (make-temp-file "neomacs-ff-dir-" t))
                  (link (concat dir "-link"))
                  (seen nil))
             (unwind-protect
                 (progn
                   (make-symbolic-link dir link)
                   (list
                    (with-current-buffer (find-file-noselect dir)
                      (derived-mode-p 'dired-mode))
                    (with-current-buffer (find-file-noselect (concat dir "/"))
                      (derived-mode-p 'dired-mode))
                    (with-current-buffer (find-file-noselect link)
                      (derived-mode-p 'dired-mode))
                    (let ((find-file-visit-truename t))
                      (with-current-buffer (find-file-noselect link)
                        (derived-mode-p 'dired-mode)))
                    (progn
                      (setq seen nil)
                      (let ((find-directory-functions
                             (list (lambda (name)
                                     (setq seen name)
                                     (current-buffer)))))
                        (find-file-noselect link))
                      (equal seen (abbreviate-file-name link)))
                    (progn
                      (setq seen nil)
                      (let ((find-file-visit-truename t)
                            (find-directory-functions
                             (list (lambda (name)
                                     (setq seen name)
                                     (current-buffer)))))
                        (find-file-noselect link))
                      (equal seen
                             (abbreviate-file-name (file-truename link))))))
               (when (file-exists-p link) (delete-file link))
               (delete-directory dir t)))"#,
        expect_test::expect![[r#""OK (dired-mode dired-mode dired-mode dired-mode t t)""#]],
    );
}

/// Failed opens still complete VISIT bookkeeping; directory rejection happens
/// after opening and must signal Read error without completing that visit.
#[cfg(unix)]
#[test]
fn insert_file_contents_directory_read_errors_do_not_complete_a_visit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(mapcar
 (lambda (operation)
   (list operation
(let ((dir (make-temp-file "neomacs-owned-directory-read-" t)))
  (unwind-protect
      (mapcar
       (lambda (spec)
         (let ((path (if (eq (car spec) 'directory)
                         dir
                       (expand-file-name "missing" dir)))
               (visit (cadr spec)))
           (with-temp-buffer
             (let ((outcome
                    (condition-case err
                        (progn (funcall operation path visit)
                               '(unexpected-success))
                      (error (list (car err) (nth 1 err) (nth 2 err)
                                   (equal (nth 3 err) path))))))
               (list (car spec) visit outcome
                     (not (null buffer-file-name))
                     (equal buffer-file-name path)
                     (buffer-modified-p) (buffer-size)
                     (visited-file-modtime))))))
       '((directory nil) (directory t) (missing nil) (missing t)))
    (delete-directory dir)))))
 '(insert-file-contents insert-file-contents-literally))"#,
        expect_test::expect![[
            r#""OK ((insert-file-contents ((directory nil (file-error \"Read error\" \"Is a directory\" t) nil nil nil 0 0) (directory t (file-error \"Read error\" \"Is a directory\" t) nil nil nil 0 0) (missing nil (file-missing \"Opening input file\" \"No such file or directory\" t) nil nil nil 0 0) (missing t (file-missing \"Opening input file\" \"No such file or directory\" t) t t nil 0 -1))) (insert-file-contents-literally ((directory nil (file-error \"Read error\" \"Is a directory\" t) nil nil nil 0 0) (directory t (file-error \"Read error\" \"Is a directory\" t) nil nil nil 0 0) (missing nil (file-missing \"Opening input file\" \"No such file or directory\" t) nil nil nil 0 0) (missing t (file-missing \"Opening input file\" \"No such file or directory\" t) t t nil 0 -1))))""#
        ]],
    );
}
