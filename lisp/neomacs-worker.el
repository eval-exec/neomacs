;;; neomacs-worker.el --- Portable calls to Neomacs worker isolates -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; Package authors can use the same call path with or without worker isolates:
;;
;;   (let ((task (neomacs-worker-call #'+ 1 2)))
;;     (neomacs-worker-wait* task)) ; => 3
;;
;;   (neomacs-worker-pmap #'1+ '(1 2 3)) ; => (2 3 4)
;;
;; `neomacs-worker-call' returns a worker task when the capability is
;; available, or a serial task otherwise.  Pair it with
;; `neomacs-worker-wait*' to get the value.  `neomacs-worker-pmap' preserves
;; input order and signals function errors in either mode.  With `:fallback t',
;; an input or result that cannot cross the isolate boundary is computed on
;; the main thread.

;;; Code:

(require 'cl-lib)

(defvar neomacs-worker--force-serial nil
  "When non-nil, make the worker library use its serial path.
This variable is intended for tests and callers comparing both paths.")

(cl-defstruct (neomacs-worker--serial-task
               (:constructor neomacs-worker--make-serial-task (function args)))
  function args)

;;;###autoload
(defun neomacs-worker-available-p ()
  "Return non-nil when worker isolates can accept tasks in this process."
  (and (not neomacs-worker--force-serial)
       (fboundp 'neomacs-worker-run)
       (fboundp 'neomacs-worker-pool-size)
       (> (neomacs-worker-pool-size) 0)))

;;;###autoload
(defun neomacs-worker-call (function &rest args)
  "Call FUNCTION with ARGS in an isolate when available.
Return a task for `neomacs-worker-wait*' to resolve."
  (if (neomacs-worker-available-p)
      (apply #'neomacs-worker-run function args)
    (neomacs-worker--make-serial-task function args)))

;;;###autoload
(defun neomacs-worker-wait* (task)
  "Resolve TASK returned by `neomacs-worker-call'.
This wrapper name avoids shadowing the primitive `neomacs-worker-wait'."
  (if (neomacs-worker--serial-task-p task)
      (apply (neomacs-worker--serial-task-function task)
             (neomacs-worker--serial-task-args task))
    (neomacs-worker-wait task)))

(defun neomacs-worker--chunks (list size)
  "Split LIST into ordered chunks of at most SIZE elements."
  (let (chunks current)
    (while list
      (push (pop list) current)
      (when (= (length current) size)
        (push (nreverse current) chunks)
        (setq current nil)))
    (when current
      (push (nreverse current) chunks))
    (nreverse chunks)))

;;;###autoload
(cl-defun neomacs-worker-pmap (function list &key (chunk 1) fallback)
  "Map FUNCTION over LIST, preserving order, using worker isolates when available.
CHUNK sets the number of elements processed by each task.  With FALLBACK
non-nil, evaluate a chunk serially if its input or result cannot cross the
worker boundary.  Function errors are signaled to the caller."
  (unless (and (integerp chunk) (> chunk 0))
    (signal 'wrong-type-argument (list '(integer 1 *) chunk)))
  (let ((chunks (neomacs-worker--chunks list chunk))
        results)
    (dolist (items chunks)
      (let* ((single (= chunk 1))
             (task (condition-case condition
                       (if single
                           (neomacs-worker-call function (car items))
                         (neomacs-worker-call #'mapcar function items))
                     (neomacs-worker-transfer-error
                      (if fallback
                          (neomacs-worker--make-serial-task
                           (if single function #'mapcar)
                           (if single items (list function items)))
                        (signal (car condition) (cdr condition))))))
             (value (condition-case condition
                        (neomacs-worker-wait* task)
                      (neomacs-worker-transfer-error
                       (if fallback
                           (if single
                               (funcall function (car items))
                             (mapcar function items))
                         (signal (car condition) (cdr condition)))))))
        (setq results (append results (if single (list value) value)))))
    results))

(provide 'neomacs-worker)

;;; neomacs-worker.el ends here
