//! Actual canonical builtins and reader-held private callback root transfer.
use super::*;
use crate::emacs_core::eval::{Context, SubrEntry, register_global_subr_entry};
use crate::emacs_core::intern::intern;
use crate::tagged::gc::mutator_registration::symbol_functions::retry::{
    self as contention, Deadline, Retry, retry_owned,
};
use crate::tagged::header::{SubrDispatchKind, SubrFn, SubrFn2};
use std::num::NonZeroU32;
use std::ops::ControlFlow;

fn oracle_selection(row: usize) -> FunctionSelection {
    let rows: [&str; 6] = include_str!("oracle.expected")
        .lines()
        .collect::<Vec<_>>()
        .try_into()
        .expect("six GNU HOF guard rows");
    match rows[row] {
        "t" => FunctionSelection::Selected,
        "nil" => FunctionSelection::Changed,
        other => panic!("unexpected GNU HOF guard row {other}"),
    }
}
fn old_new_answer(row: usize) -> usize {
    let rows: [&str; 8] = include_str!("../unary/oracle.expected")
        .lines()
        .collect::<Vec<_>>()
        .try_into()
        .expect("eight GNU callback rows");
    Value::fixnum(rows[row].parse().expect("GNU scalar callback row")).bits()
}

fn spelling(operation: HofOperation) -> &'static str {
    match operation {
        HofOperation::Mapc => "mapc",
        HofOperation::Mapcar => "mapcar",
    }
}
fn builtin(operation: HofOperation) -> SubrEntry {
    let body: SubrFn2 = match operation {
        HofOperation::Mapc => crate::emacs_core::builtins::builtin_mapc_2,
        HofOperation::Mapcar => crate::emacs_core::builtins::builtin_mapcar_2,
    };
    // The same real bodies and fixed arity registered by builtin startup, not
    // a fake seed/current-cell-derived expectation. No Context bootstrap/GC.
    SubrEntry {
        function: Some(SubrFn::A2(body)),
        min_args: 2,
        max_args: Some(2),
        dispatch_kind: SubrDispatchKind::Builtin,
        interactive_spec: None,
    }
}
fn definition(parameters: Value, answer: i64) -> Value {
    let body = Value::cons(Value::fixnum(answer), Value::NIL);
    let args = Value::cons(parameters, body);
    Value::cons(Value::from_sym_id(intern("lambda")), args)
}
fn unary(parameter: SymId, answer: i64) -> Value {
    definition(
        Value::cons(Value::from_sym_id(parameter), Value::NIL),
        answer,
    )
}

struct Inputs {
    callback: SymbolHandle,
    replacement: SymbolHandle,
    twin: SymbolHandle,
    mapper_replacement: SymbolHandle,
}

