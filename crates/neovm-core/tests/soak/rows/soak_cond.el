;;; soak_cond.el --- P11 soak: condition wake accounting (notify all / one) -*- lexical-binding: t; -*-
;;
;; SOAK_W waiters park on one condition variable.  The main thread waits (with
;; a bound) until every waiter has announced readiness, then:
;;   SOAK_KIND=all  broadcast once and join;
;;   SOAK_KIND=one  notify ONE, yield a bounded number of times, count how many
;;                  waiters woke from that single notify, then broadcast to
;;                  release the rest and join.
;; Per round the row checks the accounting invariant that holds in every mode:
;; every waiter either woke or burned the wait bound (never both, never
;; neither), the two counts sum to W, and a single notify never wakes more
;; than one waiter.  Sim documents its degeneration (wait/notify are no-ops;
;; every waiter burns the bound inline) but the accounting still balances, so
;; the state digest is mode-independent.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst cd-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst cd-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst cd-w (max 1 (string-to-number (or (getenv "SOAK_W") "8"))))
(defconst cd-bound 50)
(defconst cd-kind (or (getenv "SOAK_KIND") "all"))

(defvar cd-m (make-mutex "cd-m"))
(defvar cd-c (make-condition-variable cd-m))
(defvar cd-rm (make-mutex "cd-rm"))
(defvar cd-rc (make-condition-variable cd-rm))
(defvar cd-ready 0)
(defvar cd-go nil)
(defvar cd-woke nil)
(defvar cd-burned nil)
(defvar cd-woke-one 0)
(defvar cd-iters 0)
(defvar cd-bad nil)

(defun cd-waiter (_id)
  (with-mutex cd-m
    (with-mutex cd-rm
      (setq cd-ready (1+ cd-ready))
      (condition-notify cd-rc t))
    (let ((rounds 0))
      (while (and (not cd-go) (< rounds cd-bound))
        (setq rounds (1+ rounds))
        (condition-wait cd-c))
      (if (>= rounds cd-bound)
          (setq cd-burned (cons _id cd-burned))
        (setq cd-woke (cons _id cd-woke)))))
  _id)

(defun cd-wait-ready ()
  ;; Bounded readiness gate; Sim has already run every body inline here.
  (with-mutex cd-rm
    (let ((rounds 0))
      (while (and (< cd-ready cd-w) (< rounds 200))
        (setq rounds (1+ rounds))
        (condition-wait cd-rc)))))

(defun cd-release-all ()
  (with-mutex cd-m
    (setq cd-go t)
    (condition-notify cd-c t)))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) cd-secs)))
  (while (< (float-time) deadline)
    (setq cd-ready 0)
    (setq cd-go nil)
    (setq cd-woke nil)
    (setq cd-burned nil)
    (setq cd-woke-one 0)
    (let ((ths (mapcar (lambda (i) (make-thread (lambda () (cd-waiter i)) "cd"))
                       (number-sequence 0 (1- cd-w)))))
      (cd-wait-ready)
      (when (string= cd-kind "one")
        (with-mutex cd-m
          (setq cd-go t)
          (condition-notify cd-c))
        ;; Give the single woken waiter a bounded chance to record itself.
        (let ((rounds 0))
          (while (and (< (length cd-woke) 1) (< rounds 200))
            (setq rounds (1+ rounds))
            (thread-yield)))
        (setq cd-woke-one (length cd-woke))
        ;; Release the rest; they see go already t but are still asleep.
        (with-mutex cd-m
          (condition-notify cd-c t)))
      (when (string= cd-kind "all")
        (cd-release-all))
      (dolist (th ths)
        (thread-join th)))
    (let ((woke (length cd-woke))
          (burned (length cd-burned)))
      (unless (and (= (+ woke burned) cd-w)
                   (or (= woke cd-w) (= burned cd-w)))
        (setq cd-bad (list :woke woke :burned burned :want cd-w)))
      (when (and (string= cd-kind "one") (> cd-woke-one 1))
        (setq cd-bad (list :woke-one cd-woke-one))))
    (setq cd-iters (1+ cd-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT cond_notify_%s seed=%d iters=%d state=%s w=%d kind=%s woke=%d burned=%d woke_one=%d bad=%S\n"
               cd-kind cd-seed cd-iters (if cd-bad "fail" "ok") cd-w cd-kind
               (length (or cd-woke '())) (length (or cd-burned '())) cd-woke-one cd-bad))
