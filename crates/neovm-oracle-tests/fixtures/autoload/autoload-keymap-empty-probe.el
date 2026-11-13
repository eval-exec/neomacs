;;; autoload-keymap-empty-probe.el --- No-progress prefix fixture -*- lexical-binding: t; -*-

;; Deliberately leave the promised function definition untouched.
(setq neovm--autoload-keymap-empty-count
      (1+ neovm--autoload-keymap-empty-count))

(when (bound-and-true-p neovm--autoload-keymap-reinstall)
  (fset 'neovm--autoload-keymap-empty-prefix
        (copy-tree (symbol-function 'neovm--autoload-keymap-empty-prefix))))

;;; autoload-keymap-empty-probe.el ends here
