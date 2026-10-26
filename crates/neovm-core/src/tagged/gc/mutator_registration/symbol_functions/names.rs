//! C6.3a native name-return boundary; no combined tape or minor-GC claim yet.
use super::super::symbol_authority::PreparedNameFunction;
use super::super::{
    Mutator, ParallelHeapEra, PlainSymbolBacking, RegistrationError, RendezvousError,
};
use super::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootIdentity(usize);
#[derive(Clone, Copy)]
struct GraphFacts {
    definition: RootIdentity,
    name: RootIdentity,
}
enum SeedState {
    Empty,
    Seeded(GraphFacts),
}
#[derive(Clone, Copy)]
enum GraphKind {
    Name,
    Keyword,
    AttachedEmpty,
    PropertyGraph,
    Vector,
    Args,
    ExtraBody,
    ConsBody,
    UnboundBody,
    Alias,
}
#[derive(Clone, Copy)]
enum FixtureConfig {
    Empty(GraphKind),
    BoundName,
}
struct Handles {
    target: SymbolHandle,
    replacement: SymbolHandle,
    name: PreparedNameFunction,
}
struct Homes<'era> {
    definition: OwnerRootSlot<'era>,
    replacement: OwnerRootSlot<'era>,
    answer: OwnerRootSlot<'era>,
    cache: FunctionCallCache<'era>,
    activation: FunctionActivation<'era>,
}
enum FrameState<'era> {
    Empty,
    Seeded(Homes<'era>),
}
enum BodyState<B> {
    Ready(B),
    Used,
}
enum Resources<'era> {
    Resident {
        owner: Mutator<'era>,
        input: SharedRoot,
        frame: FrameState<'era>,
    },
    Released,
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupError {
    Registration(RegistrationError),
    Retirement(RendezvousError),
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupReceipt {
    Pending,
    Finished(Result<(), CleanupError>),
}
#[must_use = "clear every resident root before owner retirement and era.finish"]
struct Guard<'era> {
    resources: Resources<'era>,
    receipt: Rc<RefCell<CleanupReceipt>>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let state = std::mem::replace(&mut self.resources, Resources::Released);
        let result = match state {
            Resources::Released => return,
            Resources::Resident {
                mut owner,
                input,
                frame,
            } => (|| {
                drop(frame);
                contention::retry(
                    Deadline::contention(),
                    || owner.run_access()?.clear_root_slots(),
                    contention::registration,
                )
                .map_err(CleanupError::Registration)?;
                drop(input);
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failed| match contention::rendezvous(&failed.reason) {
                        Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failed),
                    },
                )
                .map_err(|failed| CleanupError::Retirement(failed.reason))
            })(),
        };
        *self.receipt.borrow_mut() = CleanupReceipt::Finished(result);
    }
}

