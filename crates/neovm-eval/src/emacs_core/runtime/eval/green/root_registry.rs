//! Cold completion of World-owned parked-root obligations.

use neovm_value::tagged::gc::QuiescentCollector;
use neovm_value::tagged::transport::{RootBatchError, RootBatchFinishFailure, RootBatchLease};

/// The caller has restored the owner with no active mark, concurrent marker or
/// synchronous collection extent; a completed mark's cooperative sweep may
/// remain. Terminated-owner callers instead hold full collector quiescence.
/// SharedRoot retirement/registration on another host thread can briefly own
/// the same table lock. Retry that contention without dropping the lease.
pub(super) fn finish(mut publication: RootBatchLease) -> Result<(), RootBatchFinishFailure> {
    loop {
        match publication.try_finish() {
            Ok(_) => return Ok(()),
            Err(failure) if matches!(failure.error(), RootBatchError::Busy) => {
                (publication, _) = failure.into_parts();
                std::thread::yield_now();
            }
            Err(failure) => return Err(failure),
        }
    }
}

impl super::GreenRuntime {
    /// All owners have restored or terminated, either after teardown's joins
    /// or at exclusive World destruction. The borrowed drain proof prevents
    /// any collection until the final registration has explicitly finished.
    pub(crate) fn finish_terminated_roots(&self, collector: &QuiescentCollector<'_>) {
        let mut parked = self.parked.borrow_mut();
        if let Err(error) = parked.close_and_drain(collector) {
            tracing::error!(%error, "terminated host registry cannot drain");
            // Exclusive World destruction cannot recover a leaked root reader.
            // Stop before automatic field destruction could reclaim backing.
            std::process::abort();
        }
        for publication in parked.values() {
            assert_eq!(publication.heap_identity(), collector.heap_identity());
        }
        while let Some(key) = parked.first_owner_key() {
            let publication = parked.remove(&key).expect("existing key");
            if let Err(failure) = finish(publication) {
                // An escaped/abandoned explicit reader is an invariant
                // violation here: safe readers borrow a live heap/lease.
                // Preserve registry ownership even during unwinding.
                let (publication, error) = failure.into_parts();
                parked.insert(key, publication);
                panic!("terminated Green root batch cannot finish: {error}");
            }
        }
        while let Some(id) = parked.first_serial_key() {
            let publication = parked.remove_serial(id).expect("existing serial park key");
            if let Err(failure) = finish(publication) {
                let (publication, error) = failure.into_parts();
                parked.insert_serial(id, publication);
                panic!("terminated serial-turn Green root batch cannot finish: {error}");
            }
        }
        while let Some(publication) = parked.pop_retained() {
            if let Err(failure) = finish(publication) {
                let (publication, error) = failure.into_parts();
                parked.push_retained(publication);
                panic!("terminated retained Green root batch cannot finish: {error}");
            }
        }
    }
}

impl super::Context {
    /// Called only after main has restored and joined every worker.
    pub(crate) fn finish_ended_parked_roots(
        &mut self,
    ) -> Result<(), super::host_service::DrainBlocked> {
        if self.with_green_runtime(|runtime| runtime.parked.borrow().is_empty()) {
            return Ok(());
        }
        let heap: *mut neovm_value::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: main holds the Baton after all joins, with no callback or
        // collection extent active. The full registered census remains rooted
        // until mark and sweep finish and every lease has explicitly retired.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                self.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .map_err(|error| {
            super::host_service::DrainBlocked::new(
                self.obarray.closing_lease(),
                super::host_service::HostRootError::Collector(error),
            )
        })?;
        self.with_green_runtime(|runtime| runtime.parked.borrow_mut().close_and_drain(&collector))
            .map_err(|error| {
                super::host_service::DrainBlocked::new(self.obarray.closing_lease(), error)
            })?;
        self.with_green_runtime(|runtime| runtime.finish_terminated_roots(&collector));
        Ok(())
    }
}

/// Retire the `RootBatchLease` returned by `recover_transferred_source` once
/// Main has restored `TurnRight::Solo(Baton)` and reattached its Context (`E6-P.4`).
pub(crate) fn retire_transfer_publication_on_main(
    ctx: &mut super::Context,
    publication: RootBatchLease,
) -> Result<(), neovm_value::error::Flow> {
    ctx.prepare_parked_root_retirement()?;
    match finish(publication) {
        Ok(()) => Ok(()),
        Err(failure) => {
            let (publication, _error) = failure.into_parts();
            ctx.with_green_runtime(|runtime| {
                runtime.parked.borrow_mut().push_retained(publication);
            });
            Ok(())
        }
    }
}
