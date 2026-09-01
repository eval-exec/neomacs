use std::time::Duration;

use crate::{ALL_THE_ICONS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Helpers shared by the workflows.
///
/// all-the-icons maps a file name, a mode, a directory or a URL to one glyph
/// from a bundled icon font and propertizes it with the font family, a height,
/// a colour face and a `raise' display adjustment.  There is no subprocess and
/// no state: the whole product is that propertized character, so the workflows
/// pin it exactly -- the code point, and each text property read individually.
///
/// `all-the-icons-test-describe' reads the properties one at a time with
/// `get-text-property' rather than comparing whole propertized strings,
/// because a plist-order difference would then be reported as an icon
/// difference (see DIVERGENCES.md entry 22, `format' reversing a propertized
/// string's plist).  The property *names* are still listed, in storage order,
/// so a genuine ordering change is visible as itself.
///
/// `all-the-icons-install-fonts' downloads six font files over HTTPS and runs
/// `fc-cache'.  That is a real network boundary, so the workflow that covers it
/// replaces `url-copy-file' and `shell-command-to-string' with recorders --
/// nothing is ever fetched -- while the package's own URL construction,
/// destination selection, directory creation and completion message run for
/// real.
const ALL_THE_ICONS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun all-the-icons-test-describe (icon)
  "Everything a caller can see about ICON, property by property."
  (if (not (stringp icon))
      (list :not-a-string icon)
    (list :codepoint (aref icon 0)
          :length (length icon)
          :face (get-text-property 0 'face icon)
          :font-lock-face (get-text-property 0 'font-lock-face icon)
          :display (get-text-property 0 'display icon)
          :rear-nonsticky (get-text-property 0 'rear-nonsticky icon)
          :property-names (let (names (plist (text-properties-at 0 icon)))
                            (while plist
                              (push (car plist) names)
                              (setq plist (cddr plist)))
                            (nreverse names)))))

(defun all-the-icons-test-face (icon)
  (and (stringp icon) (get-text-property 0 'face icon)))

(defun all-the-icons-test-sandbox (name)
  (let ((path (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p path)
      (if (file-directory-p path) (delete-directory path t) (delete-file path)))
    (make-directory path t)
    path))
"##;

fn all_the_icons_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_MELPA_PIN, "all-the-icons.el")
        .expect("prepare pinned all-the-icons source below ./tmp")
        .with_prelude(ALL_THE_ICONS_TEST_PRELUDE)
        .with_timeout(ALL_THE_ICONS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_parity` cases (2a).
pub(crate) fn assert_all_the_icons_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(all_the_icons_oracle(), &name, "all_the_icons_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_batch(&cases);
}

// END generated package batch tests
