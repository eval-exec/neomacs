//! The page sweep keeps permanent marks frozen, resets session-old marks
//! to white-at-rest, and keeps sweep-window allocations young.

use super::*;

fn float_slot(arena: &mut ObjectArena<FloatObj>, value: f64, parity: MarkParity) -> *mut FloatObj {
    let slot = arena.alloc_slot();
    // Allocated slots require a fully initialized object before sweep reads
    // the header; unallocated slot bytes are never read by these tests.
    unsafe {
        std::ptr::write(
            slot,
            FloatObj {
                header: GcHeader::new_marked(HeapObjectKind::Float, parity),
                value,
            },
        );
    }
    slot
}

#[test]
fn minor_arena_sweep_resets_old_marks_preserves_permanents_and_frees_young_garbage() {
    crate::test_utils::init_test_tracing();
    let mut arena = ObjectArena::<FloatObj>::new(None);
    let parity = MarkParity::One;
    let old = float_slot(&mut arena, 10.0, parity);
    unsafe { (*old).header.tenured = true };
    let permanent = float_slot(&mut arena, 20.0, parity);
    unsafe { (*permanent).header.make_permanent() };
    let young = float_slot(&mut arena, 30.0, parity);
    let dead = float_slot(&mut arena, 40.0, parity.flip());
    let mut reclaimed = Vec::new();
    let end = arena.pages.len();
    assert_eq!(
        arena.sweep_range::<true>(0, end, parity, CollectionScope::Young, |addr| reclaimed
            .push(addr)),
        (size_of::<FloatObj>(), 1),
    );
    assert_eq!(reclaimed, [dead as usize]);
    assert!(!arena.owns(dead.cast()));
    assert!(arena.owns(old.cast()));
    assert!(arena.owns(permanent.cast()));
    assert!(arena.owns(young.cast()));
    assert_eq!(unsafe { (*old).header.raw_mark() }, UNMARKED_AT_REST);
    assert_eq!(unsafe { (*permanent).header.raw_mark() }, parity.byte());
    assert_eq!(unsafe { (*young).header.raw_mark() }, parity.byte());
    assert!(!unsafe { (*young).header.tenured });
    assert!(
        !arena.pages[0].retired,
        "session-old pages must remain sweepable"
    );
    // Opposite parity still skips the old and permanent slots, but can
    // reclaim the formerly young survivor after it becomes unreachable.
    reclaimed.clear();
    assert_eq!(
        arena.sweep_range::<true>(0, end, parity.flip(), CollectionScope::Young, |addr| {
            reclaimed.push(addr)
        }),
        (0, 1),
    );
    assert_eq!(reclaimed, [young as usize]);
    assert!(arena.owns(old.cast()));
    assert!(arena.owns(permanent.cast()));
    assert_eq!(unsafe { (*old).value }, 10.0);
    assert_eq!(unsafe { (*permanent).value }, 20.0);
}

#[test]
fn a_reused_slot_born_in_the_sweep_window_stays_young_and_marked() {
    crate::test_utils::init_test_tracing();
    let mut arena = ObjectArena::<FloatObj>::new(None);
    let parity = MarkParity::One;
    let garbage = float_slot(&mut arena, 10.0, parity.flip());
    let old = float_slot(&mut arena, 20.0, parity);
    unsafe { (*old).header.tenured = true };
    let end = arena.pages.len();
    assert_eq!(
        arena.sweep_range::<true>(0, end, parity, CollectionScope::Young, |_| {}),
        (0, 1)
    );
    let newborn = float_slot(&mut arena, 30.0, parity);
    assert_eq!(
        newborn, garbage,
        "the new object should reuse the reclaimed slot"
    );
    assert_eq!(
        arena.sweep_range::<true>(0, end, parity, CollectionScope::Young, |_| {}),
        (size_of::<FloatObj>(), 0),
    );
    assert!(arena.owns(newborn.cast()));
    assert!(!unsafe { (*newborn).header.tenured });
    assert_eq!(unsafe { (*newborn).header.raw_mark() }, parity.byte());
    assert_eq!(unsafe { (*old).header.raw_mark() }, UNMARKED_AT_REST);
    assert_eq!(
        arena.sweep_range::<true>(0, end, parity.flip(), CollectionScope::Young, |_| {}),
        (0, 1),
    );
    assert!(!arena.owns(newborn.cast()));
    assert!(arena.owns(old.cast()));
}
