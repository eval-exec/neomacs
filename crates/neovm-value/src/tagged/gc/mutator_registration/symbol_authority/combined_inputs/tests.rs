use super::*;
use crate::intern::intern;
use crate::symbol::Obarray;
use crate::tagged::gc::ProofEra;

#[derive(Clone, Copy)]
enum Graph {
    NameDefinition,
    ScalarDefinition,
    DirectName,
    Keyword,
    Scalar,
    Unbound,
    Vector,
    Alias,
    PropertyName,
    PropertyDefinition,
    AttachedEmptyName,
    AttachedEmptyDefinition,
    Cycle,
}
enum Properties {
    None,
    Empty,
    Managed,
}
fn name(properties: Properties) -> Value {
    let value = Value::string("p3sym-c6-mixed-input-name");
    match properties {
        Properties::None => {}
        Properties::Empty => {
            value
                .with_lisp_string_mut(|s| {
                    let _ = s.intervals_mut();
                })
                .unwrap();
        }
        Properties::Managed => {
            value
                .with_lisp_string_mut(|s| {
                    s.intervals_mut().put_property_in_char_range(
                        crate::value::positions::CharRange::from_usize(0, 1),
                        Value::from_sym_id(intern("c6-input-property")),
                        Value::vector(vec![Value::string("c6-input-property-child")]),
                    );
                })
                .unwrap();
        }
    }
    value
}
fn definition(answer: Value) -> Value {
    Value::cons(
        Value::from_sym_id(intern("lambda")),
        Value::cons(Value::NIL, Value::cons(answer, Value::NIL)),
    )
}

/// The core shell owns the Context call and the backing; the init closure
/// (obarray seeding) and the era body stay here. Unit-returning so the opener
/// can be a plain fn pointer.
fn fixture<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
    backing: PlainSymbolBacking,
    graph: Graph,
    body: impl FnOnce(&KeeperSymbols<'_>, &[SymbolHandle], &PreparedNameFunction),
) {
    let mut init = |obarray: &mut Obarray, ids: &[SymId]| {
        let value = match graph {
            Graph::NameDefinition => definition(name(Properties::None)),
            Graph::ScalarDefinition => definition(Value::fixnum(73)),
            Graph::DirectName => name(Properties::None),
            Graph::Keyword => Value::string(":c6-role-input"),
            Graph::Scalar => Value::fixnum(73),
            Graph::Unbound => Value::UNBOUND,
            Graph::Vector => Value::vector(vec![name(Properties::None)]),
            Graph::Alias => Value::from_sym_id(ids[1]),
            Graph::PropertyName => name(Properties::Managed),
            Graph::PropertyDefinition => definition(name(Properties::Managed)),
            Graph::AttachedEmptyName => name(Properties::Empty),
            Graph::AttachedEmptyDefinition => definition(name(Properties::Empty)),
            Graph::Cycle => {
                let value = Value::cons(Value::NIL, Value::NIL);
                value.set_cdr(value);
                value
            }
        };
        obarray.seed_value_id(ids[0], value);
        // A later mixed session may install F0 here. Its original-target
        // journal must not become an incidental F0/name retention source.
        obarray.clear_function_silent_id(ids[0]);
        obarray.clear_function_silent_id(ids[1]);
    };
    let mut body = Some(body);
    let mut era_body = |era: ProofEra<'_, K>, ids: Vec<SymId>| {
        let handles: Vec<_> = ids
            .into_iter()
            .map(|id| era.admit_plain_symbol(id).unwrap())
            .collect();
        let target = era.prepare_name_function_symbol(&handles[0]).unwrap();
        let mut stopped = era.stop().unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let keeper = stopped.symbols().unwrap();
            (body.take().expect("one fixture body per era"))(&keeper, &handles, &target)
        }));
        // Admission/shape refusals are preflight results, never errors in
        // the real trace callback. Even a controlled assertion unwinds to
        // this total teardown before the caller observes it.
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C6_ROLE_TEARDOWN_OK {backing:?}");
        match outcome {
            Ok(()) => (),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    };
    symbol_functions_init_era(backing, &mut init, &mut era_body);
}
fn limit() -> CombinedInputLimit {
    CombinedInputLimit::try_from(2).unwrap()
}
fn name_and_scalar(
    keeper: &KeeperSymbols<'_>,
    handles: &[SymbolHandle],
    target: &PreparedNameFunction,
) {
    let requests = [
        CombinedInput::new(handles[0], CombinedInputRole::NameDefinition),
        CombinedInput::new(handles[1], CombinedInputRole::ScalarDefinition),
    ];
    let admitted = keeper
        .admit_combined_inputs(target, &requests, limit())
        .unwrap();
    assert_eq!(admitted.inputs(), &requests);
    admitted.validate_identity(keeper).unwrap();
    for (actual, expected) in admitted.inputs().iter().zip(requests) {
        assert_eq!(actual.key(), expected.key());
        assert_eq!(actual.role(), expected.role());
    }
    // No relaxation of either retained standalone executor's import boundary.
    assert!(!keeper.plain_graphs_closed_for_test(handles, 64).unwrap());
    assert!(!keeper.initial_names_closed_for_test(handles, 64).unwrap());
    eprintln!("P3SYM_C6_ROLES name_and_scalar_exact_metadata_no_graph_roots");
}

