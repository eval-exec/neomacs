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
             (error "Live font update timed out: %S" (desktop-font-live-state)))
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

(run-at-time
 0.1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (equal (font-get-system-font) "Ubuntu Mono 13")
           (error "Initial settings are not isolated: %S" (desktop-font-live-state)))
         (desktop-font-live-log "LIVE-FONT-BEFORE %S" (desktop-font-live-state))
         (if (equal (getenv "NEOMACS_GUI_LIVE_FONT_CASE") "opt-out")
             (desktop-font-live-opt-out)
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
            (desktop-font-live-pass))
          (+ (float-time) 8))))
     (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1)))))
