use std::time::Duration;

use crate::{AC_EMOJI_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

/// ac-emoji is an auto-complete source, and the commentary's whole story is
/// "call `ac-emoji-setup' then type `:name'".  The workflows therefore complete
/// through `ac-start` / `ac-update` / `ac-complete` in a window-displayed
/// buffer, which is also the only buffer `execute-kbd-macro` would reach.
const AC_EMOJI_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(defmacro ac-emoji-test-in-buffer (&rest body)
  "Run BODY in a window-displayed text buffer with auto-complete armed."
  `(let ((buffer (generate-new-buffer "*ac-emoji-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (text-mode)
           (setq ac-sources nil)
           (auto-complete-mode 1)
           ,@body)
       (kill-buffer buffer))))

(defun ac-emoji-test-candidates ()
  "Start completion at point and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defun ac-emoji-test-item (key)
  "Return KEY's popup item as (KEY DOCUMENT SUMMARY)."
  (let ((item (cl-find key ac-emoji--candidates :test #'equal)))
    (and item
         (list (substring-no-properties item)
               (get-text-property 0 'document item)
               (get-text-property 0 'summary item)))))
"##;

const AC_EMOJI_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn ac_emoji_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_EMOJI_MELPA_PIN, "ac-emoji.el")
        .expect("prepare pinned ac-emoji source below ./tmp")
        .with_prelude(AC_EMOJI_TEST_PRELUDE)
        .with_timeout(AC_EMOJI_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-emoji parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_emoji_parity` cases (2a).
pub(crate) fn assert_ac_emoji_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_emoji_oracle(), &name, "ac_emoji_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_emoji_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_emoji_batch(&cases);
}

// END generated package batch tests
