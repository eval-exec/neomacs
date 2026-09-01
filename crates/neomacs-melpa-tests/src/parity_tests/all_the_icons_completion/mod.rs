use std::time::Duration;

use crate::{ALL_THE_ICONS_COMPLETION_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_COMPLETION_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// The package is one `:around' advice on `completion-metadata-get': ask a
/// completion table for its `affixation-function' and you get one back that
/// prepends an icon.  So the workflows build real completion tables with real
/// metadata and go through `completion-metadata'/`completion-metadata-get',
/// which is the same public route a completion UI takes when it renders
/// candidates.
///
/// They deliberately do not go through the minibuffer: driving completion that
/// way runs into DIVERGENCES.md entry 11, and none of this package's behaviour
/// needs it.  The buffer category is exercised only with buffers the workflow
/// created itself, so entry 13 (`buffer-list' order) cannot reach the
/// assertions either.
///
/// What is pinned per candidate is the affixation triple: the candidate, the
/// icon prefix as character codes (readable and diff-friendly, where the raw
/// glyph is a private-use character), the icon's font family and inherited
/// face, and the suffix.  Which glyph a name maps to is all-the-icons' own
/// business and is covered by that package's suite; what this suite pins is
/// which lookup each category uses and how its result is composed into the
/// triple.
const ALL_THE_ICONS_COMPLETION_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; Built with `apply-partially' rather than a closure so the helper does not
;; depend on how the harness binds variables while evaluating the form.
(defun aic-test-table-function (candidates metadata string predicate action)
  (if (eq action 'metadata)
      (cons 'metadata metadata)
    (complete-with-action action candidates string predicate)))

(defun aic-test-table (candidates &optional metadata)
  "Return a completion table over CANDIDATES reporting METADATA."
  (apply-partially #'aic-test-table-function candidates metadata))

(defun aic-test-icon-face (prefix)
  "Return the font family and inherited face of PREFIX's icon, if it has one."
  (when (and (stringp prefix) (> (length prefix) 0))
    (let ((face (get-text-property 0 'face prefix)))
      (and (listp face)
           (list (plist-get face :family) (plist-get face :inherit))))))

(defun aic-test-affixations (table candidates)
  "Ask TABLE for its affixation function and run it over CANDIDATES."
  (let* ((metadata (completion-metadata "" table nil))
         (affix (completion-metadata-get metadata 'affixation-function)))
    (if (null affix)
        'no-affixation-function
      (mapcar (lambda (triple)
                (let ((candidate (nth 0 triple))
                      (prefix (nth 1 triple))
                      (suffix (nth 2 triple)))
                  (list (substring-no-properties candidate)
                        (append (substring-no-properties prefix) nil)
                        (aic-test-icon-face prefix)
                        (substring-no-properties suffix))))
              (funcall affix candidates)))))

(defun aic-test-advised ()
  (and (advice-member-p 'all-the-icons-completion-completion-metadata-get
                        'completion-metadata-get)
       t))

(defun aic-test-metadata-passthrough (table)
  "Report the metadata properties the advice must not disturb."
  (let ((metadata (completion-metadata "" table nil)))
    (list :category (completion-metadata-get metadata 'category)
          :cycle-sort (completion-metadata-get metadata 'cycle-sort-function)
          :annotation (and (completion-metadata-get metadata 'annotation-function) t))))

(defun aic-test-cleanup ()
  (when (bound-and-true-p all-the-icons-completion-mode)
    (all-the-icons-completion-mode 0)))
"##;

fn all_the_icons_completion_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        ALL_THE_ICONS_COMPLETION_MELPA_PIN,
        "all-the-icons-completion.el",
    )
    .expect("prepare pinned all-the-icons-completion source below ./tmp")
    .with_prelude(ALL_THE_ICONS_COMPLETION_TEST_PRELUDE)
    .with_timeout(ALL_THE_ICONS_COMPLETION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-completion parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_completion_parity` cases (2a).
pub(crate) fn assert_all_the_icons_completion_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_completion_oracle(),
        &name,
        "all_the_icons_completion_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_completion_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_completion_batch(&cases);
}

// END generated package batch tests
