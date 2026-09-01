use std::time::Duration;

use crate::{AUCTEX_CLUTTEX_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUCTEX_CLUTTEX_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const AUCTEX_CLUTTEX_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-auctex-cluttex-test--command-names ()
  "Return the current AUCTeX command names in order."
  (mapcar #'car TeX-command-list))

(defun neomacs-auctex-cluttex-test--expansion-keys ()
  "Return the current built-in expansion keys in order."
  (mapcar #'car TeX-expand-list-builtin))

(defun neomacs-auctex-cluttex-test--write-program (root exit-code)
  "Create a deterministic local cluttex executable below ROOT."
  (let ((program (expand-file-name "bin/cluttex" root)))
    (make-directory (file-name-directory program) t)
    (with-temp-file program
      (insert
       "#!/bin/sh\n"
       "printf '%s\\n' \"$PWD\" > \"$NEOMACS_CLUTTEX_CWD\"\n"
       "printf '%s\\n' \"$@\" > \"$NEOMACS_CLUTTEX_ARGUMENTS\"\n"
       (if (zerop exit-code)
           (concat
            "printf '\\033[32mClutTeX compiled release Ω\\033[0m\\n'\n"
            "printf 'artifact ready\\n'\n"
            "touch main.pdf\n"
            "touch main.synctex.gz\n")
         "printf '\\033[31mClutTeX rejected broken citation Ω\\033[0m\\n'\n")
       "exit " (number-to-string exit-code) "\n"))
    (set-file-modes program #o755)
    program))

(defun neomacs-auctex-cluttex-test--output-line-state (buffer token)
  "Return TOKEN's output line and its exact font-lock face in BUFFER."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward token)
      (let ((position (match-beginning 0)))
        (list
         :line (buffer-substring-no-properties
                (line-beginning-position) (line-end-position))
         :face (copy-tree (get-text-property position 'font-lock-face)))))))

(defun neomacs-auctex-cluttex-test--output-line-properties (buffer token)
  "Return TOKEN's line and exact ANSI face properties in BUFFER."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward token)
      (let ((position (match-beginning 0)))
        (list
         :line (buffer-substring-no-properties
                (line-beginning-position) (line-end-position))
         :face (copy-tree (get-text-property position 'face))
         :font-lock-face
         (copy-tree (get-text-property position 'font-lock-face))
         :overlay-faces
         (mapcar (lambda (overlay)
                   (copy-tree (overlay-get overlay 'face)))
                 (overlays-at position)))))))

(defun neomacs-auctex-cluttex-test--messages (regexp &optional start)
  "Return exact message lines after START that match REGEXP."
  (with-current-buffer (messages-buffer)
    (save-excursion
      (goto-char (min (or start (point-min)) (point-max)))
      (let (matches)
        (while (re-search-forward regexp nil t)
          (push
           (buffer-substring-no-properties
            (line-beginning-position) (line-end-position))
           matches))
        (nreverse matches)))))

(defun neomacs-auctex-cluttex-test--cleanup (root)
  "Kill test buffers and processes, then remove ROOT."
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (or (and file root (string-prefix-p root file))
                (string-prefix-p "*ClutTeX" (buffer-name buffer)))
        (when-let ((process (get-buffer-process buffer)))
          (ignore-errors (delete-process process)))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (when (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn auctex_cluttex_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUCTEX_CLUTTEX_MELPA_PIN, "auctex-cluttex.el")
        .expect("prepare pinned auctex-cluttex source below ./tmp")
        .with_prelude(AUCTEX_CLUTTEX_TEST_PRELUDE)
        .with_timeout(AUCTEX_CLUTTEX_TEST_TIMEOUT)
}

#[test]
fn auctex_cluttex_package_batch() {
    assert_oracle_batch_cases(
        auctex_cluttex_oracle(),
        "auctex_cluttex_package_batch",
        "auctex_cluttex_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
