//! Bounded call histories on the retained World and private census homes.
use super::*;

/// Split-era adapter: the value-side poison setter is reached only through
/// the signed authority fence, while the fixture keeps its original call
/// shape. Test-local; the original method is not visible from this crate.
trait RunPoisonForTest {
    fn set_symbol_poison_for_test(&mut self, poisoned: bool);
}
impl RunPoisonForTest for RunAccess<'_, '_> {
    fn set_symbol_poison_for_test(&mut self, poisoned: bool) {
        neovm_value::test_support::authority::set_symbol_poison_for_test(self, poisoned);
    }
}
use neovm_value::test_support::authority::plant_scalar_root_for_test;

fn finish<'era>(
    run: &mut RunAccess<'_, 'era>,
    activation: &mut FunctionActivation<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> String {
    for _ in 0..2 {
        assert_eq!(
            run.symbol_word_for_test(),
            0,
            "no symbol lease crosses a call step"
        );
        if run.sym_call_step(activation, dst, NonZeroU32::MIN).unwrap() == FunctionStep::Returned {
            assert_eq!(run.symbol_word_for_test(), 0);
            return crate::emacs_core::print_value(&value_from_raw_bits(dst.word_for_test(run)));
        }
    }
    panic!("constant call exceeded its finite instruction count")
}

#[test]
fn p3sym_c4_call_old_selection_new_binding_and_private_operand_homes() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, homes| {
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            let selected = activation.selected_epoch();
            let before = homes.dst.word_for_test(run);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                Ok(FunctionStep::Yielded)
            );
            assert_eq!(homes.dst.word_for_test(run), before);
            // These are the old preparation's caller-owned homes. Replacing
            // them cannot alias either privately reserved production home.
            run.sym_value_into(&handles[1], &mut homes.cache).unwrap();
            run.sym_value_into(&handles[1], &mut homes.activation)
                .unwrap();
            run.sym_fset_into(&handles[0], &homes.new, &mut homes.dst)
                .unwrap();
            assert_eq!(
                run.sym_call_begin(&handles[0], &mut cache, &mut activation),
                Err(FunctionError::ActiveActivation)
            );
            assert_eq!(activation.selected_epoch(), selected);
            let old = finish(run, &mut activation, &mut homes.dst);
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            let new = finish(run, &mut activation, &mut homes.dst);
            assert_eq!(format!("({old} {new})"), oracle()[5]);
            assert_eq!(cache.counts_for_test(), (2, 0, 0));
        })
        .expect("selected old definition survives a writer and public operand replacement");
    }
}

struct Stamps;
impl Drop for Stamps {
    fn drop(&mut self) {
        neovm_value::test_support::authority::force_fn_stamps_for_test(None);
    }
}
#[test]
fn p3sym_c4_call_cache_hits_stamp_resync_and_disabled_fallback() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, homes| {
            let _stamps = Stamps;
            neovm_value::test_support::authority::force_fn_stamps_for_test(Some(true));
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(finish(run, &mut activation, &mut homes.dst), oracle()[3]);
            assert_eq!(cache.counts_for_test(), (1, 0, 0));
            run.sym_fset_into(&handles[0], &homes.old, &mut homes.dst)
                .unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(finish(run, &mut activation, &mut homes.dst), oracle()[3]);
            assert_eq!(cache.counts_for_test(), (1, 0, 1));
            let stamp = run.function_facts_for_test(&handles[0]).unwrap().1;
            run.sym_fset_into(&handles[1], &homes.old, &mut homes.dst)
                .unwrap();
            let clock = run.function_facts_for_test(&handles[0]).unwrap().0;
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(cache.observed_epoch(), Some(clock));
            assert_eq!(activation.selected_epoch(), Some(clock));
            assert_eq!(run.function_facts_for_test(&handles[0]).unwrap().1, stamp);
            assert_eq!(
                cache.counts_for_test(),
                (1, 1, 1),
                "unrelated symbol resync avoided a refill"
            );
            let old = finish(run, &mut activation, &mut homes.dst);
            run.sym_fset_into(&handles[0], &homes.new, &mut homes.dst)
                .unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            let new = finish(run, &mut activation, &mut homes.dst);
            assert_eq!(format!("({old} {new})"), oracle()[5]);
            assert_eq!(cache.counts_for_test(), (2, 1, 1));
            neovm_value::test_support::authority::force_fn_stamps_for_test(Some(false));
            run.sym_fset_into(&handles[1], &homes.new, &mut homes.dst)
                .unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(finish(run, &mut activation, &mut homes.dst), new);
            assert_eq!(
                cache.counts_for_test(),
                (3, 1, 1),
                "disabled stamps conservatively refill"
            );
        })
        .expect("real canonical clock/stamp cache witness and cleanup");
    }
}

