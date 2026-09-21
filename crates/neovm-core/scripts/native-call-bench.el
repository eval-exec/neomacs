;;; native-call-bench.el --- native frame push cost -*- lexical-binding: t; -*-
;; Run with: taskset -c CORE EDITOR -Q --batch -l native-call-bench.el
;; Byte-compiled callers, 500,000 calls per sample, median of seven samples
;; after warmup. Run baseline/candidate processes in alternating order.
;; string-bytes is a ContextVec builtin in Neomacs: its one-argument row is
;; a control that uses the stack call path rather than the native frame push.
(require 'bytecomp)
;; Aliases keep the compiler from replacing these calls with dedicated opcodes.
(fset 'native-frame-zero (symbol-function 'point))
(fset 'native-frame-one (symbol-function 'string-bytes))
(fset 'native-frame-two (symbol-function 'string-lessp))
(fset 'native-frame-three (symbol-function 'get-text-property))
(defun native-frame-loop-zero (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (native-frame-zero))) answer))
(defun native-frame-loop-one (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (native-frame-one "abc"))) answer))
(defun native-frame-loop-two (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (native-frame-two "abc" "def"))) answer))
(defun native-frame-loop-three (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (native-frame-three 0 'face "abc"))) answer))
(dolist (case '((native-frame-loop-zero . 1)
                (native-frame-loop-one . 3)
                (native-frame-loop-two . t)
                (native-frame-loop-three . nil)))
  (byte-compile (car case))
  (unless (byte-code-function-p (symbol-function (car case)))
    (error "Expected bytecode for %s" (car case)))
  (dotimes (_ 40) (funcall (car case) 2000))
  (let (samples)
    (dotimes (_ 7)
      (let* ((start (float-time))
             (value (funcall (car case) 500000))
             (elapsed (- (float-time) start)))
        (unless (equal value (cdr case))
          (error "Wrong result for %s: %S" (car case) value))
        (push (* elapsed 1e6) samples)))
    (princ (format "BENCH %s n=500000 median_us=%.1f samples_us=%S\n"
                   (car case) (nth 3 (sort (copy-sequence samples) #'<))
                   (nreverse samples)))))
