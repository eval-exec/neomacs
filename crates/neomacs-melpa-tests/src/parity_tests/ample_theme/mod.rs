use std::time::Duration;

use crate::{AMPLE_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMPLE_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// ample is three themes from one palette - `ample' (dark), `ample-light' and
/// `ample-flat' - each in its own file with its own `deftheme', and each
/// reachable through an autoloaded command of the same name as the file, which
/// is how the README tells a user to switch.  The workflows go in through those
/// commands rather than through `load-theme'.  Only `ample-theme.el' is loaded
/// by the harness; the other two arrive through their autoloads, which is also
/// how a user gets them.
///
/// Every one of the 550 face specs uses the `((t ...))' shape, with no
/// `min-colors' and no `supports' clause anywhere in the package, so a batch
/// frame matches all of them and resolved appearance is the assertable surface.
///
/// One workflow exists because of a mechanism recorded in HARNESS-NOTES.md: a
/// theme's face spec *replaces* the face's standard definition rather than
/// merging with it, so every attribute the theme does not mention is dropped
/// rather than inherited.  ample is the first suite here large enough to show
/// that at scale - it sets a foreground on many stock faces that carry weight,
/// slant or an inherit of their own, and loses all of them.  A suite that reads
/// back only the attributes the theme sets cannot see any of it.
const AMPLE_THEME_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun ample-test-copy-tree (value)
  "Copy VALUE deeply, strings included, so nothing prints as a `#N=' reference."
  (cond ((consp value) (cons (ample-test-copy-tree (car value))
                             (ample-test-copy-tree (cdr value))))
        ((stringp value) (copy-sequence value))
        ((vectorp value) (apply #'vector (mapcar #'ample-test-copy-tree
                                                 (append value nil))))
        (t value)))

(defun ample-test-face-report (specs)
  "Resolve SPECS the way the display would; each entry is (FACE ATTRIBUTE...)."
  (mapcar
   (lambda (spec)
     (cons (car spec)
           (mapcar (lambda (attribute)
                     (cons attribute
                           (ample-test-copy-tree
                            (face-attribute (car spec) attribute nil t))))
                   (cdr spec))))
   specs))

(defun ample-test-disable-all ()
  "Turn off every enabled theme, whichever variant a workflow left on."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (disable-theme theme)))

(defun ample-test-token-faces (tokens)
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
             (and primary (ample-test-copy-tree
                           (face-attribute primary :foreground nil t)))
             (and primary (face-attribute primary :weight nil t))
             (and primary (face-attribute primary :slant nil t)))))
   tokens))

(defconst ample-test-variants '(ample-theme ample-light-theme ample-flat-theme)
  "The three commands the package autoloads, one per variant.")

(defconst ample-test-probe-faces
  '((default :foreground :background)
    (cursor :background)
    (region :background)
    (highlight :background :foreground)
    (mode-line :background :foreground)
    (mode-line-inactive :background :foreground)
    (fringe :background)
    (font-lock-keyword-face :foreground)
    (font-lock-string-face :foreground)
    (font-lock-comment-face :foreground)
    (font-lock-function-name-face :foreground)
    (font-lock-variable-name-face :foreground)
    (font-lock-type-face :foreground)
    (font-lock-warning-face :foreground)
    (link :foreground)
    (isearch :background :foreground)
    (minibuffer-prompt :foreground))
  "Faces a user looks at.  Every variant sets all of them but `highlight',
which no variant mentions and which stays unspecified throughout.")

(defconst ample-test-replaced-faces
  '(font-lock-warning-face
    font-lock-comment-face
    font-lock-string-face
    font-lock-keyword-face
    font-lock-doc-face
    font-lock-comment-delimiter-face
    link
    button
    show-paren-match
    header-line
    completions-annotations
    error)
  "Stock faces that carry more than a colour, all of which ample recolours.")

(defconst ample-test-replaced-attributes
  '(:inherit :weight :slant :underline)
  "The attributes those faces carry as standard and ample never mentions.")
"##;

fn ample_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMPLE_THEME_MELPA_PIN, "ample-theme.el")
        .expect("prepare pinned ample-theme source below ./tmp")
        .with_prelude(AMPLE_THEME_TEST_PRELUDE)
        .with_timeout(AMPLE_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ample-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_ample_theme_parity` cases (2a).
pub(crate) fn assert_ample_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ample_theme_oracle(), &name, "ample_theme_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ample_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ample_theme_batch(&cases);
}

// END generated package batch tests
