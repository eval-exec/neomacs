;;; future-only.el --- Incompatible package fixture  -*- lexical-binding: t; -*-

;; Version: 1.0
;; Package-Requires: ((emacs "99.0"))

;;; Code:

(defun future-only-command ()
  "Return a value that must never become available during this test."
  :future)

(provide 'future-only)

;;; future-only.el ends here