fn fixture(
    backing: PlainSymbolBacking,
    config: FixtureConfig,
    body: impl for<'owner, 'era> FnOnce(
        &mut RunAccess<'owner, 'era>,
        &Handles,
        &mut Homes<'era>,
        GraphFacts,
    ),
) -> std::thread::Result<()> {
    let seed_state = RefCell::new(SeedState::Empty);
    Context::with_test_symbol_functions_init_era(
        backing,
        |obarray, ids| {
            let kind = match config {
                FixtureConfig::Empty(kind) => kind,
                FixtureConfig::BoundName => GraphKind::Name,
            };
            let name = match kind {
                GraphKind::Keyword => Value::string(":c6-name-candidate"),
                GraphKind::Name
                | GraphKind::AttachedEmpty
                | GraphKind::PropertyGraph
                | GraphKind::Vector
                | GraphKind::Args
                | GraphKind::ExtraBody
                | GraphKind::ConsBody
                | GraphKind::UnboundBody
                | GraphKind::Alias => Value::string("p3sym-c6-constant-name"),
            };
            match kind {
                GraphKind::AttachedEmpty => {
                    name.with_lisp_string_mut(|s| {
                        let _ = s.intervals_mut();
                    })
                    .unwrap();
                }
                GraphKind::PropertyGraph => {
                    name.with_lisp_string_mut(|s| {
                        use crate::buffer::CharRange;
                        s.intervals_mut().put_property_in_char_range(
                            CharRange::from_usize(0, 1),
                            Value::from_sym_id(intern("c6-property")),
                            Value::vector(vec![Value::string("managed property graph")]),
                        );
                    })
                    .unwrap();
                }
                GraphKind::Name
                | GraphKind::Keyword
                | GraphKind::Vector
                | GraphKind::Args
                | GraphKind::ExtraBody
                | GraphKind::ConsBody
                | GraphKind::UnboundBody
                | GraphKind::Alias => {}
            }
            let answer = match kind {
                GraphKind::Vector => Value::vector(vec![name]),
                GraphKind::ConsBody => Value::cons(name, Value::NIL),
                GraphKind::UnboundBody => Value::UNBOUND,
                GraphKind::Name
                | GraphKind::Keyword
                | GraphKind::AttachedEmpty
                | GraphKind::PropertyGraph
                | GraphKind::Args
                | GraphKind::ExtraBody
                | GraphKind::Alias => name,
            };
            let parameters = match kind {
                GraphKind::Args => Value::cons(Value::from_sym_id(intern("x")), Value::NIL),
                GraphKind::Name
                | GraphKind::Keyword
                | GraphKind::AttachedEmpty
                | GraphKind::PropertyGraph
                | GraphKind::Vector
                | GraphKind::ExtraBody
                | GraphKind::ConsBody
                | GraphKind::UnboundBody
                | GraphKind::Alias => Value::NIL,
            };
            let tail = match kind {
                GraphKind::ExtraBody => Value::cons(Value::fixnum(1), Value::NIL),
                GraphKind::Name
                | GraphKind::Keyword
                | GraphKind::AttachedEmpty
                | GraphKind::PropertyGraph
                | GraphKind::Vector
                | GraphKind::Args
                | GraphKind::ConsBody
                | GraphKind::UnboundBody
                | GraphKind::Alias => Value::NIL,
            };
            let definition = match kind {
                GraphKind::Alias => Value::from_sym_id(ids[1]),
                GraphKind::Name
                | GraphKind::Keyword
                | GraphKind::AttachedEmpty
                | GraphKind::PropertyGraph
                | GraphKind::Vector
                | GraphKind::Args
                | GraphKind::ExtraBody
                | GraphKind::ConsBody
                | GraphKind::UnboundBody => Value::cons(
                    Value::from_sym_id(intern("lambda")),
                    Value::cons(parameters, Value::cons(answer, tail)),
                ),
            };
            obarray.seed_value_id(ids[0], definition);
            match config {
                FixtureConfig::Empty(_) => obarray.clear_function_silent_id(ids[0]),
                FixtureConfig::BoundName => obarray.set_symbol_function_id(ids[0], definition),
            }
            *seed_state.borrow_mut() = SeedState::Seeded(GraphFacts {
                definition: RootIdentity(definition.bits()),
                name: RootIdentity(name.bits()),
            });
        },
        |era, ids| {
            let facts = match *seed_state.borrow() {
                SeedState::Seeded(facts) => facts,
                SeedState::Empty => panic!("C6 name setup incomplete"),
            };
            let target = era.admit_plain_symbol(ids[0]).unwrap();
            let replacement = era.admit_plain_symbol(ids[1]).unwrap();
            let scalar_admission = era.prepare_function_symbol(&target);
            match config {
                FixtureConfig::Empty(_) => scalar_admission.unwrap(),
                FixtureConfig::BoundName => {
                    assert_eq!(scalar_admission, Err(FunctionError::UnsupportedDefinition))
                }
            }
            let prepared = era.prepare_name_function_symbol(&target).unwrap();
            let handles = Handles {
                target,
                replacement,
                name: prepared,
            };
            let mut stopped = era.stop().unwrap();
            let input = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: actual matching stopped heap; only immediate NIL is rooted.
                    SharedRoot::new(heap, Value::NIL)
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let permit = era.register(1, 8).unwrap();
            let owner = retry_owned(
                Deadline::contention(),
                permit,
                |permit| permit.activate(era.heap_identity(), 1),
                |failed| match contention::registration(&failed.reason) {
                    Retry::Busy => ControlFlow::Continue(failed.into_permit()),
                    Retry::Refuse => ControlFlow::Break(failed),
                },
            )
            .unwrap();
            let receipt = Rc::new(RefCell::new(CleanupReceipt::Pending));
            let mut guard = Guard {
                resources: Resources::Resident {
                    owner,
                    input,
                    frame: FrameState::Empty,
                },
                receipt: Rc::clone(&receipt),
            };
            let mut body_state = BodyState::Ready(body);
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match &mut guard.resources {
                        Resources::Released => panic!("C6 name fixture released before body"),
                        Resources::Resident {
                            owner,
                            input,
                            frame,
                        } => {
                            contention::retry(
                                Deadline::contention(),
                                || {
                                    let mut run = owner.run_access()?;
                                    *frame = FrameState::Seeded(Homes {
                                        definition: run.root_slot(input).unwrap(),
                                        replacement: run.root_slot(input).unwrap(),
                                        answer: run.root_slot(input).unwrap(),
                                        cache: FunctionCallCache::new(&mut run).unwrap(),
                                        activation: FunctionActivation::new(&mut run).unwrap(),
                                    });
                                    let homes = match frame {
                                        FrameState::Seeded(homes) => homes,
                                        FrameState::Empty => panic!("C6 name frame not seeded"),
                                    };
                                    run.sym_value_into(&handles.target, &mut homes.definition)
                                        .unwrap();
                                    run.sym_value_into(
                                        &handles.replacement,
                                        &mut homes.replacement,
                                    )
                                    .unwrap();
                                    match std::mem::replace(&mut body_state, BodyState::Used) {
                                        BodyState::Ready(body) => {
                                            body(&mut run, &handles, homes, facts)
                                        }
                                        BodyState::Used => {
                                            panic!("C6 name fixture body repeated after progress")
                                        }
                                    }
                                    Ok(())
                                },
                                contention::registration,
                            )
                            .unwrap();
                        }
                    }
                }));
            drop(guard);
            assert_eq!(
                *receipt.borrow(),
                CleanupReceipt::Finished(Ok(())),
                "C6 name independent teardown"
            );
            let mut stopped = era.stop().unwrap();
            stopped.symbols().unwrap().reconcile_functions().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era
                .finish()
                .expect("C6 name fixture finished after complete root/reconcile teardown");
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C6_NAME_TEARDOWN_OK {backing:?}");
            outcome
        },
    )
}

