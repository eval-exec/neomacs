use super::super::super::super::world::Baton;
use super::super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::super::{
    Effect, Keeper, OwnedSourceCompilerClosed, OwnedSourceHeapDrained, OwnedSourceRegistries,
    SourceEntryRefusal, SourceFrame, SourceWithdrawalFailure, SourceWithdrawalStep,
};
use super::{KeeperExported, PreparedEraStart, ReturnedMainBacking};
use neovm_value::tagged::gc::heap_transfer::TransferError;
use neovm_value::value::Value;
use std::mem::ManuallyDrop;

fn ready() -> Option<Box<super::super::super::super::Context>> {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::cache::reset_carrier_tls();
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(super::super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return Some(source);
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            None
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

fn close(source: Box<super::super::super::super::Context>) -> OwnedSourceCompilerClosed {
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

fn finish_fixture(returned: ReturnedMainBacking, private: Value) {
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

#[test]
fn pentry_e2b_bootstrap_actual_source_send_and_verified_return() {
    let Some(mut source) = ready() else {
        return;
    };
    let private = Value::string("actual-boxed-source-private-root");
    source.lexenv = private;
    let alive = source.world_lifetime_probe_for_test();
    let original = std::ptr::from_ref(&*source);
    let (shell, start) = close(source).prepare_era().unwrap();
    assert_eq!(std::ptr::from_ref(&**shell.frame.source), original);
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let end = std::thread::spawn(move || {
        let transfer = start
            .into_heap_transfer()
            .source_roundtrip_for_test()
            .expect("destination-local GEN1 loan");
        PreparedEraStart { transfer }.return_to_main().unwrap()
    })
    .join()
    .expect("service thread");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
    assert!(
        !alive(),
        "the fixture joins its actual GC service after the matched return"
    );
}

#[test]
fn pentry_e2b_bootstrap_source_drop_poisons_the_prepared_right() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(shell);
    assert!(start.return_to_main().is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_prepared_drop_and_source_forget_retain_backing() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(start);
    std::mem::forget(shell);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_catch_unwind_does_not_restore_source_execution() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let pair = close(source).prepare_era().unwrap();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _pair = pair;
        panic!("SUSPENDED_MAIN_UNWIND");
    }));
    assert!(caught.is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_foreign_tls_preparation_failure_retains_actual_shell() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let closed = close(source);
    let mut foreign = Box::new(super::super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    foreign.setup_thread_locals();
    let failure = match closed.prepare_era() {
        Err(failure) => failure,
        Ok(_) => panic!("foreign TLS was accepted"),
    };
    assert!(format!("{failure:?}").contains("InstalledTls"));
    match &failure {
        SourceWithdrawalFailure::Retained(retained) => {
            assert_eq!(retained.step, SourceWithdrawalStep::HeapTransfer);
        }
        SourceWithdrawalFailure::BeforeEffects(_) => panic!("source had already closed"),
    }
    drop(failure);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&foreign.tagged_heap);
    assert!(alive());
}

#[test]
fn pentry_e2b_bootstrap_foreign_return_cannot_reactivate_either_source() {
    let Some(first) = ready() else {
        return;
    };
    let first_alive = first.world_lifetime_probe_for_test();
    let (first, first_start) = close(first).prepare_era().unwrap();
    let second = ready().expect("the same heap mode admits a second source");
    let second_alive = second.world_lifetime_probe_for_test();
    let (second, second_start) = close(second).prepare_era().unwrap();
    let end = second_start.return_to_main().unwrap();
    let failure = match first.accept_return(end) {
        Err(failure) => failure,
        Ok(_) => panic!("foreign return was accepted"),
    };
    assert_eq!(failure.failure.reason(), TransferError::WrongReturn);
    drop(failure);
    drop(first_start);
    drop(second);
    assert!(first_alive() && second_alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_l61_keeper_service_admits_and_finishes_its_own_era() {
    use super::super::keeper_service::keeper_start;
    let Some(mut source) = ready() else {
        return;
    };
    let private = Value::string("l61-keeper-service-private-root");
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || match keeper_start(start) {
        Ok(end) => {
            tx.send(Ok(end)).expect("send keeper end");
        }
        Err(failure) => {
            tx.send(Err(failure.notice())).expect("send keeper notice");
        }
    });
    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    let end = outcome.unwrap_or_else(|notice| panic!("keeper start refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

#[test]
fn pentry_l61_keeper_service_refuses_a_poisoned_start_with_a_notice() {
    use super::super::keeper_service::{KeeperStartNotice, keeper_start};
    use neovm_value::tagged::gc::heap_transfer::ScopedEraError;
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(shell);
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || {
        let notice = match keeper_start(start) {
            Ok(_) => None,
            Err(failure) => Some(failure.notice()),
        };
        tx.send(notice).expect("send keeper notice");
    });
    let notice = rx
        .recv()
        .expect("notice")
        .expect("a poisoned start is refused");
    service.join().expect("keeper service thread");
    assert_eq!(
        notice,
        KeeperStartNotice::Scoped(ScopedEraError::Transfer(TransferError::Phase)),
        "only a send notice crosses; the confined failure stays on the thread"
    );
    assert!(alive(), "the poisoned start retains its backing");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}
