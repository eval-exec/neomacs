;;; native-scale-startup.el --- Logical geometry across output scale discovery -*- lexical-binding: t -*-

(defvar native-scale-startup-widths nil)
(defun native-scale-startup-observe (frame)
  (push (frame-width frame) native-scale-startup-widths))
(add-hook 'window-size-change-functions #'native-scale-startup-observe)
(native-scale-startup-observe (selected-frame))

(run-at-time
 2 nil
 (lambda ()
   (native-scale-startup-observe (selected-frame))
   (message "Native scale startup observed columns: %S" native-scale-startup-widths)
   ;; The Rust driver also checks events emitted before this fixture loads.
   (kill-emacs (if (= (frame-width) 80) 0 1))))