fn fixture(
    operation: HofOperation,
    backing: PlainSymbolBacking,
    body: impl for<'owner, 'era> FnOnce(
        &mut RunAccess<'owner, 'era>,
        PreparedHofBuiltin,
        PreparedUnaryFunction,
        Inputs,
    ),
) {
    for kind in [HofOperation::Mapc, HofOperation::Mapcar] {
        register_global_subr_entry(intern(spelling(kind)), builtin(kind));
    }
    let mapper_id = intern(spelling(operation));
    let canonical = Value::subr_from_sym_id(mapper_id);
    let parameter = intern("nmjit-hof-ordinary-parameter");
    let replacement = intern("nmjit-hof-replacement");
    let twin = intern("nmjit-hof-twin");
    let mapper_replacement = intern("nmjit-hof-mapper-replacement");
    let outcome = Context::try_with_test_symbol_backing_init_era(
        backing,
        |obarray, id| {
            obarray.seed_value_id(mapper_id, Value::NIL);
            obarray.set_symbol_function_id(mapper_id, canonical);
            obarray.seed_value_id(parameter, Value::NIL);
            let original = unary(parameter, 73);
            obarray.seed_value_id(id, original);
            obarray.set_symbol_function_id(id, original);
            obarray.seed_value_id(replacement, unary(parameter, 97));
            obarray.seed_value_id(twin, unary(parameter, 73));
            obarray.seed_value_id(mapper_replacement, definition(Value::NIL, 123));
        },
        |era, id| {
            let mapper_key = era.admit_plain_symbol(mapper_id).unwrap();
            let callback_key = era.admit_plain_symbol(id).unwrap();
            let callback = era.prepare_unary_function_symbol(&callback_key).unwrap();
            // Neither a shape nor a caller-supplied current value establishes
            // the mapper semantics: a fixed-op seed must match the handle.
            let wrong_operation = match operation {
                HofOperation::Mapc => HofOperation::Mapcar,
                HofOperation::Mapcar => HofOperation::Mapc,
            };
            // Both real registrations were installed before the era. Only
            // the matching canonical cell/id can mint this requested receipt.
            assert!(matches!(
                era.prepare_hof_builtin(wrong_operation, &mapper_key),
                Err(FunctionError::UnsupportedDefinition)
            ));
            assert!(matches!(
                era.prepare_function_symbol(&mapper_key),
                Err(FunctionError::UnsupportedDefinition)
            ));
            let mapper = era.prepare_hof_builtin(operation, &mapper_key).unwrap();
            let inputs = Inputs {
                callback: callback_key,
                replacement: era.admit_plain_symbol(replacement).unwrap(),
                twin: era.admit_plain_symbol(twin).unwrap(),
                mapper_replacement: era.admit_plain_symbol(mapper_replacement).unwrap(),
            };
            let mut owner = era
                .register(1, 10)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut run = owner.run_access().unwrap();
                body(&mut run, mapper, callback, inputs);
            }));
            let cleanup = (|| {
                owner
                    .run_access()
                    .and_then(|mut run| run.clear_root_slots())?;
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failure| match contention::rendezvous(&failure.reason) {
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
            let (collector, keeper) = era.finish().expect("HOF fixture total finish");
            drop(collector);
            drop(keeper);
            eprintln!("NMJIT_N4_HOF_GUARD_TEARDOWN_OK {operation:?} {backing:?}");
            assert_eq!(cleanup, Ok(()));
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
        }
    }
}
fn finish<'era>(
    run: &mut RunAccess<'_, 'era>,
    activation: &mut FunctionActivation<'era>,
    destination: &mut OwnerRootSlot<'era>,
    expected: usize,
) {
    assert_eq!(
        run.sym_call_step(activation, destination, NonZeroU32::new(2).unwrap()),
        Ok(FunctionStep::Returned)
    );
    assert_eq!(destination.word_for_test(run), expected);
}

#[test]
fn nmjit_n4_hof_guard_noop_and_unwind_finish_cleanly() {
    fixture(
        HofOperation::Mapc,
        PlainSymbolBacking::Unprojected,
        |_, _, _, _| {},
    );
    let result = std::panic::catch_unwind(|| {
        fixture(
            HofOperation::Mapcar,
            PlainSymbolBacking::Unprojected,
            |run, mapper, callback, inputs| {
                let mut expected = run.private_nil_root().unwrap();
                run.sym_function_into(&inputs.callback, &mut expected)
                    .unwrap();
                let argument = run.private_nil_root().unwrap();
                let mut activation = FunctionActivation::new(run).unwrap();
                assert_eq!(
                    run.sym_hof_call_begin(
                        &mapper,
                        &callback,
                        &expected,
                        &argument,
                        &mut activation
                    ),
                    Ok(FunctionSelection::Selected)
                );
                panic!("controlled HOF guard exit");
            },
        )
    });
    assert_eq!(result.is_err(), !cfg!(feature = "gc-memory-telemetry"));
}