/// The core shell owns the Context call and the backing; the init closure
/// and the era body stay in value. The opener is a plain fn pointer.
pub(crate) type SymbolFunctionsInitEraFn<K> = for<'a> fn(
    PlainSymbolBacking,
    &'a mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
    &'a mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
);

/// One row per former value lib-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CombinedInputsCase {
    P3symC6RolesAdmitPlain,
    P3symC6RolesAdmitProjected,
    P3symC6RolesAdmitDirectNameAndScalarWithoutWideningLegacy,
    P3symC6RolesPropertyRefusalSmall,
    P3symC6RolesRefuseWrongRolesShapesPropertiesAndCycles,
    P3symC6RolesForeignReceiptAndAllKeysCheckedBeforeDecode,
    P3symC6RolesStaleMetadataRefusesWithoutChangingLiveBacking,
    P3symC6RolesBudgetAndNoopControlFinishCleanly,
    P3symC6RolesRepeatedAndConflictingAliasesAreCheckedSeparately,
    P3symC6RolesUnwindControlFinishesBeforePropagating,
}

impl CombinedInputsCase {
    pub const ALL: &'static [Self] = &[
        Self::P3symC6RolesAdmitPlain,
        Self::P3symC6RolesAdmitProjected,
        Self::P3symC6RolesAdmitDirectNameAndScalarWithoutWideningLegacy,
        Self::P3symC6RolesPropertyRefusalSmall,
        Self::P3symC6RolesRefuseWrongRolesShapesPropertiesAndCycles,
        Self::P3symC6RolesForeignReceiptAndAllKeysCheckedBeforeDecode,
        Self::P3symC6RolesStaleMetadataRefusesWithoutChangingLiveBacking,
        Self::P3symC6RolesBudgetAndNoopControlFinishCleanly,
        Self::P3symC6RolesRepeatedAndConflictingAliasesAreCheckedSeparately,
        Self::P3symC6RolesUnwindControlFinishesBeforePropagating,
    ];
}

fn property_refusal<K: EraKeeper + std::fmt::Debug + 'static>(
    backing: PlainSymbolBacking,
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    fixture(
        symbol_functions_init_era,
        backing,
        Graph::PropertyDefinition,
        |keeper, handles, target| {
            assert_eq!(
                keeper
                    .admit_combined_inputs(
                        target,
                        &[CombinedInput::new(
                            handles[0],
                            CombinedInputRole::NameDefinition
                        )],
                        limit(),
                    )
                    .err(),
                Some(CombinedInputError::Function(
                    FunctionError::UnsupportedDefinition
                ))
            );
            // A refused input does not disturb another role or consume authority.
            let admitted = keeper
                .admit_combined_inputs(
                    target,
                    &[CombinedInput::new(
                        handles[1],
                        CombinedInputRole::ScalarDefinition,
                    )],
                    limit(),
                )
                .unwrap();
            admitted.validate_identity(keeper).unwrap();
        },
    );
}

