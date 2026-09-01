use std::time::Duration;

use crate::{CachedMelpaOracle, DEVDOCS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DEVDOCS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DEVDOCS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'devdocs)

(defun neomacs-devdocs-test-doc (&optional slug name version type mtime)
  "Return deterministic document metadata."
  `((name . ,(or name "Widget JS"))
    (slug . ,(or slug "widgetjs"))
    (type . ,(or type "javascript"))
    (version . ,(or version "2.0"))
    (mtime . ,(or mtime 200))))

(defun neomacs-devdocs-test-reset (name)
  "Reset the DevDocs data directory for NAME."
  (setq devdocs-data-dir
        (file-name-as-directory
         (expand-file-name
          (concat "devdocs-" name)
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
  (when (file-exists-p devdocs-data-dir)
    (delete-directory devdocs-data-dir t))
  (make-directory devdocs-data-dir t)
  (clrhash devdocs--cache)
  (when (get-buffer "*devdocs*") (kill-buffer "*devdocs*")))

(defun neomacs-devdocs-test-install-fixture (name)
  "Create a two-page installed document below the sandbox."
  (neomacs-devdocs-test-reset name)
  (let* ((doc (neomacs-devdocs-test-doc))
         (dir (expand-file-name "widgetjs" devdocs-data-dir))
         (entries
          [((name . "Widget") (path . "guide#widget") (type . "Classes"))
           ((name . "Widget.build") (path . "guide#build") (type . "Methods"))
           ((name . "Options") (path . "api#options") (type . "Types"))])
         (pages ["guide" "api"]))
    (make-directory dir t)
    (with-temp-file (expand-file-name "metadata" dir)
      (prin1 (cons devdocs--data-format-version doc) (current-buffer)))
    (with-temp-file (expand-file-name "index" dir)
      (prin1 `((entries . ,entries) (pages . ,pages)
               (types . ["Classes" "Methods" "Types"]))
             (current-buffer)))
    (with-temp-file (expand-file-name "guide.html" dir)
      (insert "<main><h1 id='widget'>Widget</h1><p>Create a <strong>widget</strong>.</p><h2 id='build'>Build</h2><pre data-language='javascript'>const x = Widget.build();</pre><p><a href='api#options'>Options</a></p></main>"))
    (with-temp-file (expand-file-name "api.html" dir)
      (insert "<main><h1 id='options'>Options</h1><p>Set <code>fast</code> to true.</p><p><a href='guide#widget'>Widget</a></p></main>"))
    doc))

(defun neomacs-devdocs-test-view ()
  "Describe stable visible DevDocs buffer state."
  (with-current-buffer "*devdocs*"
    (list :mode major-mode
          :text (string-trim (buffer-substring-no-properties
                              (point-min) (point-max)))
          :point (list (point) (line-number-at-pos) (current-column))
          :stack (mapcar (lambda (entry)
                           (list (alist-get 'name entry)
                                 (alist-get 'path entry)
                                 (alist-get 'fragment entry)))
                         devdocs--stack)
          :forward (mapcar (lambda (entry) (alist-get 'path entry))
                           devdocs--forward-stack)
          :docs devdocs-current-docs
          :header (format-mode-line header-line-format nil nil (current-buffer))
          :directory list-buffers-directory
          :read-only buffer-read-only
          :modified (buffer-modified-p)
          :bindings (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                            '("n" "p" "]" "[" "l" "r" "w" ".")))))

(defun neomacs-devdocs-test-files ()
  "List installed data files relative to `devdocs-data-dir'."
  (sort
   (mapcar (lambda (file) (file-relative-name file devdocs-data-dir))
           (directory-files-recursively devdocs-data-dir "."))
   #'string<))
"####;

fn devdocs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DEVDOCS_MELPA_PIN, "devdocs.el")
        .expect("prepare exact shallow DevDocs source and Compat dependency below ./tmp")
        .with_prelude(DEVDOCS_TEST_PRELUDE)
        .with_timeout(DEVDOCS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed DevDocs parity test")
        .into()
}

fn assert_devdocs_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        devdocs_oracle(),
        &current_test_name(),
        "devdocs_parity",
        cases,
    );
}

#[test]
fn devdocs_package_batch() {
    assert_devdocs_batch(&workflows::workflow_batch_cases());
}
