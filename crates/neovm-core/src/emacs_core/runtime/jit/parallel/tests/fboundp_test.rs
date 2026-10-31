use super::cell_tests::with_cell;
use super::*;
use crate::tagged::gc::{ExecutionTestHomes, ExecutionTestSeed, PlainSymbolBacking};

fn oracle() -> [ImmediateResult; 6] {
    include_str!("fboundp_oracle.expected")
        .lines()
        .map(|line| match line {
            "nil" => ImmediateResult::Nil,
            "t" => ImmediateResult::True,
            other => panic!("GNU boolean row: {other}"),
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("six GNU 31.1 rows")
}

pub(super) fn assert_empty<'owner, 'era>(
    run: &mut RunAccess<'owner, 'era>,
    home: &OwnerRootSlot<'era>,
    mut query: impl FnMut(&mut RunAccess<'owner, 'era>) -> Result<(), ParallelEntryError>,
) {
    query(run).unwrap();
    assert_eq!(ExecutionTestHomes::read(home, run), oracle()[0]);
}

#[derive(Clone, Copy)]
enum Transition {
    Observe,
    Unbind,
    BindDefinition,
    BindNil,
}

pub(super) fn assert_transitions<'owner, 'era>(
    run: &mut RunAccess<'owner, 'era>,
    home: &OwnerRootSlot<'era>,
    symbol: &SymbolHandle,
    mut query: impl FnMut(&mut RunAccess<'owner, 'era>) -> Result<(), ParallelEntryError>,
) {
    let mut definition = ExecutionTestHomes::nil(run).unwrap();
    run.sym_value_into(symbol, &mut definition).unwrap();
    assert!(ExecutionTestHomes::is_cons(&definition, run));
    let nil = ExecutionTestHomes::nil(run).unwrap();
    let transitions = [
        Transition::Observe,
        Transition::Unbind,
        Transition::BindDefinition,
        Transition::BindNil,
        Transition::BindDefinition,
    ];
    for (transition, expected) in transitions.into_iter().zip(&oracle()[1..]) {
        match transition {
            Transition::Observe => {}
            Transition::Unbind => run.sym_fmakunbound_into(symbol, &mut home.clone()).unwrap(),
            Transition::BindDefinition => run
                .sym_fset_into(symbol, &definition, &mut home.clone())
                .unwrap(),
            Transition::BindNil => run.sym_fset_into(symbol, &nil, &mut home.clone()).unwrap(),
        }
        query(run).unwrap();
        assert_eq!(ExecutionTestHomes::read(home, run), *expected);
    }
}

pub(crate) fn fboundp_manifest() -> Result<(), String> {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_cell(
            CellRead::FunctionBoundp,
            backing,
            ExecutionTestSeed::Scalar,
            |code, admission, run, home, symbol| {
                assert_empty(run, home, |run| code.run(admission, run, home, symbol));
            },
        )?;
        with_cell(
            CellRead::FunctionBoundp,
            backing,
            ExecutionTestSeed::WritableFunctionDefinition,
            |code, admission, run, home, symbol| {
                assert_transitions(run, home, symbol, |run| {
                    code.run(admission, run, home, symbol)
                });
                eprintln!("NMJIT_N4_FBOUNDP_NATIVE_GNU_OK {backing:?}");
            },
        )?;
    }
    Ok(())
}

#[test]
fn nmjit_n4_native_fboundp_matches_gnu_plain_and_projected() {
    fboundp_manifest().unwrap();
}

#[test]
fn nmjit_n4_fboundp_sealed_query_source_guard_test() {
    let bridge = include_str!("../../../../../tagged/gc/mutator_registration/execution/read.rs");
    let authority =
        include_str!("../../../../../tagged/gc/mutator_registration/symbol_authority/functions.rs");
    assert!(bridge.contains(".sym_fboundp_into("));
    assert!(authority.contains("index: OwnerRootIndex"));
    assert!(authority.contains("FunctionRead::Definition"));
    assert!(authority.contains("FunctionRead::Boundp"));
    assert!(authority.contains("self.symbol_ref(symbol)?"));
    assert!(!bridge.contains("crate::emacs_core"));
}
