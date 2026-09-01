use std::time::Duration;

use crate::{ANGULAR_SNIPPETS_MELPA_PIN, CachedMelpaOracle, YASNIPPET_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANGULAR_SNIPPETS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// angular-snippets ships a yasnippet tree for AngularJS and one command,
/// `ng-snip-show-docs-at-point'.  Both work on ordinary buffers with no
/// external tooling, so every workflow expands real snippets with
/// `yas-expand' in a real `html-mode' or `js-mode' buffer and reads the text,
/// the fields and the echo area back.
///
/// All 42 html snippets share the key "ng", so yasnippet has to ask which one
/// the user meant.  `yas-prompt-functions' is the documented option for
/// choosing that UI, so the workflows set it to a chooser that picks by
/// snippet name -- the prompt is the environmental boundary, and everything
/// yasnippet and the package do around it is real.
const ANGULAR_SNIPPETS_TEST_PRELUDE: &str = r##"
(require 'seq)

(defun ngs-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (ngs-test-plain (car value)) (ngs-test-plain (cdr value))))
        (t value)))

(defun ngs-test-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun ngs-test-echoed (from)
  "Return what has been echoed since position FROM of `*Messages*'.
Emacs collapses a message identical to the one before it into a
\"[N times]\" suffix on the existing line, so a capture starting after
that line would show only the suffix.  When that happens, take the whole
line, which is what the user sees."
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((start (min from (point-max))))
      (when (and (< start (point-max)) (eq (char-after start) ?\[))
        (setq start (save-excursion (goto-char start) (line-beginning-position))))
      (mapcar #'substring-no-properties
              (split-string
               (buffer-substring-no-properties start (point-max))
               "\n" t)))))

(defun ngs-test-expand (name)
  "Expand the snippet called NAME at point, as choosing it from the prompt.
Return what `yas-expand' returned and what it left in the buffer."
  (let* ((echoed-from (ngs-test-mark))
         (yas-prompt-functions
          (list (lambda (_prompt choices &optional display-fn)
                  (seq-find (lambda (choice)
                              (equal (funcall (or display-fn #'identity) choice)
                                     name))
                            choices))))
         (expanded (yas-expand)))
    (list :expanded expanded
          :buffer (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :in-snippet (and (yas-active-snippets) t)
          :echoed (ngs-test-echoed echoed-from))))

(defun ngs-test-keys (mode)
  "Return the snippet keys yasnippet offers in MODE."
  (with-temp-buffer
    (funcall mode)
    (yas-minor-mode 1)
    (sort (mapcar #'substring-no-properties (copy-sequence (yas-active-keys)))
          #'string<)))

(defun ngs-test-snippet-directory (mode)
  "Return the snippet file names the package ships for MODE."
  (let ((directory (expand-file-name (concat "snippets/" (symbol-name mode))
                                     angular-snippets-root)))
    (list :exists (and (file-directory-p directory) t)
          :files (sort (mapcar #'substring-no-properties
                               (directory-files directory nil "\\.yasnippet\\'"))
                       #'string<))))

(defun ngs-test-forget-timers ()
  "Return the pending timers `ng-snip/docs' has scheduled."
  (seq-filter (lambda (timer)
                (eq (timer--function timer) 'ng-snip/forget-last-docs-message))
              timer-list))

(defun ngs-test-run-forget-timers (baseline)
  "Run only the forget timers scheduled since BASELINE.
The editor keeps its own timers, and `ng-snip/docs' adds one every time
it is called without cancelling the last, so this counts and fires only
what appeared after BASELINE was taken."
  (let ((fired 0))
    (dolist (timer (ngs-test-forget-timers))
      (unless (memq timer baseline)
        (setq fired (1+ fired))
        (timer-event-handler timer)))
    fired))

(defvar ngs-test-browsed nil
  "Every URL `browse-url' was asked to open.")

(defun ngs-test-show-docs ()
  "Run `ng-snip-show-docs-at-point', capturing what it echoes or opens."
  (let ((echoed-from (ngs-test-mark))
        (browse-url-browser-function
         (lambda (url &rest _ignored)
           (setq ngs-test-browsed
                 (append ngs-test-browsed
                         (list (substring-no-properties url)))))))
    (let ((failure
           (condition-case error
               (progn (ng-snip-show-docs-at-point) nil)
             (error (ngs-test-plain error)))))
      (list :echoed (ngs-test-echoed echoed-from)
            :signalled failure
            :remembered (ngs-test-plain ng-snip/last-docs-message)))))
"##;

fn angular_snippets_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANGULAR_SNIPPETS_MELPA_PIN, "angular-snippets.el")
        .expect("prepare pinned angular-snippets source below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned Yasnippet dependency below ./tmp")
        .with_prelude(ANGULAR_SNIPPETS_TEST_PRELUDE)
        .with_timeout(ANGULAR_SNIPPETS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed angular-snippets parity test")
        .into()
}

/// Multi-probe batch for `assert_angular_snippets_parity` cases (2a).
pub(crate) fn assert_angular_snippets_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        angular_snippets_oracle(),
        &name,
        "angular_snippets_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn angular_snippets_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_angular_snippets_batch(&cases);
}

// END generated package batch tests
