;;; tramp-bootstrap-tests.el --- Tests for Tramp bootstrap loading  -*- lexical-binding:t -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; This file is part of GNU Emacs.
;;
;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.
;;
;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.
;;
;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; These tests run in a fresh batch process.  Tramp must not already be
;; loaded: the state under test is loaddefs generation from a clean source
;; checkout, before the generated tramp-loaddefs library exists.

;;; Code:

(require 'ert)

(ert-deftest tramp-bootstrap-test-load-without-loaddefs-on-darwin ()
  "Tramp source can be scraped before `tramp-loaddefs' exists on Darwin."
  (should-not (featurep 'tramp))
  (should-not (featurep 'tramp-loaddefs))
  ;; `tramp-compat.el' deliberately uses a noerror require as its guard
  ;; against this state.  Providing the absent generated library models that
  ;; noerror path without allowing another load-path entry to supply it.
  (provide 'tramp-loaddefs)
  (when (boundp 'tramp-local-host-names)
    (makunbound 'tramp-local-host-names))
  (let ((system-type 'darwin)
        ;; Exercise source loading, just as loaddefs-gen does when it needs an
        ;; unknown custom autoload macro.
        (load-suffixes '(".el")))
    (load (expand-file-name
           "tramp.el"
           (file-name-directory (locate-library "tramp.el")))
          nil nil t))
  (should (featurep 'tramp)))

(provide 'tramp-bootstrap-tests)

;;; tramp-bootstrap-tests.el ends here