fn answer(run: &RunAccess<'_, '_>, homes: &Homes<'_>) -> RootIdentity {
    RootIdentity(homes.answer.word_for_test(run))
}
fn complete_call<'era>(run: &mut RunAccess<'_, 'era>, homes: &mut Homes<'era>) {
    assert_eq!(
        run.sym_call_step(&mut homes.activation, &mut homes.answer, NonZeroU32::MIN),
        Ok(FunctionStep::Yielded)
    );
    assert_eq!(
        run.sym_call_step(&mut homes.activation, &mut homes.answer, NonZeroU32::MIN),
        Ok(FunctionStep::Returned)
    );
}

pub(in crate::tagged::gc::mutator_registration) fn return_case(backing: PlainSymbolBacking) {
    fixture(backing, FixtureConfig::Empty(GraphKind::Name), |run, handles, homes, facts| {
        let before = run.function_facts_for_test(&handles.target).unwrap();
        assert_eq!(run.sym_fset_into(&handles.target, &homes.definition, &mut homes.answer), Err(FunctionError::UnsupportedDefinition));
        assert_eq!(before, run.function_facts_for_test(&handles.target).unwrap());
        assert_eq!(answer(run, homes), RootIdentity(Value::NIL.bits()));
        run.sym_name_fset_into(&handles.name, &homes.definition, &mut homes.answer).unwrap();
        assert_eq!(answer(run, homes), facts.definition);
        let installed = run.function_facts_for_test(&handles.target).unwrap();
        assert_eq!(run.sym_fset_into(&handles.target, &homes.definition, &mut homes.answer), Err(FunctionError::UnsupportedDefinition));
        assert_eq!(installed, run.function_facts_for_test(&handles.target).unwrap());
        run.sym_name_fset_into(&handles.name, &homes.definition, &mut homes.answer).unwrap();
        assert_eq!(installed, run.function_facts_for_test(&handles.target).unwrap());
        run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation).unwrap();
        complete_call(run, homes);
        assert_eq!(answer(run, homes), facts.name);
        let cache_counts = homes.cache.counts_for_test();
        // Same symbol/clock and cached definition must still revalidate Scalar.
        assert_eq!(run.sym_call_begin(&handles.target, &mut homes.cache, &mut homes.activation), Err(FunctionError::UnsupportedDefinition));
        assert_eq!(homes.cache.counts_for_test(), cache_counts);
        assert_eq!(answer(run, homes), facts.name);
        run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation).unwrap();
        assert_eq!(run.sym_call_step(&mut homes.activation, &mut homes.answer, NonZeroU32::MIN), Ok(FunctionStep::Yielded));
        homes.cache.clear(run).unwrap();
        run.sym_fset_into(&handles.target, &homes.replacement, &mut homes.answer).unwrap();
        assert_eq!(run.sym_call_step(&mut homes.activation, &mut homes.answer, NonZeroU32::MIN), Ok(FunctionStep::Returned));
        assert_eq!(answer(run, homes), facts.name, "selected old definition returns its exact original child");
        assert_eq!(run.sym_call_step(&mut homes.activation, &mut homes.answer, NonZeroU32::MIN), Err(FunctionError::InactiveActivation));
        run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation).unwrap();
        complete_call(run, homes);
        assert_eq!(answer(run, homes), RootIdentity(Value::fixnum(97).bits()));
        run.sym_call_begin(&handles.target, &mut homes.cache, &mut homes.activation).unwrap();
        complete_call(run, homes);
        assert_eq!(answer(run, homes), RootIdentity(Value::fixnum(97).bits()));
        run.sym_fmakunbound_into(&handles.target, &mut homes.answer).unwrap();
        assert_eq!(run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation), Err(FunctionError::VoidFunction));
        eprintln!("P3SYM_C6_NAME_RETURN exact_child_scalar_cache_refusal_selected_old_return_scalar_replacement {backing:?}");
    }).unwrap();
}

