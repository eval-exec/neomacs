use crate::{CachedMelpaOracle, YTDIOUS_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YTDIOUS_TEST_PRELUDE: &str = r##"
(require 'ytdious)

(defun neomacs-melpa-ytdious--write-executable (path body)
  (with-temp-file path
    (insert "#!/bin/sh\nset -eu\n")
    (insert body))
  (set-file-modes path #o700)
  path)

(defun neomacs-melpa-ytdious--file-lines (path)
  (with-temp-buffer
    (insert-file-contents path)
    (split-string (buffer-string) "\n" t)))

(defun neomacs-melpa-ytdious--entry-state (entry)
  (let ((id (car entry))
        (columns (append (cadr entry) nil)))
    (list
     id
     (mapcar #'substring-no-properties columns)
     (mapcar
      (lambda (column)
        (and (> (length column) 0)
             (get-text-property 0 'face column)))
      columns))))

(defun neomacs-melpa-ytdious--mode-line-state ()
  (mapcar
   (lambda (group)
     (mapcar
      (lambda (item)
        (if (stringp item)
            (list (substring-no-properties item)
                  (and (> (length item) 0)
                       (get-text-property 0 'face item)))
          item))
      group))
   mode-line-misc-info))
"##;

fn ytdious_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YTDIOUS_MELPA_PIN, "ytdious.el")
        .expect("prepare pinned ytdious source below ./tmp")
        .with_prelude(YTDIOUS_TEST_PRELUDE)
}

#[test]
fn ytdious_package_batch() {
    assert_oracle_batch_cases(
        ytdious_oracle(),
        "ytdious_package_batch",
        "ytdious_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
