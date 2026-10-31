use super::*;
use crate::tagged::gc::{ExecutionTestHomes, with_test_prepared_execution};
use crate::tagged::value::{Fixnum, TaggedValue};

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
        with_test_prepared_execution(
            |quiescent| {
                ParallelCompiledCode::compile(quiescent, ScalarProgram::Constant(expected)).unwrap()
            },
            |code, admission, run, destination| {
                code.run(admission, run, destination, ImmediateResult::Nil)
                    .unwrap();
                assert_eq!(ExecutionTestHomes::read(destination, run), expected);
            },
        );
    }
}

#[test]
fn nmjit_n4_scalar_argument_returns_in_resident_home() {
    with_test_prepared_execution(
        |quiescent| ParallelCompiledCode::compile(quiescent, ScalarProgram::Argument).unwrap(),
        |code, admission, run, destination| {
            for expected in samples() {
                code.run(admission, run, destination, expected).unwrap();
                assert_eq!(ExecutionTestHomes::read(destination, run), expected);
            }
        },
    );
}

#[test]
fn nmjit_n4_scalar_stale_home_refuses_before_native_entry() {
    with_test_prepared_execution(
        |quiescent| {
            ParallelCompiledCode::compile(quiescent, ScalarProgram::Constant(ImmediateResult::True))
                .unwrap()
        },
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
    );
}

#[test]
fn nmjit_n4_scalar_code_uses_fresh_activations() {
    with_test_prepared_execution(
        |quiescent| ParallelCompiledCode::compile(quiescent, ScalarProgram::Argument).unwrap(),
        |code, admission, run, destination| {
            code.run(admission, run, destination, ImmediateResult::True)
                .unwrap();
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            code.run(admission, run, &fresh, ImmediateResult::Nil)
                .unwrap();
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
        },
    );
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
    for bits in [2, 17, u32::MAX] {
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
    with_test_prepared_execution(
        |quiescent| ParallelCompiledCode::compile(quiescent, ScalarProgram::Argument).unwrap(),
        |_code, _admission, _run, _destination| (),
    );
}

#[test]
fn nmjit_n4_scalar_abi_owner_source_guard_test() {
    let source = include_str!("../../parallel.rs");
    assert!(source.contains(".with_activation("));
    assert!(source.contains("ManuallyDrop::take(&mut self.module).free_memory()"));
    assert!(source.contains("&ParallelExecutionAdmission<'era>"));
    assert!(source.contains("_quiescent: &Gen1Admission<'_>"));
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
