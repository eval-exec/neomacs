use std::time::Duration;

use crate::{CachedMelpaOracle, WINDOW_NUMBERING_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const WINDOW_NUMBERING_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Every workflow enters through `window-numbering-mode', the way a user
/// enables numbered window shortcuts: the global minor mode assigns each
/// window of every frame a number, shows the selected window's number in the
/// mode line, and binds `M-1' through `M-0' to the `select-window-N'
/// commands.  A batch editor still has a real frame and window tree, so
/// splitting, selecting, and deleting windows all work; only the minibuffer
/// is never active, which the suite pins through the 0-number workflow.
///
/// One property of this theme's implementation decides how the workflows
/// observe it: numbering is recomputed by `window-numbering-update' from
/// `window-numbering-left', the list of unused numbers produced by
/// `window-numbering-calculate-left' -- which walks 9 down to 0 pushing
/// `(% (1+ i) 10)', so windows take numbers 1, 2, ... in `window-list'
/// order and the minibuffer (when active) takes 0.  The workflows pin that
/// order, the `debug-ignored-errors' entry the file pushes at load, the
/// mode-line `(:eval (window-numbering-get-number-string))' entry installed
/// at `window-numbering-mode-line-position', and the hook bookkeeping
/// (`minibuffer-setup-hook', `window-configuration-change-hook') the mode
/// installs and removes.
const WINDOW_NUMBERING_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst wn-test-upstream-tree
  "616379219ab6bdcbc457313a16eb4ff9f63c40bc"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst wn-test-manifest
  '(("window-numbering-pkg.el"
     . "0d9954e272dfb93ac1ad111d10fa49ea3241cc0ace1723312f9d52f6f82cfa94")
    ("window-numbering.el"
     . "d6d5f5e03d8c7ff58d3b13096f9eea7db53102f51f4d008f851197e42418f6c8"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun wn-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "window-numbering.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/window-numbering.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed window-numbering location: %S" located))
    (dolist (entry wn-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed window-numbering source: %S"
                   (car entry))))))
    (list :upstream-tree wn-test-upstream-tree
          :feature (featurep 'window-numbering)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'window-numbering package-alist))))
          :debug-ignored-errors
          (let (entries)
            (dolist (entry debug-ignored-errors)
              ;; GNU's default list mixes strings with symbols such as
              ;; `beginning-of-line'; only the strings are searchable.
              (when (and (stringp entry)
                         (string-match-p "window" entry))
                (push entry entries)))
            (nreverse entries)))))

(defun wn-test-window-numbers ()
  "The (BUFFER-NAME . NUMBER) of every window, in `window-list' order."
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-numbering-get-number window)))
          (window-list nil 0 (frame-first-window))))

(defun wn-test-reset ()
  "Restore the mode and window tree the workflows modify."
  (when (bound-and-true-p window-numbering-mode)
    (window-numbering-mode -1))
  (delete-other-windows)
  (setq window-numbering-auto-assign-0-to-minibuffer t
        window-numbering-before-hook nil
        window-numbering-assign-func nil)
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (and (string-prefix-p "wn-fixture-" name)
                 (not (eq buffer (current-buffer))))
        (ignore-errors (kill-buffer buffer))))))
"##;

fn window_numbering_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WINDOW_NUMBERING_MELPA_PIN, "window-numbering.el")
        .expect("prepare pinned window-numbering source below ./tmp")
        .with_prelude(WINDOW_NUMBERING_TEST_PRELUDE)
        .with_timeout(WINDOW_NUMBERING_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed window-numbering parity test")
        .into()
}

/// Multi-probe batch for `assert_window_numbering_parity` cases (2a).
pub(crate) fn assert_window_numbering_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        window_numbering_oracle(),
        &name,
        "window_numbering_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn window_numbering_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_window_numbering_batch(&cases);
}

// END generated package batch tests
