use std::time::Duration;

use crate::{ALL_THE_ICONS_DIRED_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_DIRED_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// all-the-icons-dired overrides `font-lock-fontify-region-function` and hangs a
/// `display` text property on the character before each file name in a Dired
/// listing.  These workflows build a real directory tree in the sandbox, open a
/// real Dired buffer over it and drive the real minor mode.
///
/// They describe each `display` value *structurally* -- placeholder or icon,
/// how long, whether the middle character carries properties -- and never
/// report which glyph or font family all-the-icons picked.  That mapping is
/// all-the-icons' own surface and is covered by its suite; embedding the
/// propertized icon string here would also make the output depend on string
/// sharing, which HARNESS-NOTES records as unstable.  Every expectation is read
/// with `get-text-property` / `text-properties-at` rather than built with
/// `format`, because of catalogue entry 22.
const ALL_THE_ICONS_DIRED_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'dired)

(setq make-backup-files nil create-lockfiles nil
      dired-listing-switches "-al"
      dired-use-ls-dired nil)

(defvar atid-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar atid-test-tree (file-name-as-directory (expand-file-name "tree" atid-test-root)))

(defun atid-test-write (name text)
  (let ((path (expand-file-name name atid-test-tree)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun atid-test-make-tree ()
  "A small but varied directory: several types, a dotfile and a subdirectory."
  (atid-test-write "README.md" "# Grüße\n")
  (atid-test-write "notes.org" "* Notizen\n")
  (atid-test-write "script.py" "print('hallo')\n")
  (atid-test-write ".hidden-config" "secret=1\n")
  (atid-test-write "subdir/nested.el" ";; nested\n")
  atid-test-tree)

;; Every helper copies the strings it returns, so nothing can print as a
;; `#N=' back reference (HARNESS-NOTES: string sharing is not stable).
(defun atid-test-describe-display (value)
  "Describe a `display' property structurally.
Deliberately does not report which glyph or font family all-the-icons chose:
that is all-the-icons' own surface, covered by its suite, and embedding the
propertized icon string here would also make the output depend on unstable
string sharing."
  (cond ((null value) 'none)
        ((stringp value)
         (list 'string (length value)
               (copy-sequence (substring-no-properties value))
               (if (text-properties-at (/ (length value) 2) value) 'icon-props 'plain)))
        ((and (consp value) (eq (car value) 'image))
         (list 'image (plist-get (cdr value) :margin)))
        (t 'other)))

(defun atid-test-lines ()
  "For every dired line: the file name and the shape of the display property
placed on the character before it."
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((pos (dired-move-to-filename)))
          (when pos
            (push (list (copy-sequence (or (dired-get-filename 'relative 'noerror) "?"))
                        (atid-test-describe-display
                         (get-text-property (1- pos) 'display)))
                  rows)))
        (forward-line 1))
      (nreverse rows))))

(defun atid-test-display-count ()
  (let ((n 0) (pos (point-min)))
    (while (< pos (point-max))
      (when (get-text-property pos 'display) (setq n (1+ n)))
      (setq pos (1+ pos)))
    n))

(defun atid-test-text ()
  (copy-sequence (buffer-substring-no-properties (point-min) (point-max))))

(defmacro atid-test-in-dired (&rest body)
  `(let ((buffer (dired-noselect (atid-test-make-tree))))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           ,@body)
       (when (buffer-live-p buffer)
         (let ((kill-buffer-query-functions nil)) (kill-buffer buffer))))))
"##;

fn all_the_icons_dired_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_DIRED_MELPA_PIN, "all-the-icons-dired.el")
        .expect("prepare pinned all-the-icons-dired source below ./tmp")
        .with_prelude(ALL_THE_ICONS_DIRED_TEST_PRELUDE)
        .with_timeout(ALL_THE_ICONS_DIRED_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-dired parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_dired_parity` cases (2a).
pub(crate) fn assert_all_the_icons_dired_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_dired_oracle(),
        &name,
        "all_the_icons_dired_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_dired_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_dired_batch(&cases);
}

// END generated package batch tests
