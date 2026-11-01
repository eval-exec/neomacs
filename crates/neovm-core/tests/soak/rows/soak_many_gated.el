;;; soak_many_gated.el --- P11 soak: many short threads behind a readiness gate -*- lexical-binding: t; -*-
;;
;; SOAK_N threads all park behind one readiness gate, then a broadcast
;; releases them and the row joins all of them.  Per round it checks:
;;   - wake accounting is all-or-nothing: every worker either observed the
;;     release or burned its bounded wait (never both, never neither), so
;;     woke+burned == N and woke is 0 (Sim) or N (real threads);
;;   - every thread-join value is either the documented Sim nil or exactly the
;;     worker's own value -- a wrong value fails in every mode.
;; This is the many_threads shape with checked state, duration-looped.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst mg-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst mg-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst mg-n (max 1 (string-to-number (or (getenv "SOAK_N") "100"))))
(defconst mg-bound 50)

(defvar mg-m (make-mutex "mg-m"))
(defvar mg-c (make-condition-variable mg-m))
(defvar mg-rm (make-mutex "mg-rm"))
(defvar mg-rc (make-condition-variable mg-rm))
(defvar mg-ready 0)
(defvar mg-go nil)
(defvar mg-observed nil)
(defvar mg-iters 0)
(defvar mg-bad nil)
(defvar mg-last-woke 0)
(defvar mg-last-join-ok 0)

(defun mg-worker (id)
  (with-mutex mg-m
    (with-mutex mg-rm
      (setq mg-ready (1+ mg-ready))
      (condition-notify mg-rc t))
    (let ((rounds 0))
      (while (and (not mg-go) (< rounds mg-bound))
        (setq rounds (1+ rounds))
        (condition-wait mg-c))
      (when (< rounds mg-bound)
        (aset mg-observed id 1))))
  (+ id 1 mg-seed))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) mg-secs)))
  (while (< (float-time) deadline)
    (setq mg-ready 0)
    (setq mg-go nil)
    (setq mg-observed (make-vector mg-n 0))
    (let* ((ths (mapcar (lambda (i) (make-thread (lambda () (mg-worker i)) "mg"))
                        (number-sequence 0 (1- mg-n))))
           (joined nil))
      (with-mutex mg-rm
        (let ((rounds 0))
          (while (and (< mg-ready mg-n) (< rounds 200))
            (setq rounds (1+ rounds))
            (condition-wait mg-rc))))
      (with-mutex mg-m
        (setq mg-go t)
        (condition-notify mg-c t))
      (let ((id 0))
        (dolist (th ths)
          (push (cons id (thread-join th)) joined)
          (setq id (1+ id))))
      (let* ((woke (cl-count 1 mg-observed))
             (burned (- mg-n woke))
             (join-ok 0)
             (wrong nil))
        (dolist (entry joined)
          (let ((expect (+ (car entry) 1 mg-seed)))
            (cond ((null (cdr entry)) nil)
                  ((= (cdr entry) expect) (setq join-ok (1+ join-ok)))
                  (t (setq wrong (list :join (car entry) (cdr entry) expect))))))
        (setq mg-last-woke woke)
        (setq mg-last-join-ok join-ok)
        (unless (and (= (+ woke burned) mg-n)
                     (or (= woke mg-n) (= woke 0)))
          (setq mg-bad (list :accounting woke burned :want mg-n)))
        (when wrong
          (setq mg-bad wrong))))
    (setq mg-iters (1+ mg-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT many_gated_%d seed=%d iters=%d state=%s n=%d woke=%d join_ok=%d bad=%S\n"
               mg-n mg-seed mg-iters (if mg-bad "fail" "ok") mg-n mg-last-woke mg-last-join-ok mg-bad))
