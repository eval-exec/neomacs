use std::time::Duration;

use crate::{AC_HTML_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HTML_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-html is an auto-complete source backed by the data files the package
/// ships under `completion-data/', so every workflow sets the buffer up the way
/// the package documents -- enable a data provider, call `ac-html-setup', put
/// its sources on `ac-sources' -- and then completes through `ac-start' /
/// `ac-update' / `ac-complete' in a window-displayed buffer.  Nothing is
/// stubbed; the candidate lists and the documentation both come off disk.
///
/// Read this before adding a workflow: **with the documented source order,
/// attribute completion never fires.**  `ac-html-tag-prefix' matches from just
/// after the `<' whatever follows it, so in `<div cl' the tag source claims the
/// prefix "div cl", matches no tag, and auto-complete stops there -- zero
/// candidates, and it looks like the data provider is broken.  To drive
/// attributes, narrow `ac-sources' for that case:
///
/// ```elisp
/// (let ((ac-sources '(ac-source-html-attr)))
///   (aht-test-offer "<div cl"))
/// ;; => (:typed "<div cl" :prefix "cl" :count 1 :candidates ("class"))
/// ```
///
/// This is upstream behaviour, pinned by
/// `the_tag_sources_prefix_shadows_attribute_completion'.  The sources this
/// version defines are `ac-source-html-tag', `ac-source-html-attr' and
/// `ac-source-html-attrv'.
const AC_HTML_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'auto-complete)
(require 'ac-html-default-data-provider)

(defmacro aht-test-in-buffer (&rest body)
  "Run BODY in a window-displayed `html-mode' buffer set up the way the
package's README documents: enable a data provider, call the language's
setup function, then put its sources on `ac-sources'."
  `(let ((buffer (generate-new-buffer "*ac-html-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (html-mode)
           (ac-html-enable-data-provider 'ac-html-default-data-provider)
           (ac-html-setup)
           (setq ac-sources
                 '(ac-source-html-tag ac-source-html-attr ac-source-html-attrv))
           (auto-complete-mode 1)
           (setq aht-test-documented nil)
           ,@body)
       (kill-buffer buffer))))

(defun aht-test-candidates ()
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defun aht-test-offer (text)
  "Retype the buffer as TEXT, record what auto-complete offers, then abort."
  (erase-buffer)
  (insert text)
  (let* ((candidates (aht-test-candidates))
         (prefix ac-prefix)
         (symbols (delete-dups
                   (mapcar (lambda (item) (get-text-property 0 'symbol item))
                           ac-candidates))))
    (ac-abort)
    (list :typed text :prefix prefix :count (length candidates)
          :symbols symbols :candidates candidates)))

(defvar aht-test-documented nil)

(defun aht-test-documentation (item)
  (let ((doc (popup-item-documentation item)))
    (and doc (substring-no-properties doc))))

(defun aht-test-offer-with-docs (text)
  "Like `aht-test-offer', but also read each candidate's documentation."
  (erase-buffer)
  (insert text)
  (let* ((candidates (aht-test-candidates))
         (prefix ac-prefix)
         (docs (mapcar (lambda (item)
                         (list (substring-no-properties item)
                               (get-text-property 0 'symbol item)
                               (aht-test-documentation item)))
                       ac-candidates)))
    (ac-abort)
    (list :typed text :prefix prefix :candidates candidates :documentation docs)))
"####;

fn ac_html_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HTML_MELPA_PIN, "ac-html.el")
        .expect("prepare pinned ac-html source below ./tmp")
        .with_prelude(AC_HTML_TEST_PRELUDE)
        .with_timeout(AC_HTML_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-html parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_html_parity` cases (2a).
pub(crate) fn assert_ac_html_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_html_oracle(), &name, "ac_html_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_html_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_html_batch(&cases);
}

// END generated package batch tests
