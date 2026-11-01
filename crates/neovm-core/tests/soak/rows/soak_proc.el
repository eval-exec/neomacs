;;; soak_proc.el --- P11 soak: accept-process-output on a real process -*- lexical-binding: t; -*-
;;
;; Each round starts a real `cat` child (make-process filter, the validated
;; blocking_io pattern), sends a seed-derived set of lines, and drains it with
;; accept-process-output while a worker thread computes a checksum.  The row
;; checks that every sent line is echoed byte-for-byte and that the concurrent
;; worker's checksum is exact; the child is killed at the end of the round.
;; Sim runs the worker inline before the I/O, Green overlaps it, GNU the same:
;; the checked result is mode-independent.

(require 'cl-lib)
(load (concat (file-name-directory (or load-file-name buffer-file-name)) "soak_common.el") nil t)

(defconst pr-secs (max 1 (string-to-number (or (getenv "SOAK_SECS") "10"))))
(defconst pr-seed (string-to-number (or (getenv "SOAK_SEED") "1")))
(defconst pr-lines (max 1 (string-to-number (or (getenv "SOAK_LINES") "8"))))

(defvar pr-m (make-mutex "pr-m"))
(defvar pr-sum nil)
(defvar pr-iters 0)
(defvar pr-bad nil)
(defvar pr-last-echoes 0)

(defun pr-worker ()
  (let ((sum 0))
    (dotimes (i 1000)
      (setq sum (+ sum (* (1+ (mod (+ pr-seed i) 7)) 3))))
    (with-mutex pr-m
      (setq pr-sum sum))
    sum))

(defun pr-worker-expected ()
  (cl-loop for i below 1000 sum (* (1+ (mod (+ pr-seed i) 7)) 3)))

(defun pr-echoed-lines (chunks)
  ;; The process filter delivers arbitrary chunks (cat may coalesce all
  ;; lines into one), so count newline-terminated lines, not callbacks.
  (let ((text (apply #'concat (nreverse (copy-sequence chunks)))))
    (butlast (split-string text "\n"))))

(soak-gc-mark "start")
(let ((deadline (+ (float-time) pr-secs)))
  (while (< (float-time) deadline)
    (setq pr-sum nil)
    (let* ((chunks nil)
           (proc (make-process :name "soak-cat" :command '("cat") :noquery t
                               :connection-type 'pipe
                               :filter (lambda (_p s) (push (substring s 0) chunks))
                               :sentinel (lambda (_p _e) nil)))
           (worker (make-thread (lambda () (pr-worker)) "pr"))
           (sent (mapcar (lambda (i) (format "soak-%d-%d" pr-seed i))
                         (number-sequence 0 (1- pr-lines)))))
      (unwind-protect
          (progn
            (dolist (line sent)
              (process-send-string proc (concat line "\n")))
            (let ((rounds 0))
              (while (and (< (length (pr-echoed-lines chunks)) pr-lines)
                          (< rounds 2000))
                (setq rounds (1+ rounds))
                (accept-process-output proc 0.01)))
            (let ((echoed (pr-echoed-lines chunks)))
              (setq pr-last-echoes (length echoed))
              (unless (equal echoed sent)
                (setq pr-bad (list :echo (length echoed) :want pr-lines
                                   :text echoed)))))
        (thread-join worker)
        (ignore-errors (delete-process proc)))
      (unless (and pr-sum (= pr-sum (pr-worker-expected)))
        (setq pr-bad (list :worker pr-sum :want (pr-worker-expected)))))
    (setq pr-iters (1+ pr-iters))))

(soak-gc-mark "end")
(princ (format "SOAK-RESULT accept_process_output seed=%d iters=%d state=%s lines=%d echoed=%d worker=%S bad=%S\n"
               pr-seed pr-iters (if pr-bad "fail" "ok") pr-lines pr-last-echoes pr-sum pr-bad))
