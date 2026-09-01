use std::time::Duration;

use crate::{COMPAT_GNU_ELPA_PIN, CachedPackageOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod buffers;
mod collections;
mod core;

// Before converting this suite, read this.  It records a measurement that
// contradicts the obvious plan, so that whoever picks it up does not spend an
// afternoon rediscovering it.
//
// The obvious plan is to test Compat's *fallbacks* - it is a shim library, so
// the fallback looks like the least-run and therefore richest path.  On this
// host there is no such path to test.  Compat elides each fallback at load
// time when the host already provides the API, and Emacs 31 provides all of
// them.  Measured with GNU Emacs 31.0.90 and Compat 31.0.0.2 loaded:
//
//     (fboundp 'compat--assoc)         => nil
//     (fboundp 'compat--string-search) => nil
//     (fboundp 'compat--plist-get)     => nil
//     (fboundp 'compat--take)          => nil
//     (fboundp 'compat--sort)          => nil
//     ;; every compat-- function bound after load, via mapatoms:
//     ;;   1 in total, and it is `compat--maybe-require'
//     (compat-function assoc)  => assoc
//     (compat-function take)   => take
//     (compat-function value<) => value<
//
// So `(compat-call assoc ...)' *is* `assoc'.  There is no second
// implementation to compare the native one against, and no configuration
// reaches one - the gating is on `emacs-version' at load time.  That is also
// why the modules below call `ignore-error' and `pos-bol' directly and read as
// testing Emacs rather than Compat: on this host, routing the same calls
// through `compat-call' would test exactly the same functions.
//
// What is worth building instead is Compat as a *specification of the host*.
// Compat is a maintained enumeration of which APIs each Emacs generation
// should have and with which extended arguments, written by people with no
// stake in Neomacs.  Asserting that each behaves as Compat documents turns
// this directory into a conformance check of the modern API surface Neomacs
// must provide.
//
// The one constraint that keeps that inside the standards: do not write it as
// an `fboundp' table.  Presence is not the finding - behaviour under the
// extended arguments is.  `assoc' existing says nothing; `assoc' honouring a
// TESTFN, `plist-get' honouring a PREDICATE, `sort' accepting its keyword
// arguments and being stable, `take' clamping past the end, and `value<'
// across mixed types are where a host either conforms or does not.  Compose
// several such calls over one realistic dataset per workflow, so a gap
// surfaces as a wrong answer rather than a missing symbol - a silently
// ignored TESTFN is a wrong lookup in somebody's package, which is the more
// useful failure than one red cell.

const COMPAT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn compat_oracle() -> CachedPackageOracle {
    CachedPackageOracle::new_from_gnu_elpa(COMPAT_GNU_ELPA_PIN, "compat.el")
        .expect("prepare pinned Compat source and dependencies below ./tmp")
        .with_timeout(COMPAT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Compat parity test").into()
}

/// Multi-probe batch for `assert_compat_parity` cases (2a).
pub(crate) fn assert_compat_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(compat_oracle(), &name, "compat_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn compat_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        buffers::buffers_public_surface_batch_cases(),
        collections::collections_public_surface_batch_cases(),
        core::core_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_compat_batch(&cases);
}

// END generated package batch tests