#[test]
fn nmjit_n4_hof_guard_roots_old_callback_then_observes_replacement() {
    for operation in [HofOperation::Mapc, HofOperation::Mapcar] {
        for backing in [
            PlainSymbolBacking::Unprojected,
            PlainSymbolBacking::Projected,
        ] {
            fixture(operation, backing, |run, mapper, callback, inputs| {
                assert_eq!(mapper.operation(), operation);
                let mut expected = run.private_nil_root().unwrap();
                run.sym_function_into(&inputs.callback, &mut expected)
                    .unwrap();
                let mut replacement = run.private_nil_root().unwrap();
                run.sym_value_into(&inputs.replacement, &mut replacement)
                    .unwrap();
                let mut twin = run.private_nil_root().unwrap();
                run.sym_value_into(&inputs.twin, &mut twin).unwrap();
                let argument = run.private_nil_root().unwrap();
                let mut dst = run.private_nil_root().unwrap();
                let mut activation = FunctionActivation::new(run).unwrap();
                assert_eq!(
                    run.sym_hof_call_begin(&mapper, &callback, &twin, &argument, &mut activation),
                    Ok(FunctionSelection::Changed)
                );
                assert_eq!(activation.selected_epoch(), None);
                let registry_reads = crate::emacs_core::eval::global_subr_lookup_count();
                assert_eq!(
                    run.sym_hof_call_begin(
                        &mapper,
                        &callback,
                        &expected,
                        &argument,
                        &mut activation
                    ),
                    Ok(FunctionSelection::Selected)
                );
                assert_eq!(
                    crate::emacs_core::eval::global_subr_lookup_count(),
                    registry_reads,
                    "selection performs no TLS registry lookup"
                );
                let original_clock = activation.selected_epoch();
                run.sym_unary_fset_into(&callback, &replacement, &mut dst)
                    .unwrap();
                finish(run, &mut activation, &mut dst, old_new_answer(1));
                assert_eq!(
                    run.sym_hof_call_begin(
                        &mapper,
                        &callback,
                        &expected,
                        &argument,
                        &mut activation
                    ),
                    Ok(FunctionSelection::Changed)
                );
                assert_eq!(
                    run.sym_hof_call_begin(
                        &mapper,
                        &callback,
                        &replacement,
                        &argument,
                        &mut activation
                    ),
                    Ok(FunctionSelection::Selected)
                );
                assert_ne!(activation.selected_epoch(), original_clock);
                finish(run, &mut activation, &mut dst, old_new_answer(4));
                eprintln!("NMJIT_N4_HOF_GUARD_ONE_READER_OLD_NEW_OK {operation:?} {backing:?}");
            });
        }
    }
}

#[test]
fn nmjit_n4_hof_guard_refuses_redefined_and_unbound_mapper() {
    fixture(
        HofOperation::Mapcar,
        PlainSymbolBacking::Unprojected,
        |run, mapper, callback, inputs| {
            let mut expected = run.private_nil_root().unwrap();
            run.sym_function_into(&inputs.callback, &mut expected)
                .unwrap();
            let mut mapper_body = run.private_nil_root().unwrap();
            run.sym_value_into(&inputs.mapper_replacement, &mut mapper_body)
                .unwrap();
            let argument = run.private_nil_root().unwrap();
            let mut dst = run.private_nil_root().unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            assert_eq!(
                run.sym_hof_call_begin(&mapper, &callback, &expected, &argument, &mut activation),
                Ok(oracle_selection(3))
            );
            run.sym_fset_into(mapper.symbol(), &mapper_body, &mut dst)
                .unwrap();
            finish(run, &mut activation, &mut dst, old_new_answer(1));
            let completed_epoch = activation.selected_epoch();
            assert_eq!(
                run.sym_hof_call_begin(&mapper, &callback, &expected, &argument, &mut activation),
                Ok(oracle_selection(4))
            );
            assert_eq!(activation.selected_epoch(), completed_epoch);
            run.sym_fmakunbound_into(mapper.symbol(), &mut dst).unwrap();
            assert_eq!(
                run.sym_hof_call_begin(&mapper, &callback, &expected, &argument, &mut activation),
                Ok(oracle_selection(5))
            );
            assert_eq!(activation.selected_epoch(), completed_epoch);
        },
    );
}

