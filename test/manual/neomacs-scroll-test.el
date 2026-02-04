;;; neomacs-scroll-test.el --- Scroll image test -*- lexical-binding: t -*-

(defun scroll-image-test ()
  "Test scrolling with images."
  (interactive)
  (let ((buf (get-buffer-create "*Scroll Image Test*")))
    (switch-to-buffer buf)
    (erase-buffer)
    (insert "*** Scroll Image Test ***\n\n")

    ;; Insert 5 images with text between
    (dotimes (i 5)
      (let ((img-file (format "%s4k_image_%d.jpg" (expand-file-name "~/Pictures/") (1+ i))))
        (insert (format "\n=== Image %d ===\n" (1+ i)))
        (when (file-exists-p img-file)
          (insert-image (create-image img-file nil nil :max-width 400 :max-height 300))
          (insert "\n"))
        (insert "\n\nSome text between images...\n")
        (insert "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n")))

    (goto-char (point-min))
    (message "Scroll test ready. Press 's' for screenshot, 'q' to quit.")))

(scroll-image-test)

;; Take screenshots at different scroll positions
(run-at-time 2 nil
             (lambda ()
               (neomacs-screenshot "/tmp/scroll-test-top.png")
               (message "Screenshot 1: top")))

(run-at-time 4 nil
             (lambda ()
               (scroll-up 10)
               (redisplay t)
               (neomacs-screenshot "/tmp/scroll-test-mid.png")
               (message "Screenshot 2: middle")))

(run-at-time 6 nil
             (lambda ()
               (goto-char (point-max))
               (redisplay t)
               (neomacs-screenshot "/tmp/scroll-test-bottom.png")
               (message "Screenshot 3: bottom")))

(run-at-time 8 nil #'kill-emacs)
