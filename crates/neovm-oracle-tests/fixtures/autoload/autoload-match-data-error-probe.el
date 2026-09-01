;;; autoload-match-data-error-probe.el --- Error-path oracle fixture -*- lexical-binding: t; -*-

(defun neovm--autoload-match-data-error-probe ()
  "Sentinel definition that must not survive the failed autoload."
  'unexpected-success)

(string-match "\\`autoload-error-clobber\\'" "autoload-error-clobber")
(error "Autoload match-data oracle fixture failure")

;;; autoload-match-data-error-probe.el ends here
