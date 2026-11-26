;;; regex-gap-move-bench.el --- regexp search across the gap -*- lexical-binding: t; -*-

;; Reproducible bench for the regexp gap-move fix (N-regex-gap-move).
;;
;; Run one arm per process, alternately on the same pinned CPUs:
;;
;;   taskset -c 0-3,14-31 target/release/neomacs -Q --batch \
;;     -l crates/neovm-core/scripts/regex-gap-move-bench.el
;;
;; Every section prints five samples and the median in ms.  The result column
;; is checked inside each arm (hit counts, final gap-position); comparing the
;; result columns across arms also shows whether the gap moved.  All data is
;; generated in code; there are no fixtures.  Pass bar for the fix: every
;; section at or below the main arm plus the A/A band, with the edit+regex and
;; miss rows the ones that matter.  Keep builds and other benchmarks off the
;; host while measuring.

(require 'bytecomp)

;; One "(defun marker)" line per 505 chars: 4,999,905 chars of repetitive
;; text in which the engine patterns below find their candidates.
(defconst rgm-unit (concat (make-string 490 ?x) "(defun marker)\n"))
(defconst rgm-n 9901)
(defconst rgm-text
  (with-temp-buffer
    (dotimes (_ rgm-n) (insert rgm-unit))
    (buffer-string)))

(defun rgm-park (pos)
  (goto-char pos) (insert "Q") (delete-char -1))

(defun rgm-forward-miss ()
  (with-temp-buffer
    (insert rgm-text)
    (rgm-park (- (point-max) 10))
    (goto-char (point-min))
    (let ((hits 0))
      (while (re-search-forward "d[e]funn" nil t) (setq hits (1+ hits)))
      (list hits (gap-position)))))

(defun rgm-backward-miss ()
  (with-temp-buffer
    (insert rgm-text)
    (rgm-park 10)
    (goto-char (point-max))
    (let ((hits 0))
      (while (re-search-backward "d[e]funn" nil t) (setq hits (1+ hits)))
      (list hits (gap-position)))))

(defun rgm-forward-hit ()
  (with-temp-buffer
    (insert rgm-text)
    (rgm-park (/ (point-max) 10))
    (goto-char (point-min))
    (let ((hits 0))
      (while (re-search-forward "d[e]fun" nil t) (setq hits (1+ hits)))
      (list hits (gap-position)))))

;; Font-lock shape: edit at P (the gap parks there), then a bounded engine
;; search from the buffer start across P, 200 times.
(defun rgm-edit-regex ()
  (with-temp-buffer
    (insert rgm-text)
    (let ((p (/ (point-max) 10)) (hits 0))
      (dotimes (_ 200)
        (goto-char p)
        (insert "Q")
        (goto-char (point-min))
        (when (re-search-forward "d[e]funn" (min (point-max) (+ p 2000)) t)
          (setq hits (1+ hits)))
        (goto-char p)
        (delete-char 1))
      (list hits (gap-position)))))

(defun rgm-literal-control ()
  (with-temp-buffer
    (insert rgm-text)
    (rgm-park (- (point-max) 10))
    (goto-char (point-min))
    (let ((hits 0))
      (while (re-search-forward "defun" nil t) (setq hits (1+ hits)))
      (list hits (gap-position)))))

(defun rgm-measure (name fn)
  (let ((expected (funcall fn)))        ; one warm-up run
    (let (samples)
      (dotimes (_ 5)
        (let ((start (float-time))
              (result (funcall fn)))
          (push (* 1000.0 (- (float-time) start)) samples)
          (unless (equal result expected)
            (error "Bad %s result %S, expected %S" name result expected))))
      (let ((sorted (sort (copy-sequence samples) #'<)))
        (princ (format "BENCH %-16s median_ms=%7.3f samples_ms=%S result=%S\n"
                       name (nth 2 sorted) (nreverse samples) expected))))))

(princ (format "BENCH text_chars=%d\n" (length rgm-text)))
(rgm-measure "forward-miss" #'rgm-forward-miss)
(rgm-measure "backward-miss" #'rgm-backward-miss)
(rgm-measure "forward-hit" #'rgm-forward-hit)
(rgm-measure "edit+regex" #'rgm-edit-regex)
(rgm-measure "literal-control" #'rgm-literal-control)
(princ "BENCH DONE\n")
