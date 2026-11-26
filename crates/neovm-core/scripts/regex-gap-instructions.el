;;; regex-gap-instructions.el --- gated split-gap search work -*- lexical-binding: t; -*-

;; Companion of regex-gap-instructions.py.  Setup and warmup finish before
;; READY.  The sampling window includes GO observation, the search loop and
;; publication of DONE.  The waits sleep, and startup is outside the window.
(require 'json)
(require 'bytecomp)

(defun rgi-required (name)
  (or (getenv name) (error "Missing %s" name)))

(defun rgi-publish (name value)
  (let* ((path (expand-file-name name (rgi-required "RGI_DIRECTORY")))
         (temporary (concat path ".tmp")))
    (with-temp-file temporary (insert (json-encode value)))
    (rename-file temporary path)))

(defun rgi-await (name)
  (let ((path (expand-file-name name (rgi-required "RGI_DIRECTORY")))
        (deadline (+ (float-time) 120)))
    (while (not (file-exists-p path))
      (when (> (float-time) deadline) (error "Timed out waiting for %s" name))
      (sleep-for 0.01))))

(defun rgi-searches (iterations)
  (let ((hits 0))
    (dotimes (_ iterations)
      (goto-char (point-min))
      ;; The actual org-outline-regexp-bol.  Its unbounded repetition means
      ;; the finite-length seam shortcut cannot make this a one-slice case.
      (while (re-search-forward "^\\*+ " nil t)
        (setq hits (1+ hits))))
    hits))
(fset 'rgi-searches (byte-compile #'rgi-searches))

(with-temp-buffer
  (let* ((arm (rgi-required "RGI_ARM"))
         (lines (string-to-number (rgi-required "RGI_LINES")))
         (iterations (string-to-number (rgi-required "RGI_ITERATIONS")))
         (case-fold-search nil)
         ;; Inline stars are ordinary Org body emphasis, never headlines.
         (body "ordinary * emphasis * and *inline* text in journal\n"))
    (dotimes (_ lines) (insert body))
    (let* ((expected-gap (if (equal arm "mid")
                             (1+ (/ (buffer-size) 2))
                           (point-max)))
           (bytes (buffer-size))
           (checksum (secure-hash 'sha256 (current-buffer))))
      (unless (member arm '("mid" "end")) (error "Invalid arm %s" arm))
      ;; Both arms execute the same edit and leave exactly the same bytes.
      (goto-char expected-gap) (insert "Q") (delete-char -1)
      (unless (= (gap-position) expected-gap) (error "Bad initial gap"))
      (unless (= (rgi-searches 2) 0) (error "Unexpected warmup hit"))
      (unless (= (gap-position) expected-gap) (error "Warmup moved gap"))
      (rgi-publish "ready.json"
                   `((schema . 1) (arm . ,arm) (lines . ,lines)
                     (iterations . ,iterations) (bytes . ,bytes)
                     (sha256 . ,checksum) (gap . ,(gap-position))))
      (rgi-await "go")
      (let ((hits (rgi-searches iterations)))
        (rgi-publish "done.json"
                     `((schema . 1) (arm . ,arm) (iterations . ,iterations)
                       (hits . ,hits) (bytes . ,(buffer-size))
                       (sha256 . ,(secure-hash 'sha256 (current-buffer)))
                       (gap . ,(gap-position)) (expected_gap . ,expected-gap)))
        (unless (and (= hits 0) (= (gap-position) expected-gap)
                     (= (buffer-size) bytes)
                     (equal checksum (secure-hash 'sha256 (current-buffer))))
          (error "Search result, bytes, or gap changed")))
      (rgi-await "exit"))))
;;; regex-gap-instructions.el ends here