#[test]
fn nmjit_n4_hof_guard_refuses_stale_homes_and_mapper_receipts() {
    fixture(
        HofOperation::Mapc,
        PlainSymbolBacking::Unprojected,
        |run, mapper, callback, inputs| {
            let mut expected = run.private_nil_root().unwrap();
            run.sym_function_into(&inputs.callback, &mut expected)
                .unwrap();
            let argument = run.private_nil_root().unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            let mut stale = mapper;
            stale.key.era = NonZeroU64::new(mapper.key.era.get() + 1).unwrap();
            assert_eq!(
                run.sym_hof_call_begin(&stale, &callback, &expected, &argument, &mut activation),
                Err(FunctionError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            assert_eq!(activation.selected_epoch(), None);
            run.clear_root_slots().unwrap();
            let fresh = run.private_nil_root().unwrap();
            let mut fresh_activation = FunctionActivation::new(run).unwrap();
            assert_eq!(
                run.sym_hof_call_begin(
                    &mapper,
                    &callback,
                    &expected,
                    &fresh,
                    &mut fresh_activation
                ),
                Err(FunctionError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            assert_eq!(fresh.word_for_test(run), Value::NIL.bits());
        },
    );
}

#[test]
fn nmjit_n4_hof_guard_native_registration_invalidates_receipt() {
    fixture(
        HofOperation::Mapcar,
        PlainSymbolBacking::Unprojected,
        |run, mapper, callback, inputs| {
            let mut expected = run.private_nil_root().unwrap();
            run.sym_function_into(&inputs.callback, &mut expected)
                .unwrap();
            let argument = run.private_nil_root().unwrap();
            let mut activation = FunctionActivation::new(run).unwrap();
            assert_eq!(
                run.sym_hof_call_begin(&mapper, &callback, &expected, &argument, &mut activation),
                Ok(FunctionSelection::Selected)
            );
            activation.clear(run).unwrap();
            // Even a registration of the same body advances the existing native
            // publication clock. Never inspect entry metadata on this run path.
            register_global_subr_entry(mapper.key.symbol, builtin(HofOperation::Mapcar));
            assert_eq!(
                run.sym_hof_call_begin(&mapper, &callback, &expected, &argument, &mut activation),
                Ok(FunctionSelection::Changed)
            );
            assert_eq!(activation.selected_epoch(), None);
            eprintln!("NMJIT_N4_HOF_STALE_NATIVE_REGISTRATION_REFUSED");
        },
    );
}

#[test]
fn nmjit_n4_hof_guard_source_guard_test() {
    let source = include_str!("../hof.rs");
    let running = source.split_once("impl<'era> RunAccess").unwrap().1;
    assert_eq!(
        running.matches(".symbol_ref(").count(),
        1,
        "one reader covers both identities"
    );
    assert!(running.contains("self.select_named(expected, activation, shape)"));
    for excluded in [
        "Context",
        "thread_local!",
        "lookup_interned",
        "subr_from_sym_id",
        "spine_addr",
        "unwrap(",
        "expect(",
    ] {
        assert!(
            !running.contains(excluded),
            "running HOF leaf contains {excluded}"
        );
    }
    assert_eq!(running.matches(".registration_clock.snapshot()").count(), 1);
    let clock = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/tagged/gc/function_write_hooks.rs"
    ));
    assert!(clock.contains("self.0.load(Ordering::Acquire)"));
    assert!(
        !clock.contains("AtomicU64::new"),
        "the observer cannot duplicate the publisher"
    );
    assert!(
        running.find(".registration_clock.snapshot()").unwrap()
            < running.find(".read_function(").unwrap()
    );
}

#[test]
fn nmjit_n4_hof_preparation_refuses_re_registered_implementation() {
    #[derive(Clone, Copy)]
    enum ChangedRegistration {
        Body,
        Arity,
        Dispatch,
    }
    for changed in [
        ChangedRegistration::Body,
        ChangedRegistration::Arity,
        ChangedRegistration::Dispatch,
    ] {
        let id = intern("mapc");
        intern("lambda");
        register_global_subr_entry(id, builtin(HofOperation::Mapc));
        let canonical = Value::subr_from_sym_id(id);
        struct Restore(SymId, SubrEntry);
        impl Drop for Restore {
            fn drop(&mut self) {
                register_global_subr_entry(self.0, self.1);
            }
        }
        let _restore = Restore(id, builtin(HofOperation::Mapc));
        let mut registration = builtin(HofOperation::Mapc);
        match changed {
            ChangedRegistration::Body => {
                registration.function = builtin(HofOperation::Mapcar).function
            }
            ChangedRegistration::Arity => registration.min_args = 1,
            ChangedRegistration::Dispatch => {
                registration.dispatch_kind = SubrDispatchKind::ContextCallable
            }
        }
        register_global_subr_entry(id, registration);
        let outcome = Context::try_with_test_symbol_backing_init_era(
            PlainSymbolBacking::Unprojected,
            |obarray, _| {
                obarray.seed_value_id(id, Value::NIL);
                obarray.set_symbol_function_id(id, canonical);
            },
            |era, _| {
                let key = era.admit_plain_symbol(id).unwrap();
                let result = era.prepare_hof_builtin(HofOperation::Mapc, &key);
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
                assert!(
                    matches!(result, Err(FunctionError::UnsupportedDefinition)),
                    "identity alone cannot authorize a re-registered mapper"
                );
                eprintln!("NMJIT_N4_HOF_CHANGED_REGISTRATION_PREPARATION_REFUSED");
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
