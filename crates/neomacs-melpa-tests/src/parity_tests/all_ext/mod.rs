use std::time::Duration;

use crate::{ALL_EXT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_EXT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// all-ext extends all.el, whose value is that `*All*' is *editable*: matching
/// lines are collected into it, each piece keeps a marker back into the source,
/// and every change is propagated to the source buffer as it is typed.  The
/// workflows therefore use a real multi-line source buffer, collect a real
/// match set, edit the collection and assert the source's exact resulting text.
///
/// Nothing is stubbed.  Two of the package's three extension points cannot run
/// on this host - the helm and anything occur bridges need helm, and
/// `mc/edit-lines-in-all' needs multiple-cursors, neither of which is installed
/// - so what is covered here is the part a user gets from installing the pair:
///   collection, write-back, the single-piece guard, and the `next-error'
///   integration all-ext contributes.
const ALL_EXT_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defconst ae-test-notes
  (concat "alpha one\n"
          "beta two\n"
          "gamma three\n"
          "alpha four\n"
          "delta five\n"
          "alpha six\n"))

(defun ae-test-copy (value)
  "Copy strings so nothing prints as a `#N=' back reference."
  (if (stringp value) (copy-sequence value) value))

(defmacro ae-test-with-source (&rest body)
  "Collect matches from a real source buffer and run BODY.
`all' kills the *All* buffer before creating it, and `kill-buffer'
signals when no such buffer exists, so one is created first."
  `(let ((source (generate-new-buffer "notes.txt")))
     (get-buffer-create "*All*")
     (unwind-protect
         (with-current-buffer source
           (insert ae-test-notes)
           (goto-char (point-min))
           ,@body)
       (when (get-buffer "*All*") (kill-buffer "*All*"))
       (kill-buffer source))))

(defun ae-test-text (buffer)
  (with-current-buffer buffer
    (copy-sequence (buffer-substring-no-properties (point-min) (point-max)))))

(defun ae-test-pieces ()
  "Describe each collected piece in *All*: its text and where it came from."
  (with-current-buffer "*All*"
    (sort
     (delq nil
           (mapcar (lambda (overlay)
                     (let ((marker (overlay-get overlay 'all-marker)))
                       (and marker
                            (list (overlay-start overlay)
                                  (overlay-end overlay)
                                  (copy-sequence
                                   (buffer-substring-no-properties
                                    (overlay-start overlay) (overlay-end overlay)))
                                  (marker-position marker)))))
                   (overlays-in (point-min) (point-max))))
     (lambda (a b) (< (car a) (car b))))))

(defun ae-test-line-numbers ()
  "The line numbers all.el renders in the left margin of *All*."
  (with-current-buffer "*All*"
    (sort
     (delq nil
           (mapcar (lambda (overlay)
                     (let ((before (overlay-get overlay 'before-string)))
                       (and before
                            (cons (overlay-start overlay)
                                  (string-trim
                                   (substring-no-properties before))))))
                   (overlays-in (point-min) (point-max))))
     (lambda (a b) (< (car a) (car b))))))

(defun ae-test-match-faces ()
  "The stretches of *All* that all.el marked with the `match' face."
  (with-current-buffer "*All*"
    (let ((position (point-min))
          (runs nil))
      (while (< position (point-max))
        (let ((next (next-single-property-change position 'face nil (point-max))))
          (when (eq (get-text-property position 'face) 'match)
            (push (list position next
                        (copy-sequence
                         (buffer-substring-no-properties position next)))
                  runs))
          (setq position next)))
      (nreverse runs))))
"##;

fn all_ext_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_EXT_MELPA_PIN, "all-ext.el")
        .expect("prepare pinned all-ext source and immutable dependencies below ./tmp")
        .with_prelude(ALL_EXT_TEST_PRELUDE)
        .with_timeout(ALL_EXT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-ext parity test")
        .into()
}

/// Multi-probe batch for `assert_all_ext_parity` cases (2a).
pub(crate) fn assert_all_ext_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(all_ext_oracle(), &name, "all_ext_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn all_ext_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_ext_batch(&cases);
}

// END generated package batch tests
