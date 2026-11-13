//! Small rooted unary callbacks, with teardown independent of assertions.
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::gc::{
    FunctionActivation, FunctionError, FunctionSelection, FunctionStep, RegistrationError,
    RunAccess, SymbolHandle,
};
use neovm_value::test_support::authority::SymbolWordOccupancy;
use neovm_value::test_support::authority::{Deadline, Retry, rendezvous, retry_owned};
use std::num::{NonZeroU32, NonZeroU64};
use std::ops::ControlFlow;

fn oracle() -> [&'static str; 8] {
    include_str!("oracle.expected")
        .lines()
        .collect::<Vec<_>>()
        .try_into()
        .expect("eight GNU unary rows")
}
fn oracle_scalar(index: usize) -> usize {
    Value::fixnum(oracle()[index].parse().expect("GNU scalar row")).bits()
}
fn oracle_selection(index: usize) -> FunctionSelection {
    match oracle()[index] {
        "t" => FunctionSelection::Selected,
        "nil" => FunctionSelection::Changed,
        other => panic!("unexpected GNU selection {other}"),
    }
}

fn definition(parameter: SymId, answer: i64) -> Value {
    let parameter = Value::cons(Value::from_sym_id(parameter), Value::NIL);
    let body = Value::cons(Value::fixnum(answer), Value::NIL);
    let args = Value::cons(parameter, body);
    Value::cons(Value::from_sym_id(intern("lambda")), args)
}

fn fixture(
    backing: PlainSymbolBacking,
    body: impl for<'era, 'owner> FnOnce(
        &mut RunAccess<'owner, 'era>,
        PreparedUnaryFunction,
        &[SymbolHandle],
    ),
) {
    crate::test_utils::ensure_eval_hooks_installed();
    let parameter = intern("nmjit-n4-ordinary-parameter");
    let replacement_holder = intern("nmjit-n4-unary-replacement");
    let outcome = Context::try_with_test_symbol_backing_init_era(
        backing,
        |obarray, id| {
            let original = definition(parameter, 73);
            let replacement = definition(parameter, 97);
            obarray.seed_value_id(parameter, Value::NIL);
            obarray.seed_value_id(id, original);
            obarray.set_symbol_function_id(id, original);
            obarray.seed_value_id(replacement_holder, replacement);
        },
        |era, id| {
            let symbols: Vec<_> = [id, replacement_holder, parameter]
                .iter()
                .map(|id| era.admit_plain_symbol(*id).unwrap())
                .collect();
            assert!(matches!(
                era.prepare_function_symbol(&symbols[0]),
                Err(FunctionError::UnsupportedDefinition)
            ));
            let callback = era.prepare_unary_function_symbol(&symbols[0]).unwrap();
            let mut owner = era
                .register(1, 8)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut run = owner.run_access().unwrap();
                body(&mut run, callback, &symbols);
            }));
            // All body loans and local capabilities have unwound. Resident
            // roots remain visible until explicitly cleared here.
            let cleanup = (|| {
                owner
                    .run_access()
                    .and_then(|mut run| run.clear_root_slots())?;
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failure| match rendezvous(&failure.reason) {
                        Retry::Busy => ControlFlow::Continue(failure.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failure),
                    },
                )
                .map_err(|_| RegistrationError::Publication)
            })();
            let mut stopped = era.stop().unwrap();
            era.reconcile_functions(&mut stopped.symbols().unwrap())
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era.finish().expect("unary fixture total finish");
            drop(collector);
            drop(keeper);
            eprintln!("NMJIT_N4_UNARY_TEARDOWN_OK {backing:?}");
            assert_eq!(
                cleanup,
                Ok(()),
                "unary fixture cleanup independent of callback"
            );
            if let Err(assertion) = result {
                std::panic::resume_unwind(assertion);
            }
        },
    );
    match outcome {
        Ok(()) => assert!(!cfg!(feature = "gc-memory-telemetry")),
        Err(reason) => {
            assert!(cfg!(feature = "gc-memory-telemetry"));
            assert_eq!(reason, RegistrationError::Publication);
            eprintln!("NMJIT_N4_UNARY_TELEMETRY_REFUSED_CLEAN");
        }
    }
}

