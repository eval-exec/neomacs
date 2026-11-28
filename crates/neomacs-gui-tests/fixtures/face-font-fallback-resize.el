;;; face-font-fallback-resize.el --- #672 face-font fallback and XLFD resize -*- lexical-binding: t -*-

;; Exercise the exact failure mode reported in #672 (Doom `SPC t b` /
;; `doom-adjust-font-size`): resizing `default`, `fixed-pitch`,
;; `fixed-pitch-serif` (:family "Monospace Serif"), `variable-pitch`, and an
;; unmapped missing-family face by reading `(face-font face)`, replacing the
;; XLFD pixel size, and resolving with `x-resolve-font-name`.
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defun font672-fb-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font672-fb-resize-xlfd (xlfd size)
  "Replace the XLFD pixel-size slot (field 6) with SIZE."
  (let ((vec (x-decompose-font-name xlfd)))
    (unless vec
      (error "x-decompose-font-name failed on %S" xlfd))
    (aset vec 6 (number-to-string size))
    (x-compose-font-name vec)))

(defun font672-fb-probe ()
  (condition-case err
      (progn
        ;; Map "Monospace Serif" (fixed-pitch-serif) to our pinned second font
        ;; ("DejaVu Sans Mono"), matching stock `lisp/faces.el` mapping
        ;; "Monospace Serif" to an installed alternative family.
        (setq face-font-family-alternatives
              (cons '("Monospace Serif" "Courier 10 Pitch" "DejaVu Sans Mono")
                    (assoc-delete-all "Monospace Serif"
                                      (copy-alist face-font-family-alternatives))))
        (internal-set-alternative-font-family-alist face-font-family-alternatives)
        (defface neo-672-missing-face
          '((t :family "NoSuchFamilyFor672"))
          "Face with an unmapped missing family for #672.")
        (set-frame-font "-SRC-Hack-regular-normal-normal-*-14-*-*-*-m-0-iso10646-1" t t)
        (redisplay t)
        (let* ((faces '(default fixed-pitch fixed-pitch-serif variable-pitch neo-672-missing-face))
               (rows
                (mapcar
                 (lambda (face)
                   (let* ((ff (face-font face))
                          (pat13 (font672-fb-resize-xlfd ff 13))
                          (pat21 (font672-fb-resize-xlfd ff 21))
                          (res13 (x-resolve-font-name pat13))
                          (res21 (x-resolve-font-name pat21)))
                     (cons face
                           `((face-font . ,ff)
                             (resolved-13 . ,res13)
                             (resolved-21 . ,res21)))))
                 faces))
               (direct-missing-resolve
                (condition-case e
                    (progn
                      (x-resolve-font-name
                       "-PfEd-Monospace Serif-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1")
                      "no-error")
                  (error (format "%s:%s" (car e) (cadr e)))))
               (direct-missing-set-frame
                (condition-case e
                    (progn
                      (set-frame-font "NoSuchFamilyFor672-13")
                      "no-error")
                  (error (format "%s:%s" (car e) (cadr e))))))
          (font672-fb-write
           `((passed . t)
             (faces . ,rows)
             (direct-missing-resolve . ,direct-missing-resolve)
             (direct-missing-set-frame . ,direct-missing-set-frame)))
          (when (fboundp 'neomacs--write-frame-snapshot)
            (neomacs--write-frame-snapshot
             (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
          (run-at-time 0.1 nil (lambda () (kill-emacs 0)))))
    (error
     (font672-fb-write `((passed . :json-false) (error . ,(format "%S" err))))
     (princ (format "Face-font fallback GUI failure: %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(run-at-time 0.3 nil #'font672-fb-probe)
