use std::time::Duration;

use crate::{ADWAITA_DARK_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADWAITA_DARK_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Every workflow enters through `load-theme', the way a user enables a theme.
///
/// One property of this theme decides how it can be observed: every one of its
/// 520 face settings is written for the display `((class color) (min-colors
/// 256))'.  A batch editor's frame is a 0-colour `mono' display, so that clause
/// matches nothing and the palette is registered but never realised -- in both
/// editors, which the first workflow pins explicitly with
/// `face-spec-set-match-display' so the reason is on the record.  The workflows
/// therefore assert the palette where it exists here: the specs the theme
/// registers on each face, which carry the exact colour strings, so a wrong
/// palette, a wrong display clause or a missing face all fail.  They also
/// assert resolved appearance with `face-attribute ... nil t' around
/// `load-theme'/`disable-theme', which is what proves the theme leaves nothing
/// behind.
///
/// The theme reads its `defcustom' toggles while the file is being loaded, so a
/// toggle only takes effect on the next `load-theme' -- the toggle workflow
/// reloads for each one and compares the registered specs.
const ADWAITA_DARK_THEME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst adwaita-test-probed-faces
  '(default cursor region highlight fringe
    mode-line mode-line-inactive mode-line-buffer-id
    font-lock-keyword-face font-lock-string-face font-lock-comment-face
    font-lock-function-name-face font-lock-variable-name-face
    font-lock-constant-face font-lock-type-face font-lock-builtin-face
    font-lock-warning-face error warning success
    isearch lazy-highlight link line-number line-number-current-line
    minibuffer-prompt vertical-border show-paren-match)
  "The faces an editing session actually shows, probed in every workflow.")

(defun adwaita-test-registered (faces)
  "Return (FACE DISPLAY ATTRIBUTES) for each of FACES the theme has registered."
  (mapcar (lambda (face)
            (let ((entry (assq 'adwaita-dark (get face 'theme-face))))
              (if (null entry)
                  (list face :not-registered)
                (let ((clause (car (cadr entry))))
                  (list face (car clause) (cadr clause))))))
          faces))

(defun adwaita-test-resolved (faces)
  "Resolved appearance of FACES on this display, following `:inherit'."
  (mapcar (lambda (face)
            (list face
                  :foreground (face-attribute face :foreground nil t)
                  :background (face-attribute face :background nil t)
                  :weight (face-attribute face :weight nil t)
                  :box (copy-tree (face-attribute face :box nil t))))
          faces))

(defun adwaita-test-display-facts ()
  "Why this display does or does not match the theme's display clause."
  (list :graphic (display-graphic-p)
        :daemon (daemonp)
        :display-type (frame-parameter nil 'display-type)
        :color-cells (display-color-cells)
        :matches-theme-clause
        (face-spec-set-match-display '((class color) (min-colors 256))
                                     (selected-frame))
        :matches-any-display (face-spec-set-match-display t (selected-frame))))

(defun adwaita-test-reset ()
  "Disable every theme the workflows may have enabled."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (disable-theme theme))
  (dolist (theme '(adwaita-dark adwaita-test-overlay))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme))))

(defun adwaita-test-registered-with (variable value faces)
  "Load the theme with VARIABLE set to VALUE and return the registered FACES.
The theme reads its toggles as the file is loaded, so each one needs its own
`load-theme'."
  (adwaita-test-reset)
  (set variable value)
  (unwind-protect
      (progn
        (load-theme 'adwaita-dark t)
        (adwaita-test-registered faces))
    (adwaita-test-reset)
    (set variable nil)))
"##;

fn adwaita_dark_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADWAITA_DARK_THEME_MELPA_PIN, "adwaita-dark-theme.el")
        .expect("prepare pinned adwaita-dark-theme source below ./tmp")
        .with_prelude(ADWAITA_DARK_THEME_TEST_PRELUDE)
        .with_timeout(ADWAITA_DARK_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed adwaita-dark-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_adwaita_dark_theme_parity` cases (2a).
pub(crate) fn assert_adwaita_dark_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        adwaita_dark_theme_oracle(),
        &name,
        "adwaita_dark_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn adwaita_dark_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_adwaita_dark_theme_batch(&cases);
}

// END generated package batch tests
