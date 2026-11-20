//! Production safepoint polls (E4-DESIGN §5).
//!
//! Each name is one call. The batch closure runs only inside [`Mutator::poll`]
//! after the phase word is not admissible. `poll_region_refill` is the only
//! one that requests a stop, and only the keeper collects.

#![forbid(unsafe_code)]

use std::num::NonZeroU64;

use neovm_value::tagged::gc::{Mutator, Rendezvous, RendezvousError, Request};
use neovm_value::tagged::transport::SharedRoot;

pub(crate) use neovm_value::tagged::gc::PollOutcome;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "READY-E2B, wired by the E4 engine slice")
)]
pub(crate) fn poll_interp_call_entry(
    owner: &mut Mutator<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    owner.poll(roots)
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "READY-E2B, wired by the E4 engine slice")
)]
pub(crate) fn poll_interp_back_edge(
    owner: &mut Mutator<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    owner.poll(roots)
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "READY-E2B, wired by the E4 engine slice")
)]
pub(crate) fn poll_vm_back_edge(
    owner: &mut Mutator<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    owner.poll(roots)
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "READY-E2B, wired by the E4 engine slice")
)]
pub(crate) fn poll_vm_call(
    owner: &mut Mutator<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    owner.poll(roots)
}

/// Does not release the witness. The E2 stop protocol does that.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "READY-E2B, wired by the E4 engine slice")
)]
pub(crate) fn poll_serial_request(
    owner: &mut Mutator<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    owner.poll(roots)
}

/// Allocation-region refill. Requests a stop, then parks through [`Mutator::poll`].
/// The closure is the complete `publish_scan` batch and runs only on that slow path.
pub(crate) fn poll_region_refill(
    owner: &mut Mutator<'_>,
    rendezvous: Rendezvous<'_>,
    roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
) -> Result<PollOutcome, RendezvousError> {
    let _epoch = request_active(rendezvous)?;
    owner.poll(roots)
}

fn request_active(rendezvous: Rendezvous<'_>) -> Result<NonZeroU64, RendezvousError> {
    loop {
        match rendezvous.request() {
            Ok(Request::Active(epoch)) => return Ok(epoch),
            Ok(Request::Queued) => std::thread::yield_now(),
            Err(RendezvousError::Busy) => std::thread::yield_now(),
            Err(error) => return Err(error),
        }
    }
}
