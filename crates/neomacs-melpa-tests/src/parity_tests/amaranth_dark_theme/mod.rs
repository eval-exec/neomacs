use std::time::Duration;

use crate::{AMARANTH_DARK_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMARANTH_DARK_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Helpers shared by the workflows.
///
/// amaranth-dark is one high-contrast dark theme: a `let' of twenty-two colour
/// roles, one `custom-theme-set-variables' call and one
/// `custom-theme-set-faces' call covering a hundred and twenty-eight faces.  The
/// whole product is what the editor looks like afterwards, so these workflows
/// go in through `load-theme' and read the faces back the way the display
/// resolves them.
///
/// Every spec in this theme uses the `((t ...))' shape apart from the three
/// flymake and two flyspell faces, so a batch frame - zero colours, no graphic
/// display - matches them and `face-attribute ... nil t' returns the real
/// colour strings.  Resolved appearance is therefore the assertable surface
/// here and nothing needs faking; see HARNESS-NOTES.md for the `min-colors'
/// families where it is not.  The five display-conditional faces get their own
/// workflow, which pins the display facts that decide which branch is chosen.
///
/// `amaranth-test-copy-tree' copies strings as well as conses.  The theme binds
/// each colour once and shares that one string across every face using it, so
/// an uncopied report renders the palette as `#1=' definitions and `#1#' back
/// references - real sharing, but sharing that is an artefact of how the theme
/// happens to be written rather than anything a user can observe.
const AMARANTH_DARK_THEME_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun amaranth-test-copy-tree (value)
  "Copy VALUE deeply, strings included, so nothing prints as a `#N=' reference."
  (cond ((consp value) (cons (amaranth-test-copy-tree (car value))
                             (amaranth-test-copy-tree (cdr value))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun amaranth-test-face-report (specs)
  "Resolve SPECS the way the display would; each entry is (FACE ATTRIBUTE...)."
  (mapcar
   (lambda (spec)
     (cons (car spec)
           (mapcar (lambda (attribute)
                     (cons attribute
                           (amaranth-test-copy-tree
                            (face-attribute (car spec) attribute nil t))))
                   (cdr spec))))
   specs))

(defun amaranth-test-face-presence (faces)
  "Whether each of FACES exists, and what it is an alias for."
  (mapcar (lambda (face)
            (list face (and (facep face) t) (get face 'face-alias)))
          faces))

(defun amaranth-test-theme-spec (face)
  "The face specification amaranth-dark registered for FACE."
  (amaranth-test-copy-tree
   (nth 3 (cl-find-if (lambda (setting)
                        (and (eq (car setting) 'theme-face)
                             (eq (cadr setting) face)))
                      (get 'amaranth-dark 'theme-settings)))))

(defmacro amaranth-test-with-theme (&rest body)
  "Enable amaranth-dark the way a user does, run BODY, then disable it."
  `(unwind-protect
       (progn (load-theme 'amaranth-dark t) ,@body)
     (disable-theme 'amaranth-dark)))

(defun amaranth-test-token-faces (tokens)
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
             (and primary (amaranth-test-copy-tree
                           (face-attribute primary :foreground nil t)))
             (and primary (face-attribute primary :weight nil t)))))
   tokens))

(defconst amaranth-test-core-faces
  '((default :foreground :background)
    (cursor :background)
    (region :background :foreground)
    (highlight :background :foreground)
    (mode-line :background :foreground)
    (mode-line-inactive :background :foreground)
    (font-lock-keyword-face :foreground :weight)
    (font-lock-comment-face :foreground)
    (font-lock-string-face :foreground)
    (font-lock-function-name-face :foreground)
    (font-lock-variable-name-face :foreground)
    (font-lock-type-face :foreground)
    (font-lock-constant-face :foreground)
    (font-lock-doc-face :foreground)
    (font-lock-warning-face :foreground)
    (link :foreground :underline)
    (link-visited :foreground :underline)
    (line-number :foreground :inherit)
    (line-number-current-line :foreground :inherit)
    (isearch :foreground :background)
    (isearch-fail :foreground :background)
    (fringe :background :foreground)
    (shadow :foreground)
    (minibuffer-prompt :foreground)
    (trailing-whitespace :foreground :background)
    (tooltip :background :foreground)
    (secondary-selection :background :foreground)
    (match :background)
    (vertical-border :foreground)
    (border :background :foreground)
    (tab-bar :background :foreground)
    (tab-bar-tab :background :foreground :weight)
    (tab-bar-tab-inactive :background))
  "The faces a user sees without loading anything, and what the theme sets.")
"##;

fn amaranth_dark_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMARANTH_DARK_THEME_MELPA_PIN, "amaranth-dark-theme.el")
        .expect("prepare pinned amaranth-dark-theme source below ./tmp")
        .with_prelude(AMARANTH_DARK_THEME_TEST_PRELUDE)
        .with_timeout(AMARANTH_DARK_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed amaranth-dark-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_amaranth_dark_theme_parity` cases (2a).
pub(crate) fn assert_amaranth_dark_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        amaranth_dark_theme_oracle(),
        &name,
        "amaranth_dark_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn amaranth_dark_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amaranth_dark_theme_batch(&cases);
}

// END generated package batch tests
