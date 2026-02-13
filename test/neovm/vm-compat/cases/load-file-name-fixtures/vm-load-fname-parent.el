(setq vm-load-fname-parent-seen load-file-name)
(load (expand-file-name "vm-load-fname-child" (file-name-directory load-file-name)) nil 'nomessage)
(setq vm-load-fname-parent-after-child load-file-name)
(provide 'vm-load-fname-parent)
