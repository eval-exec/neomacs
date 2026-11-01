;;; soak_weak_cache.el --- P11 soak: weak hash tables and string-match caches -*- lexical-binding: t; -*-
;;
;; Each round builds a weak-key `equal' hash table, keeps strong references to
;; half of the keys, runs a worker that hammers the string-match cache while
;; the main thread forces a full GC, and then checks:
;;   - every strongly held key is still present with its value;
;;   - every key held only by the table was removed by the full GC;
;;   - the worker's and main's string-match results are exactly 0.
;; The worker runs inline first under Sim and concurrently under Green/GNU;
;; the checked state is mode-independent.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst wc-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst wc-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst wc-n (max 1 (string-to-number (or (getenv "SOAK_N") "8"))))

(defvar wc-m (make-mutex "wc-m"))
(defvar wc-worker-match nil)
(defvar wc-iters 0)
(defvar wc-bad nil)
(defvar wc-last-strong 0)
(defvar wc-last-weak 0)

(defun wc-worker ()
  (let ((acc 0))
    (dotimes (_k 200)
      (setq acc (+ acc (string-match "a+b" "aaab"))))
    (with-mutex wc-m
      (setq wc-worker-match acc))
    acc))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) wc-secs)))
  (while (< (float-time) deadline)
    (setq wc-worker-match nil)
    (let ((table (make-hash-table :test 'equal :weakness 'key))
          (strong nil))
      ;; Odd keys are deliberately NOT retained by the row: only the weak
      ;; table holds them, so the full GC must purge those entries.  Equal
      ;; lookup keys are re-formatted after the GC instead of being kept.
      (dotimes (i wc-n)
        (let ((key (format "wc-%d-%d" wc-seed i)))
          (puthash key i table)
          (when (= 0 (mod i 2))
            (push key strong))))
      (let ((worker (make-thread (lambda () (wc-worker)) "wc")))
        (garbage-collect)
        (thread-join worker))
      (let ((strong-ok 0) (weak-gone 0))
        (dotimes (i wc-n)
          (if (= 0 (mod i 2))
              (if (equal (gethash (format "wc-%d-%d" wc-seed i) table) i)
                  (setq strong-ok (1+ strong-ok))
                (setq wc-bad (list :strong-missing i)))
            (let ((value (gethash (format "wc-%d-%d" wc-seed i) table)))
              (if (null value)
                  (setq weak-gone (1+ weak-gone))
                (setq wc-bad (list :weak-survived i value))))))
        (setq wc-last-strong strong-ok)
        (setq wc-last-weak weak-gone)
        ;; `strong' is the live strong-reference anchor the GC must see.
        (unless (and (= strong-ok (/ (+ wc-n 1) 2))
                     (= weak-gone (/ wc-n 2))
                     (= (length strong) (/ (+ wc-n 1) 2)))
          (setq wc-bad (list :counts strong-ok weak-gone (length strong))))
      (unless (and wc-worker-match (= wc-worker-match 0) (= (string-match "a+b" "aaab") 0))
        (setq wc-bad (list :match wc-worker-match (string-match "a+b" "aaab")))))
    (setq wc-iters (1+ wc-iters)))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT weak_hash_match_cache seed=%d iters=%d state=%s n=%d strong=%d weak_gone=%d match=%S bad=%S\n"
               wc-seed wc-iters (if wc-bad "fail" "ok") wc-n wc-last-strong wc-last-weak
               wc-worker-match wc-bad))