fn p3sym_c6_roles_admit_plain_body<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Unprojected,
        Graph::NameDefinition,
        name_and_scalar,
    );
}

fn p3sym_c6_roles_admit_projected_body<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Projected,
        Graph::NameDefinition,
        name_and_scalar,
    );
}

fn p3sym_c6_roles_admit_direct_name_and_scalar_without_widening_legacy_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for (graph, role) in [
            (Graph::DirectName, CombinedInputRole::DirectName),
            (Graph::Keyword, CombinedInputRole::DirectName),
            (Graph::Scalar, CombinedInputRole::PlainScalar),
        ] {
            fixture(
                symbol_functions_init_era,
                backing,
                graph,
                |keeper, handles, target| {
                    let request = CombinedInput::new(handles[0], role);
                    let admitted = keeper
                        .admit_combined_inputs(target, &[request], limit())
                        .unwrap();
                    assert_eq!(admitted.inputs(), &[request]);
                    admitted.validate_identity(keeper).unwrap();
                },
            );
        }
        fixture(
            symbol_functions_init_era,
            backing,
            Graph::DirectName,
            |keeper, handles, target| {
                let keys = &handles[..1];
                assert!(!keeper.plain_graphs_closed_for_test(keys, 64).unwrap());
                assert!(keeper.initial_names_closed_for_test(keys, 64).unwrap());
                target.validate_keeper(keeper).unwrap();
            },
        );
        fixture(
            symbol_functions_init_era,
            backing,
            Graph::Keyword,
            |keeper, handles, target| {
                // A graph-sharing role grants no spelling/intern eligibility.
                // C5 retains its existing refusal, including on soft lookup.
                assert!(
                    !keeper
                        .initial_names_closed_for_test(&handles[..1], 64)
                        .unwrap()
                );
                target.validate_keeper(keeper).unwrap();
            },
        );
    }
}

fn p3sym_c6_roles_property_refusal_small_body<K: EraKeeper + std::fmt::Debug + 'static>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    property_refusal(PlainSymbolBacking::Projected, symbol_functions_init_era);
}

fn p3sym_c6_roles_refuse_wrong_roles_shapes_properties_and_cycles_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        property_refusal(backing, symbol_functions_init_era);
        for (graph, role, expected) in [
            (
                Graph::NameDefinition,
                CombinedInputRole::ScalarDefinition,
                CombinedInputError::Function(FunctionError::UnsupportedDefinition),
            ),
            (
                Graph::ScalarDefinition,
                CombinedInputRole::NameDefinition,
                CombinedInputError::Role(CombinedInputRole::NameDefinition),
            ),
            (
                Graph::NameDefinition,
                CombinedInputRole::DirectName,
                CombinedInputError::Role(CombinedInputRole::DirectName),
            ),
            (
                Graph::NameDefinition,
                CombinedInputRole::PlainScalar,
                CombinedInputError::Role(CombinedInputRole::PlainScalar),
            ),
            (
                Graph::PropertyName,
                CombinedInputRole::DirectName,
                CombinedInputError::Role(CombinedInputRole::DirectName),
            ),
            (
                Graph::AttachedEmptyName,
                CombinedInputRole::DirectName,
                CombinedInputError::Role(CombinedInputRole::DirectName),
            ),
            (
                Graph::AttachedEmptyDefinition,
                CombinedInputRole::NameDefinition,
                CombinedInputError::Function(FunctionError::UnsupportedDefinition),
            ),
            (
                Graph::Vector,
                CombinedInputRole::NameDefinition,
                CombinedInputError::Function(FunctionError::UnsupportedDefinition),
            ),
            (
                Graph::Alias,
                CombinedInputRole::NameDefinition,
                CombinedInputError::Function(FunctionError::UnsupportedDefinition),
            ),
            (
                Graph::Unbound,
                CombinedInputRole::PlainScalar,
                CombinedInputError::Role(CombinedInputRole::PlainScalar),
            ),
            (
                Graph::Cycle,
                CombinedInputRole::NameDefinition,
                CombinedInputError::Function(FunctionError::UnsupportedDefinition),
            ),
        ] {
            fixture(
                symbol_functions_init_era,
                backing,
                graph,
                |keeper, handles, target| {
                    assert_eq!(
                        keeper
                            .admit_combined_inputs(
                                target,
                                &[CombinedInput::new(handles[0], role)],
                                limit()
                            )
                            .err(),
                        Some(expected)
                    );
                },
            );
        }
    }
}

