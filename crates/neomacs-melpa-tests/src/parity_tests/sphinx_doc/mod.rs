//! Practical parity for sphinx-doc Python docstring insertion.
//!
//! These cases enable the minor mode in a Python buffer, insert a
//! Sphinx skeleton for a café function, merge an existing docstring,
//! and signal when point is not on a definition.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, S_MELPA_PIN, SPHINX_DOC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'python)
(require 'sphinx-doc)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst sd494-test-tree
  "36d2b71e5219248a6f1dd99a666a43d4754fa943")
(defconst sd494-test-manifest
  '(("sphinx-doc-pkg.el" . "4c591748a531e8ec6948b230e6ff40fc0efff454c4ed38eac81dfbeb9a2e69ea")
    ("sphinx-doc.el" . "f1479e5e901cbf182780baee4a03bc9d71e9e2cdddc89ed5ada304311cb96847")))

(defun sd494-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun sd494-test-source-state ()
  (let* ((located (locate-library "sphinx-doc.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (sd494-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/sphinx-doc.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car sd494-test-manifest)))
      (error "Unexpected installed sphinx-doc payload: %S"
             (or manifest files)))
    (dolist (entry sd494-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (sd494-test-sha file) expected))
          (error "Unexpected installed sphinx-doc source: %S"
                 (cons entry manifest)))))
    (list :tree sd494-test-tree
          :manifest manifest
          :feature (featurep 'sphinx-doc)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'sphinx-doc package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SPHINX_DOC_MELPA_PIN, "sphinx-doc.el")
        .expect("prepare pinned sphinx-doc source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn insert_docstring_for_cafe_python_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_docstring_for_cafe_python_function",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      (buf (generate-new-buffer "café-sphinx.py"))
      (inhibit-message t)
      (sphinx-doc-include-types nil))
  (unwind-protect
      (with-current-buffer buf
        (python-mode)
        (sphinx-doc-mode 1)
        (insert "def greet(name, greeting='café'):\n    pass\n")
        (goto-char (point-min))
        (search-forward "def")
        (forward-char -3)
        (sphinx-doc)
        (list :source (sd494-test-source-state)
              :mode major-mode
              :minor (and sphinx-doc-mode t)
              :bind (lookup-key sphinx-doc-mode-map (kbd "C-c M-d"))
              :buffer (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buf)
      (kill-buffer buf))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "36d2b71e5219248a6f1dd99a666a43d4754fa943" :manifest (("sphinx-doc-pkg.el" . "4c591748a531e8ec6948b230e6ff40fc0efff454c4ed38eac81dfbeb9a2e69ea") ("sphinx-doc.el" . "f1479e5e901cbf182780baee4a03bc9d71e9e2cdddc89ed5ada304311cb96847")) :feature t :version "20210213.1250") :mode python-mode :minor t :bind sphinx-doc :buffer "def greet(name, greeting='café'):\n    \"\"\"TODO describe function\n\n    :param name: \n    :param greeting: \n    :returns: \n\n    \"\"\"\n    pass\n")"#
        ]],
    )
}

fn include_types_from_python_annotations() -> ParityBatchCase {
    ParityBatchCase::value(
        "include_types_from_python_annotations",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      (buf (generate-new-buffer "types-sphinx.py"))
      (inhibit-message t)
      (sphinx-doc-include-types t))
  (unwind-protect
      (with-current-buffer buf
        (python-mode)
        (sphinx-doc-mode 1)
        (insert "def add(x: int, y: int) -> int:\n    return x + y\n")
        (goto-char (point-min))
        (search-forward "def")
        (forward-char -3)
        (sphinx-doc)
        (list :source (sd494-test-source-state)
              :include sphinx-doc-include-types
              :buffer (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buf)
      (kill-buffer buf))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "36d2b71e5219248a6f1dd99a666a43d4754fa943" :manifest (("sphinx-doc-pkg.el" . "4c591748a531e8ec6948b230e6ff40fc0efff454c4ed38eac81dfbeb9a2e69ea") ("sphinx-doc.el" . "f1479e5e901cbf182780baee4a03bc9d71e9e2cdddc89ed5ada304311cb96847")) :feature t :version "20210213.1250") :include t :buffer "def add(x: int, y: int) -> int:\n    \"\"\"TODO describe function\n\n    :param x: \n    :type x: int\n    :param y: \n    :type y: int\n    :returns: \n\n    \"\"\"\n    return x + y\n")"#
        ]],
    )
}

fn merge_preserves_existing_param_descriptions() -> ParityBatchCase {
    ParityBatchCase::value(
        "merge_preserves_existing_param_descriptions",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      (buf (generate-new-buffer "merge-sphinx.py"))
      (inhibit-message t)
      (sphinx-doc-include-types nil))
  (unwind-protect
      (with-current-buffer buf
        (python-mode)
        (sphinx-doc-mode 1)
        (insert "def greet(name, city):\n    \"\"\"Say hello.\n\n    :param name: the person\n    :returns: nothing\n\n    \"\"\"\n    pass\n")
        (goto-char (point-min))
        (search-forward "def")
        (forward-char -3)
        (sphinx-doc)
        (list :source (sd494-test-source-state)
              :buffer (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buf)
      (kill-buffer buf))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "36d2b71e5219248a6f1dd99a666a43d4754fa943" :manifest (("sphinx-doc-pkg.el" . "4c591748a531e8ec6948b230e6ff40fc0efff454c4ed38eac81dfbeb9a2e69ea") ("sphinx-doc.el" . "f1479e5e901cbf182780baee4a03bc9d71e9e2cdddc89ed5ada304311cb96847")) :feature t :version "20210213.1250") :buffer "def greet(name, city):\n    \"\"\"Say hello.\n\n    :param name: the person\n    :param city: \n    :returns: nothing\n\n    \"\"\"\n    pass\n")"#
        ]],
    )
}

fn missing_def_signals_search_failed() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_def_signals_search_failed",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      (buf (generate-new-buffer "empty-sphinx.py"))
      (inhibit-message t))
  (unwind-protect
      (with-current-buffer buf
        (python-mode)
        (sphinx-doc-mode 1)
        (insert "# café notes only\nx = 1\n")
        (goto-char (point-min))
        (list :source (sd494-test-source-state)
              :failed
              (condition-case err
                  (sphinx-doc)
                (error (list (car err)
                             (error-message-string err))))))
    (when (buffer-live-p buf)
      (kill-buffer buf))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "36d2b71e5219248a6f1dd99a666a43d4754fa943" :manifest (("sphinx-doc-pkg.el" . "4c591748a531e8ec6948b230e6ff40fc0efff454c4ed38eac81dfbeb9a2e69ea") ("sphinx-doc.el" . "f1479e5e901cbf182780baee4a03bc9d71e9e2cdddc89ed5ada304311cb96847")) :feature t :version "20210213.1250") :failed (search-failed "Search failed: \"def\""))"#
        ]],
    )
}

#[test]
fn sphinx_doc_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        insert_docstring_for_cafe_python_function(),
        include_types_from_python_annotations(),
        merge_preserves_existing_param_descriptions(),
        missing_def_signals_search_failed(),
    ];
    assert_oracle_batch_cases(oracle(), "sphinx-doc-rank494", "sphinx_doc_parity", &cases);
}
