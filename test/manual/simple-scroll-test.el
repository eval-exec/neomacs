;;; simple-scroll-test.el --- Simple scroll test

(switch-to-buffer "*scroll-test*")
(erase-buffer)
(insert "=== TOP ===\n\n")
(insert-image (create-image "/home/exec/Pictures/4k_image_1.jpg" nil nil :max-width 400 :max-height 300))
(insert "\n\nText after image 1\nLine 2\nLine 3\nLine 4\nLine 5\n\n")
(insert-image (create-image "/home/exec/Pictures/4k_image_2.jpg" nil nil :max-width 400 :max-height 300))
(insert "\n\nText after image 2\nLine 2\nLine 3\nLine 4\nLine 5\n\n")
(insert "=== BOTTOM ===\n")
(goto-char (point-min))
(redisplay t)

;; Screenshot at top
(run-at-time 2 nil
  (lambda ()
    (neomacs-screenshot "/tmp/scroll-1-top.png")
    (message "Screenshot 1 saved")))

;; Scroll and screenshot
(run-at-time 4 nil
  (lambda ()
    (scroll-up 15)
    (redisplay t)
    (neomacs-screenshot "/tmp/scroll-2-scrolled.png")
    (message "Screenshot 2 saved")))

;; Exit
(run-at-time 6 nil (lambda () (kill-emacs 0)))
