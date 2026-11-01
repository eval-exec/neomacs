;;; soak_redefine.el --- P11 soak: fset of inlined callees while loops park -*- lexical-binding: t; -*-
;;
;; The green-guard scenario.  A byte-compiled hot loop calls `logand' directly
;; AND through the function cell, comparing the two every iteration.  Main
;; warms that loop first, then parks SOAK_N workers behind a readiness gate,
;; redefines `logand'/`mod' with fsets to distinctive lambdas, and releases the
;; workers: every direct call in the now-stale compiled code must agree with
;; the function cell (the epoch must invalidate an inlined leaf), and each
;; worker's sum must be exactly the all-old or all-new value, never a mix.
;; Originals are restored every round and re-checked.
;;
;; Sim runs workers inline before the fsets (old definitions); Green/GNU run
;; them after (new definitions).  The invariant is mode-independent: no
;; direct/cell mismatch, no mixed sum, restore verified.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst rd-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst rd-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst rd-n (max 1 (string-to-number (or (getenv "SOAK_N") "4"))))
(defconst rd-bound 50)
(defconst rd-iter 200)

(defvar rd-m (make-mutex "rd-m"))
(defvar rd-c (make-condition-variable rd-m))
(defvar rd-rm (make-mutex "rd-rm"))
(defvar rd-rc (make-condition-variable rd-rm))
(defvar rd-ready 0)
(defvar rd-go nil)
(defvar rd-results nil)
(defvar rd-iters 0)
(defvar rd-bad nil)
(defvar rd-last-new 0)

(defvar rd-loop
  (byte-compile
   (lambda (n)
     (let ((sum 0) (mismatch 0))
       (dotimes (i n)
         (let* ((x (+ i rd-seed 3))
                (d (logand x 6))
                (c (funcall (symbol-function 'logand) x 6)))
           (unless (= d c)
             (setq mismatch (1+ mismatch)))
           (setq sum (+ sum d))))
       (list sum mismatch)))))

(defun rd-expected-old ()
  (let ((sum 0))
    (dotimes (i rd-iter)
      (setq sum (+ sum (logand (+ i rd-seed 3) 6))))
    sum))

(defun rd-worker (id)
  (with-mutex rd-rm
    (setq rd-ready (1+ rd-ready))
    (condition-notify rd-rc t))
  (with-mutex rd-m
    (let ((rounds 0))
      (while (and (not rd-go) (< rounds rd-bound))
        (setq rounds (1+ rounds))
        (condition-wait rd-c))
      (let ((result (funcall rd-loop rd-iter)))
        (push (cons id result) rd-results))))
  id)

(defun rd-wait-ready ()
  (with-mutex rd-rm
    (let ((rounds 0))
      (while (and (< rd-ready rd-n) (< rounds 200))
        (setq rounds (1+ rounds))
        (condition-wait rd-rc)))))

;; Warm the loop under the original definitions so a tier-up that inlines
;; `logand' has happened before the first redefinition.
(dotimes (_warm 2000)
  (funcall rd-loop 4))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) rd-secs))
      (orig-logand (symbol-function 'logand))
      (orig-mod (symbol-function 'mod))
      (start (float-time))
      (force-gc (soak-force-gc-p))
      (progress-at 0))
  (while (< (float-time) deadline)
    (setq rd-ready 0)
    (setq rd-go nil)
    (setq rd-results nil)
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (rd-worker i)) "rd"))
                       (number-sequence 0 (1- rd-n)))))
      (rd-wait-ready)
      (fset 'logand (lambda (&rest _args) 4242))
      (fset 'mod (lambda (_a _b) 7777))
      (with-mutex rd-m
        (setq rd-go t)
        (condition-notify rd-c t))
      (dolist (th ths)
        (thread-join th))
      ;; Main's own compiled loop runs while the redefinition is live: it must
      ;; see the new definitions through both call shapes.
      (let* ((main-result (funcall rd-loop rd-iter))
             (main-sum (car main-result))
             (main-mismatch (cadr main-result))
             (want-new (* rd-iter 4242)))
        (unless (and (= main-mismatch 0) (= main-sum want-new))
          (setq rd-bad (list :main main-result :want want-new))))
      ;; Restore and verify the originals are callable again.
      (fset 'logand orig-logand)
      (fset 'mod orig-mod)
      (unless (= (logand 6 3) 2)
        (setq rd-bad (list :restore (logand 6 3)))))
    (let ((old 0) (new 0) (mismatch 0) (mixed 0)
          (want-old (rd-expected-old)) (want-new (* rd-iter 4242)))
      (dolist (entry rd-results)
        (let ((sum (car (cdr entry))) (mm (car (cdr (cdr entry)))))
          (setq mismatch (+ mismatch mm))
          (cond ((= sum want-new) (setq new (1+ new)))
                ((= sum want-old) (setq old (1+ old)))
                (t (setq mixed (1+ mixed))))))
      (setq rd-last-new new)
      (unless (and (= mismatch 0) (= mixed 0)
                   (or (= new rd-n) (= old rd-n))
                   (= (+ old new) rd-n))
        (setq rd-bad (list :workers old new mixed mismatch))))
    (setq rd-iters (1+ rd-iters))
    ;; Forced-GC arm (SOAK_FORCE_GC=1): one full `(garbage-collect)' every 50
    ;; rounds, the SOAK-FINDING-3 classification probe.
    (when (and force-gc (= 0 (mod rd-iters 50)))
      (setq soak-last-heap-kib (soak-heap-kib-from (garbage-collect))))
    ;; One receipt mark per minute: gcs-done, allocation counters, and the
    ;; non-forcing heap census, so RSS growth can be attributed to a heap.
    (let ((elapsed (floor (- (float-time) start))))
      (while (>= elapsed progress-at)
        (soak-progress progress-at (and force-gc soak-last-heap-kib))
        (setq progress-at (+ progress-at 60))))))
(soak-gc-mark "end")
(princ (format "SOAK-RESULT redefine_inlined_parked seed=%d iters=%d state=%s n=%d new=%d bad=%S\n"
               rd-seed rd-iters (if rd-bad "fail" "ok") rd-n rd-last-new rd-bad))
