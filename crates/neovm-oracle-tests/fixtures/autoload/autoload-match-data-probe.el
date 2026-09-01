;;; autoload-match-data-probe.el --- Autoload state oracle fixture -*- lexical-binding: t; -*-

(defun neovm--autoload-match-data-probe ()
  "Return a sentinel after this file is autoloaded."
  'autoload-loaded)

;; Loading arbitrary Lisp is allowed to change match data.  Autoloading this
;; file, however, must restore its caller's match data around the load.
(string-match "\\`autoload-clobber\\'" "autoload-clobber")

(provide 'autoload-match-data-probe)

;;; autoload-match-data-probe.el ends here
