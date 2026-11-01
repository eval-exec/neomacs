;;; soak_mutex_ordered.el --- P11 soak: mutex contention with ordered counters -*- lexical-binding: t; -*-
;;
;; SOAK_N threads each perform SOAK_K increments of a shared counter under one
;; mutex, appending (id . seq) to a shared log in the same critical section.
;; Per round the row checks: total == N*K, every thread's own log subsequence is
;; strictly increasing, and every per-thread count == K.  Rounds repeat until
;; SOAK_SECS.  A per-thread LCG (seeded by SOAK_SEED and the id) drives a
;; bounded pre-lock spin so acquisition interleavings vary by seed.
;;
;; Sim runs each thread body inline (uncontended), so the same invariants must
;; hold: state is mode-independent.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst mo-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst mo-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst mo-n (max 1 (string-to-number (or (getenv "SOAK_N") "8"))))
(defconst mo-k (max 1 (string-to-number (or (getenv "SOAK_K") "20"))))

(defvar mo-m (make-mutex "mo-m"))
(defvar mo-counter 0)
(defvar mo-log nil)
(defvar mo-counts nil)
(defvar mo-bad nil)
(defvar mo-iters 0)

(defun mo-body (id)
  (let ((rng (+ mo-seed (* 7919 (1+ id)))))
    (dotimes (k mo-k)
      (setq rng (mod (+ (* rng 1103515245) 12345) 2147483648))
      (let ((spin (mod rng 3)))
        (while (> spin 0) (setq spin (1- spin))))
      (with-mutex mo-m
        (setq mo-counter (1+ mo-counter))
        (setq mo-log (cons (cons id k) mo-log))
        (aset mo-counts id (1+ (aref mo-counts id)))))
    id))

(defun mo-check ()
  (let ((bad nil))
    (unless (= mo-counter (* mo-n mo-k))
      (setq bad (list :total mo-counter :want (* mo-n mo-k))))
    (dotimes (id mo-n)
      (unless (= (aref mo-counts id) mo-k)
        (setq bad (list :count id (aref mo-counts id) :want mo-k))))
    (unless (= (length mo-log) (* mo-n mo-k))
      (setq bad (list :log (length mo-log) :want (* mo-n mo-k))))
    ;; Each thread's own subsequence must be strictly increasing in k
    ;; (mo-log is newest-first, so scan it oldest-first).
    (dotimes (id mo-n)
      (let ((last -1))
        (dolist (entry (reverse mo-log))
          (when (= (car entry) id)
            (unless (> (cdr entry) last)
              (setq bad (list :order id (cdr entry) last)))
            (setq last (cdr entry))))))
    bad))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) mo-secs))
      (start (float-time))
      (force-gc (soak-force-gc-p))
      (progress-at 0))
  (while (< (float-time) deadline)
    (setq mo-counter 0)
    (setq mo-log nil)
    (setq mo-counts (make-vector mo-n 0))
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (mo-body i)) "mo"))
                       (number-sequence 0 (1- mo-n)))))
      (dolist (th ths)
        (thread-join th)))
    (let ((bad (mo-check)))
      (when bad
        (setq mo-bad bad)))
    (setq mo-iters (1+ mo-iters))
    ;; Forced-GC arm (SOAK_FORCE_GC=1): see soak_spawn_join.el.
    (when (and force-gc (= 0 (mod mo-iters 50)))
      (setq soak-last-heap-kib (soak-heap-kib-from (garbage-collect))))
    ;; One receipt mark per minute: gcs-done, allocation counters, and the
    ;; non-forcing heap census, so RSS growth can be attributed to a heap.
    (let ((elapsed (floor (- (float-time) start))))
      (while (>= elapsed progress-at)
        (soak-progress progress-at (and force-gc soak-last-heap-kib))
        (setq progress-at (+ progress-at 60))))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT mutex_ordered seed=%d iters=%d state=%s n=%d k=%d bad=%S\n"
               mo-seed mo-iters (if mo-bad "fail" "ok") mo-n mo-k mo-bad))
