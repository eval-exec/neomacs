use crate::{CachedMelpaOracle, YTDL_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YTDL_TEST_PRELUDE: &str = r##"
(require 'ytdl)

(defvar neomacs-melpa-ytdl--selected-format nil)

(defun neomacs-melpa-ytdl--write-executable (path body)
  (with-temp-file path
    (insert "#!/bin/sh\nset -eu\n")
    (insert body))
  (set-file-modes path #o700)
  path)

(defun neomacs-melpa-ytdl--file-lines (path)
  (if (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents path)
        (split-string (buffer-string) "\n" t))
    nil))

(defun neomacs-melpa-ytdl--entry-state (entry)
  (list
   (car entry)
   (append (cadr entry) nil)))

(defun neomacs-melpa-ytdl--goto-id (id)
  (goto-char (point-min))
  (while (and (not (equal (tabulated-list-get-id) id))
              (not (eobp)))
    (forward-line 1))
  (equal (tabulated-list-get-id) id))

(defun neomacs-melpa-ytdl--download-state ()
  (let (state)
    (maphash
     (lambda (key item)
       (push
        (list
         key
         (ytdl--list-entry-title item)
         (ytdl--list-entry-status item)
         (ytdl--list-entry-type item)
         (ytdl--list-entry-path item)
         (ytdl--list-entry-size item)
         (ytdl--list-entry-error item)
         (ytdl--list-entry-url item))
        state))
     ytdl--download-list)
    (sort state (lambda (left right) (string< (car left) (car right))))))

(defun neomacs-melpa-ytdl--run-async-jobs (jobs)
  (dolist (job (nreverse jobs))
    (funcall (cdr job) (funcall (car job)))))

(defun neomacs-melpa-ytdl--capture-error (function)
  (condition-case caught
      (list 'ok (funcall function))
    (error (list 'error caught))))
"##;

fn ytdl_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YTDL_MELPA_PIN, "ytdl.el")
        .expect("prepare pinned ytdl source below ./tmp")
        .with_prelude(YTDL_TEST_PRELUDE)
}

#[test]
fn ytdl_package_batch() {
    assert_oracle_batch_cases(
        ytdl_oracle(),
        "ytdl_package_batch",
        "ytdl_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
