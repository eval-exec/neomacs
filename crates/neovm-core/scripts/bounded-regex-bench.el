;;; bounded-regex-bench.el --- bounded line-anchor scan cost -*- lexical-binding: t; -*-
;; Run with: taskset -c CORE EDITOR -Q --batch -l bounded-regex-bench.el
;; Alternate baseline/candidate/GNU processes on the same CPU, without builds
;; or other benchmarks running. GC stays enabled. Every sample checks the
;; result, point and match data. The scaling rows keep BOUND fixed at 9.
(require 'bytecomp)

(defun bounded-regex-failure-loop (n pattern bound)
  (let (answer)
    (dotimes (_ n)
      (setq answer (re-search-forward pattern bound t)))
    answer))

(defun bounded-regex-repeat-loop (n pattern start bound backward)
  (let (answer)
    (dotimes (_ n)
      (goto-char start)
      (setq answer (if backward
                       (re-search-backward pattern bound t)
                     (re-search-forward pattern bound t))))
    answer))

(dolist (fn '(bounded-regex-failure-loop bounded-regex-repeat-loop))
  (byte-compile fn)
  (unless (byte-code-function-p (symbol-function fn))
    (error "Expected bytecode for %s" fn)))

(defun bounded-regex-measure (name fn n args expected expected-point expected-data)
  (set-match-data '(7 9))
  (dotimes (_ 2000) (apply fn 1 args))
  (dotimes (_ 20) (apply fn 100 args))
  (let (samples)
    (dotimes (_ 5)
      (let* ((start (float-time))
             (answer (apply fn n args))
             (elapsed (- (float-time) start))
             (data (mapcar (lambda (value)
                             (if (bufferp value)
                                 (eq value (current-buffer)) value))
                           (match-data t))))
        (unless (and (equal answer expected)
                     (= (point) expected-point)
                     (equal data expected-data))
          (error "Bad %s: answer=%S point=%S data=%S" name answer (point) data))
        (push (* elapsed 1e6) samples)))
    (princ (format "BENCH %s n=%d median_us=%.1f samples_us=%S\n"
                   name n (nth 2 (sort (copy-sequence samples) #'<))
                   (nreverse samples)))))

(dolist (size '(1024 65536 1048576 8388608))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (make-string size ?a))
    (let ((case-fold-search nil))
      (dolist (case '((bol . "^z") (plain . "z")))
        (goto-char 1)
        (bounded-regex-measure
         (format "%s-%d" (car case) size)
         #'bounded-regex-failure-loop 10000 (list (cdr case) 9) nil 1 '(7 9))))))

;; NAME TEXT PATTERN START BOUND BACKWARD CASE-FOLD N RESULT POINT MATCH-DATA
(dolist (case (list
               (list 'unbounded-line (make-string 65536 ?a)
                     "^z" 1 nil nil nil 10000 nil 1 '(7 9))
               (list 'forward-lines (concat (apply #'concat (make-list 1024 "a\n")) "z\n")
                     "^z" 1 nil nil nil 1000 2050 2050 '(2049 2050 t))
               (list 'backward-lines (concat "z\n" (apply #'concat (make-list 1024 "a\n")))
                     "^z" 2051 1 t nil 1000 1 1 '(1 2 t))
               '(bounded-capture "aa\nz" "^\\(z\\)" 1 5 nil nil 10000 5 5 (4 5 4 5 t))
               '(zero-at-bound "aa\nz" "^\\(\\)" 2 4 nil nil 10000 4 4 (4 4 4 4 t))
               '(multibyte "é中\n日" "^\\(日\\)" 2 5 nil nil 10000 5 5 (4 5 4 5 t))
               '(case-fold "aa\nZ" "^z" 2 5 nil t 10000 5 5 (4 5 t))
               '(optional-failure "aa\nz" "^z?x" 2 4 nil nil 10000 nil 2 (7 9))))
  (with-temp-buffer
    (insert (nth 1 case))
    (let ((case-fold-search (nth 6 case)))
      (bounded-regex-measure
       (nth 0 case) #'bounded-regex-repeat-loop (nth 7 case)
       (list (nth 2 case) (nth 3 case) (nth 4 case) (nth 5 case))
       (nth 8 case) (nth 9 case) (nth 10 case)))))
