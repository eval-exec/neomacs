;;; menu-driver-tall.el --- Shared constrained submenu fixture -*- lexical-binding: t -*-
(load (expand-file-name "menu-bottom-submenu.el" (file-name-directory load-file-name)))
(setq frame-title-format "NEOMACS-MENU-REPRO")
(modify-frame-parameters nil '((undecorated . t)))
