;;; font-name-pattern-resize.el --- Resolve and resize a pinned font -*- lexical-binding: t -*-

;; Exercise the public operation Doom's font normalization needs, without
;; loading an external Doom installation. Both editors use the same Hack face.
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defun font672-pattern (size &optional foundry)
  (format "-%s-Hack-regular-normal-normal-*-%d-*-*-*-*-0-iso10646-1"
          (or foundry "SRC") size))

(defun font672-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font672-probe ()
  (condition-case err
      (let* ((pattern (font672-pattern 13))
             (names (x-list-fonts pattern nil nil 1))
             ;; On the broken implementation this is the reporter's exact
             ;; "No fonts match" failure, despite the installed Hack face.
             (small (x-resolve-font-name pattern))
             (large (x-resolve-font-name (font672-pattern 21)))
             (case-name (x-resolve-font-name (downcase pattern)))
             (family-name (x-resolve-font-name "Hack-13"))
             (normalized
              (x-resolve-font-name
               (font-xlfd-name (font-spec :family "Hack" :size 13))))
             (zero-is-nil (null (x-list-fonts pattern nil nil 0)))
             (negative
              (condition-case problem
                  (progn (x-list-fonts pattern nil nil -1) 'no-error)
                (error (car problem))))
             (missing-is-nil
              (null (x-list-fonts
                     (font672-pattern 13 "NeomacsMissingFoundry672") nil nil 1))))
        (unless (and (display-graphic-p) (= (length names) 1)
                     (equal small (car names)) (equal small case-name)
                     (stringp family-name) (stringp normalized)
                     zero-is-nil missing-is-nil
                     (eq negative 'wrong-type-argument))
          (error "Font pattern contract failed: %S"
                 (list names small case-name family-name normalized
                       zero-is-nil negative missing-is-nil)))
        (unless (and (= (font-get (font-spec :name small) :size) 13)
                     (= (font-get (font-spec :name large) :size) 21)
                     (= (font-get (font-spec :name normalized) :size) 13))
          (error "Scalable XLFD size was not preserved: %S" (list small large normalized)))
        (switch-to-buffer (get-buffer-create "*font-pattern-resize*"))
        (insert "Hack font-name resolution and resize\n0123456789\n")
        (set-frame-font small t t)
        (redisplay t)
        (let ((small-height (frame-char-height))
              (small-width (frame-char-width)))
          (set-frame-font large t t)
          (redisplay t)
          (unless (and (> (frame-char-height) small-height)
                       (> (frame-char-width) small-width))
            (error "Resolved larger font did not resize metrics: %S"
                   (list small-height small-width
                         (frame-char-height) (frame-char-width))))
          (let ((large-height (frame-char-height))
                (large-width (frame-char-width)))
            (set-frame-font small t t)
            (redisplay t)
            (unless (and (= (frame-char-height) small-height)
                         (= (frame-char-width) small-width))
              (error "Restoring smaller font did not restore metrics: %S"
                     (list small-height small-width
                           (frame-char-height) (frame-char-width))))
            (font672-write
             `((passed . t) (small-name . ,small) (large-name . ,large)
               (family-name . ,family-name) (normalized-name . ,normalized)
               (case-insensitive . ,(equal small case-name))
               (maximum-zero . ,zero-is-nil)
               (negative-condition . ,(symbol-name negative))
               (missing-foundry . ,missing-is-nil)
               (sizes . [13 21 13]) (small-height . ,small-height)
               (small-width . ,small-width) (large-height . ,large-height)
               (large-width . ,large-width)
               (restored-height . ,(frame-char-height))
               (restored-width . ,(frame-char-width))))))
        (when (fboundp 'neomacs--write-frame-snapshot)
          (neomacs--write-frame-snapshot
           (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
        (run-at-time 0.2 nil (lambda () (kill-emacs 0))))
    (error
     (font672-write `((passed . :json-false) (error . ,(format "%S" err))))
     (princ (format "Font pattern GUI failure: %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(run-at-time 0.7 nil #'font672-probe)
