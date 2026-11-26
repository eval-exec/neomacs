;;; early-init.el --- Cache queries during real GUI startup -*- lexical-binding: t; -*-

(setq package-enable-at-startup nil
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil
      inhibit-startup-screen t)
(defvar startup-image-cache-failed nil)

(defun startup-image-cache-write (phase status value)
  (with-temp-buffer
    (insert (json-serialize
             `((phase . ,phase)
               (status . ,status)
               (value . ,(format "%S" value))
               (window_system . ,(format "%S" (car-safe value)))
               (cache_size . ,(and (equal status "completed")
                                   (car-safe (cdr-safe value))))))
            "\n")
    (write-region (point-min) (point-max)
                  (getenv "NEOMACS_STARTUP_IMAGE_CACHE_RECEIPT") t 'silent)))

(defun startup-image-cache-record (phase)
  ;; Write before calling the cache API so a blocked native acknowledgement
  ;; leaves an observable location, even if startup never reaches regular init.
  (startup-image-cache-write phase "entered" nil)
  (condition-case err
      (startup-image-cache-write
       phase "completed"
       (list window-system (progn (clear-image-cache t) (image-cache-size))))
    (error
     (setq startup-image-cache-failed t)
     (startup-image-cache-write phase "error" err))))

(startup-image-cache-record "early-init")
(add-hook 'after-init-hook
          (lambda () (startup-image-cache-record "after-init")))
(add-hook 'window-setup-hook
          (lambda ()
            (startup-image-cache-record "window-setup")
            (kill-emacs (if startup-image-cache-failed 1 0))))
