;;; soak_timers.el --- P11 soak: sleep-for/sit-for deadlines across threads -*- lexical-binding: t; -*-
;;
;; Each round spawns SOAK_N sleepers with seed-derived sub-second deadlines,
;; each recording its elapsed time under a mutex; the main thread also runs a
;; sit-for deadline.  Per round the row checks that every completed sleeper
;; waited at least its requested duration (no early wake) and that main's
;; sit-for also observed its deadline.  Completion is all-or-nothing across
;; modes: real threads complete all sleepers; Sim may park a simulated sleeper
;; (threads-bench documents Sim parking on scheduler operations), which shows
;; as done=0 and is not a divergence.  A partial set, or any early wake, fails.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst tm-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst tm-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst tm-n (max 1 (string-to-number (or (getenv "SOAK_N") "8"))))

(defvar tm-m (make-mutex "tm-m"))
(defvar tm-done nil)
(defvar tm-early nil)
(defvar tm-iters 0)
(defvar tm-bad nil)
(defvar tm-last-done 0)
(defvar tm-last-sit nil)

(defun tm-sleeper (id)
  (let* ((rng (+ tm-seed (* 48271 (1+ id))))
         (d (/ (float (1+ (mod rng 5))) 1000.0))
         (start (float-time)))
    (sleep-for d)
    (let ((elapsed (- (float-time) start)))
      (with-mutex tm-m
        (aset tm-done id 1)
        (when (< elapsed d)
          (setq tm-early (list :id id :want d :got elapsed))))))
  id)

(soak-gc-mark "start")
(let ((deadline (+ (float-time) tm-secs)))
  (while (< (float-time) deadline)
    (setq tm-done (make-vector tm-n 0))
    (setq tm-early nil)
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (tm-sleeper i)) "tm"))
                       (number-sequence 0 (1- tm-n)))))
      (dolist (th ths)
        (thread-join th)))
    (let* ((start (float-time))
           (got (sit-for 0.002))
           (elapsed (- (float-time) start)))
      (setq tm-last-sit (list got elapsed))
      (when (< elapsed 0.002)
        (setq tm-bad (list :sit-early 0.002 elapsed))))
    (let ((done (cl-count 1 tm-done)))
      (setq tm-last-done done)
      (unless (or (= done 0) (= done tm-n))
        (setq tm-bad (list :partial done :want tm-n))))
    (when tm-early
      (setq tm-bad tm-early))
    (setq tm-iters (1+ tm-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT timers seed=%d iters=%d state=%s n=%d done=%d sit=%S bad=%S\n"
               tm-seed tm-iters (if tm-bad "fail" "ok") tm-n tm-last-done tm-last-sit tm-bad))
