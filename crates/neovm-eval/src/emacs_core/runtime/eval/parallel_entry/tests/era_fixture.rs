//! The live-era fixture: a fresh closed source, its era on a keeper service
//! thread, and the owner helpers that thread's rows share. Mounted beside the
//! bootstrap rows so these bodies keep their paths; `owned_source` re-exports
//! it to the parallel-entry rows (E5.2) that drive a real keeper.

use super::super::super::super::world::Baton;
use super::super::super::source_entry::install_inactive_source_module_probe_for_test;
pub(in crate::emacs_core::eval::parallel_entry) use super::super::keeper_service::{
    KeeperEra, keeper_serve,
};
use super::super::{
    Effect, Keeper, OwnedSourceCompilerClosed, OwnedSourceHeapDrained, OwnedSourceRegistries,
    SourceEntryRefusal, SourceFrame, SourceWithdrawalFailure,
};
use super::{KeeperExported, ReturnedMainBacking};
use neovm_value::tagged::gc::{MutatorPermit, RegistrationError, RendezvousError};
use neovm_value::value::Value;
use std::mem::ManuallyDrop;
use std::time::Duration;

/// A fresh source Context for the live-era rows. Their eras need a
/// generational heap, so a full-heap-only build (`NEOVM_GC_GENERATIONAL=0`)
/// fails here loudly after checking that GEN0 refuses before any effect,
/// rather than letting every row pass without running.
pub(in crate::emacs_core::eval::parallel_entry) fn ready()
-> Box<super::super::super::super::Context> {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::cache::reset_carrier_tls();
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(super::super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return source;
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            panic!(
                "the live-era rows need a generational heap; NEOVM_GC_GENERATIONAL=0 disables it"
            )
        }
        other => panic!("GEN0 must refuse before any effect: {:?}", other.err()),
    }
}

fn capture(
    source: Box<super::super::super::super::Context>,
) -> Result<OwnedSourceRegistries, SourceWithdrawalFailure> {
    // SAFETY: fixtures own fresh serial Contexts with no collector, running
    // mutator, store callback, or borrowed heap view.
    unsafe { OwnedSourceRegistries::capture(source) }
}

pub(in crate::emacs_core::eval::parallel_entry) fn close(
    source: Box<super::super::super::super::Context>,
) -> OwnedSourceCompilerClosed {
    // SAFETY: the actual sole source, with every execution, collector, store,
    // and native argument frame already returned.
    unsafe { OwnedSourceRegistries::capture(source) }
        .unwrap()
        .withdraw_observations()
        .unwrap()
        .withdraw_carriers()
        .unwrap()
        .withdraw_jit_cache()
        .unwrap()
        .withdraw_jit_backend()
        .unwrap()
        .close_jit_compiler()
        .unwrap()
}

trait RestoreKeeper {
    type Original;
    fn restore_keeper(self, keeper: Baton) -> Self::Original;
}

impl RestoreKeeper for KeeperExported {
    type Original = Keeper;
    fn restore_keeper(self, keeper: Baton) -> Keeper {
        Keeper(ManuallyDrop::new(keeper))
    }
}

impl<R: RestoreKeeper, T> RestoreKeeper for Effect<R, T> {
    type Original = Effect<R::Original, T>;
    fn restore_keeper(self, keeper: Baton) -> Self::Original {
        Effect {
            _previous: self._previous.restore_keeper(keeper),
            _receipt: self._receipt,
        }
    }
}

pub(in crate::emacs_core::eval::parallel_entry) fn finish_fixture(
    returned: ReturnedMainBacking,
    private: Value,
) {
    let ReturnedMainBacking { frame, backing } = returned;
    // SAFETY: `accept_return` matched the original source, thread, and
    // generation, and every destination frame has returned. The publication
    // stays pinned until the source is installed again below.
    let (keeper, roots, publication) =
        unsafe { super::super::super::super::world::recover_transferred_source(backing) }
            .unwrap_or_else(|failure| panic!("returned backing: {:?}", failure.reason()));
    assert!(roots.contains_for_test(private));
    let SourceFrame { source, effects } = frame;
    // This fresh-source fixture abandons the independently retained native
    // receipts. That is not a production return protocol.
    let Effect {
        _previous: backend,
        _receipt: _compiler,
    } = effects;
    let Effect {
        _previous: cache,
        _receipt: _backend,
    } = backend;
    let Effect {
        _previous: carriers,
        _receipt: _cache,
    } = cache;
    let Effect {
        _previous: observation,
        _receipt: _carriers,
    } = carriers;
    let Effect {
        _previous: entry,
        _receipt: _exported,
    } = observation;
    let Effect {
        _previous: prefix,
        _receipt: _entry,
    } = entry;
    let restored = OwnedSourceHeapDrained {
        _frame: SourceFrame {
            source,
            effects: Effect {
                _previous: prefix.restore_keeper(keeper),
                _receipt: ManuallyDrop::new(roots),
            },
        },
    };
    super::super::census_tests::finish_returned_fixture(restored, publication);
}

pub(in crate::emacs_core::eval::parallel_entry) fn activate(
    mut permit: MutatorPermit<'_>,
    heap: neovm_value::tagged::gc::HeapIdentity,
    owner: u64,
) -> neovm_value::tagged::gc::Mutator<'_> {
    let start = std::time::Instant::now();
    loop {
        match permit.activate(heap, owner) {
            Ok(mutator) => return mutator,
            Err(failure)
                if failure.reason == RegistrationError::Busy
                    && start.elapsed() < Duration::from_secs(30) =>
            {
                permit = failure.into_permit();
                std::thread::yield_now();
            }
            Err(failure) => panic!("activate: {:?}", failure.reason),
        }
    }
}

pub(in crate::emacs_core::eval::parallel_entry) fn retry_native<T>(
    label: &str,
    mut op: impl FnMut() -> Result<T, RendezvousError>,
) -> T {
    let start = std::time::Instant::now();
    loop {
        match op() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::yield_now()
            }
            Err(reason) => panic!("{label}: {reason:?}"),
        }
    }
}

pub(in crate::emacs_core::eval::parallel_entry) fn retire(
    mut owner: neovm_value::tagged::gc::Mutator<'_>,
) {
    let start = std::time::Instant::now();
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy
                    && start.elapsed() < Duration::from_secs(30) =>
            {
                owner = failure.into_owner();
                std::thread::yield_now();
            }
            Err(failure) => panic!("retire: {:?}", failure.reason),
        }
    }
}

// Dropped while the keeper thread panics. Poison wakes `wait_request`
// with `Poisoned` before `thread::scope` joins, so a keeper panic is a
// red result instead of a 600s hang. Mirrors the restricted-lisp
// `PanicFlag` (poison on `thread::panicking`).
pub(in crate::emacs_core::eval::parallel_entry) struct KeeperPanicGuard<'a, 'service, 'heap> {
    pub(in crate::emacs_core::eval::parallel_entry) keeper: &'a KeeperEra<'service, 'heap>,
}
impl Drop for KeeperPanicGuard<'_, '_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.keeper.poison_for_test();
        }
    }
}
