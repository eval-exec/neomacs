;;; soak_yield.el --- P11 soak: thread-yield loops under spawn/join -*- lexical-binding: t; -*-
;;
;; SOAK_N workers each perform SOAK_K yields and then record completion in
;; their own slot under a mutex; the row joins them and checks that completion
;; is all-or-nothing: every worker either completed (real threads) or none did
;; (Sim parks a worker forever at its first thread-yield; threads-bench README
;; documents the degeneration).  A partial completion set is a failure in every
;; mode, so the state digest is mode-independent while the diagnostic
;; `done=<n>/<n*k>` records the liveness regime.  The first all-parked round
;; ends the Sim run: each parked worker stays live in the simulated scheduler,
;; so continuing would only accumulate them toward the sandbox cap (the
;; 30-minute Sim arm proved it), while real threads keep looping the full
;; duration.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst yl-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst yl-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst yl-n (max 1 (string-to-number (or (getenv "SOAK_N") "8"))))
(defconst yl-k (max 1 (string-to-number (or (getenv "SOAK_K") "20"))))

(defvar yl-m (make-mutex "yl-m"))
(defvar yl-done nil)
(defvar yl-iters 0)
(defvar yl-bad nil)
(defvar yl-last-done 0)

(defun yl-body (id)
  (let ((rng (+ yl-seed (* 104729 (1+ id)))))
    (dotimes (_k yl-k)
      (setq rng (mod (+ (* rng 1103515245) 12345) 2147483648))
      (thread-yield))
    (with-mutex yl-m
      (aset yl-done id 1)))
  id)

(soak-gc-mark "start")
(let ((deadline (+ (float-time) yl-secs)))
  (while (< (float-time) deadline)
    (setq yl-done (make-vector yl-n 0))
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (yl-body i)) "yl"))
                       (number-sequence 0 (1- yl-n)))))
      (dolist (th ths)
        (thread-join th)))
    (let ((done (cl-count 1 yl-done)))
      (setq yl-last-done done)
      (unless (or (= done 0) (= done yl-n))
        (setq yl-bad (list :partial done :want yl-n)))
      ;; Sim parks every worker forever at its first thread-yield (README):
      ;; the parked workers stay live in the simulated scheduler, so an
      ;; unbounded round loop accumulates 8 per round until the sandbox cap
      ;; (the 30-minute Sim arm was OOM-killed at 8 GiB after ~100 s; each
      ;; round is ~35 us there).  The degeneration is documented and checked
      ;; here, and one round is enough for the mode-independent digest.
      (when (= done 0)
        (setq deadline (- (float-time) 1))))
    (setq yl-iters (1+ yl-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT yield_loop seed=%d iters=%d state=%s n=%d k=%d done=%d/%d bad=%S\n"
               yl-seed yl-iters (if yl-bad "fail" "ok") yl-n yl-k
               yl-last-done (* yl-n yl-k) yl-bad))
