;;; COLRv1 layered-color emoji drawing probe. -*- lexical-binding: t -*-
;;; Issue #542: the emoji face is selected and the cells are reserved, but a
;;; COLRv1 font draws nothing.  The readback diagnostics report per-cell pixel
;;; averages, so the cells are given a non-grayscale foreground — the
;;; diagnostics only report colorful faces.
(require 'json)

(defun colrv1-emoji-probe ()
  (switch-to-buffer (get-buffer-create "*colrv1*"))
  (set-frame-font (font-spec :family "Spleen" :size 16))
  (erase-buffer)
  (insert (propertize "\U0001F347" 'face '(:foreground "blue"))
          " "
          (propertize "\U0001F34E" 'face '(:foreground "blue")))
  (redisplay t)
  ;; The same facts the reporter collected: which font the cells resolve to.
  ;; `font-at` already reported the emoji face before the fix, so the state
  ;; alone never distinguished "selected" from "drawn" — the readback does.
  (let ((result `((grape-font . ,(font-get (font-at 1) :family))
                  (apple-font . ,(font-get (font-at 3) :family))
                  (native-engine . ,(if (fboundp 'neomacs--write-frame-snapshot) t :json-false)))))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode result)))))

(add-hook 'window-setup-hook (lambda () (run-at-time 1 nil #'colrv1-emoji-probe)))
(add-hook 'window-setup-hook (lambda () (run-at-time 6 nil #'kill-emacs 0)) 90)
;;; init.el ends here