#[test]
fn nmjit_n4_unary_noop_and_unwind_have_total_teardown() {
    fixture(PlainSymbolBacking::Unprojected, |_, _, _| {});
    let outcome = std::panic::catch_unwind(|| {
        fixture(PlainSymbolBacking::Unprojected, |run, callback, symbols| {
            let mut expected =
                neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            run.sym_function_into(&symbols[0], &mut expected).unwrap();
            let argument = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            assert_eq!(
                run.sym_unary_call_begin(&callback, &expected, &argument, &mut activation),
                Ok(FunctionSelection::Selected)
            );
            panic!("controlled unary body exit");
        })
    });
    // Telemetry refuses before the body, by its existing typed policy.
    assert_eq!(outcome.is_err(), !cfg!(feature = "gc-memory-telemetry"));
}

#[test]
fn nmjit_n4_unary_roots_old_selection_and_revalidates_replacement() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(backing, |run, callback, symbols| {
            let mut expected =
                neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            let mut replacement =
                neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            let argument = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            let mut result = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
            run.sym_function_into(&symbols[0], &mut expected).unwrap();
            run.sym_value_into(&symbols[1], &mut replacement).unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            // Neither a generic nullary call nor its writer acquires the new
            // unary privilege merely because preparation happened.
            assert_eq!(
                run.sym_named_call_begin(&symbols[0], &expected, &mut activation),
                Err(FunctionError::UnsupportedDefinition)
            );
            assert_eq!(
                run.sym_fset_into(&symbols[0], &replacement, &mut result),
                Err(FunctionError::UnsupportedDefinition)
            );
            assert_eq!(
                run.sym_unary_call_begin(&callback, &expected, &argument, &mut activation),
                Ok(oracle_selection(0))
            );
            let selected_epoch = activation.selected_epoch();
            assert_eq!(
                run.sym_call_step(&mut activation, &mut result, NonZeroU32::new(1).unwrap()),
                Ok(FunctionStep::Yielded)
            );
            run.sym_unary_fset_into(&callback, &replacement, &mut result)
                .unwrap();
            assert_eq!(
                run.sym_call_step(&mut activation, &mut result, NonZeroU32::new(1).unwrap()),
                Ok(FunctionStep::Returned)
            );
            assert_eq!(result.word_for_test(run), oracle_scalar(1));
            assert_eq!(
                run.sym_unary_call_begin(&callback, &expected, &argument, &mut activation),
                Ok(oracle_selection(2))
            );
            assert_eq!(activation.selected_epoch(), selected_epoch);
            assert_eq!(
                run.sym_unary_call_begin(&callback, &replacement, &argument, &mut activation),
                Ok(oracle_selection(3))
            );
            let new_epoch = activation.selected_epoch();
            assert_ne!(new_epoch, selected_epoch);
            assert_eq!(
                run.sym_call_step(&mut activation, &mut result, NonZeroU32::new(2).unwrap()),
                Ok(FunctionStep::Returned)
            );
            assert_eq!(result.word_for_test(run), oracle_scalar(4));
            run.sym_unary_fset_into(&callback, &replacement, &mut result)
                .unwrap();
            assert_eq!(
                run.sym_unary_call_begin(&callback, &replacement, &argument, &mut activation),
                Ok(FunctionSelection::Selected)
            );
            assert_eq!(
                activation.selected_epoch(),
                new_epoch,
                "no-op keeps the clock"
            );
            activation.clear(run).unwrap();
        });
    }
}

