use std::time::Duration;

use crate::{AMBER_GLOW_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMBER_GLOW_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Helpers shared by the workflows.
///
/// amber-glow is a small theme: one `custom-theme-set-faces' call, eighteen
/// faces, no variables and no palette variables - every colour is written out
/// as a literal in the spec.  Every spec uses the `((t ...))' shape, so a batch
/// frame matches all of them and `face-attribute ... nil t' returns the real
/// colour strings; resolved appearance is the assertable surface and nothing
/// needs faking.  See HARNESS-NOTES.md for the `min-colors' families where it
/// is not, and `amaranth_dark_theme' for a theme in the same shape with
/// display-conditional specs as well - amber-glow has none, so there is no
/// workflow here for choosing between clauses.
///
/// Eighteen faces is few enough that what the theme *does not* set matters as
/// much as what it does, which is why one workflow is about the faces it leaves
/// alone.
///
/// `amber-test-copy-tree' copies strings as well as conses so repeated colours
/// do not print as `#1=' definitions and `#1#' back references.
const AMBER_GLOW_THEME_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun amber-test-copy-tree (value)
  "Copy VALUE deeply, strings included, so nothing prints as a `#N=' reference."
  (cond ((consp value) (cons (amber-test-copy-tree (car value))
                             (amber-test-copy-tree (cdr value))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun amber-test-face-report (specs)
  "Resolve SPECS the way the display would; each entry is (FACE ATTRIBUTE...)."
  (mapcar
   (lambda (spec)
     (cons (car spec)
           (mapcar (lambda (attribute)
                     (cons attribute
                           (amber-test-copy-tree
                            (face-attribute (car spec) attribute nil t))))
                   (cdr spec))))
   specs))

(defun amber-test-token-faces (tokens)
  "For each token, the face font lock gave it and how that face resolves."
  (mapcar
   (lambda (token)
     (goto-char (point-min))
     (search-forward token)
     (let* ((position (- (point) (length token)))
            (face (get-text-property position 'face))
            (primary (if (listp face) (car face) face)))
       (list (copy-sequence token)
             face
             (and primary (amber-test-copy-tree
                           (face-attribute primary :foreground nil t)))
             (and primary (face-attribute primary :weight nil t)))))
   tokens))

(defun amber-test-face-count ()
  "How many faces amber-glow has registered a setting for."
  (cl-count-if (lambda (setting) (eq (car setting) 'theme-face))
               (get 'amber-glow 'theme-settings)))

(defconst amber-test-themed-faces
  '((default :foreground :background)
    (cursor :background)
    (fringe :background :foreground)
    (region :background :foreground)
    (highlight :background :foreground)
    (vertical-border :background :foreground)
    (font-lock-builtin-face :foreground)
    (font-lock-comment-face :foreground)
    (font-lock-constant-face :foreground)
    (font-lock-function-name-face :foreground)
    (font-lock-keyword-face :foreground)
    (font-lock-string-face :foreground)
    (font-lock-type-face :foreground)
    (font-lock-variable-name-face :foreground)
    (font-lock-warning-face :foreground :weight :inherit)
    (mode-line :background :foreground)
    (mode-line-inactive :background :foreground)
    (minibuffer-prompt :foreground))
  "Every face amber-glow sets, and the attributes it sets on each.")

(defconst amber-test-unreached-faces
  '((isearch :background :foreground)
    (lazy-highlight :background :foreground)
    (link :foreground :underline)
    (show-paren-match :background :foreground)
    (secondary-selection :background)
    (trailing-whitespace :background)
    (tab-bar :background :foreground)
    (tooltip :background :foreground)
    (match :background)
    (shadow :foreground))
  "Faces amber-glow never mentions and never reaches by inheritance either.")
"##;

fn amber_glow_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMBER_GLOW_THEME_MELPA_PIN, "amber-glow-theme.el")
        .expect("prepare pinned amber-glow-theme source below ./tmp")
        .with_prelude(AMBER_GLOW_THEME_TEST_PRELUDE)
        .with_timeout(AMBER_GLOW_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed amber-glow-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_amber_glow_theme_parity` cases (2a).
pub(crate) fn assert_amber_glow_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        amber_glow_theme_oracle(),
        &name,
        "amber_glow_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn amber_glow_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amber_glow_theme_batch(&cases);
}

// END generated package batch tests
