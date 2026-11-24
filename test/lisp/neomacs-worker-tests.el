;;; neomacs-worker-tests.el --- Tests for worker library -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;;; Commentary:

;; Run with:
;;   neomacs -Q --batch -L lisp -l test/lisp/neomacs-worker-tests.el \
;;     -f ert-run-tests-batch-and-exit

;;; Code:

(require 'ert)
(require 'neomacs-worker)

(defun neomacs-worker-tests--both (thunk)
  "Return THUNK's result with serial mode and with normal selection."
  (let ((serial (let ((neomacs-worker--force-serial t)) (funcall thunk)))
        (parallel (let ((neomacs-worker--force-serial nil)) (funcall thunk))))
    (should (equal serial parallel))
    serial))

(defun neomacs-worker-tests--outcome (thunk)
  "Return THUNK's value or its signaled condition and data."
  (condition-case condition
      (list :value (funcall thunk))
    (error (list :error (car condition) (cdr condition)))))

(ert-deftest neomacs-worker-availability-and-call-match-serial-results ()
  ;; The availability predicate intentionally reflects the forced mode.
  (should-not (let ((neomacs-worker--force-serial t))
                (neomacs-worker-available-p)))
  (should (equal (neomacs-worker-available-p)
                 (and (fboundp 'neomacs-worker-run)
                      (> (neomacs-worker-pool-size) 0))))
  (should (= (neomacs-worker-tests--both
              (lambda ()
                (neomacs-worker-wait*
                 (neomacs-worker-call #'+ 10 20 12))))
             42)))

(ert-deftest neomacs-worker-pmap-matches-mapcar-in-both-modes ()
  (should (equal '(1 4 9 16 25)
                 (neomacs-worker-tests--both
                  (lambda ()
                    (neomacs-worker-pmap
                     (lambda (number) (* number number)) '(1 2 3 4 5))))))
  (should (equal '(2 4 6 8 10 12)
                 (neomacs-worker-tests--both
                  (lambda ()
                    (neomacs-worker-pmap #'1+ '(1 3 5 7 9 11)
                                         :chunk 2)))))
  (should (equal '(t)
                 (neomacs-worker-tests--both
                  (lambda ()
                    (neomacs-worker-pmap
                     (lambda (buffer) (buffer-live-p buffer))
                     (list (current-buffer)) :fallback t)))))
  (should (equal (neomacs-worker-tests--outcome
                  (lambda ()
                    (let ((neomacs-worker--force-serial t))
                      (neomacs-worker-pmap
                       (lambda (number)
                         (if (= number 2) (error "pmap failure") number))
                       '(1 2 3)))))
                 (neomacs-worker-tests--outcome
                  (lambda ()
                    (let ((neomacs-worker--force-serial nil))
                      (neomacs-worker-pmap
                       (lambda (number)
                         (if (= number 2) (error "pmap failure") number))
                       '(1 2 3))))))))

(provide 'neomacs-worker-tests)

;;; neomacs-worker-tests.el ends here
