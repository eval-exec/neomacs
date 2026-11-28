;;; face-font-fallback-resize.el --- #672 face-font fallback and XLFD resize -*- lexical-binding: t -*-

;; Exercise the exact failure mode reported in #672 (Doom `SPC t b` /
;; `doom-adjust-font-size`):
;; 1. Verify pdump-restored startup state BEFORE calling
;;    `internal-set-alternative-font-family-alist` (dumped non-bootstrap faces
;;    such as `completions-common-part` and `help-key-binding` survive in
;;    `face-list`/`face-id`, and `fixed-pitch-serif` resolves to a usable font
;;    out of the box).
;; 2. Verify calling `internal-set-alternative-font-family-alist` preserves
;;    `face-list` count and existing `face-id`s while updating
;;    `fixed-pitch-serif` resolution to the pinned alternative (`DejaVu Sans Mono`).
;; 3. Verify Doom-style XLFD resize (`face-font` -> `x-decompose-font-name` ->
;;    `x-compose-font-name` -> `x-resolve-font-name`) across `default`,
;;    `fixed-pitch`, `fixed-pitch-serif`, `variable-pitch`, and an unmapped
;;    missing-family face inheriting `:foundry "SRC"`.
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defun font672-fb-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font672-fb-resize-xlfd (xlfd size)
  "Replace the XLFD pixel-size slot (index 5 in `x-decompose-font-name') with SIZE."
  (let ((vec (x-decompose-font-name xlfd)))
    (unless vec
      (error "x-decompose-font-name failed on %S" xlfd))
    (aset vec 5 (number-to-string size))
    (aset vec 6 nil)
    (x-compose-font-name vec)))

(defun font672-fb-probe ()
  (condition-case err
      (progn
        (set-frame-font "-SRC-Hack-regular-normal-normal-*-14-*-*-*-m-0-iso10646-1" t t)
        (redisplay t)
        ;; 1. Observe stock `-Q` dumped startup state before touching the alist.
        (let* ((startup-face-count (length (face-list)))
               (startup-completions-id
                (condition-case _ (face-id 'completions-common-part) (error nil)))
               (startup-help-key-id
                (condition-case _ (face-id 'help-key-binding) (error nil)))
               (startup-dumped-faces-present
                (and (integerp startup-completions-id)
                     (integerp startup-help-key-id)
                     (memq 'completions-common-part (face-list))
                     (memq 'help-key-binding (face-list))
                     t))
               (startup-fps (face-font 'fixed-pitch-serif))
               (startup-fps-resolved
                (x-resolve-font-name (font672-fb-resize-xlfd startup-fps 13))))
          ;; 2. Load a non-bootstrap dynamic face (id >= 186) and verify
          ;; `internal-set-alternative-font-family-alist' preserves it while
          ;; mapping "Monospace Serif" (`fixed-pitch-serif') to our pinned
          ;; second font ("DejaVu Sans Mono"), matching stock `lisp/faces.el'.
          (require 'ansi-color)
          (let* ((before-alist-face-count (length (face-list)))
                 (before-alist-ansi-red-id
                  (condition-case _ (face-id 'ansi-color-red) (error nil))))
            (setq face-font-family-alternatives
                  (cons '("Monospace Serif" "Courier 10 Pitch" "DejaVu Sans Mono")
                        (assoc-delete-all "Monospace Serif"
                                          (copy-alist face-font-family-alternatives))))
            (internal-set-alternative-font-family-alist face-font-family-alternatives)
            (let* ((after-alist-face-count (length (face-list)))
                   (after-alist-ansi-red-id
                    (condition-case _ (face-id 'ansi-color-red) (error nil)))
                   (after-alist-ansi-red-listed
                    (and (memq 'ansi-color-red (face-list)) t)))
              (defface neo-672-missing-face
                '((t :family "NoSuchFamilyFor672"))
                "Face with an unmapped missing family for #672.")
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
                   (startup-face-count . ,startup-face-count)
                   (startup-dumped-faces-present . ,(if startup-dumped-faces-present t :json-false))
                   (startup-completions-id . ,startup-completions-id)
                   (startup-help-key-id . ,startup-help-key-id)
                   (startup-fps . ,startup-fps)
                   (startup-fps-resolved . ,startup-fps-resolved)
                   (before-alist-face-count . ,before-alist-face-count)
                   (before-alist-ansi-red-id . ,before-alist-ansi-red-id)
                   (after-alist-face-count . ,after-alist-face-count)
                   (after-alist-ansi-red-id . ,after-alist-ansi-red-id)
                   (after-alist-ansi-red-listed . ,(if after-alist-ansi-red-listed t :json-false))
                   (faces . ,rows)
                   (direct-missing-resolve . ,direct-missing-resolve)
                   (direct-missing-set-frame . ,direct-missing-set-frame)))
                (when (fboundp 'neomacs--write-frame-snapshot)
                  (neomacs--write-frame-snapshot
                   (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
                (run-at-time 0.1 nil (lambda () (kill-emacs 0))))))))
    (error
     (font672-fb-write `((passed . :json-false) (error . ,(format "%S" err))))
     (princ (format "Face-font fallback GUI failure: %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(run-at-time 0.3 nil #'font672-fb-probe)
