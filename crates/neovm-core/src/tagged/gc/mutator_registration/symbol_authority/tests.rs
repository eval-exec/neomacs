use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use crate::tagged::gc::ProofEra as ParallelHeapEra;

fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}
fn setup(context: &mut Context) -> SymId {
    let id = intern("p3sym-c2-global");
    context
        .obarray_mut()
        .seed_value("p3sym-c2-global", Value::fixnum(73));
    id
}

#[test]
fn p3sym_real_backing_handoff_and_keeper_mint() {
    Context::with_test_symbol_era(|era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let heap = era.heap_identity();
        let mut owner = era.register(1, 8).unwrap().activate(heap, 1).unwrap();
        assert!(matches!(
            era.admit_plain_symbol(id),
            Err(RegistrationError::Busy)
        ));
        {
            let mut access = owner.run_access().unwrap();
            let reader = access.symbol_ref(&handle).unwrap();
            assert_eq!(reader.backing(), PlainSymbolBacking::Unprojected);
            // SAFETY: actual retained World and counted read lease, no raw
            // bits escape to the executor; this is assertion-only test code.
            assert_eq!(
                unsafe { reader.run.owner.registry.symbols.backing.probe(id) },
                Value::fixnum(73).bits()
            );
        }
        {
            let mut access = owner.run_access().unwrap();
            let writer = access.symbol_mut(&handle).unwrap();
            assert_eq!(writer.backing(), PlainSymbolBacking::Unprojected);
        }
        owner.retire_rooted(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        {
            let keeper = stopped.symbols().unwrap();
            assert_eq!(keeper.backing(&handle), Ok(PlainSymbolBacking::Unprojected));
            assert_eq!(
                unsafe { keeper.registry.symbols.backing.probe(id) },
                Value::fixnum(73).bits()
            );
        }
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        finish(era);
    });
}

mod readers;

#[test]
fn p3sym_two_readers_overlap_and_writer_refuses() {
    readers::check(readers::Scenario::BothReaders);
}

#[test]
fn p3sym_reader_refusal_releases_peer_and_finishes_era() {
    readers::check(readers::Scenario::RefuseSecond);
}

#[test]
fn p3sym_foreign_and_stale_keys_refuse_before_backing_deref() {
    Context::with_test_collection_era(setup, |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let mut stale = handle;
        stale.era = NonZeroU64::new(handle.era.get() + 1).unwrap();
        let mut foreign = handle;
        foreign.world ^= 1;
        let heap = era.heap_identity();
        let mut owner = era.register(1, 8).unwrap().activate(heap, 1).unwrap();
        {
            let mut access = owner.run_access().unwrap();
            assert!(matches!(
                access.symbol_ref(&stale),
                Err(RegistrationError::StaleRegistration)
            ));
            assert!(matches!(
                access.symbol_mut(&foreign),
                Err(RegistrationError::StaleRegistration)
            ));
            assert_eq!(
                access.owner.registry.symbols.word.load(Ordering::Acquire),
                0
            );
        }
        owner.finish(NonZeroU64::MIN).unwrap();
        finish(era);
    });
}

#[test]
fn p3sym_retired_world_handle_cannot_admit_in_another_heap() {
    let retired = Context::with_test_collection_era(setup, |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        finish(era);
        handle
    });
    Context::with_test_collection_era(setup, |era, _| {
        let heap = era.heap_identity();
        let mut owner = era.register(1, 8).unwrap().activate(heap, 1).unwrap();
        {
            let mut access = owner.run_access().unwrap();
            assert!(matches!(
                access.symbol_ref(&retired),
                Err(RegistrationError::WrongHeap)
            ));
        }
        owner.finish(NonZeroU64::MIN).unwrap();
        finish(era);
    });
}

#[test]
fn p3sym_constant_alias_localized_and_absent_admission_refuse() {
    Context::with_test_collection_era(
        |ctx| {
            let plain = setup(ctx);
            let alias = intern("p3sym-c2-alias");
            ctx.obarray_mut().make_alias(alias, plain).unwrap();
            let local = intern("p3sym-c2-local");
            ctx.obarray_mut().make_symbol_localized(local, Value::NIL);
            [intern("nil"), alias, local, intern("p3sym-c2-absent")]
        },
        |era, ids| {
            for id in ids {
                assert!(matches!(
                    era.admit_plain_symbol(id),
                    Err(RegistrationError::Publication)
                ));
            }
            finish(era);
        },
    );
}

#[test]
fn p3sym_forgotten_lease_keeps_run_admitted_and_poisoned() {
    Context::with_test_collection_era(setup, |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let heap = era.heap_identity();
        let mut owner = era.register(1, 8).unwrap().activate(heap, 1).unwrap();
        {
            let mut access = owner.run_access().unwrap();
            std::mem::forget(access.symbol_mut(&handle).unwrap());
        }
        assert!(owner.entry().admitted());
        assert!(matches!(
            era.registry().symbols.ready(),
            Err(RegistrationError::Poisoned)
        ));
        assert!(era.registry().park.poisoned.load(Ordering::Acquire));
        assert!(matches!(
            owner.run_access(),
            Err(RegistrationError::Poisoned)
        ));
        // Intentionally abandon both: the terminal protocol retains backing.
        drop(owner);
        drop(era);
    });
}

#[test]
fn p3sym_unwind_poisons_before_writer_release() {
    Context::with_test_collection_era(setup, |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let heap = era.heap_identity();
        let mut owner = era.register(1, 8).unwrap().activate(heap, 1).unwrap();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut access = owner.run_access().unwrap();
            let _writer = access.symbol_mut(&handle).unwrap();
            panic!("p3sym injected partial commit");
        }));
        assert!(failure.is_err());
        assert!(word::quiescent(&era.registry().symbols.word));
        assert!(matches!(
            era.registry().symbols.ready(),
            Err(RegistrationError::Poisoned)
        ));
        drop(owner);
        drop(era);
    });
}

#[test]
fn p3sym_source_receiver_guard() {
    let executor = include_str!("../restricted_lisp.rs");
    for forbidden in [
        "SymbolBacking",
        "symbol_authority",
        "symbol_ref(",
        "symbol_mut(",
        "admit_plain_symbol(",
    ] {
        assert!(
            !executor.contains(forbidden),
            "executor facade must not reach raw symbol authority: {forbidden}"
        );
    }
    let api = include_str!("../symbol_authority.rs");
    for forbidden in [
        "-> Value",
        "-> TaggedValue",
        "-> &Obarray",
        "-> &mut Obarray",
        "-> FwdRef",
        "-> usize",
        "pub trait",
    ] {
        assert!(
            !api.contains(forbidden),
            "sealed surface exports forbidden receiver/extension: {forbidden}"
        );
    }
}
