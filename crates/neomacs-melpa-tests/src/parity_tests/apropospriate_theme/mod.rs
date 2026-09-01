use std::time::Duration;

use crate::{APROPOSPRIATE_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APROPOSPRIATE_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APROPOSPRIATE_THEME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun apropospriate-test-face-at
    (token)
  (goto-char
   (point-min))
  (search-forward token)
  (or
   (get-text-property
    (match-beginning 0)
    'face)
   (get-text-property
    (match-beginning 0)
    'font-lock-face)))

(defun apropospriate-test-face-view
    (token)
  (let ((face
         (apropospriate-test-face-at
          token)))
    (list
     face
     (face-attribute
      face
      :foreground
      nil
      'default)
     (face-attribute
      face
      :background
      nil
      'default)
     (face-attribute
      face
      :weight
      nil
      'default))))

(defun apropospriate-test-load-color-theme
    (theme)
  (let ((original-frame-parameter
         (symbol-function
          'frame-parameter)))
    (cl-letf
        (((symbol-function
           'display-color-cells)
          (lambda
              (&optional _frame)
            16777216))
         ((symbol-function
           'frame-parameter)
          (lambda
              (frame parameter)
            (if
                (eq parameter
                    'display-type)
                'color
              (funcall
               original-frame-parameter
               frame
               parameter)))))
      (load-theme
       theme
       t))))

(defun apropospriate-test-disable-themes ()
  (mapc
   #'disable-theme
   (copy-sequence
    custom-enabled-themes)))
"##;

fn apropospriate_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APROPOSPRIATE_THEME_MELPA_PIN, "apropospriate-theme.el")
        .expect("prepare pinned apropospriate-theme source below ./tmp")
        .with_prelude(APROPOSPRIATE_THEME_TEST_PRELUDE)
        .with_timeout(APROPOSPRIATE_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apropospriate-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_apropospriate_theme_parity` cases (2a).
pub(crate) fn assert_apropospriate_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        apropospriate_theme_oracle(),
        &name,
        "apropospriate_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn apropospriate_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apropospriate_theme_batch(&cases);
}

// END generated package batch tests
