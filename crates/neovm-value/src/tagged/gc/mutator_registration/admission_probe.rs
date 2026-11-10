//! Shared admission-scan witness, available with every test feature set.
#![cfg(any(
    test,
    all(feature = "test-support", not(feature = "gc-memory-telemetry"))
))]
use super::symbol_functions::retry::{self as contention, Deadline, Retry, retry, retry_owned};
use super::*;
use crate::tagged::gc::ProofEra;
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

/// One row per former value lib-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdmissionProbeCase {
    InSliceAdmissionWordRefusesCatalogTransition,
}

impl AdmissionProbeCase {
    pub const ALL: &'static [Self] = &[Self::InSliceAdmissionWordRefusesCatalogTransition];
}

/// Dispatcher for the core shell: the caller admits the era and supplies the
/// baton World walk; this crate does not name `Context`. Assertions stay in
/// the body below.
pub(crate) fn run_admission_probe_test<K, W>(
    case: AdmissionProbeCase,
    era: ProofEra<'_, K>,
    world_roots: W,
) where
    K: EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    match case {
        AdmissionProbeCase::InSliceAdmissionWordRefusesCatalogTransition => {
            in_slice_admission_word_refuses_catalog_transition_body(era, world_roots);
        }
    }
}

std::thread_local! {
    static PROBE_RUNNER: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
}

/// Install the era-backed check for the legacy 0-arg control row. The core
/// shell calls this on the test thread before the value-side control table
/// runs; the runner opens the real era and executes the body above.
pub(crate) fn install_admission_probe_runner(runner: fn()) {
    PROBE_RUNNER.with(|cell| cell.set(Some(runner)));
}

/// Execute the legacy 0-arg control row. Requires the runner installed on
/// this thread; without it the row fails loudly at setup, before any
/// assertions run.
pub(crate) fn run_admission_probe_control() {
    let runner = PROBE_RUNNER.with(std::cell::Cell::get).unwrap_or_else(|| {
        panic!(
            "admission-probe runner is not installed on this thread; call \
             neovm_value::test_support::authority::install_admission_probe_runner_for_test \
             before running the legacy control row"
        )
    });
    runner();
}

/// Legacy 0-arg entry point, kept as a `fn()` for the value-side control
/// tables: rendezvous::mapping_tests stores it in an `Option<fn()>` column.
/// The executable body is supplied by the core shell through the runner.
pub(super) fn in_slice_admission_word_refuses_catalog_transition() {
    run_admission_probe_control();
}

#[test]
fn legacy_admission_probe_adapter_requires_an_installed_runner() {
    let panic = std::panic::catch_unwind(run_admission_probe_control)
        .expect_err("the control row must refuse without a runner");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("admission-probe runner is not installed"),
        "unexpected panic: {message}"
    );
}

fn in_slice_admission_word_refuses_catalog_transition_body<K, W>(
    era: ProofEra<'_, K>,
    world_roots: W,
) where
    K: EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    // No cons allocation or Lisp roots are needed to test catalog admission.
    // Use the real minimal retained World in both GENs, without setup collection.
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
    era.collect_stopped(&mut stopped, world_roots).unwrap();
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
}
