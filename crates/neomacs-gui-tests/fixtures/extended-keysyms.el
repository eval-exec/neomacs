;;; extended-keysyms.el --- Observe real native key binding dispatch -*- lexical-binding: t -*-
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil)
(defvar extended-keysyms-observations nil)
(defvar extended-keysyms-finished nil)

(defun extended-keysyms-finish ()
  (unless extended-keysyms-finished
    (setq extended-keysyms-finished t)
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode
               `((observations . ,(vconcat (nreverse extended-keysyms-observations)))))))
    (when (and (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
               (fboundp 'neomacs--write-frame-snapshot))
      (neomacs--write-frame-snapshot
       (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
    (run-at-time 0.3 nil (lambda () (kill-emacs 0)))))

(defun extended-keysyms-record ()
  (interactive)
  (push (vector (symbol-name last-command-event)
                (key-description (this-command-keys-vector)))
        extended-keysyms-observations)
  (insert (format "%s\n" last-command-event))
  (when (= (length extended-keysyms-observations) 32)
    (run-at-time 0.3 nil #'extended-keysyms-finish)))

;; Configure bindings during startup, as user init files do. Installing a
;; different buffer's local map from a timer would also exercise map refresh
;; while the command reader is already waiting, which is a separate contract.
(dolist (event (append (mapcar (lambda (n) (intern (format "f%d" n)))
                             (number-sequence 13 35))
                      '(undo redo menu XF86Back XF86Forward XF86Copy XF86Cut XF86Paste C-S-f20)))
  (global-set-key (vector event) #'extended-keysyms-record))

(defun extended-keysyms-start ()
  (switch-to-buffer (get-buffer-create "*extended-keysyms*"))
  (erase-buffer)
  (insert "Native extended-key bindings:\n")
  (modify-frame-parameters nil '((name . "extended-keysyms") (title . "extended-keysyms")))
  (redisplay t)
  (with-temp-file (getenv "NEOMACS_EXTENDED_KEYS_READY")
    (insert (number-to-string (emacs-pid))))
  ;; Flush partial observations as well: a silently dropped key must fail the
  ;; state assertion rather than leave the editor waiting indefinitely.
  (run-at-time 20 nil #'extended-keysyms-finish))

(run-at-time 0.5 nil #'extended-keysyms-start)