fn p3sym_c6_roles_foreign_receipt_and_all_keys_checked_before_decode_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    let mut receipt = None;
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Unprojected,
        Graph::NameDefinition,
        |keeper, handles, target| {
            receipt = Some(
                keeper
                    .admit_combined_inputs(
                        target,
                        &[CombinedInput::new(
                            handles[0],
                            CombinedInputRole::NameDefinition,
                        )],
                        limit(),
                    )
                    .unwrap(),
            );
        },
    );
    // The receipt legitimately outlives its era, but contains metadata only.
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Projected,
        Graph::Vector,
        |keeper, handles, target| {
            let receipt = receipt
                .take()
                .expect("the first fixture admitted a receipt");
            assert_eq!(
                receipt.validate_identity(keeper),
                Err(RegistrationError::WrongHeap)
            );
            assert_eq!(
                keeper
                    .admit_combined_inputs(
                        &receipt.target(),
                        &[CombinedInput::new(
                            handles[1],
                            CombinedInputRole::ScalarDefinition
                        )],
                        limit(),
                    )
                    .err(),
                Some(CombinedInputError::Authority(RegistrationError::WrongHeap))
            );
            // The first input has a wrong shape, but the foreign key must be
            // refused before the first shape is even decoded.
            assert_eq!(
                keeper
                    .admit_combined_inputs(
                        target,
                        &[
                            CombinedInput::new(handles[0], CombinedInputRole::NameDefinition),
                            receipt.inputs()[0],
                        ],
                        limit(),
                    )
                    .err(),
                Some(CombinedInputError::Authority(RegistrationError::WrongHeap))
            );
        },
    );
}

fn p3sym_c6_roles_stale_metadata_refuses_without_changing_live_backing_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Projected,
        Graph::NameDefinition,
        |keeper, handles, target| {
            // Synthetic captured metadata only; the live World is untouched.
            let mut stale = handles[0];
            stale.obarray = stale.obarray.checked_add(1).unwrap();
            assert_eq!(
                keeper
                    .admit_combined_inputs(
                        target,
                        &[CombinedInput::new(stale, CombinedInputRole::NameDefinition)],
                        limit(),
                    )
                    .err(),
                Some(CombinedInputError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            name_and_scalar(keeper, handles, target);
        },
    );
}

fn p3sym_c6_roles_budget_and_noop_control_finish_cleanly_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    assert_eq!(
        CombinedInputLimit::try_from(0),
        Err(CombinedInputError::InvalidLimit)
    );
    assert_eq!(
        CombinedInputLimit::try_from(usize::MAX),
        Err(CombinedInputError::InvalidLimit)
    );
    assert_eq!(
        CombinedInputLimit::try_from(usize::from(u16::MAX) + 2),
        Err(CombinedInputError::InvalidLimit)
    );
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture(
            symbol_functions_init_era,
            backing,
            Graph::NameDefinition,
            |keeper, handles, target| {
                assert_eq!(
                    keeper.admit_combined_inputs(target, &[], limit()).err(),
                    Some(CombinedInputError::EmptyInputs)
                );
                let inputs = [
                    CombinedInput::new(handles[0], CombinedInputRole::NameDefinition),
                    CombinedInput::new(handles[1], CombinedInputRole::ScalarDefinition),
                ];
                assert_eq!(
                    keeper
                        .admit_combined_inputs(
                            target,
                            &inputs,
                            CombinedInputLimit::try_from(1).unwrap()
                        )
                        .err(),
                    Some(CombinedInputError::Capacity)
                );
                name_and_scalar(keeper, handles, target);
            },
        );
    }
}

