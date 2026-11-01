;;; soak_spawn_join.el --- P11 soak: make-thread/thread-join churn at N live -*- lexical-binding: t; -*-
;;
;; Each round spawns SOAK_N threads, each records its id in a shared list under
;; a mutex, then the row joins all of them and checks the multiset of ids is
;; exactly 0..N-1.  Rounds repeat until SOAK_SECS elapse.  The seed drives a
;; small bounded pre-lock spin per thread so different seeds exercise different
;; acquisition orders.
;;
;; Sim degeneration (threads-bench README): make-thread runs each body inline
;; and thread-join returns nil; the shared list still receives every id, so the
;; invariant is mode-independent and the state must be identical in Sim, Green
;; and GNU.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst sj-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst sj-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst sj-n (max 1 (string-to-number (or (getenv "SOAK_N") "16"))))
(defvar sj-m (make-mutex "sj-m"))
(defvar sj-seen nil)
(defvar sj-bad nil)
(defvar sj-iters 0)

(defun sj-body (id)
  ;; Per-thread local LCG: workers never mutate shared variables outside the
  ;; mutex, so the interleaving cannot be a data race on Lisp globals.
  (let* ((rng (+ sj-seed (* 1103 (1+ id))))
         (rng (mod (+ (* rng 1103515245) 12345) 2147483648))
         (spin (mod rng 3)))
    (while (> spin 0) (setq spin (1- spin))))
  (with-mutex sj-m
    (setq sj-seen (cons id sj-seen)))
  id)

(soak-gc-mark "start")
(let ((deadline (+ (float-time) sj-secs))
      (start (float-time))
      (force-gc (soak-force-gc-p))
      (progress-at 0))
  (while (< (float-time) deadline)
    (setq sj-seen nil)
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (sj-body i)) "sj"))
                       (number-sequence 0 (1- sj-n)))))
      (dolist (th ths)
        (thread-join th)))
    (unless (and (= (length sj-seen) sj-n)
                 (equal (sort (copy-sequence sj-seen) #'<)
                        (number-sequence 0 (1- sj-n))))
      (setq sj-bad (list :n sj-n :seen (length sj-seen))))
    (setq sj-iters (1+ sj-iters))
    ;; Forced-GC arm (SOAK_FORCE_GC=1): one full `(garbage-collect)' every 50
    ;; rounds -- the classification probe: an explicit full GC holds RSS flat
    ;; if the growth is uncollected Lisp data rather than a native leak.
    (when (and force-gc (= 0 (mod sj-iters 50)))
      (setq soak-last-heap-kib (soak-heap-kib-from (garbage-collect))))
    ;; One receipt mark per minute: gcs-done, allocation counters, and the
    ;; non-forcing heap census, so RSS growth can be attributed to a heap.
    (let ((elapsed (floor (- (float-time) start))))
      (while (>= elapsed progress-at)
        (soak-progress progress-at (and force-gc soak-last-heap-kib))
        (setq progress-at (+ progress-at 60))))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT spawn_join_live_%d seed=%d iters=%d state=%s n=%d seen=%d bad=%S\n"
               sj-n sj-seed sj-iters (if sj-bad "fail" "ok") sj-n
               (length (or sj-seen '())) sj-bad))
