use std::time::Duration;

use crate::{AMSREFTEX_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMSREFTEX_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// amsreftex teaches RefTeX to read `amsrefs` `.ltb` bibliography databases
/// instead of BibTeX ones, so every workflow works on real files in the
/// sandbox and enters through RefTeX itself: `reftex-mode' and
/// `reftex-parse-all' scan a real `.tex' master, `C-c [' cites, `C-c &' views
/// the crossref, and `M-x amsreftex-sort-bibliography' sorts.
///
/// The only stand-in is `completing-read': the citation regexp is read from
/// the minibuffer, which unattended execution cannot supply -- and supplying
/// it from a keyboard macro is catalogued divergence 1
/// (`end-of-file "Error reading from stdin"' in Neomacs).  Everything after
/// the prompt is real: the `*RefTeX Select*' display, its keymap, its
/// `:data' entries and the insertion into the document.
const AMSREFTEX_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defvar amsref-test-selections nil
  "Every `*RefTeX Select*' display captured during the current workflow.")

(defun amsref-test-plain (value)
  "Return VALUE with every string freshly copied and stripped of properties."
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (amsref-test-plain (car value)) (amsref-test-plain (cdr value))))
        (t value)))

(defun amsref-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun amsref-test-write (name text)
  "Write TEXT to NAME below the sandbox and return its absolute path."
  (let ((path (amsref-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun amsref-test-open (path)
  "Visit PATH as the document being worked on and scan it with RefTeX."
  (find-file path)
  (reftex-mode 1)
  ;; `execute-kbd-macro' only reaches the buffer of the selected window.
  (set-window-buffer (selected-window) (current-buffer))
  (reftex-parse-all)
  (current-buffer))

(defun amsref-test-face-runs ()
  "Return the current buffer's `face' runs as (FACE . TEXT) pairs."
  (let (runs (pos (point-min)))
    (while (< pos (point-max))
      (let ((face (get-text-property pos 'face))
            (next (or (next-single-property-change pos 'face) (point-max))))
        (when face
          (setq runs
                (cons (cons face (buffer-substring-no-properties pos next))
                      runs)))
        (setq pos next)))
    (nreverse runs)))

(defun amsref-test-selection-entries ()
  "Return the entry alist RefTeX attached to each line of the selection."
  (let (entries (pos (point-min)))
    (while (< pos (point-max))
      (let ((data (get-text-property pos :data))
            (next (or (next-single-property-change pos :data) (point-max))))
        (when data (setq entries (cons (amsref-test-plain data) entries)))
        (setq pos next)))
    (nreverse entries)))

(defun amsref-test-record-selection ()
  "Record the selection display RefTeX has just filled in."
  (when (equal (buffer-name) "*RefTeX Select*")
    (setq amsref-test-selections
          (append amsref-test-selections
                  (list (list :text (buffer-substring-no-properties
                                     (point-min) (point-max))
                              :faces (amsref-test-face-runs)
                              :entries (amsref-test-selection-entries)))))))

(defun amsref-test-cite (regexp &rest keys)
  "Cite through the `C-c [' binding, answering the prompt with REGEXP.
KEYS are pressed in the selection display before RET.  Return the
error the command signalled, or nil."
  (setq amsref-test-selections nil)
  (add-hook 'reftex-display-copied-context-hook #'amsref-test-record-selection)
  (unwind-protect
      (condition-case failure
          (cl-letf (((symbol-function 'completing-read)
                     (lambda (&rest _ignored) regexp)))
            (execute-kbd-macro
             (vconcat (kbd "C-c [") (apply #'vector keys) [?\r]))
            nil)
        (error (amsref-test-plain failure)))
    (remove-hook 'reftex-display-copied-context-hook
                 #'amsref-test-record-selection)))

(defun amsref-test-selection (index key)
  "Return KEY of the INDEXth selection display captured by `amsref-test-cite'."
  (plist-get (nth index amsref-test-selections) key))

(defun amsref-test-overlays ()
  "Return every overlay in the current buffer as (START END FACE)."
  (sort (mapcar (lambda (overlay)
                  (list (overlay-start overlay)
                        (overlay-end overlay)
                        (overlay-get overlay 'face)))
                (overlays-in (point-min) (point-max)))
        (lambda (left right) (< (car left) (car right)))))

(defvar amsref-test-advised-functions
  '(reftex-locate-bibliography-files
    reftex-parse-bibtex-entry
    reftex-get-crossref-alist
    reftex-extract-bib-entries
    reftex-extract-bib-entries-from-thebibliography
    reftex-pop-to-bibtex-entry
    reftex-echo-cite
    reftex-parse-from-file
    reftex-bibtex-selection-callback
    reftex-end-of-bib-entry)
  "The RefTeX functions amsreftex's commentary says it takes over.")

(defun amsref-test-advice ()
  "Return the advice currently installed on each RefTeX function."
  (delq nil
        (mapcar (lambda (function)
                  (let (installed)
                    (advice-mapc (lambda (advice _props)
                                   (setq installed (cons advice installed)))
                                 function)
                    (when installed (cons function (nreverse installed)))))
                amsref-test-advised-functions)))

(defun amsref-test-docstruct ()
  "Return the scanned document structure of the current document."
  (amsref-test-plain (symbol-value reftex-docstruct-symbol)))
"##;

fn amsreftex_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMSREFTEX_MELPA_PIN, "amsreftex.el")
        .expect("prepare pinned amsreftex source below ./tmp")
        .with_prelude(AMSREFTEX_TEST_PRELUDE)
        .with_timeout(AMSREFTEX_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed amsreftex parity test")
        .into()
}

/// Multi-probe batch for `assert_amsreftex_parity` cases (2a).
pub(crate) fn assert_amsreftex_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(amsreftex_oracle(), &name, "amsreftex_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn amsreftex_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amsreftex_batch(&cases);
}

// END generated package batch tests