#[test]
fn p3sym_c4_call_unbind_retains_active_selection_and_void_preserves_answer() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, homes| {
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            run.sym_fmakunbound_into(&handles[0], &mut homes.dst)
                .unwrap();
            let old = finish(run, &mut activation, &mut homes.dst);
            assert_eq!(old, oracle()[3]);
            unsafe { plant_scalar_root_for_test(run, &homes.dst, Value::fixnum(23)) };
            assert_eq!(
                run.sym_call_begin(&handles[0], &mut cache, &mut activation),
                Err(FunctionError::VoidFunction)
            );
            assert_eq!(
                format!(
                    "(void-function {})",
                    crate::emacs_core::print_value(&value_from_raw_bits(
                        homes.dst.word_for_test(run)
                    ))
                ),
                oracle()[13]
            );
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                Err(FunctionError::InactiveActivation)
            );
            run.sym_fset_into(&handles[0], &homes.new, &mut homes.dst)
                .unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            let new = finish(run, &mut activation, &mut homes.dst);
            assert_eq!(format!("({old} {new})"), oracle()[5]);
        })
        .expect("unbinding changes later selections, never an active callee");
    }
}

#[test]
fn p3sym_c4_call_fuel_terminal_state_and_explicit_clear() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, homes| {
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            let before = homes.dst.word_for_test(run);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                Err(FunctionError::InactiveActivation)
            );
            assert_eq!(homes.dst.word_for_test(run), before);
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                Ok(FunctionStep::Yielded)
            );
            assert_eq!(homes.dst.word_for_test(run), before);
            assert_eq!(run.symbol_word_for_test(), 0);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MAX),
                Ok(FunctionStep::Returned)
            );
            assert_eq!(
                crate::emacs_core::print_value(&value_from_raw_bits(homes.dst.word_for_test(run))),
                oracle()[3]
            );
            let answer = homes.dst.word_for_test(run);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MAX),
                Err(FunctionError::InactiveActivation)
            );
            assert_eq!(homes.dst.word_for_test(run), answer);
            cache.clear(run).unwrap();
            activation.clear(run).unwrap();
            assert_eq!(cache.observed_epoch(), None);
            assert_eq!(activation.selected_epoch(), None);
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            assert_eq!(cache.counts_for_test().0, 2);
            activation.clear(run).unwrap();
            assert_eq!(
                run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                Err(FunctionError::InactiveActivation)
            );
            assert_eq!(homes.dst.word_for_test(run), answer);
        })
        .expect("finite instruction budget and explicit lifecycle");
    }
}

#[test]
fn p3sym_c4_call_stale_generations_index_reuse_and_capacity() {
    use crate::tagged::gc::RegistrationError;
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, _| {
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            run.clear_root_slots().unwrap();
            let mut fresh = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            for _ in 1..8 {
                let _ = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            }
            let mut fresh_cache = FunctionCallCache::new(run).unwrap();
            let mut fresh_activation = FunctionActivation::new(run).unwrap();
            let stale = Err(FunctionError::Authority(
                RegistrationError::StaleRegistration,
            ));
            assert_eq!(
                run.sym_call_begin(&handles[0], &mut cache, &mut fresh_activation),
                stale
            );
            assert_eq!(
                run.sym_call_begin(&handles[0], &mut fresh_cache, &mut activation),
                stale
            );
            assert_eq!(cache.clear(run), stale);
            assert_eq!(activation.clear(run), stale);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut fresh, NonZeroU32::MIN),
                Err(FunctionError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            assert!(unsafe { value_from_raw_bits(fresh.word_for_test(run)) }.is_nil());
            run.sym_call_begin(&handles[0], &mut fresh_cache, &mut fresh_activation)
                .unwrap();
            assert_eq!(finish(run, &mut fresh_activation, &mut fresh), oracle()[3]);
            let mut reserved = Vec::new();
            loop {
                let before = run.root_bank_for_test().0;
                match FunctionCallCache::new(run) {
                    Ok(value) => {
                        reserved.push(value);
                        assert!(reserved.len() <= 16);
                    }
                    Err(error) => {
                        assert_eq!(error, FunctionError::Authority(RegistrationError::Capacity));
                        assert_eq!(run.root_bank_for_test().0, before);
                        break;
                    }
                }
            }
        })
        .expect("opaque homes cannot revive after clear or grow the reserved bank");
    }
}

