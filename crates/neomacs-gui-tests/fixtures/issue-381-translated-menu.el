;;; issue-381-translated-menu.el --- CJK native popup redraw -*- lexical-binding: t -*-

(load (expand-file-name "native-menus.el" (file-name-directory load-file-name)))
;; Reduced from #381's translated Edit menu. Control does not change any
;; label, binding, enable predicate, selection, or shortcut in this fixture.
(let ((menu (make-sparse-keymap "文件")))
  (define-key menu [latin] '(menu-item "Select All" ignore :keys "C-a"))
  (define-key menu [paste] '(menu-item "从剪贴历史粘贴" ignore :keys "C-y"))
  (define-key menu [undo] '(menu-item "撤销" ignore :keys "C-x u"))
  (define-key global-map [menu-bar file] (list 'menu-item "文件" menu)))
(blink-cursor-mode -1)
(setq-default cursor-type nil)
(setq inhibit-startup-screen t)
