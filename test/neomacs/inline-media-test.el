;;; inline-media-test.el --- Test inline image, video, and webkit in one buffer -*- lexical-binding: t -*-

;; This test creates a buffer containing all three inline media types:
;; - Image (via standard Emacs create-image/insert-image)
;; - Video (via neomacs-video-insert)
;; - WebKit (via neomacs-insert-webkit)

;;; Commentary:
;; Run with: ./test/neomacs/run-inline-media-test.sh
;; Or manually: DISPLAY=:0 ./src/emacs -Q -l test/neomacs/inline-media-test.el

;;; Code:

(require 'neomacs-webkit nil t)
(require 'neomacs-video nil t)
(require 'neomacs-image nil t)

(defvar inline-media-test-image-path
  (or (let ((home-pic (expand-file-name "~/Pictures/559-4K.jpg")))
        (and (file-exists-p home-pic) home-pic))
      (let ((test-pic (expand-file-name "test/data/image/black.jpg")))
        (and (file-exists-p test-pic) test-pic))
      nil)
  "Path to test image, or nil if not found.")

(defvar inline-media-test-video-path
  (or (let ((home-vid (expand-file-name "~/Videos/4k_f1.mp4")))
        (and (file-exists-p home-vid) home-vid))
      (let ((test-vid (expand-file-name "test/data/video/test.mp4")))
        (and (file-exists-p test-vid) test-vid))
      nil)
  "Path to first test video, or nil if not found.")

(defvar inline-media-test-video-path-2
  (or (let ((home-vid (expand-file-name "~/Videos/4k_test.mp4")))
        (and (file-exists-p home-vid) home-vid))
      (let ((test-vid (expand-file-name "test/data/video/test.mp4")))
        (and (file-exists-p test-vid) test-vid))
      nil)
  "Path to second test video, or nil if not found.")

(defvar inline-media-test-url "https://www.reddit.com/"
  "URL to load in WebKit view.")


(defun inline-media-test-log (fmt &rest args)
  "Log FMT with ARGS to stderr."
  (princ (concat (apply #'format fmt args) "\n") #'external-debugging-output))


(defun inline-media-test-run ()
  "Run the inline media test."
  (interactive)

  ;; Resize frame large enough for all media side-by-side
  (set-frame-size (selected-frame) 160 70)  ; columns x lines
  (set-frame-position (selected-frame) 50 50)

  (switch-to-buffer (get-buffer-create "*Inline Media Test*"))
  (erase-buffer)
  (setq buffer-read-only nil)

  (insert "=== Neomacs Inline Media Test ===\n")
  (insert "This buffer contains inline image, video, and webkit.\n\n")

  (let ((media-width 320)
        (media-height 180)  ; 16:9 aspect, smaller to fit all 3
        (success-count 0)
        (total-count 3))

    ;; === Section 1: Inline WebKit ===
    (insert "--- 1. Inline WebKit ---\n")
    (condition-case err
        (progn
          (neomacs-webkit-init)
          (let ((spec (neomacs-insert-webkit inline-media-test-url
                                              media-width media-height t)))
            (if spec
                (let ((view-id (plist-get (cdr spec) :id)))
                  (insert (propertize " " 'display spec))
                  (insert "\n")
                  (insert (format "WebKit ID: %d, URL: %s (%dx%d)\n"
                                  view-id inline-media-test-url
                                  media-width media-height))
                  (setq success-count (1+ success-count))
                  (inline-media-test-log "WebKit: OK (id=%d)" view-id))
              (insert "[WebKit creation failed]\n")
              (inline-media-test-log "WebKit: FAILED (neomacs-insert-webkit returned nil)"))))
      (error
       (insert (format "[WebKit error: %S]\n" err))
       (inline-media-test-log "WebKit: ERROR %S" err)))
    (insert "\n")

    ;; === Section 2: Inline Video (two side-by-side) ===
    (insert "--- 2. Inline Video (x2 side-by-side) ---\n")
    (if (and inline-media-test-video-path inline-media-test-video-path-2)
        (condition-case err
            (progn
              ;; Insert both videos on the same line, then play both
              (let ((video-id-1 (neomacs-video-insert inline-media-test-video-path
                                                      media-width media-height)))
                ;; Point is now after video 1 (same line, no newline)
                (let ((video-id-2 (neomacs-video-insert inline-media-test-video-path-2
                                                        media-width media-height)))
                  ;; Now play both simultaneously
                  (when video-id-1 (neomacs-video-play video-id-1))
                  (when video-id-2 (neomacs-video-play video-id-2))
                  (cond
                   ((and video-id-1 video-id-2)
                    (insert "\n")
                    (insert (format "Video 1: %s (ID %d), Video 2: %s (ID %d) — %dx%d each\n"
                                    (file-name-nondirectory inline-media-test-video-path)
                                    video-id-1
                                    (file-name-nondirectory inline-media-test-video-path-2)
                                    video-id-2
                                    media-width media-height))
                    (insert "Playing both videos simultaneously...\n")
                    (setq success-count (1+ success-count))
                    (inline-media-test-log "Video: OK (id1=%d, id2=%d)" video-id-1 video-id-2))
                   (t
                    (insert "\n[Video creation failed]\n")
                    (inline-media-test-log "Video: FAILED (one or both returned nil)"))))))
          (error
           (insert (format "[Video error: %S]\n" err))
           (inline-media-test-log "Video: ERROR %S" err)))
      (insert "[No test video found - skipping]\n")
      (insert "Need both ~/Videos/4k_f1.mp4 and ~/Videos/4k_test.mp4\n")
      (inline-media-test-log "Video: SKIPPED (no file)"))
    (insert "\n")

    ;; === Section 3: Inline Image ===
    (insert "--- 3. Inline Image ---\n")
    (if inline-media-test-image-path
        (condition-case err
            (let ((img (create-image inline-media-test-image-path nil nil
                                     :max-width media-width
                                     :max-height media-height)))
              (if img
                  (progn
                    (insert-image img "[IMAGE]")
                    (insert "\n")
                    (insert (format "Image: %s (%dx%d max)\n"
                                    (file-name-nondirectory inline-media-test-image-path)
                                    media-width media-height))
                    (setq success-count (1+ success-count))
                    (inline-media-test-log "Image: OK"))
                (insert "[Image creation failed]\n")
                (inline-media-test-log "Image: FAILED (create-image returned nil)")))
          (error
           (insert (format "[Image error: %S]\n" err))
           (inline-media-test-log "Image: ERROR %S" err)))
      (insert "[No test image found - skipping]\n")
      (insert "Set inline-media-test-image-path or place image at ~/Pictures/559-4K.jpg\n")
      (inline-media-test-log "Image: SKIPPED (no file)"))
    (insert "\n")

    ;; === Summary ===
    (insert "--- Summary ---\n")
    (insert (format "Media types loaded: %d/%d\n" success-count total-count))
    (if (= success-count total-count)
        (insert "All inline media types working!\n")
      (insert "Some media types failed or were skipped.\n"))

    (inline-media-test-log "=== Result: %d/%d media types loaded ===" success-count total-count))

  (goto-char (point-min))
  (setq buffer-read-only t)
  (redisplay t)

  ;; Keep running for manual inspection (shell script handles screenshot)
  (inline-media-test-log "Test buffer ready. Keeping Emacs open for 30 seconds...")
  (run-at-time 30 nil
               (lambda ()
                 (inline-media-test-log "Test timeout - exiting")
                 (kill-emacs 0))))

;; Auto-run when loaded
(add-hook 'emacs-startup-hook #'inline-media-test-run)

(provide 'inline-media-test)
;;; inline-media-test.el ends here
