use std::time::Duration;

use crate::{ANCIENT_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANCIENT_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ancient is a hand-written theme: one `deftheme' and 236
/// `custom-theme-set-faces' settings over a 24-colour palette.  Every one of
/// the 236 specs is written `((t ...))', with no display clause at all, so
/// unlike a `min-colors'-gated theme this one paints even the 0-colour
/// `static-gray' frame a batch editor has.
///
/// That makes resolved appearance the right thing to assert here, and the
/// workflows do: real `face-attribute' values for every themed face that
/// exists, for the faces `dired', `org', `flymake' and `pulse' bring in, and
/// for every token of a fontified Emacs Lisp buffer.  The registered specs
/// are pinned whole alongside them, because the colours are the product.
const ANCIENT_THEME_TEST_PRELUDE: &str = r##"
(require 'seq)

(defvar anc-test-theme 'ancient)

(defun anc-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (anc-test-plain (car value)) (anc-test-plain (cdr value))))
        (t value)))

(defun anc-test-settings ()
  "Return (FACE . SPEC) for every face the theme sets, in registration order."
  (let (settings)
    (dolist (setting (get anc-test-theme 'theme-settings))
      (when (eq (car setting) 'theme-face)
        (setq settings
              (cons (cons (nth 1 setting) (copy-tree (nth 3 setting)))
                    settings))))
    (nreverse settings)))

(defun anc-test-face-names ()
  (mapcar #'car (anc-test-settings)))

(defun anc-test-strings (form)
  "Return every string in FORM, each a fresh copy."
  (cond ((stringp form) (list (substring-no-properties form)))
        ((consp form)
         (append (anc-test-strings (car form)) (anc-test-strings (cdr form))))
        (t nil)))

(defun anc-test-palette ()
  "Return each colour the theme names with the faces that use it."
  (let (palette)
    (dolist (setting (anc-test-settings))
      (dolist (color (delete-dups (anc-test-strings (cdr setting))))
        (let ((cell (assoc color palette)))
          (unless cell
            (setq cell (list color))
            (setq palette (append palette (list cell))))
          (setcdr cell (append (cdr cell) (list (car setting)))))))
    palette))

(defun anc-test-palette-counts ()
  (mapcar (lambda (cell) (cons (car cell) (length (cdr cell))))
          (anc-test-palette)))

(defun anc-test-clause-census ()
  "Return how many settings carry each distinct display clause."
  (let (census)
    (dolist (setting (anc-test-settings))
      (let* ((clause (car (car (cdr setting))))
             (cell (assoc clause census)))
        (unless cell
          (setq cell (cons clause 0))
          (setq census (append census (list cell))))
        (setcdr cell (1+ (cdr cell)))))
    census))

(defun anc-test-appearance (face)
  "Return the attributes of FACE that resolve to something on this display."
  (let (resolved)
    (dolist (attribute '(:foreground :background :weight :slant :underline
                         :overline :strike-through :box :inherit :height))
      (let ((value (face-attribute face attribute nil t)))
        (unless (eq value 'unspecified)
          (setq resolved
                (append resolved (list attribute (anc-test-plain value)))))))
    (cons face resolved)))

(defun anc-test-existing-faces ()
  "Return the themed faces that exist in this editor right now."
  (seq-filter (lambda (face) (and (facep face) t))
              (delete-dups (anc-test-face-names))))

(defun anc-test-display ()
  "Return what kind of display these editors actually offer in batch."
  (list :color-cells (display-color-cells)
        :visual-class (display-visual-class)
        :color-p (display-color-p)
        :graphic-p (display-graphic-p)
        :ungated-clause-matches (and (face-spec-set-match-display t nil) t)
        :eighty-nine-colour-clause-matches
        (and (face-spec-set-match-display
              '((class color) (min-colors 89)) nil)
             t)))

(defun anc-test-stack (face)
  "Return the themes that have a spec for FACE, outermost first."
  (mapcar #'car (get face 'theme-face)))

(defun anc-test-clause-of (theme face)
  "Return the display clause THEME's spec for FACE is gated on."
  (anc-test-plain (car (car (cadr (assq theme (get face 'theme-face)))))))
"##;

fn ancient_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANCIENT_THEME_MELPA_PIN, "ancient-theme.el")
        .expect("prepare pinned ancient-theme source below ./tmp")
        .with_prelude(ANCIENT_THEME_TEST_PRELUDE)
        .with_timeout(ANCIENT_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ancient-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_ancient_theme_parity` cases (2a).
pub(crate) fn assert_ancient_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ancient_theme_oracle(), &name, "ancient_theme_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ancient_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ancient_theme_batch(&cases);
}

// END generated package batch tests
