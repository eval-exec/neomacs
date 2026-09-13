;;; desktop-font-live.el --- Public live desktop-font oracle -*- lexical-binding: t -*-
(setq native-comp-jit-compilation nil native-comp-enable-subr-trampolines nil)

(defun desktop-font-live-log (format-string &rest args)
  (princ (concat (apply #'format format-string args) "\n")
         'external-debugging-output))

(defun desktop-font-live-state ()
  (list :monospace (font-get-system-font)
        :application (font-get-system-normal-font)
        :family (face-attribute 'default :family)
        :height (face-attribute 'default :height)
        :frame-font (frame-parameter nil 'font)
        :cell (list (window-font-width) (window-font-height))
        :columns (frame-width)
        :dynamic-setting (featurep 'dynamic-setting)
        :handler (lookup-key special-event-map [config-changed-event])
        :opt-in font-use-system-font
        :display (terminal-name)
        :graphical-display (display-graphic-p (terminal-name))
        :display-frames (frames-on-display-list (terminal-name))))

(defun desktop-font-live-await (predicate continuation deadline)
  (condition-case err
      (cond ((funcall predicate) (funcall continuation))
            ((> (float-time) deadline)
             (error "Live font update timed out: %S geometry=%S text=%S receipt=%S"
                    (desktop-font-live-state) (desktop-font-live-frame-state)
                    (list (frame-text-width) (frame-text-height))
                    (desktop-font-live-receipt)))
            (t (run-at-time 0.05 nil #'desktop-font-live-await
                            predicate continuation deadline)))
    (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1))))

(defun desktop-font-live-write (key value)
  ;; This child inherits only this test's keyfile backend and config root.
  ;; No writes reach the user's desktop settings.
  (unless (eq 0 (call-process "gsettings" nil nil nil "set"
                             "org.gnome.desktop.interface" key value))
    (error "Isolated GSettings writer failed")))

(defun desktop-font-live-frame-state ()
  (list (face-attribute 'default :family) (face-attribute 'default :height)
        (frame-parameter nil 'font) (window-font-width) (window-font-height)
        (frame-width) (frame-height) (frame-pixel-width) (frame-pixel-height)))

(defun desktop-font-live-unchanged (before font)
  (unless (and (equal before (desktop-font-live-frame-state))
               (eq font (face-attribute 'default :font)))
    (error "Preference-only change reopened the font or changed the frame: %S -> %S"
           before (desktop-font-live-frame-state))))

(defun desktop-font-live-pass ()
  (desktop-font-live-log "LIVE-FONT-PASS %S" (desktop-font-live-state))
  (kill-emacs 0))

(defun desktop-font-live-opt-out ()
  (setq font-use-system-font nil)
  (let ((before (desktop-font-live-frame-state))
        (font (face-attribute 'default :font)))
    (desktop-font-live-write "monospace-font-name" "DejaVu Sans Mono 16")
    (desktop-font-live-await
     (lambda () (equal (font-get-system-font) "DejaVu Sans Mono 16"))
     (lambda ()
       (desktop-font-live-unchanged before font)
       ;; Enabling adoption alone must not replay the skipped preference.
       ;; A later application update gives an observed native-event barrier.
       (setq font-use-system-font t)
       (desktop-font-live-write "font-name" "DejaVu Sans 12")
       (desktop-font-live-await
        (lambda () (equal (font-get-system-normal-font) "DejaVu Sans 12"))
        (lambda ()
          (desktop-font-live-unchanged before font)
          (desktop-font-live-pass))
        (+ (float-time) 8)))
     (+ (float-time) 8))))

(defun desktop-font-live-opt-in (&optional continuation)
  (setq font-use-system-font t)
  (desktop-font-live-write "monospace-font-name" "DejaVu Sans Mono 16")
  (desktop-font-live-await
   (lambda ()
     (and (equal (font-get-system-font) "DejaVu Sans Mono 16")
          (equal (face-attribute 'default :family) "DejaVu Sans Mono")
          ;; Independently observed in GNU X11 at 96 logical DPI.
          (= (face-attribute 'default :height) 158)
          (= (window-font-width) 13)
          (= (window-font-height) 25)))
   (lambda ()
     (unless (equal (font-get-system-normal-font) "Ubuntu 10")
       (error "Monospace update replaced the application preference"))
     (funcall (or continuation #'desktop-font-live-pass)))
   (+ (float-time) 8)))

(defun desktop-font-live-explicit-and-future ()
  (set-frame-font "DejaVu Sans Mono 12")
  (let ((existing (make-frame '((font . "Ubuntu Mono 11")))))
    (desktop-font-live-opt-in
     (lambda ()
       (unless (and (equal (face-attribute 'default :family existing) "DejaVu Sans Mono")
                    (= (face-attribute 'default :height existing) 158))
         (error "Explicit font on another existing frame escaped opt-in: %S"
                (frame-parameters existing)))
       (let ((future (make-frame))
             (explicit (make-frame '((font . "Ubuntu Mono 11")))))
         (unless (and (equal (face-attribute 'default :family future) "DejaVu Sans Mono")
                      (= (face-attribute 'default :height future) 158))
           (error "Ordinary future frame did not inherit live defaults: %S"
                  (frame-parameters future)))
         (unless (equal (face-attribute 'default :family explicit) "Ubuntu Mono")
           (error "Explicit future-frame font lost precedence: %S"
                  (frame-parameters explicit)))
         (desktop-font-live-pass))))))

(defun desktop-font-live-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer (insert-file-contents file) (read (current-buffer))))))

(defun desktop-font-live-presented (after)
  (or (not (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
      (let ((receipt (desktop-font-live-receipt)))
        (and receipt
             (eq (plist-get receipt :outcome) 'presented)
             (> (plist-get receipt :submission) after)
             (= (plist-get receipt :width) (frame-pixel-width))
             (= (plist-get receipt :height) (frame-pixel-height))
             (integerp (plist-get receipt :clock-id))
             (natnump (plist-get receipt :seconds))
             (natnump (plist-get receipt :nanoseconds))
             (< (plist-get receipt :nanoseconds) 1000000000)))))

(defun desktop-font-live-grid-ready ()
  (and (= (frame-width) 80) (= (frame-text-lines) 24)
       (= (frame-text-width) (* 80 (frame-char-width)))
       (= (frame-text-height) (* 24 (frame-char-height)))))

(defun desktop-font-live-geometry ()
  ;; Isolate the font-owned text grid from platform-specific bar chrome.
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (set-frame-size nil 80 24)
  (desktop-font-live-await
   (lambda () (and (desktop-font-live-grid-ready)
                   (desktop-font-live-presented 0)))
   (lambda ()
     (let ((submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       (desktop-font-live-log "LIVE-GEOMETRY-BEFORE %S" (desktop-font-live-frame-state))
       (desktop-font-live-opt-in
        (lambda ()
          (desktop-font-live-await
           (lambda () (and (desktop-font-live-grid-ready)
                           (desktop-font-live-presented submission)))
           (lambda ()
             (desktop-font-live-log "LIVE-GEOMETRY-AFTER %S receipt=%S"
                                    (desktop-font-live-frame-state)
                                    (desktop-font-live-receipt))
             (desktop-font-live-pass))
           (+ (float-time) 8))))))
   (+ (float-time) 8)))

(run-at-time
 0.1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (equal (font-get-system-font) "Ubuntu Mono 13")
           (error "Initial settings are not isolated: %S" (desktop-font-live-state)))
         (desktop-font-live-log "LIVE-FONT-BEFORE %S" (desktop-font-live-state))
         (pcase (getenv "NEOMACS_GUI_LIVE_FONT_CASE")
           ("opt-out" (desktop-font-live-opt-out))
           ("explicit-and-future" (desktop-font-live-explicit-and-future))
           ("geometry" (desktop-font-live-geometry))
           (_ (desktop-font-live-opt-in))))
     (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1)))))
