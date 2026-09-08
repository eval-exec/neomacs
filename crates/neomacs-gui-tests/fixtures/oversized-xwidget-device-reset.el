;;; oversized-xwidget-device-reset.el --- Static page GPU recovery -*- lexical-binding: t -*-

(load (expand-file-name "oversized-xwidget.el" (file-name-directory load-file-name))
      nil t)

;; The page has no animation or post-load DOM mutations. Replacing the device
;; must recover it without navigation, resize tricks, or fresh page damage.
(run-at-time 1.5 nil #'neomacs--debug-lose-device)
(run-at-time 2.5 nil #'neomacs-gui-oversized-xwidget-capture)
