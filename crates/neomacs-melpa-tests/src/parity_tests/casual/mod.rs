use std::time::Duration;

use crate::{CASUAL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const CASUAL_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CASUAL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'casual-editkit)
(require 'casual-elisp)
(require 'casual-csv)
(require 'casual-dired)
(require 'casual-ibuffer)

(defun neomacs-casual-test-command (prefix key)
  "Return the command exposed by PREFIX at KEY."
  (plist-get (cdr (transient-get-suffix prefix key)) :command))

(defun neomacs-casual-test-write-file (path contents)
  "Write CONTENTS to PATH without messages or interactive prompts."
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-casual-test-ibuffer-targets ()
  "Return visible Casual fixture buffers with their marks, in display order."
  (save-excursion
    (goto-char (point-min))
    (let (rows buffer)
      (while (not (eobp))
        (setq buffer (ibuffer-current-buffer))
        (when (and (buffer-live-p buffer)
                   (string-prefix-p "*casual-" (buffer-name buffer)))
          (push (list (buffer-name buffer)
                      (if (eq (ibuffer-current-mark) ibuffer-marked-char)
                          'marked
                        'empty))
                rows))
        (forward-line 1))
      (nreverse rows))))
"##;

fn casual_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CASUAL_MELPA_PIN, "casual.el")
        .expect("prepare pinned Casual source below ./tmp")
        .with_prelude(CASUAL_TEST_PRELUDE)
        .with_timeout(CASUAL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Casual parity test").into()
}

#[test]
fn casual_package_batch() {
    let cases = workflows::workflows_public_surface_batch_cases();
    assert_oracle_batch_cases(
        casual_oracle(),
        &current_test_name(),
        "casual_parity",
        &cases,
    );
}
