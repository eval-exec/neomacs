;;; gap-bounded-bench.el --- Check bounded search after edits -*- lexical-binding: t; -*-

;; Run with a release/profiling Neomacs and GNU Emacs:
;;   EDITOR -Q --batch -l crates/neovm-core/scripts/gap-bounded-bench.el
;; The gap is parked beyond a short search bound before every search.
;; Increasing total buffer size must not increase the text moved for it.
;; Each row reports five checked samples of a byte-compiled loop; setup and
;; complete-buffer validation are outside the timed interval. GC stays enabled.

(require 'bytecomp)

(defun gap-bounded-bench-loop (n edit-at search)
  (let (answer)
    (dotimes (_ n)
      (goto-char edit-at)
      (insert "a")
      (delete-char -1)
      (goto-char 1)
      (setq answer (and search (re-search-forward "z" 9 t))))
    answer))

(byte-compile 'gap-bounded-bench-loop)
(unless (byte-code-function-p (symbol-function 'gap-bounded-bench-loop))
  (error "Expected a byte-compiled benchmark loop"))

(dolist (multibyte '(nil t))
  (dolist (size '(1024 65536 1048576 8388608))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (let ((original (if multibyte
                          (concat "aaaaaaaa中" (make-string (- size 9) ?a))
                        (make-string size ?a)))
            (case-fold-search nil)
            (edit-at (1+ (/ size 2))))
        (insert original)
        (dolist (case '((edit-only . nil) (edit-search . t)))
          (gap-bounded-bench-loop 100 edit-at (cdr case))
          (let (samples)
            (dotimes (_ 5)
              (let* ((start (float-time))
                     (answer (gap-bounded-bench-loop 1000 edit-at (cdr case)))
                     (elapsed (- (float-time) start)))
                (unless (and (null answer) (= (point) 1)
                             (= (buffer-size) size)
                             (equal (buffer-string) original))
                  (error "Bad %s result at size %s" (car case) size))
                (push (* elapsed 1e6) samples)))
            (princ (format "BENCH %s-%s-%d n=1000 median_us=%.1f samples_us=%S\n"
                           (if multibyte 'multibyte 'unibyte) (car case) size
                           (nth 2 (sort (copy-sequence samples) #'<))
                           (nreverse samples)))))))))
