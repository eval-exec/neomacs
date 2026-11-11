;;; autoload-keymap-chain-probe.el --- Redirected prefix oracle fixture -*- lexical-binding: t; -*-

(defvar neovm--autoload-keymap-chain-count 0)
(setq neovm--autoload-keymap-chain-count (1+ neovm--autoload-keymap-chain-count))
(autoload 'neovm--autoload-keymap-prefix "autoload-keymap-probe" nil t 'keymap)

;;; autoload-keymap-chain-probe.el ends here
