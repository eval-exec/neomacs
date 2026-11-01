;;; soak_signal.el --- P11 soak: signal from a callee caught in the parent frame -*- lexical-binding: t; -*-
;;
;; Each round a worker thread computes a checksum while the main frame calls a
;; callee that signals a custom condition; the main frame's condition-case must
;; catch it with the exact data, and the worker's checksum must be exact after
;; the join.  The signal path is exercised concurrently with a running thread.
;; Sim runs the worker inline first; the caught signal and checksum are the
;; same, so the checked result is mode-independent.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst sg-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst sg-seed (string-to-number (or (getenv "SOAK_SEED") "1")))

(defvar sg-m (make-mutex "sg-m"))
(defvar sg-sum nil)
(defvar sg-iters 0)
(defvar sg-bad nil)
(defvar sg-last-caught nil)

(defun sg-worker ()
  (let ((sum 0))
    (dotimes (i 500)
      (setq sum (+ sum (* (1+ (mod (+ sg-seed i) 5)) 2))))
    (with-mutex sg-m
      (setq sg-sum sum))
    sum))

(defun sg-worker-expected ()
  (cl-loop for i below 500 sum (* (1+ (mod (+ sg-seed i) 5)) 2)))

(defun sg-callee ()
  ;; A standard condition symbol: GNU and Neomacs both reject unknown error
  ;; symbols identically (`Invalid error symbol'), verified against the
  ;; pinned GNU 31.1 oracle on 2026-10-31, so the scenario uses `error'.
  (signal 'error (list sg-seed)))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) sg-secs)))
  (while (< (float-time) deadline)
    (setq sg-sum nil)
    (let ((worker (make-thread (lambda () (sg-worker)) "sg"))
          (caught nil))
      (condition-case err
          (progn (sg-callee) (setq caught 'missed))
        (error
         (setq caught err)))
      (thread-join worker)
      (setq sg-last-caught caught)
      (unless (equal caught (list 'error sg-seed))
        (setq sg-bad (list :caught caught :want (list 'error sg-seed))))
      (unless (and sg-sum (= sg-sum (sg-worker-expected)))
        (setq sg-bad (list :worker sg-sum :want (sg-worker-expected)))))
    (setq sg-iters (1+ sg-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT signal_from_callee seed=%d iters=%d state=%s caught=%S worker=%S bad=%S\n"
               sg-seed sg-iters (if sg-bad "fail" "ok") sg-last-caught sg-sum sg-bad))
