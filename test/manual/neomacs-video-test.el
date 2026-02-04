;;; neomacs-video-test.el --- Test inline video rendering -*- lexical-binding: t -*-

;;; Commentary:
;; Test displaying inline video in a buffer using GPU rendering.
;; Uses neomacs-video-insert for inline video display.

;;; Code:

(require 'neomacs-video nil t)

(defvar video-test-file "/home/exec/Videos/4k_f1.mp4"
  "Path to test video file.")

(defvar video-test-id nil
  "ID of the loaded video for control.")

(defun neomacs-video-test ()
  "Test inline video display."
  (interactive)
  (let ((buf (get-buffer-create "*Neomacs Video Test*")))
    (switch-to-buffer buf)
    (erase-buffer)
    (insert "*** Neomacs Inline Video Test ***\n\n")

    ;; Check if video file exists
    (if (not (file-exists-p video-test-file))
        (progn
          (insert (format "ERROR: Video file not found: %s\n" video-test-file))
          (insert "\nPlease set video-test-file to a valid video path.\n"))

      ;; Display video info
      (insert (format "Video file: %s\n" video-test-file))
      (insert (format "File size: %s\n"
                      (file-size-human-readable
                       (file-attribute-size (file-attributes video-test-file)))))
      (insert "\n")

      ;; Try to insert inline video
      (condition-case err
          (progn
            (insert "Inserting inline video...\n\n")

            ;; Check if neomacs-video-insert is available
            (if (fboundp 'neomacs-video-insert)
                (progn
                  ;; Insert inline video at 800x450
                  (setq video-test-id (neomacs-video-insert video-test-file 800 450))
                  (insert "\n\n")
                  (insert (format "Video inserted with ID: %s\n" video-test-id))

                  ;; Start playback
                  (when (and video-test-id (fboundp 'neomacs-video-play))
                    (neomacs-video-play video-test-id)
                    (insert "Video playback started\n"))

                  ;; Set up frame update timer for video decoding
                  (when (fboundp 'neomacs-video-update)
                    (run-with-timer 0.033 0.033  ; ~30fps
                                    (lambda ()
                                      (when video-test-id
                                        (neomacs-video-update video-test-id)
                                        (force-window-update)))))

                  (insert "\nControls: [p]lay [s]top [space]pause\n"))

              ;; Fallback: try direct load and display property
              (if (fboundp 'neomacs-video-load)
                  (progn
                    (setq video-test-id (neomacs-video-load video-test-file))
                    (insert (format "Video loaded with ID: %d\n" video-test-id))
                    (let ((start (point)))
                      (insert " ")
                      (put-text-property start (point) 'display
                                         `(video :id ,video-test-id :width 800 :height 450)))
                    (insert "\n\nVideo inserted via display property\n")
                    (when (fboundp 'neomacs-video-play)
                      (neomacs-video-play video-test-id))
                    (when (fboundp 'neomacs-video-update)
                      (run-with-timer 0.033 0.033
                                      (lambda ()
                                        (when video-test-id
                                          (neomacs-video-update video-test-id)
                                          (force-window-update))))))
                (insert "neomacs-video functions not available\n"))))
        (error
         (insert (format "\nError: %s\n" (error-message-string err)))
         (insert "\nVideo display failed. Check:\n")
         (insert "1. GStreamer is installed\n")
         (insert "2. Video codecs are available\n")
         (insert "3. The video feature is enabled in neomacs-display\n"))))

    (insert "\n\n=== Test Info ===\n")
    (insert (format "Emacs version: %s\n" emacs-version))
    (insert (format "System type: %s\n" system-type))
    (insert (format "neomacs-video-insert available: %s\n" (fboundp 'neomacs-video-insert)))
    (insert (format "neomacs-video-load available: %s\n" (fboundp 'neomacs-video-load)))

    (goto-char (point-min))
    (message "Video test ready.")))

;; Key bindings for video control
(defun neomacs-video-test-play ()
  "Play the test video."
  (interactive)
  (when (and video-test-id (fboundp 'neomacs-video-play))
    (neomacs-video-play video-test-id)
    (message "Video playing")))

(defun neomacs-video-test-pause ()
  "Pause the test video."
  (interactive)
  (when (and video-test-id (fboundp 'neomacs-video-pause))
    (neomacs-video-pause video-test-id)
    (message "Video paused")))

(defun neomacs-video-test-stop ()
  "Stop the test video."
  (interactive)
  (when (and video-test-id (fboundp 'neomacs-video-stop))
    (neomacs-video-stop video-test-id)
    (message "Video stopped")))

;; Run test
(neomacs-video-test)

;; Take screenshot after 5 seconds
(run-at-time 5 nil
             (lambda ()
               (message "Test complete. Check video display.")))

;; Exit after 10 seconds
(run-at-time 30 nil #'kill-emacs)

;;; neomacs-video-test.el ends here
