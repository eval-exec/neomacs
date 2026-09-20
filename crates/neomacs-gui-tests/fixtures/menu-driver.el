;;; menu-driver.el --- Shared native desktop fixture -*- lexical-binding: t -*-
(load (expand-file-name "menu-interaction.el" (file-name-directory load-file-name)))
;; Keep the native discovery title stable across redisplay and buffer changes.
(setq frame-title-format "NEOMACS-MENU-REPRO")
;; Window-server bounds are also content bounds, on both native drivers.
;; This avoids treating a guessed macOS title-bar height as observed geometry.
(modify-frame-parameters nil '((undecorated . t)))
