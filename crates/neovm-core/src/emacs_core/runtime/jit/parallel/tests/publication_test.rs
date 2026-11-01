//! Finite native readers, sealed writers and fresh names on the same real era.
use super::owner_handoff_tests::{function, registration, retry};
use super::*;
use crate::emacs_core::intern::{
    SymId,
    parallel::{identity_census_for_test, name_publication_for_test},
};
use crate::tagged::gc::function_write_hooks::HofOperation;
use crate::tagged::gc::{
    CohortPhase, CohortSize, ExecutionFixtureReceipt, ExecutionTestDeadline, ExecutionTestHomes,
    ExecutionTestRetry, FunctionStep, GlobalError, InitialInternError, PlainSymbolBacking,
    PublicationInputs, execution_test_retry, with_test_prepared_publication_cohort,
};
use crate::tagged::value::TaggedValue;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum OracleRow {
    Miss,
    OldMap,
    OldCall,
    NewCall,
    OldIdentity,
    NewMap,
    Winner,
    FreshFunction,
    FreshBoundp,
    Value,
    Boundp,
    SoftWinner,
}
fn oracle(row: OracleRow) -> &'static str {
    let rows = include_str!("publication_oracle.expected")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 12);
    rows[match row {
        OracleRow::Miss => 0,
        OracleRow::OldMap => 1,
        OracleRow::OldCall => 2,
        OracleRow::NewCall => 3,
        OracleRow::OldIdentity => 4,
        OracleRow::NewMap => 5,
        OracleRow::Winner => 6,
        OracleRow::FreshFunction => 7,
        OracleRow::FreshBoundp => 8,
        OracleRow::Value => 9,
        OracleRow::Boundp => 10,
        OracleRow::SoftWinner => 11,
    }]
}
fn immediate(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> String {
    match ExecutionTestHomes::read(home, run) {
        ImmediateResult::Nil => "nil".into(),
        ImmediateResult::True => "t".into(),
        ImmediateResult::Fixnum(number) => i64::from(number).to_string(),
    }
}
fn mapped(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> String {
    let value = TaggedValue::from_bits(ExecutionTestHomes::word(home, run));
    assert!(value.is_cons());
    assert!(value.cons_cdr_unobserved().is_nil());
    format!(
        "({})",
        value
            .cons_car_unobserved()
            .as_fixnum()
            .expect("bounded map result")
    )
}
fn function_retry<T>(operation: impl FnMut() -> Result<T, FunctionError>) -> Result<T, String> {
    execution_test_retry(ExecutionTestDeadline::contention(), operation, function)
        .map_err(|error| format!("publication function: {error:?}"))
}
fn global_retry<T>(operation: impl FnMut() -> Result<T, GlobalError>) -> Result<T, String> {
    execution_test_retry(
        ExecutionTestDeadline::contention(),
        operation,
        |error| match error {
            GlobalError::Authority(error) => registration(error),
            GlobalError::VoidVariable => ExecutionTestRetry::Refuse,
        },
    )
    .map_err(|error| format!("publication global: {error:?}"))
}
fn intern_retry<T>(operation: impl FnMut() -> Result<T, InitialInternError>) -> Result<T, String> {
    execution_test_retry(
        ExecutionTestDeadline::contention(),
        operation,
        |error| match error {
            InitialInternError::Authority(error) => registration(error),
            InitialInternError::ExpectedString
            | InitialInternError::ExpectedPlainSymbol
            | InitialInternError::UnsupportedName => ExecutionTestRetry::Refuse,
        },
    )
    .map_err(|error| format!("publication intern: {error:?}"))
}
#[derive(Clone, Copy, Debug)]
enum OwnerRole {
    Publisher,
    Observer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NameWord(usize);
#[derive(Clone, Copy, Debug)]
struct WinnerReceipt {
    shared: SymId,
    distinct: SymId,
    shared_name: NameWord,
    distinct_name: NameWord,
}
#[derive(Clone, Copy)]
enum Issuance {
    Pending,
    Complete(WinnerReceipt),
}
struct Kernels {
    value: ParallelCompiledCellCode,
    function: ParallelCompiledCellCode,
    boundp: ParallelCompiledCellCode,
    fboundp: ParallelCompiledCellCode,
    named: ParallelCompiledNamedCode,
    mapping: ParallelCompiledHofCode,
}
impl Kernels {
    fn compile(admission: &Gen1Admission<'_>) -> Result<Self, ParallelCompileError> {
        Ok(Self {
            value: ParallelCompiledCellCode::compile(admission, CellRead::GlobalValue)?,
            function: ParallelCompiledCellCode::compile(admission, CellRead::Function)?,
            boundp: ParallelCompiledCellCode::compile(admission, CellRead::GlobalBoundp)?,
            fboundp: ParallelCompiledCellCode::compile(admission, CellRead::FunctionBoundp)?,
            named: ParallelCompiledNamedCode::compile(admission)?,
            mapping: ParallelCompiledHofCode::compile(admission, HofOperation::Mapcar)?,
        })
    }
}
struct Program {
    role: OwnerRole,
    kernels: Result<Kernels, ParallelCompileError>,
    issuance: Cell<Issuance>,
    winners: Arc<Mutex<Vec<WinnerReceipt>>>,
    size: CohortSize,
}
static_assertions::assert_impl_all!(Program: Send);
static_assertions::assert_not_impl_any!(Program: Sync, Clone, Copy);
fn id(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> SymId {
    TaggedValue::from_bits(ExecutionTestHomes::word(home, run))
        .as_bare_symbol_id()
        .expect("intern returns a symbol")
}
#[derive(Clone, Copy)]
enum NameClass {
    Shared,
    Distinct,
}
fn issue<'era>(
    kernels: &Kernels,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut PublicationInputs<'era>,
    class: NameClass,
) -> Result<SymId, String> {
    let name = match class {
        NameClass::Shared => &inputs.shared_name,
        NameClass::Distinct => &inputs.distinct_name,
    };
    intern_retry(|| run.initial_intern_into(name, &mut inputs.fresh_symbol))?;
    let issued = id(&inputs.fresh_symbol, run);
    let handle = intern_retry(|| run.initial_symbol_handle(&inputs.fresh_symbol))?;
    retry(|| kernels.fboundp.run(admission, run, home, &handle))?;
    assert_eq!(immediate(home, run), oracle(OracleRow::FreshFunction));
    if matches!(class, NameClass::Distinct) {
        retry(|| kernels.boundp.run(admission, run, home, &handle))?;
        assert_eq!(immediate(home, run), oracle(OracleRow::FreshBoundp));
    }
    global_retry(|| run.sym_set_into(&handle, &inputs.fresh_value, &mut home.clone()))?;
    retry(|| kernels.value.run(admission, run, home, &handle))?;
    assert_eq!(immediate(home, run), oracle(OracleRow::Value));
    retry(|| kernels.boundp.run(admission, run, home, &handle))?;
    assert_eq!(immediate(home, run), oracle(OracleRow::Boundp));
    Ok(issued)
}
/// Only the finite program count is bounded here; every Busy uses Deadline.
fn mapping<'era>(
    kernels: &Kernels,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut PublicationInputs<'era>,
    phase: CohortPhase,
) -> Result<(), String> {
    inputs.hof.singleton = HofSingleton::new(
        run,
        ImmediateResult::from_word(TaggedValue::fixnum(7).bits()).unwrap(),
    )
    .map_err(|error| format!("publication singleton: {error:?}"))?;
    let previous = ExecutionTestHomes::word(home, run);
    let selected = retry(|| {
        kernels.mapping.run(
            admission,
            run,
            home,
            &inputs.hof.mapper,
            &inputs.hof.callback,
            &inputs.hof.expected,
            &mut inputs.hof.singleton,
            &mut inputs.hof.scratch,
        )
    })?;
    match selected {
        FunctionSelection::Selected => {
            assert!(!matches!(phase, CohortPhase::AfterSecondCollection));
            assert_eq!(mapped(home, run), oracle(OracleRow::OldMap));
        }
        FunctionSelection::Changed => {
            assert!(!matches!(phase, CohortPhase::Initial));
            assert_eq!(ExecutionTestHomes::word(home, run), previous);
            assert_eq!(
                retry(|| kernels.mapping.run(
                    admission,
                    run,
                    home,
                    &inputs.hof.mapper,
                    &inputs.hof.callback,
                    &inputs.hof.replacement,
                    &mut inputs.hof.singleton,
                    &mut inputs.hof.scratch
                ))?,
                FunctionSelection::Selected
            );
            assert_eq!(mapped(home, run), oracle(OracleRow::NewMap));
        }
    }
    Ok(())
}
fn body<'era>(
    program: &Program,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut PublicationInputs<'era>,
    phase: CohortPhase,
) -> Result<(), String> {
    let kernels = match &program.kernels {
        Ok(kernels) => kernels,
        Err(ParallelCompileError::BackendUnavailable) => {
            assert!(!cfg!(feature = "jit"));
            eprintln!("NMJIT_N4_PUBLICATION_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            return Ok(());
        }
        Err(
            error @ (ParallelCompileError::Module(_)
            | ParallelCompileError::Define(_)
            | ParallelCompileError::Finalize(_)
            | ParallelCompileError::UnsupportedPointerWidth
            | ParallelCompileError::AbiLayout),
        ) => return Err(format!("publication compile: {error:?}")),
    };
    match phase {
        CohortPhase::Initial => {
            intern_retry(|| {
                run.initial_intern_soft_into(&inputs.shared_name, &mut inputs.fresh_symbol)
            })?;
            assert_eq!(
                immediate(&inputs.fresh_symbol, run),
                oracle(OracleRow::Miss)
            );
            retry(|| {
                kernels
                    .value
                    .run(admission, run, home, &inputs.hof.callback_symbol)
            })?;
            assert_eq!(
                ExecutionTestHomes::word(home, run),
                ExecutionTestHomes::word(&inputs.hof.expected, run)
            );
            retry(|| {
                kernels
                    .function
                    .run(admission, run, home, &inputs.hof.callback_symbol)
            })?;
            assert_eq!(
                ExecutionTestHomes::word(home, run),
                ExecutionTestHomes::word(&inputs.hof.expected, run)
            );
            assert_eq!(
                retry(|| kernels.named.select(
                    admission,
                    run,
                    home,
                    &inputs.named_symbol,
                    &inputs.named_symbol,
                    &inputs.named_old,
                    &mut inputs.named_activation
                ))?,
                FunctionSelection::Selected
            );
            // Retain the actual bounded Rust activation through both stops.
            mapping(kernels, admission, run, home, inputs, phase)?;
        }
        CohortPhase::AfterFirstCollection => {
            assert_eq!(mapped(home, run), oracle(OracleRow::OldMap));
            match program.role {
                OwnerRole::Publisher => {
                    function_retry(|| {
                        run.sym_unary_fset_into(
                            &inputs.hof.callback,
                            &inputs.hof.replacement,
                            &mut home.clone(),
                        )
                    })?;
                    function_retry(|| {
                        run.sym_fset_into(
                            &inputs.named_symbol,
                            &inputs.named_new,
                            &mut home.clone(),
                        )
                    })?;
                    let shared = issue(kernels, admission, run, home, inputs, NameClass::Shared)?;
                    let distinct =
                        issue(kernels, admission, run, home, inputs, NameClass::Distinct)?;
                    record(program, run, inputs, shared, distinct)?;
                }
                OwnerRole::Observer => {
                    // Separate operations have separate linearization points.
                    // Do not assert a cross-cell snapshot around the writer.
                    for _ in 0..4 {
                        mapping(kernels, admission, run, home, inputs, phase)?;
                    }
                    let shared = issue(kernels, admission, run, home, inputs, NameClass::Shared)?;
                    let distinct =
                        issue(kernels, admission, run, home, inputs, NameClass::Distinct)?;
                    record(program, run, inputs, shared, distinct)?;
                }
            }
            mapping(kernels, admission, run, home, inputs, phase)?;
        }
        CohortPhase::AfterSecondCollection => {
            let last = mapped(home, run);
            assert!(last == oracle(OracleRow::OldMap) || last == oracle(OracleRow::NewMap));
            assert_eq!(
                function_retry(|| run.sym_call_step(
                    &mut inputs.named_activation,
                    &mut home.clone(),
                    NonZeroU32::new(2).unwrap()
                ))?,
                FunctionStep::Returned
            );
            assert_eq!(immediate(home, run), oracle(OracleRow::OldCall));
            let stale = retry(|| {
                kernels.named.select(
                    admission,
                    run,
                    home,
                    &inputs.named_symbol,
                    &inputs.named_symbol,
                    &inputs.named_old,
                    &mut inputs.named_activation,
                )
            })?;
            assert_eq!(
                match stale {
                    FunctionSelection::Selected => "t",
                    FunctionSelection::Changed => "nil",
                },
                oracle(OracleRow::OldIdentity)
            );
            assert_eq!(
                retry(|| kernels.named.select(
                    admission,
                    run,
                    home,
                    &inputs.named_symbol,
                    &inputs.named_symbol,
                    &inputs.named_new,
                    &mut inputs.named_activation
                ))?,
                FunctionSelection::Selected
            );
            assert_eq!(
                function_retry(|| run.sym_call_step(
                    &mut inputs.named_activation,
                    &mut home.clone(),
                    NonZeroU32::new(2).unwrap()
                ))?,
                FunctionStep::Returned
            );
            assert_eq!(immediate(home, run), oracle(OracleRow::NewCall));
            verify_committed(program, kernels, admission, run, home, inputs)?;
            mapping(kernels, admission, run, home, inputs, phase)?;
            eprintln!("NMJIT_N4_PUBLICATION_OLD_NEW_ROOTS_OK {:?}", program.role);
        }
    }
    eprintln!(
        "NMJIT_N4_NATIVE_PUBLICATION_PHASE_OK {phase:?} {:?}",
        program.role
    );
    Ok(())
}
fn record(
    program: &Program,
    run: &RunAccess<'_, '_>,
    inputs: &PublicationInputs<'_>,
    shared: SymId,
    distinct: SymId,
) -> Result<(), String> {
    assert!(matches!(program.issuance.get(), Issuance::Pending));
    let receipt = WinnerReceipt {
        shared,
        distinct,
        shared_name: NameWord(ExecutionTestHomes::word(&inputs.shared_name, run)),
        distinct_name: NameWord(ExecutionTestHomes::word(&inputs.distinct_name, run)),
    };
    program.issuance.set(Issuance::Complete(receipt));
    program
        .winners
        .lock()
        .map_err(|_| "publication winners poisoned")?
        .push(receipt);
    Ok(())
}
fn verify_committed<'era>(
    program: &Program,
    kernels: &Kernels,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut PublicationInputs<'era>,
) -> Result<(), String> {
    let own = match program.issuance.get() {
        Issuance::Pending => return Err("publication never issued its names".into()),
        Issuance::Complete(receipt) => receipt,
    };
    let winners = program
        .winners
        .lock()
        .map_err(|_| "publication winners poisoned")?
        .clone();
    assert_eq!(winners.len(), program.size.count());
    assert_eq!(
        if winners.iter().all(|row| row.shared == own.shared) {
            "t"
        } else {
            "nil"
        },
        oracle(OracleRow::Winner)
    );
    assert_eq!(
        winners
            .iter()
            .map(|row| row.distinct.0)
            .collect::<BTreeSet<_>>()
            .len(),
        program.size.count()
    );
    assert!(winners.iter().all(|row| row.shared != row.distinct));
    assert_eq!(
        winners
            .iter()
            .map(|row| row.shared_name.0)
            .collect::<BTreeSet<_>>()
            .len(),
        program.size.count()
    );
    let unique = program.size.count() + 1;
    let census = identity_census_for_test();
    inputs.facts.registry.assert_issuance(census, program.size);
    let roots = name_publication_for_test(inputs.facts.heap, own.shared, own.shared_name.0);
    let canonical = roots.0.expect("shared canonical name root");
    assert!(roots.1 && roots.3);
    assert_eq!(roots.2, canonical == own.shared_name.0);
    assert!(winners.iter().any(|row| row.shared_name.0 == canonical));
    assert_eq!(
        name_publication_for_test(inputs.facts.heap, own.distinct, own.distinct_name.0),
        (Some(own.distinct_name.0), true, true, true)
    );
    intern_retry(|| run.initial_intern_into(&inputs.shared_name, &mut inputs.fresh_symbol))?;
    assert_eq!(id(&inputs.fresh_symbol, run), own.shared);
    intern_retry(|| run.initial_intern_soft_into(&inputs.shared_name, &mut inputs.fresh_symbol))?;
    assert_eq!(
        if id(&inputs.fresh_symbol, run) == own.shared {
            "t"
        } else {
            "nil"
        },
        oracle(OracleRow::SoftWinner)
    );
    intern_retry(|| run.initial_intern_soft_into(&inputs.distinct_name, &mut inputs.fresh_symbol))?;
    assert_eq!(id(&inputs.fresh_symbol, run), own.distinct);
    let handle = intern_retry(|| run.initial_symbol_handle(&inputs.fresh_symbol))?;
    retry(|| kernels.value.run(admission, run, home, &handle))?;
    assert_eq!(immediate(home, run), oracle(OracleRow::Value));
    assert_eq!(
        identity_census_for_test(),
        census,
        "existing/soft lookups never bump the registry epoch"
    );
    let after = execution_test_retry(
        ExecutionTestDeadline::contention(),
        || run.initial_table_statistics_for_test(),
        registration,
    )
    .map_err(|error| format!("publication table statistics: {error:?}"))?;
    assert_ne!(after.generation, inputs.facts.tables.generation);
    assert!(after.retired.get() > inputs.facts.tables.retired.get());
    admission
        .with_activation(run, home, |activation| {
            let view = activation.table_view();
            let table = view.table();
            assert!(table.slots().get() > inputs.facts.slots.get());
            assert!(table.chunk_count().get() >= inputs.facts.chunks.get() + 2);
            assert_eq!(table.chunk_slots(), inputs.facts.chunk_slots);
            assert!(inputs.facts.original_chunk_is_retained(table));
        })
        .map_err(|error| format!("publication descriptor view: {error:?}"))?;
    eprintln!(
        "NMJIT_N4_PUBLICATION_WINNERS_EPOCH_ROOTS_OK owners={} unique={unique}",
        program.size.count()
    );
    Ok(())
}
fn history(size: CohortSize) {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let next = Cell::new(OwnerRole::Publisher);
        let winners = Arc::new(Mutex::new(Vec::new()));
        let receipt = with_test_prepared_publication_cohort(
            backing,
            HofOperation::Mapcar,
            size,
            |admission| Program {
                role: next.replace(OwnerRole::Observer),
                kernels: Kernels::compile(admission),
                issuance: Cell::new(Issuance::Pending),
                winners: Arc::clone(&winners),
                size,
            },
            body,
        );
        match receipt {
            ExecutionFixtureReceipt::Gen1Completed => {
                eprintln!("NMJIT_N4_PUBLICATION_HISTORY_COMPLETED {size:?} {backing:?}")
            }
            ExecutionFixtureReceipt::Gen0Refused | ExecutionFixtureReceipt::TelemetryRefused => {
                eprintln!("NMJIT_N4_PUBLICATION_TYPED_REFUSAL {size:?}")
            }
        }
    }
}
const CHILD: &str = "NEOVM_TEST_NMJIT_PUBLICATION_EXACT_CHILD";
fn isolated(size: CohortSize, name: &str) {
    let name = name
        .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
        .expect("libtest omits the crate prefix");
    // One exact test in a fresh harness, including cargo test's parallel mode.
    // There is no separately registered child test that silently returns.
    if std::env::var(CHILD).ok().as_deref() == Some(name) {
        history(size);
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("test harness"))
        .args([name, "--exact", "--test-threads", "1", "--nocapture"])
        .env(CHILD, name)
        .env_remove("RUST_LOG")
        .output()
        .expect("exact publication history");
    assert!(
        output.status.success(),
        "publication history {name}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("test result: ok. 1 passed; 0 failed; 0 ignored;"),
        "exact child must run one real case, not an empty filter: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
}
#[test]
fn nmjit_n4_one_owner_native_writer_and_intern_publication() {
    isolated(
        CohortSize::One,
        concat!(
            module_path!(),
            "::nmjit_n4_one_owner_native_writer_and_intern_publication"
        ),
    );
}
#[test]
fn nmjit_n4_two_owner_native_writer_and_intern_publication() {
    isolated(
        CohortSize::Two,
        concat!(
            module_path!(),
            "::nmjit_n4_two_owner_native_writer_and_intern_publication"
        ),
    );
}
#[test]
fn nmjit_n4_four_owner_native_writer_and_intern_publication() {
    isolated(
        CohortSize::Four,
        concat!(
            module_path!(),
            "::nmjit_n4_four_owner_native_writer_and_intern_publication"
        ),
    );
}
#[test]
fn nmjit_n4_publication_fixture_small_noop_control() {
    with_test_prepared_publication_cohort(
        PlainSymbolBacking::Unprojected,
        HofOperation::Mapcar,
        CohortSize::One,
        |_| (),
        |(), _, _, _, _, _| Ok(()),
    );
}
