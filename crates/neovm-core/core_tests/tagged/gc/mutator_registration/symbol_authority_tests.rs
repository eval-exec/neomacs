//! Symbol-authority shells.
//! The shell owns the Context entry points and the obarray setup; the bodies
//! and assertions stay in value. One #[test] per former value row, generated
//! from the same list the completeness check compares with the typed case ALL.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use neovm_eval::emacs_core::eval::Baton;
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{
    ProofEra, SymbolAuthorityCase, run_symbol_authority_case_for_test,
};

fn symbol_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, SymId)) {
    Context::with_test_symbol_era(|era, id| body(era, id));
}

fn seeded_global_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, SymId)) {
    Context::with_test_collection_era(
        |context| {
            let id = intern("p3sym-c2-global");
            context
                .obarray_mut()
                .seed_value("p3sym-c2-global", TaggedValue::fixnum(73));
            id
        },
        |era, id| body(era, id),
    );
}

fn alias_setup_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, [SymId; 4])) {
    Context::with_test_collection_era(
        |context| {
            let plain = intern("p3sym-c2-global");
            context
                .obarray_mut()
                .seed_value("p3sym-c2-global", TaggedValue::fixnum(73));
            let alias = intern("p3sym-c2-alias");
            context.obarray_mut().make_alias(alias, plain).unwrap();
            let local = intern("p3sym-c2-local");
            context
                .obarray_mut()
                .make_symbol_localized(local, TaggedValue::NIL);
            [intern("nil"), alias, local, intern("p3sym-c2-absent")]
        },
        |era, ids| body(era, ids),
    );
}

fn run(case: SymbolAuthorityCase) {
    // The eval symbol fixture boots a bare Context; install the canonical
    // core eval hooks first, the same boot `new_context` performs for the
    // collection-era fixture.
    let _hooks = crate::emacs_core::eval_hook_installers::install_eval_hooks();
    run_symbol_authority_case_for_test(
        case,
        symbol_era_impl,
        seeded_global_era_impl,
        alias_setup_era_impl,
    );
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(SymbolAuthorityCase::$case);
        })*
        const SHELL_CASES: &[SymbolAuthorityCase] = &[$(SymbolAuthorityCase::$case),*];
    };
}

shells! {
    p3sym_real_backing_handoff_and_keeper_mint => P3symRealBackingHandoffAndKeeperMint,
    p3sym_two_readers_overlap_and_writer_refuses => P3symTwoReadersOverlapAndWriterRefuses,
    p3sym_reader_refusal_releases_peer_and_finishes_era => P3symReaderRefusalReleasesPeerAndFinishesEra,
    p3sym_foreign_and_stale_keys_refuse_before_backing_deref => P3symForeignAndStaleKeysRefuseBeforeBackingDeref,
    p3sym_retired_world_handle_cannot_admit_in_another_heap => P3symRetiredWorldHandleCannotAdmitInAnotherHeap,
    p3sym_constant_alias_localized_and_absent_admission_refuse => P3symConstantAliasLocalizedAndAbsentAdmissionRefuse,
    p3sym_forgotten_lease_keeps_run_admitted_and_poisoned => P3symForgottenLeaseKeepsRunAdmittedAndPoisoned,
    p3sym_unwind_poisons_before_writer_release => P3symUnwindPoisonsBeforeWriterRelease,
}

#[test]
fn shell_cases_cover_all_symbol_authority_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = SymbolAuthorityCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per moved value row");
}
