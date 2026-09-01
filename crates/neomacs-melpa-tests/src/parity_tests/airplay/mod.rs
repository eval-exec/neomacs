use std::time::Duration;

use crate::{AIRPLAY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AIRPLAY_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// airplay cannot be loaded in its own pinned closure.  `airplay.el' opens
/// with `(require 'request-deferred)', and the pinned `request'
/// 20250219.2213 no longer ships `request-deferred.el' -- the built package
/// contains only `request.el', its `.elc', its autoloads and its pkg file.
/// So `(require 'airplay)' fails, in both editors, byte for byte.
///
/// The source these workflows load is therefore `airplay-autoloads.el',
/// which is what `package-initialize' really loads for a user and which
/// loads perfectly well.  That is the honest entry point: the autoloads
/// define every command, so the package appears installed and working until
/// one of those commands is invoked.
///
/// No stand-in is used, and that is the point.  The corpus this replaces
/// passed `(provide 'request-deferred)' as its prelude -- a bare `provide'
/// with nothing behind it, which makes the `require' succeed while leaving
/// `request-deferred' undefined.  Supplying the missing library for real
/// would be worse still: it would assert behaviour no user can reach in the
/// configuration MELPA actually ships.
const AIRPLAY_TEST_PRELUDE: &str = r##"
(defun airplay-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (airplay-test-plain (car value)) (airplay-test-plain (cdr value))))
        (t value)))

(defconst airplay-test-commands
  '(airplay/image:view
    airplay:stop
    airplay/video:play
    airplay/video:scrub
    airplay/video:seek
    airplay/video:info
    airplay/video:pause
    airplay/video:resume)
  "Every function `airplay-autoloads.el' defines for the user.
Only three of them carry an `interactive' form, which the workflow pins.")

(defun airplay-test-load ()
  "Try to load the package the way `require' would, and report the outcome."
  (condition-case error
      (progn (require 'airplay) :loaded)
    (error (airplay-test-plain error))))
"##;

fn airplay_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIRPLAY_MELPA_PIN, "airplay-autoloads.el")
        .expect("prepare pinned airplay source below ./tmp")
        .with_prelude(AIRPLAY_TEST_PRELUDE)
        .with_timeout(AIRPLAY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed airplay parity test")
        .into()
}

/// Multi-probe batch for `assert_airplay_parity` cases (2a).
pub(crate) fn assert_airplay_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(airplay_oracle(), &name, "airplay_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn airplay_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_airplay_batch(&cases);
}

// END generated package batch tests