#[test]
fn p3sym_c4_call_empty_unbound_and_unprepared_refuse_cleanly() {
    use crate::tagged::gc::RegistrationError;
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for initial in [InitialFunction::Empty, InitialFunction::Unbound] {
            fixture_state(kind, initial, |run, handles, homes| {
                let mut cache = FunctionCallCache::new(run).unwrap();
                let mut activation = FunctionActivation::new(run).unwrap();
                let answer = homes.dst.word_for_test(run);
                assert_eq!(
                    run.sym_call_begin(&handles[0], &mut cache, &mut activation),
                    Err(FunctionError::VoidFunction)
                );
                assert_eq!(cache.observed_epoch(), None);
                assert_eq!(activation.selected_epoch(), None);
                assert_eq!(homes.dst.word_for_test(run), answer);
                assert_eq!(run.symbol_word_for_test(), 0);
            })
            .expect("void selection never begins an activation");
        }
        fixture_config(
            kind,
            InitialFunction::Bound,
            false,
            |run, handles, homes| {
                let mut cache = FunctionCallCache::new(run).unwrap();
                let mut activation = FunctionActivation::new(run).unwrap();
                let answer = homes.dst.word_for_test(run);
                assert_eq!(
                    run.sym_call_begin(&handles[0], &mut cache, &mut activation),
                    Err(FunctionError::Authority(RegistrationError::Publication))
                );
                assert_eq!(cache.observed_epoch(), None);
                assert_eq!(activation.selected_epoch(), None);
                assert_eq!(homes.dst.word_for_test(run), answer);
            },
        )
        .expect("call admission requires the closed prepared definition set");
    }
}

#[test]
fn p3sym_c4_call_sidecar_poison_refuses_before_registry_poison() {
    use crate::tagged::gc::RegistrationError;
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(kind, |run, handles, homes| {
            let mut cache = FunctionCallCache::new(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                .unwrap();
            let answer = homes.dst.word_for_test(run);
            let selected = activation.selected_epoch();
            let cached = cache.observed_epoch();
            let capacity = run.root_bank_for_test().0;
            // Model the publication interval: only the sidecar is poisoned.
            // No cell write is performed; restore this test flag before asserts
            // so the unchanged real fixture can always release its homes.
            run.set_symbol_poison_for_test(true);
            let step = run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MAX);
            let begin = run.sym_call_begin(&handles[0], &mut cache, &mut activation);
            let cache_clear = cache.clear(run);
            let activation_clear = activation.clear(run);
            let new_cache = FunctionCallCache::new(run).err();
            let new_activation = FunctionActivation::new(run).err();
            let after_capacity = run.root_bank_for_test().0;
            run.set_symbol_poison_for_test(false);
            let poison = FunctionError::Authority(RegistrationError::Poisoned);
            assert_eq!(step, Err(poison));
            assert_eq!(begin, Err(poison));
            assert_eq!(cache_clear, Err(poison));
            assert_eq!(activation_clear, Err(poison));
            assert_eq!(new_cache, Some(poison));
            assert_eq!(new_activation, Some(poison));
            assert_eq!(after_capacity, capacity);
            assert_eq!(activation.selected_epoch(), selected);
            assert_eq!(cache.observed_epoch(), cached);
            assert_eq!(homes.dst.word_for_test(run), answer);
            assert_eq!(finish(run, &mut activation, &mut homes.dst), oracle()[3]);
        })
        .expect("sidecar poison refuses without consuming or changing roots");
    }
}
