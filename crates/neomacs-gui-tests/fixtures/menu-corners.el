;;; menu-corners.el --- Menus large enough to require bottom-edge adjustment -*- lexical-binding: t -*-
(load (expand-file-name "menu-interaction.el" (file-name-directory load-file-name)))
(dotimes (index 6)
  (define-key-after neomacs-menu-test-map (vector (intern (format "extra-%d" index)))
    (list 'menu-item (format "Extra %d" index) #'ignore :enable nil)))
