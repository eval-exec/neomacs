use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_SEARCH_HIGHLIGHT_PERSIST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-search-highlight-persist)

(defun neomacs-eshp-test-overlays ()
  "Describe persistent Highlight overlays in source order."
  (mapcar
   (lambda (overlay)
     (list :range (list (overlay-start overlay) (overlay-end overlay))
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))
           :face (overlay-get overlay hlt-face-prop)
           :highlight (overlay-get overlay 'hlt-highlight)
           :priority (overlay-get overlay 'priority)))
   (sort (cl-remove-if-not
          (lambda (overlay) (overlay-get overlay 'hlt-highlight))
          (overlays-in (point-min) (point-max)))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun neomacs-eshp-test-buffer-state ()
  "Describe the current buffer's persistent-search state."
  (list :mode evil-search-highlight-persist
        :enabled evil-search-highlight-persist-enabled
        :overlays (neomacs-eshp-test-overlays)
        :binding (lookup-key evil-search-highlight-persist-map (kbd "C-x SPC"))
        :text (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-eshp-test-mark (query regexp-p)
  "Persistently highlight QUERY, interpreting it by REGEXP-P."
  (let ((isearch-regexp regexp-p)
        (search-ring (unless regexp-p (list query)))
        (regexp-search-ring (when regexp-p (list query))))
    (evil-search-highlight-persist-mark)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        EVIL_SEARCH_HIGHLIGHT_PERSIST_MELPA_PIN,
        "evil-search-highlight-persist.el",
    )
    .expect("prepare exact Evil Search Highlight Persist source below ./tmp")
    .with_prelude(PRELUDE)
    .with_timeout(TEST_TIMEOUT)
}

#[test]
fn evil_search_highlight_persist_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "evil_search_highlight_persist_package_batch",
        "evil_search_highlight_persist_parity",
        &workflows::workflow_batch_cases(),
    );
}
