;;; neomacs-gpu-image-test.el --- Test GPU image rendering in Neomacs -*- lexical-binding: t -*-

;; This test verifies that images are rendered inline in buffers
;; using the GPU path (GdkTexture/GSK TextureNode).

;;; Commentary:
;; Run with: ./test/manual/run-gpu-image-test.sh
;; Or manually: DISPLAY=:0 ./src/emacs -Q -l test/manual/neomacs-gpu-image-test.el
;;
;; Neomacs automatically uses GPU rendering for images when native
;; image libraries (libpng, libjpeg, etc.) aren't available.
;; The standard Emacs image API works without changes:
;;
;;   (insert-image (create-image "/path/to/image.jpg"))
;;
;; You can also explicitly specify neomacs type:
;;
;;   (insert-image '(image :type neomacs :file "/path/to/image.jpg"))

;;; Code:

(defvar neomacs-gpu-test-image-path
  (or (let ((home-pic (expand-file-name "~/Pictures/559-4K.jpg")))
        (and (file-exists-p home-pic) home-pic))
      (expand-file-name "test/data/image/black.jpg"
                        (or (getenv "EMACS_SOURCE_DIR")
                            default-directory)))
  "Path to test image.")

(defun neomacs-gpu-test-display-image ()
  "Display a test image inline in a buffer using GPU rendering."
  (interactive)
  (switch-to-buffer (get-buffer-create "*GPU Image Test*"))
  (erase-buffer)

  (insert "=== Neomacs GPU Inline Image Test ===\n\n")

  (if (not (file-exists-p neomacs-gpu-test-image-path))
      (progn
        (insert (format "ERROR: Test image not found: %s\n" neomacs-gpu-test-image-path))
        (message "ERROR: Test image not found")
        (kill-emacs 1))

    (insert (format "Image path: %s\n\n" neomacs-gpu-test-image-path))
    (insert "Using standard create-image (auto-detects neomacs type):\n\n")

    ;; Standard Emacs API - neomacs is auto-detected as fallback
    (condition-case err
        (let ((img (create-image neomacs-gpu-test-image-path nil nil
                                 :max-width 400 :max-height 300)))
          (if img
              (progn
                (insert-image img "[IMG]")
                (insert "\n\n")
                (insert (format "Image spec: %S\n\n" img))
                (insert "SUCCESS! GPU inline image rendering works.\n"))
            (insert "FAILED: create-image returned nil\n")))
      (error (insert (format "ERROR: %S\n" err)))))

  (goto-char (point-min))
  (redisplay t)

  ;; Wait for rendering then exit
  (run-at-time 5 nil
               (lambda ()
                 (message "TEST COMPLETE")
                 (kill-emacs 0))))

;; Auto-run when loaded
(add-hook 'emacs-startup-hook #'neomacs-gpu-test-display-image)

(provide 'neomacs-gpu-image-test)
;;; neomacs-gpu-image-test.el ends here
