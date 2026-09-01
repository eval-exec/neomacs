use std::time::Duration;

use crate::{ASTUTE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod casefold;
mod font_lock;
mod keywords;
mod mode;
mod registry;

const ASTUTE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASTUTE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun astute-test-display-map
    ()
  (let (result)
    (dotimes (offset
              (-
               (point-max)
               (point-min)))
      (let* ((position
              (+
               (point-min)
               offset))
             (display
              (get-text-property
               position
               'display)))
        (when display
          (push
           (list
            offset
            (char-after position)
            display)
           result))))
    (nreverse result)))

(defun astute-test-fontify
    (text transforms &optional exceptions)
  (with-temp-buffer
    (insert text)
    (text-mode)
    (setq-local
     astute-transform-list
     transforms)
    (when exceptions
      (setq-local
       astute-prefix-single-quote-exceptions
       exceptions))
    (set-buffer-modified-p nil)
    (astute-mode 1)
    (font-lock-ensure)
    (list
     (buffer-substring-no-properties
      (point-min)
      (point-max))
     (buffer-modified-p)
     astute-mode
     (length astute--keywords)
     (astute-test-display-map))))

(defun astute-test-match-summary
    (regexp strings)
  (mapcar
   (lambda (string)
     (when
         (string-match regexp string)
       (list
        (match-string 0 string)
        (match-string 1 string)
        (match-beginning 1)
        (match-end 1))))
   strings))
"##;

fn astute_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASTUTE_MELPA_PIN, source_file)
        .expect("prepare pinned astute source below ./tmp")
        .with_prelude(ASTUTE_TEST_PRELUDE)
        .with_timeout(ASTUTE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed astute parity test").into()
}

/// Multi-probe batch for `assert_astute_autoload_parity` cases (2a).
pub(crate) fn assert_astute_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        astute_oracle("astute-autoloads.el"),
        &name,
        "astute_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_astute_parity` cases (2a).
pub(crate) fn assert_astute_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(astute_oracle("astute.el"), &name, "astute_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn astute_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_astute_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_astute_autoload_batch(&cases);
}

#[test]
fn astute_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        casefold::casefold_public_surface_batch_cases(),
        font_lock::font_lock_public_surface_batch_cases(),
        keywords::keywords_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        registry::registry_astute_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_astute_batch(&cases);
}

// END generated package batch tests
