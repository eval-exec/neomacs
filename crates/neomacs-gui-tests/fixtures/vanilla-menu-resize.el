;;; vanilla-menu-resize.el --- Vanilla menu/native resize observation -*- lexical-binding: t -*-

;; Keep the standard menus, toolbar, scratch text and faces. The host supplies
;; physical menu input and native resize events, and observes pixels. A Lisp
;; polling timer would be unrelated input that could conceal stale redisplay.
(require 'json)
(blink-cursor-mode -1)
(setq frame-resize-pixelwise t)
(modify-frame-parameters nil '((title . "NEOMACS-VANILLA-MENU-RESIZE")))
(global-set-key [f12] (lambda () (interactive) (kill-emacs 0)))

(run-at-time
 0.3 nil
 (lambda ()
   (let* ((directory (getenv "NEOMACS_GUI_MENU_CONTROL"))
          (pending (expand-file-name "state.pending" directory))
          (ready (expand-file-name "state.json" directory)))
     (with-temp-file pending
       (insert (json-encode `((char-width . ,(frame-char-width))
                              (char-height . ,(frame-char-height))))))
     (rename-file pending ready t))))

;; Failure cleanup only; no periodic observer or forced redisplay runs while
;; the host checks the idle resize. Successful host runs exit through the
;; standard quit chord (C-x C-c); the [f12] binding remains for manual use.
(run-at-time 45 nil (lambda () (kill-emacs 3)))

;;; vanilla-menu-resize.el ends here
