;;; startup-gpu-image-size.el --- CPU image metadata during GPU preparation -*- lexical-binding: t; -*-

;; Reuse the valid 5x3 RGB PNG from image-size-oracle.el. GNU image.c:image-size
;; returns decoded pixel dimensions for PIXELS=t; no texture presentation is
;; part of that public query.
(defconst startup-gpu-image-png
  "iVBORw0KGgoAAAANSUhEUgAAAAUAAAADCAIAAADUVFKvAAAAEElEQVR4nGP4z8CAjBgI8AF1yA7yBqYM2wAAAABJRU5ErkJggg==")

;; Run in the loaded fixture, independently of command-loop timers or the first
;; scene. The parent opens this gate after procfs observes the GPU worker's FIFO.
(with-temp-file (getenv "NEOMACS_GUI_GPU_READY_FILE") (insert "ready\n"))
(let ((deadline (+ (float-time) 10)))
  (while (not (file-exists-p (getenv "NEOMACS_GUI_GPU_IMAGE_GO")))
    (when (> (float-time) deadline)
      (princ "GPU-IMAGE-SIZE-GATE-TIMEOUT\n" 'external-debugging-output)
      (kill-emacs 2))
    (sleep-for 0.01)))

(let* ((result (getenv "NEOMACS_GUI_GPU_IMAGE_RESULT"))
       (value
        (condition-case err
            (let ((image (create-image
                          (base64-decode-string startup-gpu-image-png) 'png t)))
              (with-temp-file (concat result ".entered") (insert "entered\n"))
              (princ "GPU-IMAGE-SIZE-QUERY-ENTER\n" 'external-debugging-output)
              (image-size image t))
          (error (list 'error (error-message-string err)))))
       (pending (concat result ".pending")))
  (with-temp-file pending (prin1 value (current-buffer)))
  (rename-file pending result t)
  ;; Publishing the result must not release GPU acquisition. Only the parent,
  ;; after checking the result and the still-blocked worker, releases the FIFO.
  (let ((deadline (+ (float-time) 10)))
    (while (not (file-exists-p (getenv "NEOMACS_GUI_GPU_RELEASE_FILE")))
      (when (> (float-time) deadline)
        (princ "GPU-IMAGE-SIZE-RELEASE-TIMEOUT\n" 'external-debugging-output)
        (kill-emacs 2))
      (sleep-for 0.01)))
  (if (equal value '(5 . 3))
      (progn
        (princ "GPU-IMAGE-SIZE-PASS\n" 'external-debugging-output)
        (kill-emacs 0))
    (princ (format "GPU-IMAGE-SIZE-FAIL: %S\n" value)
           'external-debugging-output)
    (kill-emacs 1)))
