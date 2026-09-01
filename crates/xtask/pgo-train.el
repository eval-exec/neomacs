;;; pgo-train.el --- training workload for `fresh-build --profile release-pgo'  -*- lexical-binding: t; -*-

;; Drives the paths a PGO build should optimise for. Committed (rather than
;; left to whatever benchmark happens to be lying around) so the profile is
;; reproducible and reviewable: a PGO profile bakes in assumptions about what
;; is hot, and code on unprofiled paths gets pessimised, so what is trained on
;; is part of the build's semantics.
;;
;; Deliberately covers MORE than the editing benchmarks used to discover the
;; win. Training only on font-lock measured better on font-lock (-24%) but
;; risks biasing against everything else; byte-compilation and startup are
;; included so the common non-editing paths keep their profile too.
;;
;; KNOWN GAP -- this trains NOTHING in the redisplay path. It runs under
;; --batch, where redisplay never happens, so the counters are dominated by
;; startup and by fontification called directly. Measured consequence: a real
;; TTY keystroke->redisplay loop is +2% under PGO while startup is -17%. If
;; interactive latency is the goal, this file has to drive an actual TTY
;; session (see tools/bench/pty-run.py, which makes that deterministic) rather
;; than call font-lock functions in batch.

(defun nm-pgo--edit-pass (file mode-fn iters)
  "Fontify and edit around FILE the way interactive editing does."
  (let ((buf (find-file-noselect file)))
    (with-current-buffer buf
      (funcall mode-fn)
      (font-lock-set-defaults)
      (let* ((sz (buffer-size))
             (step (max 1 (/ sz (max 1 iters)))))
        (dotimes (i iters)
          (let ((pos (min (max (point-min) (* i step))
                          (max (point-min) (- (point-max) 2)))))
            (goto-char pos)
            (beginning-of-line)
            (let* ((win-start (point))
                   (win-end (save-excursion (forward-line 50) (point))))
              (font-lock-unfontify-region win-start win-end)
              (font-lock-fontify-region win-start win-end)
              (goto-char win-start)
              (insert "x")
              (let ((ins (point)))
                (font-lock-fontify-region (line-beginning-position)
                                          (line-end-position))
                (syntax-ppss ins)
                (delete-region (1- ins) ins))))))
      (set-buffer-modified-p nil)
      (kill-buffer buf))))

(defun nm-pgo--org-pass (iters)
  "Same shape as `nm-pgo--edit-pass' on a generated org buffer.
