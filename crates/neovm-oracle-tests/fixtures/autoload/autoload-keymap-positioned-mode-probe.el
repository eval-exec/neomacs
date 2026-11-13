;;; autoload-keymap-positioned-mode-probe.el --- Changed symbol mode fixture -*- lexical-binding: t; -*-

(setq symbols-with-pos-enabled t)
(let ((map (make-sparse-keymap)))
  (define-key map [?x] 'neovm--autoload-positioned-mode-command)
  (setcar map (position-symbol 'keymap 29))
  (fset 'neovm--autoload-positioned-mode-target map))
(fset 'neovm--autoload-positioned-mode-prefix
      (position-symbol 'neovm--autoload-positioned-mode-target 17))

;;; autoload-keymap-positioned-mode-probe.el ends here
