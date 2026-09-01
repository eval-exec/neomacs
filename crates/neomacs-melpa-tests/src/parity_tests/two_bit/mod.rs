use std::time::Duration;

use crate::{CachedMelpaOracle, TWO_BIT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod binary;
mod commands;
mod sequences;

const TWO_BIT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const TWO_BIT_FIXTURE_PRELUDE: &str = r##"
  (defun neomacs-2bit--word (value endian)
    (if (eq endian 'big)
        (unibyte-string
         (logand (ash value -24) 255)
         (logand (ash value -16) 255)
         (logand (ash value -8) 255)
         (logand value 255))
      (unibyte-string
       (logand value 255)
       (logand (ash value -8) 255)
       (logand (ash value -16) 255)
       (logand (ash value -24) 255))))

  (defun neomacs-2bit--write-fixture
      (file &optional endian version signature)
    (with-temp-buffer
      (set-buffer-multibyte nil)
      (insert
       (neomacs-2bit--word
        (or signature 440477507)
        endian)
       (neomacs-2bit--word
        (or version 0)
        endian)
       (neomacs-2bit--word 2 endian)
       (neomacs-2bit--word 0 endian)
       (unibyte-string 5)
       "alpha"
       (neomacs-2bit--word 35 endian)
       (unibyte-string 4)
       "beta"
       (neomacs-2bit--word 70 endian)
       ;; alpha: TCAGTCAGTCAG, N at [2, 5), mask at [6, 10).
       (neomacs-2bit--word 12 endian)
       (neomacs-2bit--word 1 endian)
       (neomacs-2bit--word 2 endian)
       (neomacs-2bit--word 3 endian)
       (neomacs-2bit--word 1 endian)
       (neomacs-2bit--word 6 endian)
       (neomacs-2bit--word 4 endian)
       (neomacs-2bit--word 0 endian)
       (unibyte-string 27 27 27)
       ;; beta: GGGGAAAA, no N or mask blocks.
       (neomacs-2bit--word 8 endian)
       (neomacs-2bit--word 0 endian)
       (neomacs-2bit--word 0 endian)
       (neomacs-2bit--word 0 endian)
       (unibyte-string 255 170))
      (let ((coding-system-for-write 'binary))
        (write-region
         (point-min)
         (point-max)
         file nil 'silent)))
    file)
"##;

fn two_bit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TWO_BIT_MELPA_PIN, "2bit.el")
        .expect("prepare pinned 2bit source below ./tmp")
        .with_prelude(TWO_BIT_FIXTURE_PRELUDE)
        .with_timeout(TWO_BIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed 2bit parity test").into()
}

/// Multi-probe batch for `assert_two_bit_parity` cases (2a).
pub(crate) fn assert_two_bit_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(two_bit_oracle(), &name, "two_bit_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn two_bit_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        binary::binary_public_surface_batch_cases(),
        commands::commands_public_surface_batch_cases(),
        sequences::sequences_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_two_bit_batch(&cases);
}

// END generated package batch tests
