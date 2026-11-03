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
