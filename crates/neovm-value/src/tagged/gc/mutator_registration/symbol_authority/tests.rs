use super::super::EraKeeper;
use super::*;
use crate::intern::intern;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::value::Value;

fn finish<K: EraKeeper>(era: ParallelHeapEra<'_, K>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}

/// Opener slots are plain fn pointers; the core shell owns the Context entry
/// points and the obarray setup, the bodies and assertions stay in value.
pub(crate) type SymbolEraFn<K> =
    for<'a> fn(&'a mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, SymId));
pub(crate) type AliasSetupEraFn<K> =
    for<'a> fn(&'a mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, [SymId; 4]));

/// One row per former value lib-test that needed a Context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolAuthorityCase {
    P3symRealBackingHandoffAndKeeperMint,
    P3symTwoReadersOverlapAndWriterRefuses,
    P3symReaderRefusalReleasesPeerAndFinishesEra,
    P3symForeignAndStaleKeysRefuseBeforeBackingDeref,
    P3symRetiredWorldHandleCannotAdmitInAnotherHeap,
    P3symConstantAliasLocalizedAndAbsentAdmissionRefuse,
    P3symForgottenLeaseKeepsRunAdmittedAndPoisoned,
    P3symUnwindPoisonsBeforeWriterRelease,
}

impl SymbolAuthorityCase {
    pub const ALL: &'static [Self] = &[
        Self::P3symRealBackingHandoffAndKeeperMint,
        Self::P3symTwoReadersOverlapAndWriterRefuses,
        Self::P3symReaderRefusalReleasesPeerAndFinishesEra,
        Self::P3symForeignAndStaleKeysRefuseBeforeBackingDeref,
        Self::P3symRetiredWorldHandleCannotAdmitInAnotherHeap,
        Self::P3symConstantAliasLocalizedAndAbsentAdmissionRefuse,
        Self::P3symForgottenLeaseKeepsRunAdmittedAndPoisoned,
        Self::P3symUnwindPoisonsBeforeWriterRelease,
    ];
}

mod readers;

fn p3sym_real_backing_handoff_and_keeper_mint_body<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_era: SymbolEraFn<K>,
) {
    symbol_era(&mut |era, id| {
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

fn p3sym_two_readers_overlap_and_writer_refuses_body<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_era: SymbolEraFn<K>,
) {
    readers::check_body(readers::Scenario::BothReaders, symbol_era);
}

fn p3sym_reader_refusal_releases_peer_and_finishes_era_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_era: SymbolEraFn<K>,
) {
    readers::check_body(readers::Scenario::RefuseSecond, symbol_era);
}

fn p3sym_foreign_and_stale_keys_refuse_before_backing_deref_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    seeded_global_era: SymbolEraFn<K>,
) {
    seeded_global_era(&mut |era, id| {
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

fn p3sym_retired_world_handle_cannot_admit_in_another_heap_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    seeded_global_era: SymbolEraFn<K>,
) {
    let mut retired = None;
    seeded_global_era(&mut |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        finish(era);
        retired = Some(handle);
    });
    seeded_global_era(&mut |era, _| {
        let retired = retired.expect("the first era admitted a handle");
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

fn p3sym_constant_alias_localized_and_absent_admission_refuse_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    alias_setup_era: AliasSetupEraFn<K>,
) {
    alias_setup_era(&mut |era, ids| {
        for id in ids {
            assert!(matches!(
                era.admit_plain_symbol(id),
                Err(RegistrationError::Publication)
            ));
        }
        finish(era);
    });
}

fn p3sym_forgotten_lease_keeps_run_admitted_and_poisoned_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    seeded_global_era: SymbolEraFn<K>,
) {
    seeded_global_era(&mut |era, id| {
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

fn p3sym_unwind_poisons_before_writer_release_body<K: EraKeeper + std::fmt::Debug + 'static>(
    seeded_global_era: SymbolEraFn<K>,
) {
    seeded_global_era(&mut |era, id| {
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

/// One runner for the symbol-authority subtree. The match is exhaustive over
/// the typed case list; `name: &str` dispatch is not used.
pub(crate) fn run_symbol_authority_case<K: EraKeeper + std::fmt::Debug + 'static>(
    case: SymbolAuthorityCase,
    symbol_era: SymbolEraFn<K>,
    seeded_global_era: SymbolEraFn<K>,
    alias_setup_era: AliasSetupEraFn<K>,
) {
    match case {
        SymbolAuthorityCase::P3symRealBackingHandoffAndKeeperMint => {
            p3sym_real_backing_handoff_and_keeper_mint_body(symbol_era)
        }
        SymbolAuthorityCase::P3symTwoReadersOverlapAndWriterRefuses => {
            p3sym_two_readers_overlap_and_writer_refuses_body(symbol_era)
        }
        SymbolAuthorityCase::P3symReaderRefusalReleasesPeerAndFinishesEra => {
            p3sym_reader_refusal_releases_peer_and_finishes_era_body(symbol_era)
        }
        SymbolAuthorityCase::P3symForeignAndStaleKeysRefuseBeforeBackingDeref => {
            p3sym_foreign_and_stale_keys_refuse_before_backing_deref_body(seeded_global_era)
        }
        SymbolAuthorityCase::P3symRetiredWorldHandleCannotAdmitInAnotherHeap => {
            p3sym_retired_world_handle_cannot_admit_in_another_heap_body(seeded_global_era)
        }
        SymbolAuthorityCase::P3symConstantAliasLocalizedAndAbsentAdmissionRefuse => {
            p3sym_constant_alias_localized_and_absent_admission_refuse_body(alias_setup_era)
        }
        SymbolAuthorityCase::P3symForgottenLeaseKeepsRunAdmittedAndPoisoned => {
            p3sym_forgotten_lease_keeps_run_admitted_and_poisoned_body(seeded_global_era)
        }
        SymbolAuthorityCase::P3symUnwindPoisonsBeforeWriterRelease => {
            p3sym_unwind_poisons_before_writer_release_body(seeded_global_era)
        }
    }
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