#[test]
fn p3sym_c6_name_noop_control_finishes() {
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::Empty(GraphKind::Name),
        |_, _, _, _| {},
    )
    .unwrap();
}
#[test]
fn p3sym_c6_name_unwind_control_releases_reader_and_frame() {
    let outcome = fixture(
        PlainSymbolBacking::Projected,
        FixtureConfig::Empty(GraphKind::Name),
        |run, handles, _, _| {
            let _reader = run.symbol_ref(&handles.target).unwrap();
            panic!("C6 controlled name fixture unwind");
        },
    );
    let payload = outcome.expect_err("name fixture preserves controlled unwind after teardown");
    assert_eq!(
        payload.downcast_ref::<&str>().copied(),
        Some("C6 controlled name fixture unwind")
    );
}
#[test]
fn p3sym_c6_name_preparation_does_not_enable_legacy_scalar_begin() {
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::BoundName,
        |run, handles, homes, facts| {
            assert_eq!(
                run.sym_call_begin(&handles.target, &mut homes.cache, &mut homes.activation),
                Err(FunctionError::UnsupportedDefinition)
            );
            run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation)
                .unwrap();
            complete_call(run, homes);
            assert_eq!(answer(run, homes), facts.name);
        },
    )
    .unwrap();
}
fn shape_refusal(graph: GraphKind) {
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::Empty(graph),
        |run, handles, homes, _| {
            let before = run.function_facts_for_test(&handles.target).unwrap();
            assert_eq!(
                run.sym_name_fset_into(&handles.name, &homes.definition, &mut homes.answer),
                Err(FunctionError::UnsupportedDefinition)
            );
            assert_eq!(
                before,
                run.function_facts_for_test(&handles.target).unwrap()
            );
            assert_eq!(answer(run, homes), RootIdentity(Value::NIL.bits()));
            assert_eq!(homes.activation.selected_epoch(), None);
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_name_shape_and_empty_sidecar_refusals_preserve_clock_and_answer() {
    for graph in [
        GraphKind::AttachedEmpty,
        GraphKind::Vector,
        GraphKind::Args,
        GraphKind::ExtraBody,
        GraphKind::ConsBody,
        GraphKind::UnboundBody,
        GraphKind::Alias,
    ] {
        shape_refusal(graph);
    }
}
#[test]
fn p3sym_c6_name_property_graph_refusal_small() {
    shape_refusal(GraphKind::PropertyGraph);
}
#[test]
fn p3sym_c6_name_keyword_return_keeps_c5_intern_refusal() {
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::Empty(GraphKind::Keyword),
        |run, handles, homes, facts| {
            run.sym_name_fset_into(&handles.name, &homes.definition, &mut homes.answer)
                .unwrap();
            run.sym_name_call_begin(&handles.name, &mut homes.cache, &mut homes.activation)
                .unwrap();
            complete_call(run, homes);
            assert_eq!(answer(run, homes), facts.name);
            assert_eq!(
                run.initial_intern_into(&homes.answer, &mut homes.definition),
                Err(super::super::InitialInternError::UnsupportedName)
            );
            assert_eq!(
                RootIdentity(homes.definition.word_for_test(run)),
                facts.definition
            );
            assert_eq!(answer(run, homes), facts.name);
        },
    )
    .unwrap();
}
#[derive(Clone, Copy)]
enum PreparedState {
    Empty,
    Retired(PreparedNameFunction),
}
#[test]
fn p3sym_c6_name_retired_preparation_refuses_another_heap() {
    let state = RefCell::new(PreparedState::Empty);
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::Empty(GraphKind::Name),
        |_, handles, _, _| {
            *state.borrow_mut() = PreparedState::Retired(handles.name);
        },
    )
    .unwrap();
    let retired = match *state.borrow() {
        PreparedState::Retired(key) => key,
        PreparedState::Empty => panic!("name capability not retained"),
    };
    fixture(
        PlainSymbolBacking::Unprojected,
        FixtureConfig::Empty(GraphKind::Name),
        |run, handles, homes, _| {
            assert_eq!(
                run.sym_name_fset_into(&retired, &homes.definition, &mut homes.answer),
                Err(FunctionError::Authority(RegistrationError::WrongHeap))
            );
            assert_eq!(
                run.sym_name_call_begin(&retired, &mut homes.cache, &mut homes.activation),
                Err(FunctionError::Authority(RegistrationError::WrongHeap))
            );
            assert_eq!(answer(run, homes), RootIdentity(Value::NIL.bits()));
            run.sym_name_fset_into(&handles.name, &homes.definition, &mut homes.answer)
                .unwrap();
        },
    )
    .unwrap();
}
