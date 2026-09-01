use std::time::Duration;

use crate::{AIRLINE_THEMES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod filesystem;
mod lifecycle;
mod modeline;
mod palettes;
mod registry;
mod workflows;

/// Helpers shared by the workflows.
///
/// airline-themes is an *ungated* theme package.  Its 245 theme files carry no
/// face specs at all -- each is a palette bound in a `let' around a call to
/// `airline-themes-set-deftheme', which builds the specs in airline-themes.el
/// and interpolates the palette into them.  Every one of those specs is
/// written `((t ...))', with no `min-colors' and no `(class color)' clause
/// anywhere, so a batch frame resolves them all for real and resolved
/// `face-attribute' is the right assertion.  The existing files already read
/// resolved colours back for the airline faces; these helpers cover the part
/// they do not reach.
///
/// That part is what the theme replaces rather than what it adds.  Alongside
/// its own `airline-*' faces the package re-specifies stock faces --
/// `mode-line', `mode-line-inactive', `mode-line-buffer-id',
/// `minibuffer-prompt' and the three `tab-bar' faces -- and since a theme spec
/// replaces the standard definition instead of merging with it, everything
/// those specs omit is dropped.  `face-default-spec' travels with every
/// recorded loss, because it is the only thing that distinguishes a loss every
/// user suffers from one that was never in force on a frame reporting no
/// colours.
const AIRLINE_THEMES_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const AIRLINE_THEMES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defconst airline-test-attributes
  '(:foreground :background :weight :slant :underline :overline
    :strike-through :inverse-video :box :height :width :family :foundry
    :inherit)
  "Every face attribute the workflows read back.")

(defun airline-test-resolved (face)
  "Return FACE's resolved attributes that are actually specified.

`member', not `memq': an unset attribute reads back as the symbol
`unspecified', as nil, or as the strings \"unspecified-bg\"/\"unspecified-fg\"
depending on the attribute and on whether a theme is enabled.  An `eq' test
keeps the strings, which makes every face appear to lose its background; and
omitting nil hides the `:inherit' losses, which are the real ones here.  Both
mistakes were made while writing the ahungry-theme suite and both stayed green."
  (let (specified)
    (dolist (attribute airline-test-attributes)
      (let ((value (face-attribute face attribute nil 'default)))
        (unless (member value
                        '(nil unspecified "unspecified-bg" "unspecified-fg"))
          (push (cons attribute (copy-tree value)) specified))))
    (nreverse specified)))

(defun airline-test-capture (faces)
  (mapcar (lambda (face)
            (cons face (and (facep face) (airline-test-resolved face))))
          faces))

(defun airline-test-losses (before after)
  "Attributes specified in BEFORE and gone in AFTER, per face.

A theme's face spec REPLACES the standard definition rather than merging with
it, so every attribute the theme omits is dropped for as long as it is enabled.
`face-default-spec' is reported beside each loss because it is the only thing
that separates a loss every user suffers -- an attribute on an unconditional
`(t ...)' or a `default' clause -- from one that was only ever in force because
this frame reports no colours."
  (let (losses)
    (dolist (entry before)
      (let* ((face (car entry))
             (now (alist-get face after))
             (gone (seq-remove (lambda (pair) (assq (car pair) now))
                               (cdr entry))))
        (when gone
          (push (list face
                      (mapcar #'car gone)
                      (copy-tree (face-default-spec face)))
                losses))))
    (nreverse losses)))

(defun airline-test-changes (before after)
  "Attributes specified in BOTH captures whose value differs, per face.

Separate from `airline-test-losses' because the interesting cases are not all
disappearances.  `mode-line-buffer-id' is `((t (:weight bold)))', and under a
theme that does not restate the weight it resolves to `normal' from the
`default' face rather than to nothing -- the buffer name silently stops being
bold while the attribute is still, technically, specified."
  (let (changes)
    (dolist (entry before)
      (let ((face (car entry))
            (now (alist-get (car entry) after)))
        (dolist (pair (cdr entry))
          (let ((current (assq (car pair) now)))
            (when (and current (not (equal (cdr pair) (cdr current))))
              (push (list face (car pair) (cdr pair) (cdr current))
                    changes))))))
    (nreverse changes)))

(defun airline-test-with-theme (theme body)
  "Enable THEME, call BODY, then disable it again."
  (unwind-protect (progn (load-theme theme t) (funcall body))
    (when (memq theme custom-enabled-themes) (disable-theme theme))))
"##;

fn airline_themes_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIRLINE_THEMES_MELPA_PIN, source_file)
        .expect("prepare pinned airline-themes source below ./tmp")
        .with_prelude(AIRLINE_THEMES_TEST_PRELUDE)
        .with_timeout(AIRLINE_THEMES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed airline-themes parity test")
        .into()
}

/// Multi-probe batch for `assert_airline_themes_autoload_parity` cases (2a).
pub(crate) fn assert_airline_themes_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        airline_themes_oracle("airline-themes-autoloads.el"),
        &name,
        "airline_themes_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_airline_themes_parity` cases (2a).
pub(crate) fn assert_airline_themes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        airline_themes_oracle("airline-themes.el"),
        &name,
        "airline_themes_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn airline_themes_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_airline_themes_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_airline_themes_autoload_batch(&cases);
}

#[test]
fn airline_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        filesystem::filesystem_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        modeline::modeline_public_surface_batch_cases(),
        palettes::palettes_public_surface_batch_cases(),
        registry::registry_airline_themes_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_airline_themes_batch(&cases);
}

// END generated package batch tests