#[test]
fn nmjit_n4_unary_refuses_stale_homes_capabilities_and_managed_argument() {
    fixture(PlainSymbolBacking::Unprojected, |run, callback, symbols| {
        let mut expected = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
        run.sym_function_into(&symbols[0], &mut expected).unwrap();
        let argument = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
        let mut activation = FunctionActivation::new(run).unwrap();
        assert_eq!(
            run.sym_unary_call_begin(&callback, &expected, &expected, &mut activation),
            Err(FunctionError::UnsupportedDefinition)
        );
        let mut stale = callback;
        stale
            .key_mut_for_test()
            .set_era_for_test(NonZeroU64::new(callback.symbol().era_for_test().get() + 1).unwrap());
        assert_eq!(
            run.sym_unary_call_begin(&stale, &expected, &argument, &mut activation),
            Err(FunctionError::Authority(
                RegistrationError::StaleRegistration
            ))
        );
        assert_eq!(activation.selected_epoch(), None);
        run.clear_root_slots().unwrap();
        let fresh = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
        let mut new_activation = FunctionActivation::new(run).unwrap();
        assert_eq!(
            run.sym_unary_call_begin(&callback, &expected, &fresh, &mut new_activation),
            Err(FunctionError::Authority(
                RegistrationError::StaleRegistration
            ))
        );
        assert_eq!(new_activation.selected_epoch(), None);
        assert_eq!(fresh.word_for_test(run), Value::NIL.bits());
    });
}

#[test]
fn nmjit_n4_unary_preparation_refuses_special_and_marker_parameters() {
    crate::test_utils::ensure_eval_hooks_installed();
    for parameter_name in [
        "&optional",
        "&rest",
        ":nmjit-parameter",
        "t",
        "nil",
        "nmjit-special-parameter",
    ] {
        let outcome = Context::try_with_test_symbol_backing_init_era(
            PlainSymbolBacking::Unprojected,
            |obarray, id| {
                let parameter = intern(parameter_name);
                obarray.seed_value_id(parameter, Value::NIL);
                if parameter_name == "nmjit-special-parameter" {
                    obarray.make_special_id(parameter);
                }
                obarray.set_symbol_function_id(id, definition(parameter, 73));
            },
            |era, id| {
                let key = era.admit_plain_symbol(id).unwrap();
                let refusal = era.prepare_unary_function_symbol(&key);
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
                assert!(matches!(refusal, Err(FunctionError::UnsupportedDefinition)));
                eprintln!("NMJIT_N4_UNARY_PARAMETER_TEARDOWN_OK");
            },
        );
        match outcome {
            Ok(()) => assert!(!cfg!(feature = "gc-memory-telemetry")),
            Err(reason) => {
                assert!(cfg!(feature = "gc-memory-telemetry"));
                assert_eq!(reason, RegistrationError::Publication);
            }
        }
    }
}

#[test]
fn nmjit_n4_unary_busy_refusal_preserves_roots_and_activation() {
    fixture(PlainSymbolBacking::Unprojected, |run, callback, symbols| {
        let mut expected = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
        run.sym_function_into(&symbols[0], &mut expected).unwrap();
        let argument = neovm_value::test_support::authority::nil_root_for_test(run).unwrap();
        let mut activation = FunctionActivation::new(run).unwrap();
        let writer = run
            .occupy_symbol_word_for_test(SymbolWordOccupancy::Writer)
            .unwrap();
        assert_eq!(
            run.sym_unary_call_begin(&callback, &expected, &argument, &mut activation),
            Err(FunctionError::Authority(RegistrationError::Busy))
        );
        assert_eq!(activation.selected_epoch(), None);
        assert_eq!(argument.word_for_test(run), Value::NIL.bits());
        drop(writer);
        assert_eq!(
            run.sym_unary_call_begin(&callback, &expected, &argument, &mut activation),
            Ok(FunctionSelection::Selected)
        );
        activation.clear(run).unwrap();
    });
}
