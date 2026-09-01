use std::time::Duration;

use crate::{CachedMelpaOracle, JS_DOC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const JS_DOC_TEST_TIMEOUT: Duration = Duration::from_secs(180);
/// iswitchb was removed from modern Emacs; js-doc still requires it at load
/// time for interactive tag completion. Stub the feature so practical insert
/// and metadata paths can load.
const JS_DOC_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(unless (featurep 'iswitchb)
  (provide 'iswitchb)
  (defun iswitchb-read-buffer (&rest _)
    ""))
(require 'js-doc)
(setq js-doc-author "Test Author"
      js-doc-mail-address "test@example.com"
      js-doc-url "https://example.com"
      js-doc-license "MIT")
"####;

fn js_doc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JS_DOC_MELPA_PIN, "js-doc.el")
        .expect("prepare exact shallow js-doc source below ./tmp")
        .with_prelude(JS_DOC_TEST_PRELUDE)
        .with_timeout(JS_DOC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed js-doc parity test")
        .into()
}

fn assert_js_doc_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        js_doc_oracle(),
        &current_test_name(),
        "js_doc_parity",
        cases,
    );
}

#[test]
fn js_doc_package_batch() {
    assert_js_doc_batch(&workflows::workflow_batch_cases());
}
