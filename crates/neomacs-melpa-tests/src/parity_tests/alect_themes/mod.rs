use std::time::Duration;

use crate::{ALECT_THEMES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALECT_THEMES_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// alect is a theme *family*: six themes (light, dark and black, each with an
/// inverted `-alt' variant) generated from one palette by a macro that runs
/// when the theme file is loaded.  Everything a user can configure --
/// `alect-colors', `alect-display-class', `alect-overriding-faces',
/// `alect-ignored-faces' -- is read at that moment, so the workflows change a
/// setting and load the theme again, which is exactly what the package's own
/// documentation tells a user to do.
///
/// Unlike the themes converted before it, this one can be asserted by *resolved*
/// appearance without any faking.  `alect-display-class' is a defcustom whose
/// documented "All terminals" value is nil, which produces a face spec with a
/// nil display -- and a nil display matches every terminal, batch included.  The
/// first workflow pins the stock `((type graphic))' behaviour (registered but
/// not realised here, in both editors, with the display facts on the record);
/// the rest set the option and read real colours back with `face-attribute
/// ... nil t'.  Nothing is stubbed.
const ALECT_THEMES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst alect-test-faces
  '(default cursor link fringe mode-line font-lock-keyword-face
    font-lock-string-face font-lock-comment-face)
  "The faces every workflow probes.")

(defun alect-test-registered (face)
  "Return (THEME DISPLAY ATTRIBUTES) for FACE, or nil if unthemed."
  (let ((entry (car (get face 'theme-face))))
    (and entry
         (let ((clause (car (cadr entry))))
           (list (car entry) (car clause) (cdr clause))))))

(defun alect-test-registrations (&optional faces)
  (mapcar (lambda (face) (cons face (alect-test-registered face)))
          (or faces alect-test-faces)))

(defun alect-test-resolved (&optional faces)
  "Resolved appearance of FACES on this display, following `:inherit'."
  (mapcar (lambda (face)
            (list face
                  (face-attribute face :foreground nil t)
                  (face-attribute face :background nil t)))
          (or faces alect-test-faces)))

(defun alect-test-display-facts ()
  "Why a display clause does or does not match here."
  (list :graphic (display-graphic-p)
        :display-type (frame-parameter nil 'display-type)
        :color-cells (display-color-cells)
        :matches-graphic (face-spec-set-match-display '((type graphic))
                                                      (selected-frame))
        :matches-256 (face-spec-set-match-display '((class color) (min-colors 256))
                                                  (selected-frame))
        :matches-nil (face-spec-set-match-display nil (selected-frame))))

(defun alect-test-settings (theme)
  "Report how many faces and variables THEME registered."
  (let ((faces 0) (variables 0))
    (dolist (setting (get theme 'theme-settings))
      (if (eq (car setting) 'theme-face)
          (setq faces (1+ faces))
        (setq variables (1+ variables))))
    (list :faces faces :variables variables)))

(defun alect-test-variables (theme)
  "Return the variables THEME sets, sorted by name."
  (sort (cl-loop for setting in (get theme 'theme-settings)
                 unless (eq (car setting) 'theme-face)
                 collect (nth 1 setting))
        #'string<))

(defun alect-test-reset ()
  (dolist (theme (copy-sequence custom-enabled-themes))
    (disable-theme theme)))
"##;

fn alect_themes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALECT_THEMES_MELPA_PIN, "alect-themes.el")
        .expect("prepare pinned alect-themes source below ./tmp")
        .with_prelude(ALECT_THEMES_TEST_PRELUDE)
        .with_timeout(ALECT_THEMES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alect-themes parity test")
        .into()
}

/// Multi-probe batch for `assert_alect_themes_parity` cases (2a).
pub(crate) fn assert_alect_themes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alect_themes_oracle(), &name, "alect_themes_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alect_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alect_themes_batch(&cases);
}

// END generated package batch tests
