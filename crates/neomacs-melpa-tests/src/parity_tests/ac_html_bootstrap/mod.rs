use std::time::Duration;

use crate::{AC_HTML_BOOTSTRAP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HTML_BOOTSTRAP_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-html-bootstrap is a completion *data provider*: `ac-html-bootstrap+' and
/// `ac-html-fa+' push their shipped data directories onto the buffer-local
/// `web-completion-data-sources', and a consumer -- company-web, or a later
/// ac-html -- reads those directories and offers what it finds.  The package
/// contains no completion code at all; the data files are the product.
///
/// Neither consumer is available at the pinned versions: company-web is not in
/// `melpa-package-lock.tsv', and the pinned ac-html (20151005.731) never looks at
/// `web-completion-data-sources', because its own
/// `ac-html-default-data-provider.el' redefines
/// `web-completion-data-html-source-dir' as a `defconst' pointing at ac-html's
/// bundled `completion-data' directory.  This was checked against that source
/// rather than assumed, and matches what `parity_tests/ac_html_angular/'
/// records for the same registry.
///
/// So the workflows enter through the package's own commands and then read the
/// registration exactly as a consumer must: resolve each entry's location (a
/// symbol or a directory), then read `html-attributes-complete/TAG-ATTR' with
/// a `global-ATTR' fallback for attribute values -- the Bootstrap and Font
/// Awesome class names this package exists to supply -- plus
/// `html-attributes-list/TAG', `html-attributes-short-docs/TAG-ATTR' and
/// `html-tag-list'.  A line is `NAME[ DOC]', so the candidate is its first
/// field and its documentation is the rest.  Which tag and attribute is being
/// completed comes from parsing the buffer at point, which is also the
/// consumer's job.  That resolution is the whole of the fixture; every
/// candidate and every documentation string asserted below is the package's
/// shipped data.
const AC_HTML_BOOTSTRAP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'web-completion-data)

(defconst achb-test-document
  (concat "<!doctype html>\n"
          "<html lang=\"en\">\n"
          "  <head>\n"
          "    <meta charset=\"utf-8\">\n"
          "    <title>Dashboard</title>\n"
          "  </head>\n"
          "  <body data-spy=\"scroll\">\n"
          "    <div class=\"panel-\">\n"
          "      <span class=\"label-\">status</span>\n"
          "      <button class=\"btn btn-\" data-toggle=\"\">Save</button>\n"
          "      <i class=\"fa-sp\"></i>\n"
          "      <table class=\"table\">\n"
          "        <tr><td class=\"dan\">cell</td></tr>\n"
          "      </table>\n"
          "    </div>\n"
          "  </body>\n"
          "</html>\n")
  "A real Bootstrap page, with point placed inside its attribute values.")

(defun achb-test-sources ()
  "Every registered source, resolved the way web-completion-data specifies."
  (mapcar (lambda (entry)
            (let ((location (cdr entry)))
              (cons (car entry)
                    (if (symbolp location) (symbol-value location) location))))
          web-completion-data-sources))

(defun achb-test-directory (source)
  (cdr (assoc source (achb-test-sources))))

(defun achb-test-locations ()
  "Every registered source as (NAME PACKAGE/SUBDIR EXISTS).
The absolute path runs through the harness's content-addressed install cache,
so only the package-relative tail is meaningful."
  (mapcar (lambda (entry)
            (let* ((directory (directory-file-name (cdr entry)))
                   (package (file-name-nondirectory
                             (directory-file-name
                              (file-name-directory directory)))))
              (list (car entry)
                    (concat package "/" (file-name-nondirectory directory))
                    (file-directory-p directory))))
          (achb-test-sources)))

(defun achb-test-lines (source relative)
  (let* ((directory (achb-test-directory source))
         (path (and directory (expand-file-name relative directory))))
    (when (and path (file-exists-p path))
      (with-temp-buffer
        (let ((coding-system-for-read 'utf-8))
          (insert-file-contents path))
        (split-string (buffer-string) "\n" t)))))

(defun achb-test-entries (source relative)
  "Return (NAME . DOC) for every `NAME[ DOC]' line of RELATIVE in SOURCE."
  (mapcar (lambda (line)
            (let ((space (string-match " " line)))
              (cons (if space (substring line 0 space) line)
                    (and space (substring line (1+ space))))))
          (achb-test-lines source relative)))

(defun achb-test-values (source tag attribute)
  "What SOURCE offers inside <TAG ATTRIBUTE=\"...\">: its own then the global."
  (append (achb-test-entries
           source (format "html-attributes-complete/%s-%s" tag attribute))
          (achb-test-entries
           source (format "html-attributes-complete/global-%s" attribute))))

(defun achb-test-attributes (source tag)
  "What SOURCE offers inside <TAG ...>: its own attributes then the global."
  (append (achb-test-entries source (format "html-attributes-list/%s" tag))
          (achb-test-entries source "html-attributes-list/global")))

(defun achb-test-attribute-doc (source tag attribute)
  "The documentation SOURCE shows for ATTRIBUTE of TAG."
  (car (or (achb-test-lines
            source (format "html-attributes-short-docs/%s-%s" tag attribute))
           (achb-test-lines
            source (format "html-attributes-short-docs/global-%s" attribute)))))

(defun achb-test-shipped (source)
  "The data directories SOURCE ships, which is what bounds what it can offer."
  (let ((directory (achb-test-directory source)))
    (and directory
         (sort (cl-remove-if-not
                (lambda (name) (file-directory-p (expand-file-name name directory)))
                (directory-files directory nil "\\`[^.]"))
               #'string<))))

(defun achb-test-context ()
  "Return (TAG ATTRIBUTE TYPED) for the attribute value under construction.
Parsing the buffer is the consumer's job, not the provider's -- this package
ships data only -- so this is the smallest parse a consumer needs.  `class'
holds a space-separated list, so TYPED is the token point is inside, not the
whole attribute value."
  (save-excursion
    (when (re-search-backward
           "<\\([a-zA-Z0-9]+\\)[^<>]*[ \t]\\([a-zA-Z0-9-]+\\)=\"\\([^\"<>]*\\)\\="
           nil t)
      (let ((tag (match-string-no-properties 1))
            (attribute (match-string-no-properties 2))
            (value (match-string-no-properties 3)))
        (list tag attribute
              (if (string-match "[^ \t]*\\'" value)
                  (match-string 0 value)
                value))))))

(defun achb-test-offer (source)
  "What SOURCE offers at point: the parsed context and the matching values."
  (let* ((context (achb-test-context))
         (values (and context
                      (achb-test-values source (nth 0 context) (nth 1 context))))
         (matching (cl-remove-if-not
                    (lambda (entry) (string-prefix-p (nth 2 context) (car entry)))
                    values)))
    (list :context context
          :offered (length values)
          :matching (length matching)
          :candidates matching)))

(defun achb-test-goto (marker)
  "Move point just after the first occurrence of MARKER and return it."
  (goto-char (point-min))
  (search-forward marker)
  (point))

(defmacro achb-test-in-document (&rest body)
  "Run BODY in a window-displayed `html-mode' buffer holding the fixture page."
  `(let ((buffer (generate-new-buffer "*bootstrap-page*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (html-mode)
           (insert achb-test-document)
           ,@body)
       (kill-buffer buffer))))
"##;

fn ac_html_bootstrap_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HTML_BOOTSTRAP_MELPA_PIN, "ac-html-bootstrap.el")
        .expect("prepare pinned ac-html-bootstrap source below ./tmp")
        .with_prelude(AC_HTML_BOOTSTRAP_TEST_PRELUDE)
        .with_timeout(AC_HTML_BOOTSTRAP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-html-bootstrap parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_html_bootstrap_parity` cases (2a).
pub(crate) fn assert_ac_html_bootstrap_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_html_bootstrap_oracle(),
        &name,
        "ac_html_bootstrap_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ac_html_bootstrap_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_html_bootstrap_batch(&cases);
}

// END generated package batch tests
