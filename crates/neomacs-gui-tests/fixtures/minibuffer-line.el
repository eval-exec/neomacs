;;; minibuffer-line.el --- Display status info in the minibuffer window  -*- lexical-binding: t; -*-

;; Copyright (C) 2015  Free Software Foundation, Inc.

;; Author: Stefan Monnier <monnier@iro.umontreal.ca>
;; Keywords:
;; Version: 0.1

;; This program is free software; you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; This program is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with this program.  If not, see <http://www.gnu.org/licenses/>.

;;; Commentary:

;; This package lets you display various status information in the minibuffer
;; window instead of the mode-line.  Of course, this is only displayed when the
;; minibuffer window is not already used for other things (e.g. a minibuffer or
;; an each area message).
;;
;; The contents and aspect is controlled by the `minibuffer-line-format'
;; variable and the `minibuffer-line' face.  Their current default kind of
;; sucks: suggestions for improvements welcome.

;;; Code:

(defgroup minibuffer-line ()
  "Use the idle minibuffer window to display status information."
  :group 'mode-line)

(defcustom minibuffer-line-format
  '("" (:eval system-name) " | " (:eval (format-time-string "%F %R")))
  "Specification of the contents of the minibuffer-line.
Uses the same format as `mode-line-format'."
  :type 'sexp)

(defface minibuffer-line
  '((t :inherit mode-line-inactive))
  "Face to use for the minibuffer-line.")

(defcustom minibuffer-line-refresh-interval 60
  "The frequency at which the minibuffer-line is updated, in seconds."
  :type 'integer)

(defconst minibuffer-line--buffer " *Minibuf-0*")

(defvar minibuffer-line--timer nil)

;;;###autoload
(define-minor-mode minibuffer-line-mode
  "Display status info in the minibuffer window."
  :global t
  (with-current-buffer minibuffer-line--buffer
    (erase-buffer))
  (when minibuffer-line--timer
    (cancel-timer minibuffer-line--timer)
    (setq minibuffer-line--timer nil))
  (when minibuffer-line-mode
    (setq minibuffer-line--timer
          (run-with-timer t minibuffer-line-refresh-interval
                          #'minibuffer-line--update))
    (minibuffer-line--update)))

(defun minibuffer-line--update ()
  (with-current-buffer minibuffer-line--buffer
    (erase-buffer)
    (insert (format-mode-line minibuffer-line-format 'minibuffer-line))))

(provide 'minibuffer-line)
;;; minibuffer-line.el ends here
