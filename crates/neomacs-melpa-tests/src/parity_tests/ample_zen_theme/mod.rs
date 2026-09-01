use std::time::Duration;

use crate::{AMPLE_ZEN_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMPLE_ZEN_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// ample-zen is a single dark theme built by crossing ample with zenburn, and
/// it inherits zenburn's construction: a named palette alist,
/// `ample-zen-colors-alist', and a macro, `ample-zen-with-color-variables',
/// that `let'-binds every colour in it along with `class' - the display clause
/// `((class color) (min-colors 89))'.
///
/// The clause shape is what makes this theme different from the other two in
/// this batch, and it is mixed rather than uniform.  412 of its 421 face
/// settings are written `((t ...))' and apply on any display, so resolved
/// appearance is the assertable surface for almost all of it.  The other nine
/// are written against `class' - and every one of them carries a `(t ...)'
/// fallback, so on a display below 89 colours the theme does not fail to apply,
/// it applies something deliberately different.  A batch frame is such a
/// display, which makes those fallbacks testable here rather than unreachable;
/// see HARNESS-NOTES.md for the `min-colors' families where no fallback exists
/// and resolved appearance is genuinely unavailable.
///
/// One workflow applies the mechanism recorded in HARNESS-NOTES: a theme's face
/// spec replaces the standard definition rather than merging with it, so every
/// attribute the theme does not mention is dropped.  ample-zen loses one on 29
/// of the 43 faces it sets that exist at startup.
const AMPLE_ZEN_THEME_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun zen-test-copy-tree (value)
  "Copy VALUE deeply, strings included, so nothing prints as a `#N=' reference."
  (cond ((consp value) (cons (zen-test-copy-tree (car value))
                             (zen-test-copy-tree (cdr value))))
        ((stringp value) (copy-sequence value))
        ((vectorp value) (apply #'vector (mapcar #'zen-test-copy-tree
                                                 (append value nil))))
        (t value)))

(defun zen-test-face-report (specs)
  "Resolve SPECS the way the display would; each entry is (FACE ATTRIBUTE...)."
  (mapcar
   (lambda (spec)
     (cons (car spec)
           (mapcar (lambda (attribute)
                     (cons attribute
                           (zen-test-copy-tree
                            (face-attribute (car spec) attribute nil t))))
                   (cdr spec))))
   specs))

(defun zen-test-theme-spec (face)
  "The face specification ample-zen registered for FACE."
  (zen-test-copy-tree
   (nth 3 (cl-find-if (lambda (setting)
                        (and (eq (car setting) 'theme-face)
                             (eq (cadr setting) face)))
                      (get 'ample-zen 'theme-settings)))))

(defun zen-test-token-faces (tokens)
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
             (and primary (zen-test-copy-tree
                           (face-attribute primary :foreground nil t)))
             (and primary (face-attribute primary :weight nil t))
             (and primary (face-attribute primary :slant nil t)))))
   tokens))

(defconst zen-test-probe-faces
  '((default :foreground :background)
    (cursor :foreground :background)
    (fringe :foreground :background)
    (highlight :foreground :background)
    (minibuffer-prompt :foreground)
    (link :foreground :underline :weight)
    (link-visited :foreground :underline :weight)
    (button :underline)
    (isearch :foreground :background)
    (lazy-highlight :foreground :background)
    (font-lock-keyword-face :foreground :weight)
    (font-lock-string-face :foreground)
    (font-lock-comment-face :foreground)
    (font-lock-function-name-face :foreground)
    (font-lock-variable-name-face :foreground)
    (font-lock-type-face :foreground)
    (font-lock-warning-face :foreground :weight)
    (mode-line-buffer-id :foreground :weight)
    (mode-line-inactive :foreground :background :weight)
    (secondary-selection :background)
    (trailing-whitespace :background)
    (vertical-border :foreground))
  "Faces ample-zen writes for any display, and what it sets on each.")

(defconst zen-test-class-faces
  '(mode-line region diff-added diff-removed diff-header diff-file-header
    hl-line hl-line-face hl-sexp-face)
  "The nine faces ample-zen writes against `class', each with a fallback.")

(defconst zen-test-replaced-faces
  '(font-lock-warning-face
    font-lock-doc-face
    font-lock-comment-delimiter-face
    font-lock-preprocessor-face
    font-lock-comment-face
    font-lock-string-face
    link
    link-visited
    button
    header-line
    mode-line-inactive
    show-paren-match)
  "Stock faces carrying more than a colour, all of which ample-zen recolours.")

(defconst zen-test-replaced-attributes '(:inherit :weight :slant :underline)
  "Attributes those faces carry as standard and ample-zen never mentions.")
"##;

fn ample_zen_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMPLE_ZEN_THEME_MELPA_PIN, "ample-zen-theme.el")
        .expect("prepare pinned ample-zen-theme source below ./tmp")
        .with_prelude(AMPLE_ZEN_THEME_TEST_PRELUDE)
        .with_timeout(AMPLE_ZEN_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ample-zen-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_ample_zen_theme_parity` cases (2a).
pub(crate) fn assert_ample_zen_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ample_zen_theme_oracle(),
        &name,
        "ample_zen_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ample_zen_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ample_zen_theme_batch(&cases);
}

// END generated package batch tests
