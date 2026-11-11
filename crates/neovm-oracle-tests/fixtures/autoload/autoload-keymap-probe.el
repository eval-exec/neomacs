;;; autoload-keymap-probe.el --- Prefix keymap oracle fixture -*- lexical-binding: t; -*-

(when (bound-and-true-p neovm--autoload-keymap-gc)
  (garbage-collect))

(defvar neovm--autoload-keymap-load-count 0)
(setq neovm--autoload-keymap-load-count (1+ neovm--autoload-keymap-load-count))

(let ((map (make-sparse-keymap))
      (nested (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key parent [?x] 'neovm--autoload-inherited-command)
  (set-keymap-parent nested parent)
  (define-key nested [?x] 'neovm--autoload-own-command)
  (define-key map [?n] nested)
  (fset 'neovm--autoload-keymap-prefix map))

(provide 'autoload-keymap-probe)

;;; autoload-keymap-probe.el ends here
