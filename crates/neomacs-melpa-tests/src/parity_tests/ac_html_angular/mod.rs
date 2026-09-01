use std::time::Duration;

use crate::{AC_HTML_ANGULAR_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HTML_ANGULAR_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Helpers shared by the workflows.
///
/// ac-html-angular is a completion *data provider*: `ac-html-angular+' pushes
/// its `html-stuff' directory onto the buffer-local
/// `web-completion-data-sources' and a consumer -- company-web, or a later
/// ac-html -- reads that directory and offers what it finds.  Neither consumer
/// is available at the pinned versions: company-web is not in
/// `melpa-package-lock.tsv', and the pinned ac-html (20151005.731) never looks at
/// `web-completion-data-sources' at all, because its bundled
/// `ac-html-default-data-provider.el' defines its own
/// `web-completion-data-html-source-dir' pointing at ac-html's own
/// `completion-data' directory.
///
/// So the workflows enter through the package's own commands and then read the
/// registration exactly as web-completion-data's commentary specifies a
/// consumer must: resolve each entry's location (a symbol or a directory),
/// then read `html-tag-list', `html-attributes-list/TAG', the `global'
/// attribute list, `html-attributes-short-docs/TAG-ATTR' with a `global-ATTR'
/// fallback, and `html-tag-short-docs/TAG'.  A line in a list file is
/// `NAME[ DOC]', so the candidate is its first field -- the Angular files carry
/// bare names, the default html files carry a trailing doc.  That resolution is
/// the whole of the fixture; the candidates, documentation and ordering the
/// workflows assert are the package's shipped data.
const AC_HTML_ANGULAR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'web-completion-data)

(defun ac-html-angular-test-sources ()
  "Every registered source, resolved the way a consumer resolves it."
  (mapcar (lambda (entry)
            (let ((location (cdr entry)))
              (cons (car entry)
                    (if (symbolp location) (symbol-value location) location))))
          web-completion-data-sources))

(defun ac-html-angular-test-directory (source)
  (cdr (assoc source (ac-html-angular-test-sources))))

(defun ac-html-angular-test-read (path)
  (when (file-exists-p path)
    (with-temp-buffer
      (let ((coding-system-for-read 'utf-8))
        (insert-file-contents path))
      (buffer-string))))

(defun ac-html-angular-test-candidates (source relative)
  "The candidate names RELATIVE inside SOURCE offers, in file order."
  (let* ((directory (ac-html-angular-test-directory source))
         (contents (and directory
                        (ac-html-angular-test-read
                         (expand-file-name relative directory)))))
    (when contents
      (mapcar (lambda (line) (car (split-string line)))
              (split-string contents "\n" t)))))

(defun ac-html-angular-test-attributes (source tag)
  "What SOURCE offers inside <TAG ...>: its own attributes, then the global ones."
  (append (ac-html-angular-test-candidates
           source (format "html-attributes-list/%s" tag))
          (ac-html-angular-test-candidates source "html-attributes-list/global")))

(defun ac-html-angular-test-attribute-doc (source tag attribute)
  "The documentation SOURCE shows for ATTRIBUTE of TAG."
  (let ((directory (ac-html-angular-test-directory source)))
    (and directory
         (or (ac-html-angular-test-read
              (expand-file-name
               (format "html-attributes-short-docs/%s-%s" tag attribute) directory))
             (ac-html-angular-test-read
              (expand-file-name
               (format "html-attributes-short-docs/global-%s" attribute) directory))))))

(defun ac-html-angular-test-tag-doc (source tag)
  (let ((directory (ac-html-angular-test-directory source)))
    (and directory
         (ac-html-angular-test-read
          (expand-file-name (format "html-tag-short-docs/%s" tag) directory)))))

(defun ac-html-angular-test-undocumented (source)
  "Every (TAG ATTRIBUTE) SOURCE offers without shipping documentation for it."
  (let (missing)
    (dolist (tag (ac-html-angular-test-candidates source "html-tag-list")
                 (nreverse missing))
      (dolist (attribute (ac-html-angular-test-attributes source tag))
        (unless (ac-html-angular-test-attribute-doc source tag attribute)
          (push (list tag attribute) missing))))))

(defun ac-html-angular-test-shipped-directories (source)
  (let ((directory (ac-html-angular-test-directory source)))
    (and directory
         (sort (cl-remove-if-not
                (lambda (name) (file-directory-p (expand-file-name name directory)))
                (directory-files directory nil "\\`[^.]"))
               #'string<))))

(defconst ac-html-angular-test-template
  "<div ng-app=\"shop\" ng-controller=\"CartController\">\n  <form name=\"checkout\">\n    <input type=\"text\" ng-\n    <select ng-\n  </form>\n  <ng-include src=\"'cart.html'\"></ng-include>\n</div>\n")

(defmacro ac-html-angular-test-with-template (&rest body)
  "Run BODY in a window-displayed html-mode buffer holding an Angular template."
  `(let ((buffer (generate-new-buffer "*angular-template*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (html-mode)
           (insert ac-html-angular-test-template)
           ,@body)
       (kill-buffer buffer))))
"##;

fn ac_html_angular_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HTML_ANGULAR_MELPA_PIN, "ac-html-angular.el")
        .expect("prepare pinned ac-html-angular source below ./tmp")
        .with_prelude(AC_HTML_ANGULAR_TEST_PRELUDE)
        .with_timeout(AC_HTML_ANGULAR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-html-angular parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_html_angular_parity` cases (2a).
pub(crate) fn assert_ac_html_angular_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_html_angular_oracle(),
        &name,
        "ac_html_angular_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ac_html_angular_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_html_angular_batch(&cases);
}

// END generated package batch tests
