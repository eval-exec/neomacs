;; -*- lexical-binding: t -*-
(unless (and (string= emacs-version "31.1")
             (equal (getenv "NEOVM_ORACLE_MODE") "refresh")
             (equal (getenv "UPDATE_EXPECT") "1"))
  (error "N4 HOF guard oracle requires GNU 31.1 and explicit refresh mode"))
(dolist (mapper '(mapc mapcar))
  (let ((canonical (symbol-function mapper)))
    (unwind-protect
        (progn
          (prin1 (eq (symbol-function mapper) canonical)) (terpri)
          (fset mapper (list 'lambda nil 123))
          (prin1 (eq (symbol-function mapper) canonical)) (terpri)
          (fmakunbound mapper)
          (prin1 (fboundp mapper)) (terpri))
      (fset mapper canonical))))