fn p3sym_c6_roles_repeated_and_conflicting_aliases_are_checked_separately_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    fixture(
        symbol_functions_init_era,
        PlainSymbolBacking::Projected,
        Graph::NameDefinition,
        |keeper, handles, target| {
            let same = CombinedInput::new(handles[0], CombinedInputRole::NameDefinition);
            for _ in 0..2 {
                let admitted = keeper
                    .admit_combined_inputs(target, &[same, same], limit())
                    .unwrap();
                assert_eq!(admitted.inputs(), &[same, same]);
                admitted.validate_identity(keeper).unwrap();
            }
            assert_eq!(
                keeper
                    .admit_combined_inputs(
                        target,
                        &[
                            same,
                            CombinedInput::new(handles[0], CombinedInputRole::ScalarDefinition),
                        ],
                        limit(),
                    )
                    .err(),
                Some(CombinedInputError::Function(
                    FunctionError::UnsupportedDefinition
                ))
            );
            name_and_scalar(keeper, handles, target);
        },
    );
}

fn p3sym_c6_roles_unwind_control_finishes_before_propagating_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    let outcome = std::panic::catch_unwind(|| {
        fixture(
            symbol_functions_init_era,
            PlainSymbolBacking::Projected,
            Graph::NameDefinition,
            |keeper, handles, target| {
                name_and_scalar(keeper, handles, target);
                panic!("C6 controlled role admission unwind");
            },
        );
    });
    let payload = outcome.unwrap_err();
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"C6 controlled role admission unwind")
    );
}

/// One runner for the combined-inputs subtree. The match is exhaustive over
/// the typed case list; `name: &str` dispatch is not used.
pub(crate) fn run_combined_inputs_case<K: EraKeeper + std::fmt::Debug + 'static>(
    case: CombinedInputsCase,
    symbol_functions_init_era: SymbolFunctionsInitEraFn<K>,
) {
    match case {
        CombinedInputsCase::P3symC6RolesAdmitPlain => {
            p3sym_c6_roles_admit_plain_body(symbol_functions_init_era)
        }
        CombinedInputsCase::P3symC6RolesAdmitProjected => {
            p3sym_c6_roles_admit_projected_body(symbol_functions_init_era)
        }
        CombinedInputsCase::P3symC6RolesAdmitDirectNameAndScalarWithoutWideningLegacy => {
            p3sym_c6_roles_admit_direct_name_and_scalar_without_widening_legacy_body(
                symbol_functions_init_era,
            )
        }
        CombinedInputsCase::P3symC6RolesPropertyRefusalSmall => {
            p3sym_c6_roles_property_refusal_small_body(symbol_functions_init_era)
        }
        CombinedInputsCase::P3symC6RolesRefuseWrongRolesShapesPropertiesAndCycles => {
            p3sym_c6_roles_refuse_wrong_roles_shapes_properties_and_cycles_body(
                symbol_functions_init_era,
            )
        }
        CombinedInputsCase::P3symC6RolesForeignReceiptAndAllKeysCheckedBeforeDecode => {
            p3sym_c6_roles_foreign_receipt_and_all_keys_checked_before_decode_body(
                symbol_functions_init_era,
            )
        }
        CombinedInputsCase::P3symC6RolesStaleMetadataRefusesWithoutChangingLiveBacking => {
            p3sym_c6_roles_stale_metadata_refuses_without_changing_live_backing_body(
                symbol_functions_init_era,
            )
        }
        CombinedInputsCase::P3symC6RolesBudgetAndNoopControlFinishCleanly => {
            p3sym_c6_roles_budget_and_noop_control_finish_cleanly_body(symbol_functions_init_era)
        }
        CombinedInputsCase::P3symC6RolesRepeatedAndConflictingAliasesAreCheckedSeparately => {
            p3sym_c6_roles_repeated_and_conflicting_aliases_are_checked_separately_body(
                symbol_functions_init_era,
            )
        }
        CombinedInputsCase::P3symC6RolesUnwindControlFinishesBeforePropagating => {
            p3sym_c6_roles_unwind_control_finishes_before_propagating_body(
                symbol_functions_init_era,
            )
        }
    }
}
