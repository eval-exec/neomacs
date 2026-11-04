//! Real compiled manifest; Rust ABI controls remain separate Miri evidence.
use super::*;
use neovm_value::tagged::gc::RunAccessUnaryExt;
use neovm_value::tagged::gc::{
    ExecutionFixtureReceipt, ExecutionTestHomes, HofTestInputs, PlainSymbolBacking,
    with_test_prepared_hof_execution,
};
use neovm_value::tagged::value::TaggedValue;

fn oracle() -> [&'static str; 6] {
    include_str!("../../tests/hof_oracle.expected")
        .lines()
        .collect::<Vec<_>>()
        .try_into()
        .expect("six refreshed GNU mapping rows")
}
fn list(word: usize) -> String {
    let value = TaggedValue::from_bits(word);
    assert!(value.is_cons());
    assert!(value.cons_cdr_unobserved().is_nil());
    format!(
        "({})",
        value
            .cons_car_unobserved()
            .as_fixnum()
            .expect("one scalar answer")
    )
}
fn assert_answer(operation: HofOperation, row: usize, input: usize, result: usize) {
    match operation {
        HofOperation::Mapc => {
            assert_eq!(oracle()[1], if result == input { "t" } else { "nil" });
            assert_eq!(list(result), oracle()[5]);
        }
        HofOperation::Mapcar => {
            assert_eq!(list(result), oracle()[row]);
            assert_eq!(oracle()[2], if result == input { "t" } else { "nil" });
        }
    }
}
fn history<'era>(
    operation: HofOperation,
    run: &mut RunAccess<'_, 'era>,
    destination: &OwnerRootSlot<'era>,
    mut inputs: HofTestInputs<'era>,
    mut query: impl FnMut(
        &mut RunAccess<'_, 'era>,
        &OwnerRootSlot<'era>,
        &mut HofTestInputs<'era>,
        &OwnerRootSlot<'era>,
    ) -> Result<FunctionSelection, ParallelEntryError>,
) {
    let original = inputs.expected.clone();
    let replacement = inputs.replacement.clone();
    let input = inputs.singleton.word_for_test(run);
    assert_eq!(
        query(run, destination, &mut inputs, &original),
        Ok(FunctionSelection::Selected)
    );
    let result = ExecutionTestHomes::word(destination, run);
    assert_answer(operation, 0, input, result);
    assert_eq!(
        query(run, destination, &mut inputs, &original),
        Err(ParallelEntryError::Hof(HofError::ConsumedInput))
    );
    assert_eq!(ExecutionTestHomes::word(destination, run), result);
    run.sym_unary_fset_into(&inputs.callback, &replacement, &mut destination.clone())
        .unwrap();
    inputs.singleton = HofSingleton::new(
        run,
        ImmediateResult::from_word(TaggedValue::fixnum(7).bits()).unwrap(),
    )
    .unwrap();
    let before = ExecutionTestHomes::word(destination, run);
    assert_eq!(
        query(run, destination, &mut inputs, &original),
        Ok(FunctionSelection::Changed)
    );
    assert_eq!(ExecutionTestHomes::word(destination, run), before);
    let input = inputs.singleton.word_for_test(run);
    assert_eq!(
        query(run, destination, &mut inputs, &replacement),
        Ok(FunctionSelection::Selected)
    );
    assert_answer(
        operation,
        3,
        input,
        ExecutionTestHomes::word(destination, run),
    );
}
fn with_native(
    backing: PlainSymbolBacking,
    operation: HofOperation,
    body: impl for<'era, 'owner> FnOnce(
        &ParallelCompiledHofCode,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        HofTestInputs<'era>,
    ),
) -> Result<(), String> {
    let result = std::cell::RefCell::new(Ok(()));
    with_test_prepared_hof_execution(
        backing,
        operation,
        |admission| ParallelCompiledHofCode::compile(admission, operation),
        |prepared, admission, run, home, inputs| match prepared {
            Ok(code) => {
                assert!(cfg!(feature = "jit"));
                body(code, admission, run, home, inputs);
            }
            Err(ParallelCompileError::BackendUnavailable) => {
                assert!(!cfg!(feature = "jit"));
                eprintln!("NMJIT_N4_HOF_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            }
            Err(
                error @ (ParallelCompileError::Module(_)
                | ParallelCompileError::Define(_)
                | ParallelCompileError::Finalize(_)
                | ParallelCompileError::UnsupportedPointerWidth
                | ParallelCompileError::AbiLayout),
            ) => {
                *result.borrow_mut() = Err(format!("HOF compilation: {error:?}"));
            }
        },
    );
    result.into_inner()
}

pub(crate) fn manifest() -> Result<(), String> {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for operation in [HofOperation::Mapc, HofOperation::Mapcar] {
            with_native(backing, operation, |code, admission, run, home, inputs| {
                history(
                    operation,
                    run,
                    home,
                    inputs,
                    |run, home, inputs, expected| {
                        code.run(
                            admission,
                            run,
                            home,
                            &inputs.mapper,
                            &inputs.callback,
                            expected,
                            &mut inputs.singleton,
                            &mut inputs.scratch,
                        )
                    },
                );
                #[cfg(feature = "jit")]
                {
                    let ir = &code.native.emission;
                    assert_eq!(ir.matches("atomic_load").count(), 1);
                    assert_eq!(ir.matches("udiv").count(), 2);
                    assert!(!ir.contains("call "));
                    assert!(!ir.contains("call_indirect"));
                }
                eprintln!("NMJIT_N4_HOF_NATIVE_GNU_OK {operation:?} {backing:?}");
            })?;
        }
    }
    Ok(())
}
#[test]
fn nmjit_n4_native_hof_maps_singleton_and_revalidates_callback() {
    manifest().unwrap();
}

#[test]
fn nmjit_n4_native_hof_refuses_redefined_unbound_mapper() {
    for operation in [HofOperation::Mapc, HofOperation::Mapcar] {
        with_native(
            PlainSymbolBacking::Unprojected,
            operation,
            |code, admission, run, home, mut inputs| {
                run.sym_fset_into(
                    inputs.mapper.symbol(),
                    &inputs.mapper_replacement,
                    &mut home.clone(),
                )
                .unwrap();
                let before = ExecutionTestHomes::word(home, run);
                assert_eq!(
                    code.run(
                        admission,
                        run,
                        home,
                        &inputs.mapper,
                        &inputs.callback,
                        &inputs.expected,
                        &mut inputs.singleton,
                        &mut inputs.scratch
                    ),
                    Ok(FunctionSelection::Changed)
                );
                assert_eq!(ExecutionTestHomes::word(home, run), before);
                run.sym_fmakunbound_into(inputs.mapper.symbol(), &mut home.clone())
                    .unwrap();
                let before = ExecutionTestHomes::word(home, run);
                assert_eq!(
                    code.run(
                        admission,
                        run,
                        home,
                        &inputs.mapper,
                        &inputs.callback,
                        &inputs.expected,
                        &mut inputs.singleton,
                        &mut inputs.scratch
                    ),
                    Ok(FunctionSelection::Changed)
                );
                assert_eq!(ExecutionTestHomes::word(home, run), before);
            },
        )
        .unwrap();
    }
}
#[test]
fn nmjit_n4_native_hof_refuses_stale_homes() {
    with_native(
        PlainSymbolBacking::Unprojected,
        HofOperation::Mapcar,
        |code, admission, run, _home, mut inputs| {
            run.clear_root_slots().unwrap();
            let home = ExecutionTestHomes::nil(run).unwrap();
            assert_eq!(
                code.run(
                    admission,
                    run,
                    &home,
                    &inputs.mapper,
                    &inputs.callback,
                    &inputs.expected,
                    &mut inputs.singleton,
                    &mut inputs.scratch
                ),
                Err(ParallelEntryError::Hof(HofError::Authority(
                    RegistrationError::StaleRegistration
                )))
            );
            assert_eq!(ExecutionTestHomes::read(&home, run), ImmediateResult::Nil);
        },
    )
    .unwrap();
}

/// # Safety
/// Pointers name the initialized ParallelRuntime and HofInput retained by
/// lookup. The live activation retains this publication through return.
unsafe extern "C" fn paired_lookup(
    runtime: *const ParallelRuntime<'_>,
    _activation: *mut c_void,
    input: *const c_void,
    _output: *mut ScalarOutput,
) -> u32 {
    // SAFETY: lookup supplies the declared live initialized ABI objects.
    let (runtime, input) = unsafe { (&*runtime, &*input.cast::<HofInput>()) };
    if runtime.version.0 != NativeAbiVersion::V1.0 {
        return NativeStatus::UnsupportedAbi as u32;
    }
    let view = runtime.publication.load();
    let table = view.table();
    for slot in [input.mapper, input.callback] {
        let index = slot.get();
        if index >= table.slots().get()
            || table
                .chunk((index / table.chunk_slots().get()).into())
                .is_none()
        {
            return NativeStatus::SlotAbsent as u32;
        }
    }
    NativeStatus::CellReadReady as u32
}
fn rust_query<'era>(
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut HofTestInputs<'era>,
    expected: &OwnerRootSlot<'era>,
) -> Result<FunctionSelection, ParallelEntryError> {
    admission
        .with_activation(run, home, |activation| {
            // SAFETY: this Rust test entry obeys the exact paired read-only ABI;
            // it is not a replacement for native execution in the feature manifest.
            unsafe {
                lookup(
                    paired_lookup,
                    activation,
                    inputs.mapper.symbol(),
                    inputs.callback.symbol(),
                )
            }?;
            activation
                .map_singleton(
                    &inputs.mapper,
                    &inputs.callback,
                    expected,
                    &mut inputs.singleton,
                    &mut inputs.scratch,
                )
                .map_err(ParallelEntryError::Hof)
        })
        .map_err(ParallelEntryError::Authority)?
}
#[test]
fn nmjit_n4_hof_abi_roots_mapping_results_after_paired_lookup() {
    for operation in [HofOperation::Mapc, HofOperation::Mapcar] {
        with_test_prepared_hof_execution(
            PlainSymbolBacking::Projected,
            operation,
            |_| (),
            |(), admission, run, home, inputs| {
                history(
                    operation,
                    run,
                    home,
                    inputs,
                    |run, home, inputs, expected| {
                        rust_query(admission, run, home, inputs, expected)
                    },
                );
            },
        );
    }
}
#[test]
fn nmjit_n4_hof_abi_published_list_mutation_cannot_reuse_captured_element() {
    with_test_prepared_hof_execution(
        PlainSymbolBacking::Unprojected,
        HofOperation::Mapc,
        |_| (),
        |(), admission, run, home, mut inputs| {
            let expected = inputs.expected.clone();
            assert_eq!(
                rust_query(admission, run, home, &mut inputs, &expected),
                Ok(FunctionSelection::Selected)
            );
            // After mapc the caller can mutate the exposed original list. Make
            // both its head and tail unsuitable for the captured singleton
            // assumption. A second entry MUST refuse before trusting it.
            run.with_slice(|slice| {
                let list = slice.bind(home).unwrap();
                let replacement = slice.bind(&inputs.replacement).unwrap();
                slice.store_car(&list, &replacement).unwrap();
                slice.store_cdr(&list, &list).unwrap();
            })
            .unwrap();
            let before = ExecutionTestHomes::word(home, run);
            assert_eq!(
                rust_query(admission, run, home, &mut inputs, &expected),
                Err(ParallelEntryError::Hof(HofError::ConsumedInput))
            );
            assert_eq!(ExecutionTestHomes::word(home, run), before);
            eprintln!("NMJIT_N4_HOF_PUBLISHED_INPUT_REUSE_REFUSED");
        },
    );
}
#[test]
fn nmjit_n4_hof_fixture_noop_and_unwind_teardown() {
    let receipt = with_test_prepared_hof_execution(
        PlainSymbolBacking::Unprojected,
        HofOperation::Mapcar,
        |_| (),
        |(), _, _, _, _| (),
    );
    let result = std::panic::catch_unwind(|| {
        with_test_prepared_hof_execution(
            PlainSymbolBacking::Unprojected,
            HofOperation::Mapcar,
            |_| (),
            |(), _, _, _, _| panic!("NMJIT_N4_HOF_BODY_CONTROL"),
        );
    });
    match receipt {
        ExecutionFixtureReceipt::Gen1Completed => assert!(result.is_err()),
        ExecutionFixtureReceipt::Gen0Refused | ExecutionFixtureReceipt::TelemetryRefused => {
            assert!(result.is_ok())
        }
    }
}
#[test]
fn nmjit_n4_hof_scope_source_guard_test() {
    let source = include_str!("../../hof.rs");
    let leaf = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-value/src/tagged/gc/mutator_registration/execution/hof.rs"
    ));
    for forbidden in [
        "Context",
        "thread_local!",
        "unwrap(",
        "expect(",
        "spine_addr",
    ] {
        assert!(!source.contains(forbidden));
        assert!(!leaf.contains(forbidden));
    }
    assert!(source.find(".lookup_hof(").unwrap() < source.find(".map_singleton(").unwrap());
    assert!(leaf.contains("input.state = SingletonState::Consumed"));
    assert!(
        leaf.find("input.state = SingletonState::Consumed").unwrap()
            < leaf.find("*destination = MarkWord::of(result)").unwrap()
    );
}
