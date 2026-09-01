use std::time::Duration;

use crate::{AHUNGRY_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod lifecycle;
mod rendering;
mod workflows;

const AHUNGRY_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Helpers shared by the workflows.
///
/// ahungry-theme is an *ungated* theme: all 215 of its face specs are written
/// `((t ...))', with no `min-colors' and no `(class color)' clause anywhere in
/// the file.  A batch frame therefore resolves every one of them for real, so
/// resolved `face-attribute' is the right assertion throughout and pinning
/// registered specs instead would leave the product untested.  `rendering.rs'
/// and `lifecycle.rs' already read resolved colours back out of real fontified
/// buffers; these helpers cover what those two do not reach.
///
/// The one thing the theme *does* gate is its background, on
/// `display-graphic-p' evaluated once at load time.  That is a load-time
/// predicate rather than a display clause, so the workflows pin the gate's
/// answer and the stored spec on each side of it, and never resolve appearance
/// against a graphical frame this editor does not have.
const AHUNGRY_THEME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defconst ahungry-test-attributes
  '(:foreground :background :weight :slant :underline :overline
    :strike-through :inverse-video :box :height :width :family :foundry
    :inherit)
  "Every face attribute the workflows read back.")

(defun ahungry-test-theme-faces ()
  "Every face the ahungry theme sets, deduplicated, in theme-settings order."
  (let (faces)
    (dolist (setting (get 'ahungry 'theme-settings))
      (when (and (eq (car setting) 'theme-face)
                 (not (memq (nth 1 setting) faces)))
        (push (nth 1 setting) faces)))
    (nreverse faces)))

(defun ahungry-test-face-spec-count (face)
  "How many separate specs the theme registered for FACE."
  (seq-count (lambda (setting)
               (and (eq (car setting) 'theme-face)
                    (eq (nth 1 setting) face)))
             (get 'ahungry 'theme-settings)))

(defconst ahungry-test-colour
  '(:foreground :background :weight :slant :underline :overline
    :strike-through :inverse-video :box)
  "The attributes a colour-focused workflow reads.

Deliberately excludes `:family', `:foundry' and `:height': setting a family
alongside a colour on the `default' face is a known GNU/Neomacs divergence
(DIVERGENCES.md), and only the font workflow should witness it.")

(defun ahungry-test-resolved (face &optional attributes)
  "Return FACE's resolved attributes that are actually specified.

Read against the `default' face, which is what the user sees; attributes that
resolve to `unspecified' are omitted so a diff between two captures is short
enough to read.  ATTRIBUTES defaults to `ahungry-test-attributes'; pass a
narrower list when a workflow is about colour rather than font, so that it does
not also witness the `:family' divergence a font workflow already covers."
  (let (specified)
    (dolist (attribute (or attributes ahungry-test-attributes))
      (let ((value (face-attribute face attribute nil 'default)))
        ;; "Unset" has four spellings here and a filter that misses any of them
        ;; silently invents or hides differences.  Both mistakes were made
        ;; while writing this suite, and both produced green tests:
        ;;
        ;;   * `memq' instead of `member' keeps the STRINGS, because an
        ;;     unset background reads back as "unspecified-bg" with no theme
        ;;     and as the symbol `unspecified' under one -- so all 28 faces
        ;;     appeared to lose their background;
        ;;   * omitting nil hides real losses, because a dropped `:inherit'
        ;;     reads back as nil rather than as `unspecified' -- so the
        ;;     `:inherit' losses this theme really does cause were reported
        ;;     as zero.
        (unless (member value '(nil unspecified "unspecified-bg" "unspecified-fg"))
          (push (cons attribute (copy-tree value)) specified))))
    (nreverse specified)))

(defun ahungry-test-capture (faces)
  "Capture the resolved attributes of every face in FACES."
  (mapcar (lambda (face) (cons face (ahungry-test-resolved face))) faces))

(defun ahungry-test-losses (before after)
  "Attributes specified in BEFORE and gone in AFTER, per face.

A theme's face spec REPLACES the standard definition rather than merging with
it, so every attribute the theme omits is dropped.  `face-default-spec' is
reported beside each loss, because an attribute carried on an unconditional
clause was in force for every user, while one carried on a colour-conditional
clause's fallback was only ever in force on a display like this one."
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

(defun ahungry-test-stored-spec (face)
  "Return the spec the theme registered for FACE, as the theme stored it."
  (let (found)
    (dolist (setting (get 'ahungry 'theme-settings))
      (when (and (eq (car setting) 'theme-face)
                 (eq (nth 1 setting) face)
                 (not found))
        (setq found (copy-tree (nth 3 setting)))))
    found))

(defun ahungry-test-all-stored-specs (face)
  "Return every spec the theme registered for FACE, newest first."
  (let (found)
    (dolist (setting (get 'ahungry 'theme-settings))
      (when (and (eq (car setting) 'theme-face)
                 (eq (nth 1 setting) face))
        (push (copy-tree (nth 3 setting)) found)))
    (nreverse found)))

(defun ahungry-test-with-theme-off (body)
  "Call BODY with the ahungry theme disabled, restoring it afterwards."
  (let ((was-enabled (memq 'ahungry custom-enabled-themes)))
    (when was-enabled (disable-theme 'ahungry))
    (unwind-protect (funcall body)
      (when was-enabled (enable-theme 'ahungry)))))
"##;

fn ahungry_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AHUNGRY_THEME_MELPA_PIN, "ahungry-theme.el")
        .expect("prepare pinned ahungry-theme source below ./tmp")
        .with_prelude(AHUNGRY_THEME_TEST_PRELUDE)
        .with_timeout(AHUNGRY_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ahungry-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_ahungry_theme_parity` cases (2a).
pub(crate) fn assert_ahungry_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ahungry_theme_oracle(), &name, "ahungry_theme_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ahungry_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        lifecycle::lifecycle_public_surface_batch_cases(),
        rendering::rendering_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ahungry_theme_batch(&cases);
}

// END generated package batch tests