Org exercises a different frontier (Lisp-heavy fontification, text
properties, GC) than an elisp buffer does."
  (let ((buf (get-buffer-create "*pgo-org*")))
    (with-current-buffer buf
      (erase-buffer)
      (dotimes (n 80)
        (insert (format "* Heading %d :tag:\n" n))
        (insert "Text with *bold*, /italic/, =code= and a [[https://e.com][link]].\n")
        (insert "#+begin_src emacs-lisp\n(defun f (x) (* x x))\n#+end_src\n")
        (insert "| a | 1 |\n|---+---|\n| b | 2 |\n\n"))
      (when (fboundp 'org-mode) (org-mode))
      (font-lock-set-defaults)
      (let* ((sz (buffer-size))
             (step (max 1 (/ sz (max 1 iters)))))
        (dotimes (i iters)
          (let ((pos (min (max (point-min) (* i step))
                          (max (point-min) (- (point-max) 2)))))
            (goto-char pos)
            (beginning-of-line)
            (let* ((win-start (point))
                   (win-end (save-excursion (forward-line 50) (point))))
              (font-lock-unfontify-region win-start win-end)
              (font-lock-fontify-region win-start win-end)))))
      (kill-buffer buf))))

(defun nm-pgo--byte-compile-pass (files)
  "Byte-compile FILES into a scratch dir, then discard it.
Byte-compilation is the other workload users wait on, and it stresses
the reader, macro expansion and the compiler rather than redisplay."
  (let ((dir (make-temp-file "nm-pgo-bc" t)))
    (unwind-protect
        (dolist (f files)
          (when (file-readable-p f)
            (let ((copy (expand-file-name (file-name-nondirectory f) dir)))
              (copy-file f copy t)
              (byte-compile-file copy))))
      (delete-directory dir t))))


(defun nm-pgo--search-pass (file iters)
  "Regexp and literal search, plus replace -- the search engine and its
case-translation tables, which fontification alone barely touches."
  (let ((buf (find-file-noselect file)))
    (with-current-buffer buf
      (dotimes (_ iters)
        (goto-char (point-min))
        (while (re-search-forward "(defun \\([a-z-]+\\)" nil t)
          (match-string 1))
        (goto-char (point-min))
        (while (search-forward "let" nil t))
        (let ((case-fold-search t))
          (goto-char (point-min))
          (while (re-search-forward "[A-Z][a-z]+" nil t))))
      (set-buffer-modified-p nil)
      (kill-buffer buf))))

(defun nm-pgo--text-pass ()
  "Buffer mutation: insert/delete/kill/yank/undo and markers -- the edit
primitives every command goes through, distinct from fontification."
  (let ((buf (get-buffer-create "*pgo-text*")))
    (with-current-buffer buf
      (erase-buffer)
      (buffer-enable-undo)
      (dotimes (i 400)
        (insert (format "line %d with some words to move around\n" i)))
      (dotimes (_ 30)
        (goto-char (point-min))
        (kill-line 5) (goto-char (point-max)) (yank)
        (goto-char (point-min))
        (forward-word 20) (set-mark (point)) (forward-word 10)
        (upcase-region (region-beginning) (region-end))
        (undo-boundary))
      (dotimes (_ 10) (ignore-errors (undo)))
      (kill-buffer buf))))

(defun nm-pgo--mode-pass (files)
  "Fontify buffers in OTHER major modes. Without this, every mode a user
opens that is not elisp or org is laid out as cold code."
  (dolist (f files)
    (when (file-readable-p f)
      (let ((buf (find-file-noselect f)))
        (with-current-buffer buf
          (ignore-errors (normal-mode))
          (ignore-errors
            (font-lock-set-defaults)
            (font-lock-fontify-region (point-min) (min (point-max) 60000)))
          (set-buffer-modified-p nil))
        (kill-buffer buf)))))

(defun nm-pgo--lisp-data-pass ()
  "Reader, printer, sorting, hash tables and string formatting -- the
general-purpose Lisp machinery under every command."
  (let ((data nil))
    (dotimes (i 2000)
      (push (cons (format "key-%d" i) (list i (* i i) (number-to-string i))) data))
    (setq data (sort data (lambda (a b) (string< (car a) (car b)))))
    (let ((h (make-hash-table :test #'equal)))
      (dolist (d data) (puthash (car d) (cdr d) h))
      (dolist (d data) (gethash (car d) h)))
    (dotimes (_ 20)
      (car (read-from-string (prin1-to-string data))))))

(let* ((root (or (getenv "NEOMACS_RUNTIME_ROOT") default-directory))
       (el (expand-file-name "lisp/emacs-lisp/cl-macs.el" root))
       (bc (mapcar (lambda (n) (expand-file-name (concat "lisp/emacs-lisp/" n) root))
                   '("seq.el" "map.el" "pcase.el" "rx.el"))))
  (when (file-readable-p el)
    (nm-pgo--edit-pass el #'emacs-lisp-mode 60))
  (ignore-errors (require 'org))
  (ignore-errors (nm-pgo--org-pass 40))
  (nm-pgo--byte-compile-pass bc)
  ;; Breadth matters as much as depth: PGO PESSIMISES what it does not see,
  ;; and an editor's surface is enormous. Measured consequence of training on
  ;; fontification alone: the interactive edit loop came out +2%. These passes
  ;; keep the other common subsystems from being laid out as cold code.
  (ignore-errors (nm-pgo--search-pass el 3))
  (ignore-errors (nm-pgo--text-pass))
  (ignore-errors (nm-pgo--lisp-data-pass))
  (ignore-errors
    (nm-pgo--mode-pass
     (mapcar (lambda (n) (expand-file-name n root))
             '("lisp/progmodes/python.el" "lisp/progmodes/cc-mode.el"
               "lisp/dired.el" "lisp/net/tramp.el"))))
  (message "pgo-train: done"))
