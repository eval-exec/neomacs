use super::super::tests::{FixtureSeed, prepared_cell_fixture};
use super::*;
use crate::tagged::gc::{
    ExecutionTestHomes, ImmediateResult, PlainSymbolBacking, RegistrationError,
};
use crate::tagged::value::TaggedValue;

fn each_backing(body: impl Fn(PlainSymbolBacking)) {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        body(backing);
    }
}

#[test]
fn nmjit_n4_cell_symbol_slot_is_checked_metadata() {
    let id = crate::emacs_core::intern::intern("p3sym-c2-global");
    let expected = SymbolSlotIndex::new(usize::try_from(id.0).unwrap());
    let closed = std::cell::Cell::new(None);
    each_backing(|backing| {
        prepared_cell_fixture(
            backing,
            FixtureSeed::Scalar,
            |_| (),
            |(), admission, run, home, symbol| {
                admission
                    .with_activation(run, home, |activation| {
                        let slot = activation.symbol_slot(symbol).unwrap();
                        assert_eq!(slot, expected);
                        let view = activation.table_view();
                        let table = view.table();
                        assert!(slot.get() < table.slots().get());
                        assert!(slot.get() / table.chunk_slots().get() < table.chunk_count().get());
                        if let Some(stale) = closed.get() {
                            assert!(matches!(
                                activation.symbol_slot(&stale),
                                Err(RegistrationError::WrongHeap
                                    | RegistrationError::StaleRegistration)
                            ));
                        }
                    })
                    .unwrap();
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
                closed.set(Some(*symbol));
            },
        );
    });
}

#[test]
fn nmjit_n4_cell_read_scalar_boundp_and_empty_function() {
    each_backing(|backing| {
        prepared_cell_fixture(
            backing,
            FixtureSeed::Scalar,
            |_| (),
            |(), admission, run, home, symbol| {
                for (operation, expected) in [
                    (
                        CellRead::GlobalValue,
                        ImmediateResult::from_word(TaggedValue::fixnum(73).bits()).unwrap(),
                    ),
                    (CellRead::GlobalBoundp, ImmediateResult::True),
                    (CellRead::Function, ImmediateResult::Nil),
                ] {
                    admission
                        .with_activation(run, home, |activation| {
                            activation.read_cell(symbol, operation)
                        })
                        .unwrap()
                        .unwrap();
                    assert_eq!(ExecutionTestHomes::read(home, run), expected);
                }
            },
        );
    });
}

#[test]
fn nmjit_n4_cell_read_void_preserves_home_and_boundp_returns_nil() {
    each_backing(|backing| {
        prepared_cell_fixture(
            backing,
            FixtureSeed::Scalar,
            |_| (),
            |(), admission, run, home, symbol| {
                run.sym_makunbound_into(symbol, &mut home.clone()).unwrap();
                admission
                    .with_activation(run, home, |activation| {
                        activation.store_immediate(ImmediateResult::True).unwrap();
                        assert_eq!(
                            activation.read_cell(symbol, CellRead::GlobalValue),
                            Err(CellReadError::Global(GlobalError::VoidVariable))
                        );
                    })
                    .unwrap();
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
                admission
                    .with_activation(run, home, |activation| {
                        activation.read_cell(symbol, CellRead::GlobalBoundp)
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
            },
        );
    });
}

#[test]
fn nmjit_n4_cell_read_managed_global_and_function_stay_in_owner_home() {
    each_backing(|backing| {
        prepared_cell_fixture(
            backing,
            FixtureSeed::FunctionDefinition,
            |_| (),
            |(), admission, run, home, symbol| {
                admission
                    .with_activation(run, home, |activation| {
                        activation.read_cell(symbol, CellRead::GlobalValue)
                    })
                    .unwrap()
                    .unwrap();
                let definition = home.word_for_test(run);
                assert!(TaggedValue::from_bits(definition).is_cons());
                // The method returned and its reader lease is gone, but the
                // canonical resident home already contains the managed definition.
                let lease = run.symbol_mut(symbol).unwrap();
                drop(lease);
                assert_eq!(home.word_for_test(run), definition);
                admission
                    .with_activation(run, home, |activation| {
                        activation.read_cell(symbol, CellRead::Function)
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!(home.word_for_test(run), definition);
                assert!(TaggedValue::from_bits(home.word_for_test(run)).is_cons());
            },
        );
    });
}

#[test]
fn nmjit_n4_cell_read_stale_symbol_refuses_without_changing_home() {
    let old = std::cell::Cell::new(None);
    prepared_cell_fixture(
        PlainSymbolBacking::Unprojected,
        FixtureSeed::Scalar,
        |_| (),
        |(), _, _, _, symbol| {
            old.set(Some(*symbol));
        },
    );
    // GEN0 and telemetry refuse the real production admission before the body.
    if let Some(stale) = old.get() {
        prepared_cell_fixture(
            PlainSymbolBacking::Unprojected,
            FixtureSeed::Scalar,
            |_| (),
            |(), admission, run, home, _| {
                for operation in [
                    CellRead::GlobalValue,
                    CellRead::GlobalBoundp,
                    CellRead::Function,
                ] {
                    let result = admission
                        .with_activation(run, home, |activation| {
                            activation.store_immediate(ImmediateResult::True).unwrap();
                            activation.read_cell(&stale, operation)
                        })
                        .unwrap();
                    assert!(matches!(
                        result,
                        Err(CellReadError::Global(GlobalError::Authority(
                            RegistrationError::WrongHeap | RegistrationError::StaleRegistration
                        ))) | Err(CellReadError::Function(FunctionError::Authority(
                            RegistrationError::WrongHeap | RegistrationError::StaleRegistration
                        )))
                    ));
                    assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
                }
            },
        );
    }
}

#[test]
fn nmjit_n4_cell_read_sealed_home_source_guard_test() {
    let source = include_str!("../../read.rs");
    assert!(source.contains("self.result.clone()"));
    for primitive in ["sym_value_into", "sym_boundp_into", "sym_function_into"] {
        assert!(source.contains(primitive));
    }
    for forbidden in [
        "crate::emacs_core",
        "thread_local!",
        "unsafe {",
        "unwrap(",
        "expect(",
        "panic!(",
    ] {
        assert!(
            !source.contains(forbidden),
            "cell bridge contains {forbidden}"
        );
    }
}
