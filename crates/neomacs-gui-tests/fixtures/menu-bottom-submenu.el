;;; menu-bottom-submenu.el --- A child that must slide above its parent row -*- lexical-binding: t -*-
(load (expand-file-name "menu-corners.el" (file-name-directory load-file-name)))
(defun neomacs-menu-bottom-last ()
  (interactive)
  (neomacs-menu-record "bottom-last"))
(let ((child (make-sparse-keymap)))
  (define-key-after child [first] '(menu-item "First child" neomacs-menu-nested))
  (dotimes (index 6)
    (define-key-after child (vector (intern (format "filler-%d" index)))
      (list 'menu-item (format "Child filler %d" index) #'ignore :enable nil)))
  (define-key-after child [last] '(menu-item "Last child" neomacs-menu-bottom-last))
  (define-key-after neomacs-menu-test-map [bottom-child]
    (list 'menu-item "Bottom submenu" child)))
