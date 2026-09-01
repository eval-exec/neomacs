use std::time::Duration;

use crate::{BERT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const BERT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const BERT_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)

(defun neomacs-bert-test-hex (bytes)
  "Render unibyte BYTES as a stable lowercase hexadecimal string."
  (mapconcat (lambda (byte) (format "%02x" byte))
             (string-to-list (string-as-unibyte bytes))
             ""))
"##;

fn bert_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BERT_MELPA_PIN, "bert.el")
        .expect("prepare pinned BERT source below ./tmp")
        .with_prelude(BERT_TEST_PRELUDE)
        .with_timeout(BERT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed BERT parity test").into()
}

pub(crate) fn assert_bert_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(bert_oracle(), &name, "bert_parity", cases);
}

#[test]
fn bert_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_bert_batch(&cases);
}
