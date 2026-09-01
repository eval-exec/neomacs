;;; image-size-oracle.el --- image-size / metadata GUI parity oracle -*- lexical-binding: t -*-

;; Runs identically under GNU Emacs and Neomacs on a window-system frame
;; (the GUI test harness starts Xvfb and sets DISPLAY). Computes a battery
;; of image probes and prin1's them to the file named by
;; NEOMACS_GUI_IMAGE_RESULT, so the Rust test can diff the two editors'
;; results. Probes are pixel-only (font-independent) so a mismatch always
;; points at image code, never at font selection.

(defconst neomacs-image-oracle-png-b64
  ;; A 5x3 solid-red PNG; the decoded bytes are the ground truth both
  ;; editors must size identically.
  "iVBORw0KGgoAAAANSUhEUgAAAAUAAAADCAIAAADUVFKvAAAAEElEQVR4nGP4z8CAjBgI8AF1yA7yBqYM2wAAAABJRU5ErkJggg==")

(defun neomacs-image-oracle-cell (name thunk)
  "Call THUNK with no args; return (NAME VALUE) or (NAME error MSG)."
  (let ((value (condition-case err
                  (funcall thunk)
                (error (list 'error (error-message-string err))))))
    (list name value)))

(defun neomacs-image-oracle-result ()
  (let ((d (base64-decode-string neomacs-image-oracle-png-b64)))
    (list
     (neomacs-image-oracle-cell
      :pixels
      (lambda () (image-size (create-image d 'png t) t)))
     (neomacs-image-oracle-cell
      :margin-2
      (lambda () (image-size (create-image d 'png t :margin 2) t)))
     (neomacs-image-oracle-cell
      :relief-3
      (lambda () (image-size (create-image d 'png t :relief 3) t)))
     ;; image-metadata now returns nil like GNU (the dual-extent geometry
     ;; moved to the neomacs-image-extent companion), so this is a real
     ;; parity probe rather than an intentional split.
     (neomacs-image-oracle-cell
      :metadata
      (lambda () (image-metadata (create-image d 'png t))))
     (neomacs-image-oracle-cell
      :flush-reread
      (lambda ()
        (let ((img (create-image d 'png t)))
          (image-size img t)
          (image-flush img)
          (image-size img t)))))))

(defun neomacs-image-oracle-write ()
  (let ((path (getenv "NEOMACS_GUI_IMAGE_RESULT")))
    (when path
      (make-directory (file-name-directory path) t)
      (with-temp-file path
        (prin1 (neomacs-image-oracle-result) (current-buffer))
        (insert "\n")))))

;; Compute and write during load: a window-system frame already exists at
;; this point under the GUI harness (the font selection oracle relies on
;; the same top-level timing). Defer kill-emacs to the event loop: calling
;; it at top-level mid-init hangs GNU's GTK teardown.
(neomacs-image-oracle-write)
(run-at-time 2 nil (lambda () (kill-emacs 0)))

;;; image-size-oracle.el ends here
