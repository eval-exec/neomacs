;;; autoload-keymap-positioned-no-progress-probe.el --- Alias progress fixture -*- lexical-binding: t; -*-

(setq neovm--autoload-positioned-repeat-count
      (1+ neovm--autoload-positioned-repeat-count))
(when (> neovm--autoload-positioned-repeat-count 2)
  (error "Autoload positioned alias repeated"))
(fset 'neovm--autoload-positioned-repeat-prefix
      (position-symbol 'neovm--autoload-positioned-repeat-target 11))

;;; autoload-keymap-positioned-no-progress-probe.el ends here
