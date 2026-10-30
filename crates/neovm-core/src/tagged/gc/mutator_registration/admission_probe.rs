//! Shared admission-scan witness, available with every test feature set.
use super::symbol_functions::retry::{self as contention, Deadline, Retry, retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use std::ops::ControlFlow;

/// Restoring OUT is total even when the probe assertion unwinds. This guard
/// owns the original permit and never manufactures an owner activation.
struct ProbePermit<'era>(MutatorPermit<'era>);
impl Drop for ProbePermit<'_> {
    fn drop(&mut self) {
        self.0
            .entry
            .admission
            .store(AdmissionWord::OUT, Ordering::Release);
    }
}

#[test]
pub(super) fn in_slice_admission_word_refuses_catalog_transition() {
    // No cons allocation or Lisp roots are needed to test catalog admission.
    // Use the real minimal retained World in both GENs, without setup collection.
    Context::with_test_symbol_era(|era, _| {
        let permit = ProbePermit(era.register(1731, 1).unwrap());
        permit
            .0
            .entry
            .admission
            .store(AdmissionWord::in_slice(0), Ordering::Release);
        assert_eq!(
            era.start_cons_storage(),
            Err(RegistrationError::OutstandingLoan)
        );
        permit
            .0
            .entry
            .admission
            .store(AdmissionWord::OUT, Ordering::Release);
        retry(
            Deadline::contention(),
            || era.start_cons_storage(),
            contention::registration,
        )
        .unwrap();
        let mut stopped = retry(
            Deadline::contention(),
            || era.stop(),
            contention::rendezvous,
        )
        .unwrap();
        retry(
            Deadline::contention(),
            || era.return_cons_storage(&stopped),
            contention::registration,
        )
        .unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        retry_owned(
            Deadline::contention(),
            stopped,
            |stopped| stopped.complete(),
            |failure| match contention::rendezvous(&failure.reason) {
                Retry::Busy => ControlFlow::Continue(failure.into_stopped()),
                Retry::Refuse => ControlFlow::Break(failure),
            },
        )
        .unwrap();
        drop(permit);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}
