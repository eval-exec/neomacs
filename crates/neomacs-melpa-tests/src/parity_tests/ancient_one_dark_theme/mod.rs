use std::time::Duration;

use crate::{ANCIENT_ONE_DARK_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANCIENT_ONE_DARK_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ancient-one-dark is a `ThemeCreator' theme: one `deftheme' and 202
/// `custom-theme-set-faces' settings.  The specs are the product, so the
/// workflows pin them whole -- every display clause and every colour string
/// -- and then check what the display a batch editor actually has does with
/// them.
///
/// Nothing about the display is faked.  191 of the 202 settings are gated on
/// `((class color) (min-colors 89))', which a 0-colour `static-gray' batch
/// frame does not satisfy, so those faces keep their stock appearance; the 11
/// settings written `((t ...))' do apply, and those are asserted by resolved
/// `face-attribute'.  Forcing the gated clause to match by redefining
/// `display-color-cells' and `face-spec-set-match-display' -- which the corpus
/// this replaces did -- asserts an appearance no user of these editors gets.
const ANCIENT_ONE_DARK_THEME_TEST_PRELUDE: &str = r##"
(require 'seq)

(defvar aod-test-theme 'ancient-one-dark)

(defun aod-test-settings ()
  "Return (FACE . SPEC) for every face the theme sets, in registration order.
`custom-theme-set-faces' hoists `default' to the front and stores the
rest last-first, so this is the order Custom applies them in."
  (let (settings)
    (dolist (setting (get aod-test-theme 'theme-settings))
      (when (eq (car setting) 'theme-face)
        (setq settings
              (cons (cons (nth 1 setting) (copy-tree (nth 3 setting)))
                    settings))))
    (nreverse settings)))

(defun aod-test-face-names ()
  (mapcar #'car (aod-test-settings)))

(defun aod-test-strings (form)
  "Return every string in FORM, each a fresh copy."
  (cond ((stringp form) (list (substring-no-properties form)))
        ((consp form)
         (append (aod-test-strings (car form)) (aod-test-strings (cdr form))))
        (t nil)))

(defun aod-test-palette ()
  "Return each colour the theme names with the faces that use it."
  (let (palette)
    (dolist (setting (aod-test-settings))
      (dolist (color (delete-dups (aod-test-strings (cdr setting))))
        (let ((cell (assoc color palette)))
          (unless cell
            (setq cell (list color))
            (setq palette (append palette (list cell))))
          (setcdr cell (append (cdr cell) (list (car setting)))))))
    palette))

(defun aod-test-palette-counts ()
  (mapcar (lambda (cell) (cons (car cell) (length (cdr cell))))
          (aod-test-palette)))

(defun aod-test-duplicated ()
  "Return the faces the theme sets more than once, with every spec it wrote.
The specs are listed in file order, so the one Custom registered can be
read off against the one that was written first."
  (let ((seen (make-hash-table :test 'eq)) duplicated)
    (dolist (setting (aod-test-settings))
      (puthash (car setting)
               (append (gethash (car setting) seen) (list (cdr setting)))
               seen))
    (dolist (face (delete-dups (aod-test-face-names)))
      (let ((specs (reverse (gethash face seen))))
        (when (cdr specs)
          (setq duplicated
                (append duplicated
                        (list (list face
                                    :written-in-file-order specs
                                    :registered
                                    (copy-tree
                                     (cadr (assq aod-test-theme
                                                 (get face 'theme-face)))))))))))
    duplicated))

(defun aod-test-clause-kinds ()
  "Split the settings by the display clause they are gated on."
  (let (gated ungated other)
    (dolist (setting (aod-test-settings))
      (let ((clause (car (car (cdr setting)))))
        (cond ((equal clause '((class color) (min-colors 89)))
               (setq gated (cons (car setting) gated)))
              ((eq clause t) (setq ungated (cons (car setting) ungated)))
              (t (setq other (cons (car setting) other))))))
    (list :gated-on-89-colors (length gated)
          :ungated (nreverse ungated)
          :other-clauses (nreverse other))))

(defun aod-test-appearance (face)
  "Return the attributes of FACE that resolve to something on this display."
  (let (resolved)
    (dolist (attribute '(:foreground :background :weight :slant :underline
                         :overline :box :inherit :height))
      (let ((value (face-attribute face attribute nil t)))
        (unless (eq value 'unspecified)
          (setq resolved
                (append resolved
                        (list attribute (aod-test-plain value)))))))
    (cons face resolved)))

(defun aod-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (aod-test-plain (car value)) (aod-test-plain (cdr value))))
        (t value)))

(defun aod-test-existing-faces ()
  "Return the themed faces that exist in this editor, without any package."
  (seq-filter #'facep (delete-dups (aod-test-face-names))))

(defun aod-test-display ()
  "Return what kind of display these editors actually offer in batch."
  (list :color-cells (display-color-cells)
        :visual-class (display-visual-class)
        :color-p (display-color-p)
        :graphic-p (display-graphic-p)
        :gated-clause-matches
        (and (face-spec-set-match-display '((class color) (min-colors 89)) nil)
             t)
        :ungated-clause-matches
        (and (face-spec-set-match-display t nil) t)))
"##;

fn ancient_one_dark_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        ANCIENT_ONE_DARK_THEME_MELPA_PIN,
        "ancient-one-dark-theme.el",
    )
    .expect("prepare pinned ancient-one-dark-theme source below ./tmp")
    .with_prelude(ANCIENT_ONE_DARK_THEME_TEST_PRELUDE)
    .with_timeout(ANCIENT_ONE_DARK_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ancient-one-dark-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_ancient_one_dark_theme_parity` cases (2a).
pub(crate) fn assert_ancient_one_dark_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ancient_one_dark_theme_oracle(),
        &name,
        "ancient_one_dark_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ancient_one_dark_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ancient_one_dark_theme_batch(&cases);
}

// END generated package batch tests
