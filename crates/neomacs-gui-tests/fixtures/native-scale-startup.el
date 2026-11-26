;;; native-scale-startup.el --- Logical geometry across output scale discovery -*- lexical-binding: t -*-

(defvar native-scale-startup-widths nil)
(defvar native-scale-startup-timer nil)
(defun native-scale-startup-observe (frame)
  (push (frame-width frame) native-scale-startup-widths))
(add-hook 'window-size-change-functions #'native-scale-startup-observe)
(native-scale-startup-observe (selected-frame))

(defun native-scale-startup-ready ()
  ;; The HiDpi4k output discovers scale 2 either before the fixture loads or
  ;; at the first native configure.  Wait for the frame's scale-factor
  ;; receipt instead of a fixed observation window, so a loaded compositor
  ;; cannot make the process exit before the public WindowResize event.
  (= (frame-scale-factor (selected-frame)) 2))

(defun native-scale-startup-advance ()
  (when (native-scale-startup-ready)
    (cancel-timer native-scale-startup-timer)
    (setq native-scale-startup-timer nil)
    (native-scale-startup-observe (selected-frame))
    (message "Native scale startup observed columns: %S" native-scale-startup-widths)
    ;; The Rust driver also checks events emitted before this fixture loads.
    (kill-emacs (if (= (frame-width) 80) 0 1))))

(setq native-scale-startup-timer
      (run-at-time 0 0.02 #'native-scale-startup-advance))
