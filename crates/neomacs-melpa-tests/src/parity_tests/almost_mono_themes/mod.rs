use std::time::Duration;

use crate::{ALMOST_MONO_THEMES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALMOST_MONO_THEMES_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// almost-mono-themes generates four variants - white, black, gray and cream -
/// from one palette table and one shared face specification, so these workflows
/// cover the generation rather than one variant's output: the same probe faces
/// are resolved under each variant, which is what shows that only the palette
/// differs.
///
/// The family's specs use the `((t ...))' display shape, so a batch frame -
/// zero colours, no graphic display - still matches them and
/// `face-attribute ... nil t' returns the real colour strings.  That makes
/// resolved appearance the assertable surface here, and nothing needs faking;
/// see HARNESS-NOTES.md for the `min-colors' families where it is not.
const ALMOST_MONO_THEMES_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun am-test-copy (value)
  "Copy strings so nothing prints as a `#N=' back reference."
  (if (stringp value) (copy-sequence value) value))

(defconst am-test-variants '(almost-mono-white almost-mono-black
                             almost-mono-gray almost-mono-cream))

(defconst am-test-probe-faces
  '((default :background :foreground)
    (region :background :foreground)
    (isearch :background :weight)
    (lazy-highlight :background)
    (font-lock-comment-face :foreground :slant)
    (font-lock-string-face :foreground)
    (font-lock-keyword-face :weight)
    (font-lock-type-face :slant)
    (line-number :foreground)
    (hl-line :background)
    (mode-line :background :foreground :box)
    (org-todo :foreground :weight)
    (org-done :foreground :weight)
    (show-paren-match :foreground :weight)
    (minibuffer-prompt :foreground :weight)
    (completions-common-part :weight :underline)
    (vertical-border :foreground))
  "Faces a user looks at, with the attributes each theme sets on them.")

(defun am-test-face-report (&optional faces)
  "Resolve FACES the way the display would, one entry per attribute."
  (mapcar
   (lambda (entry)
     (cons (car entry)
           (mapcar (lambda (attribute)
                     (cons attribute
                           (am-test-copy
                            (face-attribute (car entry) attribute nil t))))
                   (cdr entry))))
   (or faces am-test-probe-faces)))

(defmacro am-test-with-theme (theme &rest body)
  "Enable THEME, run BODY, then disable it again."
  `(let ((theme ,theme))
     (unwind-protect
         (progn (load-theme theme t) ,@body)
       (disable-theme theme))))

(defun am-test-token-faces (tokens)
  "For each token, the face font lock gave it and how that face resolves."
  (mapcar
   (lambda (token)
     (goto-char (point-min))
     (search-forward token)
     (let* ((position (- (point) (length token)))
            (face (get-text-property position 'face))
            (primary (if (listp face) (car face) face)))
       (list (am-test-copy token)
             face
             (and primary (am-test-copy (face-attribute primary :foreground nil t)))
             (and primary (face-attribute primary :weight nil t))
             (and primary (face-attribute primary :slant nil t)))))
   tokens))
"##;

fn almost_mono_themes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALMOST_MONO_THEMES_MELPA_PIN, "almost-mono-themes.el")
        .expect("prepare pinned almost-mono-themes source below ./tmp")
        .with_prelude(ALMOST_MONO_THEMES_TEST_PRELUDE)
        .with_timeout(ALMOST_MONO_THEMES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed almost-mono-themes parity test")
        .into()
}

/// Multi-probe batch for `assert_almost_mono_themes_parity` cases (2a).
pub(crate) fn assert_almost_mono_themes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        almost_mono_themes_oracle(),
        &name,
        "almost_mono_themes_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn almost_mono_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_almost_mono_themes_batch(&cases);
}

// END generated package batch tests
