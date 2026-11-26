;;; ctxmenu-popup.el --- M-CTXMENU: popup menus from read_char and list forms -*- lexical-binding: t; -*-
;;
;; Repro fixture for the right-click popup menu report (2026-11-20).  Runs
;; unchanged on GNU Emacs and Neomacs; the host injects X11 input and reads
;; NEOMACS_CTXMENU_RESULT after touching NEOMACS_CTXMENU_STOP.
;;
;; Case 1 binds [down-mouse-3] to a menu KEYMAP, the `context-menu-entry'
;; shape (lisp/mouse.el:689-703): GNU pops it from read_char itself
;; (src/keyboard.c:2921 -> read_char_x_menu_prompt at :10094), so the
;; DOWN click opens the menu before any up event is read.
;; Case 2 pops the old-fashioned list menu by calling `x-popup-menu'
;; directly (src/menu.c:1352-1364, `list_of_panes').
;; Case 2b pops a list of keymaps (src/menu.c:1318-1345, `keymap_panes'
;; over several maps, the `mouse-buffer-menu' shape).
;; Case 3 enables `context-menu-mode', the user-reported scenario; the
;; host types C-c c to arm it after the earlier cases.

(menu-bar-mode -1)
(tool-bar-mode -1)
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))
(when (fboundp 'blink-cursor-mode)
  (blink-cursor-mode -1))
(switch-to-buffer (get-buffer-create "ctxmenu"))
(erase-buffer)
(dotimes (_i 10)
  (insert "context menu repro line\n"))
(goto-char (point-min))

(defvar ctxmenu-events nil)
(defun ctxmenu-record (event)
  (push event ctxmenu-events))
(defun ctxmenu-choose ()
  (interactive)
  (ctxmenu-record 'keymap-choose))
(defun ctxmenu-other-choose ()
  (interactive)
  (ctxmenu-record 'keymap-list-choose))

(defvar ctxmenu-map (make-sparse-keymap "Context"))
(define-key ctxmenu-map [choose] '(menu-item "Repro Choose" ctxmenu-choose))
(defvar ctxmenu-other-map (make-sparse-keymap "Other"))
(define-key ctxmenu-other-map [other] '(menu-item "Other Choose" ctxmenu-other-choose))

;; Case 1: the binding IS the menu keymap.  The up event must not run a
;; command after the menu selection, like `context-menu-mode-map'.
(define-key global-map [down-mouse-3] ctxmenu-map)
(define-key global-map [mouse-3] #'ignore)
;; The up event must not run a command or echo "undefined" after each
;; case, matching how `context-menu-mode-map' treats its button.
(define-key global-map [mouse-2] #'ignore)

;; Cases 2 and 2b: a command pops the list form first, then the keymap
;; list.  `t' positions the menu at the pointer, so the host controls
;; where it opens.  The return value is recorded; a menu that never
;; displays returns nil without selecting anything.
(defvar ctxmenu-list-step 0)
(defun ctxmenu-list-menu ()
  (interactive)
  (setq ctxmenu-list-step (1+ ctxmenu-list-step))
  (ctxmenu-record
   (condition-case err
       (list 'call ctxmenu-list-step
             (if (= ctxmenu-list-step 1)
                 (x-popup-menu t
                               '("Repro Title"
                                 ("Pane"
                                  ("Choose Item" . choose))))
               (x-popup-menu t (list ctxmenu-map ctxmenu-other-map))))
     (error (list 'error ctxmenu-list-step (error-message-string err))))))
(define-key global-map [down-mouse-2] #'ctxmenu-list-menu)

;; Case 3: arm `context-menu-mode' on demand so its minor-mode map does
;; not shadow the case 1 binding during the earlier cases.
(defun ctxmenu-arm-context-mode ()
  (interactive)
  (when (fboundp 'context-menu-mode)
    (context-menu-mode 1))
  (ctxmenu-record (list 'context-menu-mode-armed
                        (if (fboundp 'context-menu-mode) t nil))))
(global-set-key "\C-cc" #'ctxmenu-arm-context-mode)

(defun ctxmenu-stop ()
  (when (file-exists-p (getenv "NEOMACS_CTXMENU_STOP"))
    (with-temp-file (getenv "NEOMACS_CTXMENU_RESULT")
      (prin1 (reverse ctxmenu-events) (current-buffer)))
    (kill-emacs 0)))
(run-at-time 0.2 0.05 #'ctxmenu-stop)
;; Watchdog: the host stops the editor through the control file; without
;; host input, exit through the same path instead of hanging the run.
(run-with-timer 45 nil #'ctxmenu-stop)
(with-temp-file (getenv "NEOMACS_CTXMENU_READY")
  (insert "ready"))
