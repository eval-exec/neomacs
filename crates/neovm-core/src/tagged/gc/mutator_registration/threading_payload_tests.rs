//! Real-World stopped collection of the three concrete threading payloads.
//! Children have no SharedRoot or private-owner root that could hide an edge.
use super::*;
use crate::buffer::CharRange;
use crate::emacs_core::eval::Context;
use crate::emacs_core::threads::ThreadName;
use crate::heap_types::LispString;
use crate::heap_types::threading::ThreadName as PayloadThreadName;
use crate::tagged::gc::{ProofEra, with_tagged_heap};
use crate::tagged::header::VectorObj;
use crate::tagged::value::TaggedValue as Value;

#[derive(Clone, Copy)]
struct ObjectWord(usize);

#[derive(Clone, Copy)]
struct VectorWord(usize);

struct Graph {
    owners: [ObjectWord; 3],
    children: [VectorWord; 10],
    unrelated: VectorWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Age {
    Young,
    Old,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CollectionCycle(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Snapshot {
    cycle: CollectionCycle,
    children: [Age; 10],
}

#[derive(Clone, Copy)]
enum Unrelated {
    Retained,
    Reclaimed,
}

#[derive(Clone, Copy)]
enum Cycle {
    Full,
    Minor,
}

fn named(text: &str, property: Value, child: Value) -> LispString {
    let mut name = LispString::from_utf8(text);
    name.intervals_mut()
        .put_property_in_char_range(CharRange::from_usize(0, 1), property, child);
    name
}

fn graph(context: &mut Context) -> Graph {
    context.gc_stress = false;
    context.gc_collect_exact();
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert!(
        context.threads.thread_handle(0).is_some(),
        "initialized main thread"
    );
    let native = context
        .threads
        .try_create_thread(Value::NIL, ThreadName::UNNAMED)
        .expect("new thread identity")
        .native_key();
    let thread = context.threads.thread_handle(native).unwrap();
    // The old object will gain entirely fresh children through the real
    // pre-store barrier. A minor must reach them through its remembered owner.
    context.gc_collect_exact();
    context.with_gc_inhibited(|context| {
        let children: [Value; 10] =
            std::array::from_fn(|index| Value::vector(vec![Value::fixnum(index as i64)]));
        let property = Value::symbol("p15-stopped-name-child");
        let thread_name = Value::heap_string(named("thread", property, children[7]));
        let thread_name = PayloadThreadName::from_value(thread_name).unwrap();
        with_tagged_heap(|heap| {
            // SAFETY: the live map roots this same-World object. Setup holds
            // exclusive serial World access; the closure copies prepared words
            // without allocating, collecting, invoking Lisp or yielding.
            unsafe {
                heap.with_thread_payload_mut(thread, |payload| {
                    payload.name = thread_name;
                    payload.function = children[0];
                    payload.result = children[1];
                    payload.last_error = Some(children[2]);
                    payload.buffer_disposition = children[3];
                    payload.event_object = children[4];
                    payload.blocked_remaining_forms = children[5];
                    payload.error_data = children[6];
                })
            }
            .expect("same-World thread mutation")
        });
        let mutex = context
            .threads
            .create_mutex(Some(named("mutex", property, children[8])))
            .expect("mutex object");
        let condition = context
            .threads
            .create_condition_variable(mutex, Some(named("condition", property, children[9])))
            .expect("condition object");
        // The mutex has no World variable or registry root. Its only object
        // edge is the condition's immutable, validated association.
        context.set_variable("p15-stopped-condition", condition.value());
        let unrelated = Value::vector(vec![Value::fixnum(12345)]);
        Graph {
            owners: [
                ObjectWord(thread.bits()),
                ObjectWord(mutex.value().bits()),
                ObjectWord(condition.value().bits()),
            ],
            children: children.map(|value| VectorWord(value.bits())),
            unrelated: VectorWord(unrelated.bits()),
        }
    })
}

fn inspect(
    era: &ProofEra<'_>,
    stopped: &AllStopped<'_>,
    graph: &Graph,
    unrelated: Unrelated,
) -> Snapshot {
    era.with_stopped_heap_for_test(stopped, |heap| {
        for ObjectWord(word) in graph.owners {
            assert!(
                heap.owns_heap_value_for_test(Value::from_bits(word)),
                "threading owner reclaimed"
            );
        }
        for VectorWord(word) in graph.children {
            assert!(
                heap.owns_heap_value_for_test(Value::from_bits(word)),
                "sole-edge threading child reclaimed"
            );
        }
        // Membership for every observed word precedes every payload decode.
        let children = std::array::from_fn(|index| {
            let value = Value::from_bits(graph.children[index].0);
            let ptr = value.gc_veclike_ptr().expect("known vector word");
            // SAFETY: exact membership above retains this initialized object;
            // AllStopped and the collector loan exclude mutation and sweeping.
            // Read raw GC accessors, not a serial TLS observation facade.
            unsafe {
                assert_eq!((*ptr).type_tag, VecLikeType::Vector);
                let object = &*ptr.cast::<VectorObj>();
                assert_eq!(object.data.as_slice(), &[Value::fixnum(index as i64)]);
                match (*ptr).gc.tenured {
                    true => Age::Old,
                    false => Age::Young,
                }
            }
        });
        match unrelated {
            Unrelated::Retained => {
                assert!(heap.owns_heap_value_for_test(Value::from_bits(graph.unrelated.0)))
            }
            Unrelated::Reclaimed => assert!(
                !heap.owns_heap_value_for_test(Value::from_bits(graph.unrelated.0)),
                "unreachable control stayed rooted"
            ),
        }
        Snapshot {
            cycle: CollectionCycle(heap.gc_collections()),
            children,
        }
    })
    .unwrap()
}

fn exercise(cycles: &[Cycle]) {
    Context::with_test_collection_era(graph, |era, graph| {
        // This real publication has no child roots. It exercises the existing
        // all-owner job without manufacturing a second root for any payload.
        let mut owner = era
            .register(1515, 0)
            .unwrap()
            .activate(era.heap_identity(), 1515)
            .unwrap();
        let mut unrelated = Unrelated::Retained;
        for &cycle in cycles {
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            let before = inspect(&era, &stopped, &graph, unrelated);
            let generations = era
                .with_stopped_heap_for_test(&stopped, |heap| heap.generational_enabled())
                .unwrap();
            if let Cycle::Minor = cycle {
                // Preparation must not have secretly promoted these children
                // while draining an active serial cycle before era admission.
                if let Unrelated::Retained = unrelated {
                    assert_eq!(before.children, [Age::Young; 10]);
                }
            }
            match cycle {
                Cycle::Full => {
                    let report = era.collect_stopped(&mut stopped).unwrap();
                    assert!(report.major);
                    unrelated = Unrelated::Reclaimed;
                }
                Cycle::Minor => match era.collect_stopped_minor(&mut stopped) {
                    Ok(report) => {
                        assert!(generations);
                        assert!(!report.major);
                        unrelated = Unrelated::Reclaimed;
                    }
                    Err(MinorCollectionError::GenerationalDisabled) => {
                        assert!(!generations);
                        assert_eq!(
                            inspect(&era, &stopped, &graph, unrelated),
                            before,
                            "GEN0 minor refusal changes no cycle, child or age"
                        );
                        // The typed refusal preceded AllStopped::trace. A
                        // no-op acknowledgement permits normal fixture cleanup;
                        // it performs no collection or payload traversal.
                        stopped.trace(|_, _| Ok(())).unwrap();
                    }
                    Err(error @ MinorCollectionError::Collection(_)) => {
                        panic!("threading adapter minor failed: {error:?}");
                    }
                },
            }
            let after = inspect(&era, &stopped, &graph, unrelated);
            match cycle {
                Cycle::Minor if !generations => assert_eq!(after, before),
                Cycle::Full | Cycle::Minor => {
                    assert_eq!(after.cycle, CollectionCycle(before.cycle.0 + 1));
                    if generations {
                        assert_eq!(after.children, [Age::Old; 10]);
                    }
                }
            }
            stopped.complete().unwrap();
            owner.leave_native().unwrap();
        }
        owner.retire_rooted(&[]).unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn real_world_stopped_full_traces_owned_threading_payloads() {
    exercise(&[Cycle::Full, Cycle::Full]);
}

#[test]
fn real_world_stopped_minor_uses_owned_threading_schema_and_preserves_gen0_refusal() {
    exercise(&[Cycle::Minor, Cycle::Minor, Cycle::Full, Cycle::Minor]);
}
