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

(run-at-time
 0.1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (equal (font-get-system-font) "Ubuntu Mono 13")
           (error "Initial settings are not isolated: %S" (desktop-font-live-state)))
         (setq font-use-system-font t)
         (desktop-font-live-log "LIVE-FONT-BEFORE %S" (desktop-font-live-state))
         ;; This child inherits only this test's keyfile backend and config root.
         ;; No writes reach the user's desktop settings.
         (unless (eq 0 (call-process "gsettings" nil nil nil "set"
                                    "org.gnome.desktop.interface"
                                    "monospace-font-name" "DejaVu Sans Mono 16"))
           (error "Isolated GSettings writer failed"))
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
            (desktop-font-live-log "LIVE-FONT-PASS %S" (desktop-font-live-state))
            (kill-emacs 0))
          (+ (float-time) 8)))
     (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1)))))
