;;; native-display-contract.el --- Native startup/font/resize contract -*- lexical-binding: t -*-

(defun neomacs-native-contract-await (predicate continuation deadline)
  (condition-case err
      (cond ((funcall predicate) (funcall continuation))
            ((> (float-time) deadline) (error "Native geometry did not settle"))
            (t (run-at-time 0.05 nil #'neomacs-native-contract-await
                            predicate continuation deadline)))
    (error (message "Native display contract: %S" err) (kill-emacs 1))))

(switch-to-buffer (get-buffer-create "*native-display-contract*"))
(insert "Native font selection and frame geometry\n")
(dotimes (i 30) (insert (format "Measured text row %02d\n" i)))
(goto-char (point-min))

(run-at-time
 0.1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (= (frame-width) 80)
           (error "Initial font geometry: expected 80 columns, got %S" (frame-width)))
         (unless (and (> (frame-char-width) 0) (> (frame-char-height) 0)
                      (fontp (face-attribute 'default :font)))
           (error "Initial frame has no opened default font"))
         ;; GNU has no system-monospace preference subscription on these hosts.
         (when (memq system-type '(darwin windows-nt))
           (unless (and (null (font-get-system-font))
                        (null (font-get-system-normal-font)))
             (error "Platform fallback was exposed as a system preference")))
         (let ((old-height (frame-native-height))
               (deadline (+ (float-time) 12)))
           (set-frame-width nil 91)
           (neomacs-native-contract-await
            (lambda () (and (= (frame-width) 91)
                            (= (frame-native-height) old-height)))
            (lambda ()
              (let ((height (face-attribute 'default :height)))
                (setq frame-inhibit-implied-resize nil)
                (set-face-attribute 'default nil :height (+ height 20))
                (neomacs-native-contract-await
                 (lambda () (and (= (frame-width) 91)
                                 (> (frame-native-height) old-height)))
                 (lambda ()
                   (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
                     (insert "{\"contract\":\"native-display\",\"columns\":91}\n"))
                   (neomacs--write-frame-snapshot
                    (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
                   (run-at-time 0.2 nil #'kill-emacs 0))
                 deadline)))
            deadline)))
     (error (message "Native display contract: %S" err) (kill-emacs 1)))))
