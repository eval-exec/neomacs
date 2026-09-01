use std::time::Duration;

use crate::{AUCTEX_LATEXMK_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUCTEX_LATEXMK_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const AUCTEX_LATEXMK_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(load (getenv "NEOMACS_PACKAGE_SOURCE") nil t t)
(auctex-latexmk-setup)

(defun neomacs-auctex-latexmk-test--write-program (root outcome)
  "Create a deterministic local latexmk executable for OUTCOME below ROOT."
  (let ((program (expand-file-name "bin/latexmk" root)))
    (make-directory (file-name-directory program) t)
    (with-temp-file program
      (insert
       "#!/bin/sh\n"
       "printf '%s\\n' \"$PWD\" > \"$NEOMACS_LATEXMK_CWD\"\n"
       "printf '%s\\n' \"$@\" > \"$NEOMACS_LATEXMK_ARGUMENTS\"\n"
       "printf '%s\\n' \"${LATEXENC-unset}\" > \"$NEOMACS_LATEXMK_ENCODING\"\n"
       (if (eq outcome 'recenter)
           ""
         (concat
          "touch \"$NEOMACS_LATEXMK_READY\"\n"
          "while [ ! -f \"$NEOMACS_LATEXMK_RELEASE\" ]; do sleep 0.01; done\n"))
       (pcase outcome
         ('success
          (concat
           "printf '%s\\n' \"latexmk: applying document rules\"\n"
           "printf '%s\\n' \"Run number 1 of rule 'bibtex main'\"\n"
           "printf '%s\\n' \"BibTeX preparation complete\"\n"
           "printf '%s\\n' \"Rule 'bibtex main': finished\"\n"
           "printf '%s\\n' \"Run number 1 of rule 'pdflatex'\"\n"
           "printf '%s\\n' \"Latexmk preamble one\"\n"
           "printf '%s\\n' \"Latexmk preamble two\"\n"
           "printf '%s\\n' \"Latexmk preamble three\"\n"
           "printf '%s\\n' \"Latexmk preamble four\"\n"
           "printf '%s\\n' \"This is pdfTeX, Version deterministic\"\n"
           "printf '%s\\n' \"LaTeX2e <2024-11-01>\"\n"
           "printf '%s\\n' \"Output written on main.pdf (1 page, 64 bytes).\"\n"
           "printf '%s\\n' \"Transcript written on main.log.\"\n"
           "printf '%s\\n' \"Latexmk: All targets are up-to-date\"\n"
           "printf '%s' 'PDF release Ω' > main.pdf\n"
           "printf '%s' 'aux release' > main.aux\n"
           "printf '%s' 'database release' > main.fdb_latexmk\n"
           "printf '%s' 'file list release' > main.fls\n"
           "printf '%s' 'log release' > main.log\n"
           "exit 0\n"))
         ('nothing
          (concat
           "printf '%s\\n' \"latexmk: inspecting existing targets\"\n"
           "printf '%s\\n' \"Latexmk: Nothing to do for 'main.tex'.\"\n"
           "exit 0\n"))
         ('latex-failure
          (concat
           "printf '%s\\n' \"latexmk: applying document rules\"\n"
           "printf '%s\\n' \"Run number 1 of rule 'pdflatex'\"\n"
           "printf '%s\\n' \"Latexmk preamble one\"\n"
           "printf '%s\\n' \"Latexmk preamble two\"\n"
           "printf '%s\\n' \"Latexmk preamble three\"\n"
           "printf '%s\\n' \"Latexmk preamble four\"\n"
           "printf '%s\\n' \"This is pdfTeX, Version deterministic\"\n"
           "printf '%s\\n' \"! Undefined control sequence.\"\n"
           "printf '%s\\n' \"l.4 \\\\brokencommand\"\n"
           "printf '%s\\n' \"Collected error summary (may duplicate other messages):\"\n"
           "printf '%s\\n' \"  pdflatex: Command for 'pdflatex' gave return code 1\"\n"
           "printf '%s\\n' \"Latexmk: Errors, so I did not complete making targets\"\n"
           "exit 12\n"))
         ('bibtex-failure
          (concat
           "printf '%s\\n' \"latexmk: applying bibliography rules\"\n"
           "printf '%s\\n' \"Run number 1 of rule 'bibtex main'\"\n"
           "printf '%s\\n' \"Rule 'bibtex main': reasons for rerun\"\n"
           "printf '%s\\n' \"BibTeX setup one\"\n"
           "printf '%s\\n' \"BibTeX setup two\"\n"
           "printf '%s\\n' \"BibTeX setup three\"\n"
           "printf '%s\\n' \"BibTeX setup four\"\n"
           "printf '%s\\n' \"Warning--I didn't find a database entry for 'missing'\"\n"
           "printf '%s\\n' \"(There was 1 error message)\"\n"
           "printf '%s\\n' \"Rule 'pdflatex': not run\"\n"
           "printf '%s\\n' \"Collected error summary (may duplicate other messages):\"\n"
           "printf '%s\\n' \"  bibtex main: Command for 'bibtex main' gave return code 2\"\n"
           "printf '%s\\n' \"Latexmk: Errors, so I did not complete making targets\"\n"
           "exit 13\n"))
         ('recenter
          (concat
           "printf '%s\\n' \"latexmk: live bibliography build\"\n"
           "printf '%s\\n' \"Run number 1 of rule 'bibtex main'\"\n"
           "printf '%s\\n' \"Rule 'bibtex main': live diagnostics\"\n"
           "printf '%s\\n' \"BibTeX diagnostic one\"\n"
           "printf '%s\\n' \"BibTeX diagnostic two\"\n"
           "printf '%s\\n' \"Rule 'pdflatex': pending\"\n"
           "touch \"$NEOMACS_LATEXMK_READY\"\n"
           "while [ ! -f \"$NEOMACS_LATEXMK_RELEASE\" ]; do sleep 0.01; done\n"
           "printf '%s\\n' \"Latexmk: Nothing to do after inspection\"\n"
           "exit 0\n"))
         (_ (error "Unsupported latexmk fixture outcome: %S" outcome)))))
    (set-file-modes program #o755)
    program))

(defun neomacs-auctex-latexmk-test--normalize-text (text root)
  "Normalize only nondeterministic parts of TEXT produced below ROOT."
  (let ((normalized text))
    (setq normalized
          (replace-regexp-in-string
           (regexp-quote root) "<sandbox>/" normalized t t))
    (replace-regexp-in-string
     " at [^\n]*\\(?:\n\\|\\'\\)"
     (lambda (_match) " at <time>\n")
     normalized t nil)))

(defun neomacs-auctex-latexmk-test--messages (start root)
  "Return every ordered message line after START, normalized below ROOT."
  (with-current-buffer (messages-buffer)
    (let ((transcript
           (neomacs-auctex-latexmk-test--normalize-text
            (buffer-substring-no-properties
             (min start (point-max)) (point-max))
            root)))
      (split-string (string-trim transcript) "\n" t))))

(defun neomacs-auctex-latexmk-test--output-transcript (buffer root)
  "Return BUFFER's exact ordered transcript, normalized below ROOT."
  (with-current-buffer buffer
    (string-trim-right
     (neomacs-auctex-latexmk-test--normalize-text
      (buffer-substring-no-properties (point-min) (point-max)) root))))

(defun neomacs-auctex-latexmk-test--read-lines (file)
  "Read FILE as an exact list of nonempty lines."
  (with-temp-buffer
    (insert-file-contents file)
    (split-string (buffer-string) "\n" t)))

(defun neomacs-auctex-latexmk-test--wait (process)
  "Wait boundedly for PROCESS and drain its remaining output."
  (let ((attempts 0))
    (while (and (< attempts 200) (process-live-p process))
      (accept-process-output process 0.05)
      (setq attempts (1+ attempts)))
    (accept-process-output process 0.1)
    (when (process-live-p process)
      (error "latexmk fixture did not finish"))))

(defun neomacs-auctex-latexmk-test--cleanup (root)
  "Kill fixture buffers and processes, clear AUCTeX globals, and remove ROOT."
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (or (and file root (string-prefix-p root file))
                (and root
                     (string-match-p
                      (regexp-quote root) (buffer-name buffer))))
        (when-let ((process (get-buffer-process buffer)))
          (ignore-errors (delete-process process)))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (setq TeX-command-buffer nil)
  (when (boundp 'compilation-in-progress)
    (setq compilation-in-progress
          (cl-remove-if-not #'process-live-p compilation-in-progress)))
  (when (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn auctex_latexmk_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUCTEX_LATEXMK_MELPA_PIN, "auctex-latexmk.el")
        .expect("prepare pinned auctex-latexmk source below ./tmp")
        .with_prelude(AUCTEX_LATEXMK_TEST_PRELUDE)
        .with_installed_autoloads()
        .with_timeout(AUCTEX_LATEXMK_TEST_TIMEOUT)
}

#[test]
fn auctex_latexmk_package_batch() {
    assert_oracle_batch_cases(
        auctex_latexmk_oracle(),
        "auctex_latexmk_package_batch",
        "auctex_latexmk_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
