;;; font-otf-spec.el --- GNU :otf font-spec selection -*- lexical-binding: t -*-

;; #785: a font-spec's `:otf' requirement must reach the native font
;; matcher.  Both editors run against the same pinned Fontconfig catalog:
;; Noto Sans Arabic advertises the arab script with init/medi/fina/rtlm,
;; while DejaVu Sans lacks the arabic script representative characters and
;; the rtlm feature.  Features and scripts that are absent must reject the
;; font; an opened font object must satisfy `font-match-p'.

(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defun font785-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font785-boolean (value)
  (if value t :json-false))

(defun font785-family (entity)
  (and entity (format "%s" (font-get entity :family))))

(defun font785-probe ()
  (condition-case err
      (let* ((pos (find-font (font-spec :family "Noto Sans Arabic"
                                        :otf '(arab nil (init)))))
             (neg-feature (find-font (font-spec :family "Noto Sans Arabic"
                                                :otf '(arab nil (zzzz)))))
             (neg-script (find-font (font-spec :family "Noto Sans Arabic"
                                               :otf '(zzzz nil (init)))))
             (plain (find-font (font-spec :family "Noto Sans Arabic")))
             ;; DejaVu Sans has arab/init but lacks U+06C1, one of GNU's
             ;; script-representative-chars for arabic, so the added
             ;; Fontconfig charset requirement must reject it.
             (dejavu-arab (find-font (font-spec :family "DejaVu Sans"
                                                :otf '(arab nil (init)))))
             ;; GNU only applies `:otf' to a font-object, not to an entity.
             ;; (The opened-font arm is unit-tested; Neomacs's `open-font'
             ;; is a separate pre-existing stub, so this fixture compares
             ;; only the entity path across engines.)
             (match-entity (font-match-p (font-spec :otf '(arab nil (init)))
                                         plain))
             ;; A spec with only basic name slots is compared by `eq'.
             (match-name (font-match-p (font-spec :family "Noto Sans Arabic")
                                       plain))
             (match-name-mismatch
              (font-match-p (font-spec :family "DejaVu Sans") plain)))
        (set-fontset-font "fontset-default" 'arabic
                          (font-spec :family "DejaVu Sans"
                                     :otf '(arab nil (init)))
                          nil 'append)
        (set-fontset-font "fontset-default" 'arabic
                          (font-spec :family "Noto Sans Arabic"
                                     :otf '(arab nil (init)))
                          nil 'append)
        (switch-to-buffer (get-buffer-create "*font-otf-arabic*"))
        (erase-buffer)
        (insert "سلام")
        (goto-char (point-min))
        (redisplay t)
        (let ((font-at-family (font785-family (font-at (point-min)))))
          (unless (and (display-graphic-p)
                       pos
                       (null neg-feature)
                       (null neg-script)
                       (null dejavu-arab)
                       (equal (font785-family plain) "Noto Sans Arabic")
                       (null match-entity)
                       match-name
                       (null match-name-mismatch)
                       (equal font-at-family "Noto Sans Arabic"))
            (error "font :otf contract failed: %S"
                   (list :pos (font785-family pos)
                         :neg-feature (font785-family neg-feature)
                         :neg-script (font785-family neg-script)
                         :dejavu-arab (font785-family dejavu-arab)
                         :plain (font785-family plain)
                         :match-entity match-entity
                         :match-name match-name
                         :match-name-mismatch match-name-mismatch
                         :font-at font-at-family)))
          (font785-write
           `((passed . t)
             (positive-family . ,(font785-family pos))
             (negative-feature . ,(font785-boolean neg-feature))
             (negative-script . ,(font785-boolean neg-script))
             (dejavu-arab . ,(font785-boolean dejavu-arab))
             (plain-family . ,(font785-family plain))
             (match-entity . ,(font785-boolean match-entity))
             (match-name . ,(font785-boolean match-name))
             (match-name-mismatch . ,(font785-boolean match-name-mismatch))
             (font-at-family . ,font-at-family)
             (graphic . ,(font785-boolean (display-graphic-p)))))
          (when (fboundp 'neomacs--write-frame-snapshot)
            (neomacs--write-frame-snapshot
             (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
          (run-at-time 0.2 nil (lambda () (kill-emacs 0)))))
    (error
     (font785-write `((passed . :json-false) (error . ,(format "%S" err))))
     (princ (format "font :otf GUI failure: %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(run-at-time 0.7 nil #'font785-probe)
