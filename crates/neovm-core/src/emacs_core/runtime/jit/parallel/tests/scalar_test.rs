use super::*;
use crate::tagged::gc::{ExecutionTestHomes, with_test_prepared_execution};
use crate::tagged::value::{Fixnum, TaggedValue};

fn with_scalar(
    program: ScalarProgram,
    body: impl for<'era, 'owner> FnOnce(
        &ParallelCompiledCode,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
    ),
) -> Result<(), String> {
    let outcome = std::cell::RefCell::new(Ok(()));
    with_test_prepared_execution(
        |quiescent| ParallelCompiledCode::compile(quiescent, program),
        |compiled, admission, run, destination| match compiled {
            Ok(code) => {
                assert!(cfg!(feature = "jit"));
                body(code, admission, run, destination);
            }
            Err(ParallelCompileError::BackendUnavailable) => {
                assert!(!cfg!(feature = "jit"));
                eprintln!("NMJIT_N4_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            }
            Err(
                error @ (ParallelCompileError::Module(_)
                | ParallelCompileError::Define(_)
                | ParallelCompileError::Finalize(_)
                | ParallelCompileError::UnsupportedPointerWidth
                | ParallelCompileError::AbiLayout),
            ) => {
                *outcome.borrow_mut() = Err(format!("native compilation: {error:?}"));
            }
        },
    );
    outcome.into_inner()
}

pub(crate) fn compiled_entry_manifest() -> Result<(), String> {
    let expected = ImmediateResult::Fixnum(Fixnum::try_from(73).map_err(|e| e.to_string())?);
    with_scalar(
        ScalarProgram::Argument,
        |code, admission, run, destination| {
            code.run(admission, run, destination, expected)
                .expect("real compiled entry");
            assert_eq!(ExecutionTestHomes::read(destination, run), expected);
            eprintln!("NMJIT_N4_COMPILED_ENTRY_NATIVE_RETURN_OK");
        },
    )
}

fn samples() -> [ImmediateResult; 6] {
    [
        ImmediateResult::Nil,
        ImmediateResult::True,
        ImmediateResult::Fixnum(Fixnum::try_from(73).unwrap()),
        ImmediateResult::Fixnum(Fixnum::try_from(-97).unwrap()),
        ImmediateResult::Fixnum(Fixnum::try_from(TaggedValue::MOST_NEGATIVE_FIXNUM).unwrap()),
        ImmediateResult::Fixnum(Fixnum::try_from(TaggedValue::MOST_POSITIVE_FIXNUM).unwrap()),
    ]
}

#[test]
fn nmjit_n4_scalar_constant_returns_in_resident_home() {
    for expected in samples() {
        with_scalar(
            ScalarProgram::Constant(expected),
            |code, admission, run, destination| {
                code.run(admission, run, destination, ImmediateResult::Nil)
                    .unwrap();
                assert_eq!(ExecutionTestHomes::read(destination, run), expected);
            },
        )
        .unwrap();
    }
}

#[test]
fn nmjit_n4_scalar_argument_returns_in_resident_home() {
    with_scalar(
        ScalarProgram::Argument,
        |code, admission, run, destination| {
            for expected in samples() {
                code.run(admission, run, destination, expected).unwrap();
                assert_eq!(ExecutionTestHomes::read(destination, run), expected);
            }
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n4_scalar_stale_home_refuses_before_native_entry() {
    with_scalar(
        ScalarProgram::Constant(ImmediateResult::True),
        |code, admission, run, destination| {
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            assert_eq!(
                code.run(admission, run, destination, ImmediateResult::Nil),
                Err(ParallelEntryError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
            code.run(admission, run, &fresh, ImmediateResult::Nil)
                .unwrap();
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::True);
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n4_scalar_code_uses_fresh_activations() {
    with_scalar(
        ScalarProgram::Argument,
        |code, admission, run, destination| {
            code.run(admission, run, destination, ImmediateResult::True)
                .unwrap();
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            code.run(admission, run, &fresh, ImmediateResult::Nil)
                .unwrap();
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n4_scalar_status_decode_is_total() {
    assert!(matches!(
        NativeStatus::checked(0),
        Ok(NativeStatus::Returned)
    ));
    assert!(matches!(
        NativeStatus::checked(1),
        Ok(NativeStatus::UnsupportedAbi)
    ));
    assert!(matches!(
        NativeStatus::checked(2),
        Ok(NativeStatus::CellReadReady)
    ));
    assert!(matches!(
        NativeStatus::checked(3),
        Ok(NativeStatus::SlotAbsent)
    ));
    for bits in [4, 17, u32::MAX] {
        assert!(matches!(
            NativeStatus::checked(bits),
            Err(ParallelEntryError::MalformedStatus)
        ));
    }
}

#[test]
fn nmjit_n4_scalar_compile_drop_without_entry() {
    // Code prepared with the real exclusive witness, never entered, then freed
    // only after the fixture returns its collector/keeper and finishes the era.
    with_scalar(
        ScalarProgram::Argument,
        |_code, _admission, _run, _destination| (),
    )
    .unwrap();
}

#[test]
fn nmjit_n4_scalar_abi_owner_source_guard_test() {
    let facade = include_str!("../../parallel.rs");
    let source = include_str!("../native.rs");
    assert!(source.contains(".with_activation("));
    assert!(source.contains("ManuallyDrop::take(&mut self.module).free_memory()"));
    assert!(source.contains("&ParallelExecutionAdmission<'era>"));
    assert!(facade.contains("_quiescent: &Gen1Admission<'_>"));
    assert!(facade.contains("Err(ParallelCompileError::BackendUnavailable)"));
    for forbidden in [
        "crate::emacs_core::eval",
        "cache::",
        "thread_local!",
        "generational_enabled",
        "unwrap(",
        "expect(",
    ] {
        assert!(
            !source.contains(forbidden),
            "scalar entry source contains {forbidden}"
        );
    }
}

fn assert_slot_probe(index: usize) {
    let slot = TableSlotIndex::new(index);
    with_scalar(
        ScalarProgram::HasSlotBacking(slot),
        |code, admission, run, home| {
            let covered = admission
                .with_activation(run, home, |activation| {
                    let view = activation.table_view();
                    let table = view.table();
                    index < table.slots().get()
                        && table
                            .chunk((index / table.chunk_slots().get()).into())
                            .is_some()
                })
                .unwrap();
            code.run(admission, run, home, ImmediateResult::Nil)
                .unwrap();
            assert_eq!(
                ExecutionTestHomes::read(home, run),
                if covered {
                    ImmediateResult::True
                } else {
                    ImmediateResult::Nil
                }
            );
            #[cfg(feature = "jit")]
            {
                let ir = &code.native.emission;
                assert_eq!(
                    ir.matches("atomic_load").count(),
                    1,
                    "one descriptor identity: {ir}"
                );
                assert!(ir.contains("udiv"), "bounded chunk lookup: {ir}");
                assert!(
                    !ir.contains("call "),
                    "closed probe has no helper call: {ir}"
                );
            }
            eprintln!("NMJIT_N4_NATIVE_DESCRIPTOR_SLOT_OK {index} {covered}");
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n4_native_descriptor_first_and_chunk_boundary() {
    // Use the backing's own chunk-size definition, then compare with the
    // actual immutable table rather than assuming the fixture's chunk count.
    let chunk = crate::emacs_core::symbol::OBARRAY_CHUNK_SLOTS;
    for index in [0, 1, chunk - 1, chunk, chunk + 1, chunk * 2 - 1, chunk * 2] {
        assert_slot_probe(index);
    }
}

#[test]
fn nmjit_n4_native_descriptor_large_slot_is_a_total_miss() {
    for index in [usize::MAX, usize::MAX - 1, usize::MAX / 2] {
        assert_slot_probe(index);
    }
}

#[test]
fn nmjit_n4_native_descriptor_source_guard_test() {
    let native = include_str!("../native.rs");
    let execution = include_str!("../../../../../tagged/gc/mutator_registration/execution.rs");
    assert_eq!(native.matches(".atomic_load(").count(), 1);
    assert!(native.contains("activation.publication()"));
    assert!(execution.contains("pub(crate) fn publication(&self) -> PublicationView<'_>"));
    assert!(!native.contains("spine_addr"));
    assert!(!native.contains("initial_symbols"));
}
