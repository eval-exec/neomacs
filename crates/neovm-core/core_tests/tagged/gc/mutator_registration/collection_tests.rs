//! Real STW collection shells.
//! The shell performs the serial setup, admits each era, and passes the baton
//! World walk; the bodies and their assertions stay in value.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use neovm_eval::emacs_core::eval::Baton;
use neovm_eval::test_support::authority::{
    baton_collect_parallel_world_roots, set_lexenv_for_test,
};
use neovm_value::tagged::gc::QuiescentCollector;
use neovm_value::test_support::authority::{
    CollectionCase, ProofEra, run_collection_test_for_test,
};
use neovm_value::value::buffer_vocab::BufferId;

type Graph = (SharedRoot, [usize; 2], Value);
type ThenFixture = (Graph, [usize; 2], [usize; 2]);
type AfterWords = ([usize; 2], [usize; 2], [usize; 2]);
type RequestersFixture = (
    Graph,
    Graph,
    [usize; 2],
    [usize; 2],
    [Graph; 3],
    usize,
    Graph,
    usize,
    bool,
);

fn graph(context: &mut Context, n: i64) -> Graph {
    let child = unsafe { context.tagged_heap_mut() }.alloc_cons(Value::fixnum(n), Value::NIL);
    let head = unsafe { context.tagged_heap_mut() }.alloc_cons(child, Value::NIL);
    // SAFETY: setup is the sole serial owner; no collection or callback occurs
    // between allocation and admission to the matching root table.
    let root = unsafe { SharedRoot::new(context.tagged_heap(), head) };
    (root, [head.bits(), child.bits()], head)
}

fn graph_era_impl(n: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, Graph)) {
    Context::with_test_collection_era(|context| graph(context, n), |era, graph| body(era, graph));
}

fn graph_pair_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, (Graph, Graph))) {
    Context::with_test_collection_era(
        |context| (graph(context, 201), graph(context, 202)),
        |era, pair| body(era, pair),
    );
}

fn requesters_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RequestersFixture)) {
    Context::with_test_collection_era(
        |context| {
            let a = graph(context, 101);
            let b = graph(context, 102);
            let world = graph(context, 103);
            context.set_variable("p1rv-a5-world-root", unsafe {
                Value::from_bits(world.1[0])
            });
            drop(world.0);
            let legacy = graph(context, 104);
            set_lexenv_for_test(context, unsafe { Value::from_bits(legacy.1[0]) });
            drop(legacy.0);
            let logs = [
                graph(context, 105),
                graph(context, 106),
                graph(context, 107),
            ];
            let shared_only = graph(context, 108);
            let internal = unsafe { context.tagged_heap_mut() }.alloc_buffer(BufferId(900001));
            unsafe { context.tagged_heap_mut() }.register_buffer_value(BufferId(900001), internal);
            let dead = unsafe { context.tagged_heap_mut() }
                .alloc_cons(Value::fixnum(999), Value::NIL)
                .bits();
            (
                a,
                b,
                world.1,
                legacy.1,
                logs,
                dead,
                shared_only,
                internal.bits(),
                context.tagged_heap().tagged_heap_generational_enabled(),
            )
        },
        |era, fixture| body(era, fixture),
    );
}

fn empty_outer_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)) {
    Context::with_test_collection_era(|_| (), |era, ()| body(era));
}

fn empty_nested_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)) {
    Context::with_test_collection_era(|_| (), |era, ()| body(era));
}

fn mutator_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)) {
    Context::with_test_mutator_era(|era| body(era));
}

fn graph_era_then_impl(
    body: &mut dyn for<'e> FnMut(
        ProofEra<'e, Baton>,
        ThenFixture,
    ) -> (QuiescentCollector<'e>, Baton, AfterWords),
) {
    Context::with_test_collection_era_then(
        |context| {
            let participant = graph(context, 501);
            let world = graph(context, 502);
            context.set_variable("p1rv-a5-persistent-world", unsafe {
                Value::from_bits(world.1[0])
            });
            drop(world.0);
            let legacy = graph(context, 503);
            set_lexenv_for_test(context, unsafe { Value::from_bits(legacy.1[0]) });
            drop(legacy.0);
            (participant, world.1, legacy.1)
        },
        |era, graph| body(era, graph),
        |context, (participant, world, legacy)| {
            assert!(!context.tagged_heap().facade_mark_is_excluded());
            context.gc_collect_exact();
            for word in participant {
                assert!(
                    !context
                        .tagged_heap()
                        .owns_heap_value_for_test(unsafe { Value::from_bits(word) })
                );
            }
            for word in world.into_iter().chain(legacy) {
                let value = unsafe { Value::from_bits(word) };
                assert!(context.tagged_heap().owns_heap_value_for_test(value));
            }
        },
    );
}

fn run(case: CollectionCase) {
    let mut graph_era = graph_era_impl;
    let mut graph_pair_era = graph_pair_era_impl;
    let mut requesters_era = requesters_era_impl;
    let mut empty_outer_era = |_flag: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)| {
        Context::with_test_collection_era(|_| (), |era, ()| body(era));
    };
    let mut empty_nested_era = |_flag: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)| {
        Context::with_test_collection_era(|_| (), |era, ()| body(era));
    };
    let mut mutator_era = |_flag: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)| {
        Context::with_test_mutator_era(|era| body(era));
    };
    let mut graph_era_then = graph_era_then_impl;
    run_collection_test_for_test(
        case,
        &mut graph_era,
        &mut graph_pair_era,
        &mut requesters_era,
        &mut empty_outer_era,
        &mut empty_nested_era,
        &mut mutator_era,
        &mut graph_era_then,
        baton_collect_parallel_world_roots,
    );
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(CollectionCase::$case);
        })*
        const SHELL_CASES: &[CollectionCase] = &[$(CollectionCase::$case),*];
    };
}

shells! {
    two_requesters_collect_transitive_world_private_and_image_roots => TwoRequestersCollectTransitiveWorldPrivateAndImageRoots,
    native_image_logs_fold_once_across_repeated_full_majors => NativeImageLogsFoldOnceAcrossRepeatedFullMajors,
    foreign_stop_guard_and_missing_root_admission_refuse_before_collection => ForeignStopGuardAndMissingRootAdmissionRefuseBeforeCollection,
    retained_reader_refuses_collection_and_abort_keeps_backing => RetainedReaderRefusesCollectionAndAbortKeepsBacking,
    explicitly_finished_reader_allows_real_collection => ExplicitlyFinishedReaderAllowsRealCollection,
    repeated_worlds_have_distinct_heaps_and_independent_collection_cycles => RepeatedWorldsHaveDistinctHeapsAndIndependentCollectionCycles,
    serial_heap_restoration_reclaims_former_owner_roots_after_resume => SerialHeapRestorationReclaimsFormerOwnerRootsAfterResume,
}

#[test]
fn shell_cases_cover_all_collection_case_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = CollectionCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}
