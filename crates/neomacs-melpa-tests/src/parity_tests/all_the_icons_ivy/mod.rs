use std::time::Duration;

use crate::{ALL_THE_ICONS_IVY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_IVY_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// all-the-icons-ivy registers two display transformers with ivy and provides
/// the transformers themselves, which turn a candidate string into the same
/// string prefixed by an icon.
///
/// Three notes on what these workflows do and do not need.  The package has no
/// graphical gate -- no `display-graphic-p`, `window-system` or `noninteractive`
/// guard anywhere in it -- so unlike the ibuffer package there is no open and
/// closed path to assert.  Nothing here needs a live ivy session either: the
/// transformers are pure functions of a string, and the registration is checked
/// by reading ivy's own transformer registry, which is what ivy consults, so no
/// workflow depends on catalogue entry 1.  And the glyph and its font family
/// remain all-the-icons' surface: candidates are described structurally, and
/// where a workflow must distinguish two icons it compares them rather than
/// naming either.
const ALL_THE_ICONS_IVY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ivy)

(setq make-backup-files nil create-lockfiles nil)

(defvar ativ-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ativ-test-write (name text)
  (let ((path (expand-file-name name ativ-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

;; Describe a transformed candidate structurally.  The glyph and its font
;; family belong to all-the-icons and are covered by its own suite, so this
;; reports only what this package contributes: that the leading character is a
;; tab carrying a `display' property, which separator follows, and what the
;; candidate text and its face are.  Properties are read one at a time with
;; `get-text-property' rather than by comparing propertized strings, so a
;; plist-order difference shows up as a named property rather than as an
;; opaque string mismatch.
;;
;; Catalogue entry 22 cannot be reached from here, which was measured rather
;; than assumed.  That entry is about the FORMAT string -- `(format s)' where
;; `s' itself carries the properties.  This package propertizes only its
;; arguments; its template is `(concat "%s" all-the-icons-spacer "%s")', which
;; is unpropertized in both editors.  The one route to a propertized template
;; is setting `all-the-icons-spacer' to a propertized string, and that round
;; trips identically in both: `concat' reverses the plist and `format'
;; reverses it back.  The per-property reading is kept anyway, so the suite
;; would notice if this ever did start to bite.
(defun ativ-test-describe (result)
  (if (not (stringp result))
      (list 'not-a-string result)
    (let* ((plain (copy-sequence (substring-no-properties result)))
           (icon (get-text-property 0 'display result)))
      (list :text plain
            :length (length plain)
            :first-char (aref plain 0)
            ;; Property NAMES in order.  The order is what a plist-reversal
            ;; bug would disturb, so it is the thing to pin; the values are
            ;; all-the-icons' surface and the icon string also shares
            ;; structure unstably, so neither appears.
            :prop-names-at-0 (cl-loop for (key _value) on (text-properties-at 0 result)
                                      by #'cddr collect key)
            :icon-one-char-string (and (stringp icon) (= (length icon) 1))
            :icon-prop-names (and (stringp icon)
                                  (cl-loop for (key _value) on (text-properties-at 0 icon)
                                           by #'cddr collect key))
            :face-on-name (copy-tree (get-text-property (1- (length plain)) 'face result))))))

(defun ativ-test-transformer-for (command)
  "What ivy would actually call for COMMAND, read from ivy's own registry."
  (let ((entry (assq command ivy--display-transformers-alist)))
    (and entry (cdr entry))))
"##;

fn all_the_icons_ivy_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_IVY_MELPA_PIN, "all-the-icons-ivy.el")
        .expect("prepare pinned all-the-icons-ivy source below ./tmp")
        .with_prelude(ALL_THE_ICONS_IVY_TEST_PRELUDE)
        .with_timeout(ALL_THE_ICONS_IVY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-ivy parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_ivy_parity` cases (2a).
pub(crate) fn assert_all_the_icons_ivy_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_ivy_oracle(),
        &name,
        "all_the_icons_ivy_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_ivy_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_ivy_batch(&cases);
}

// END generated package batch tests
