//! Capture-sized traversals exceed the recent-read shortcut while retaining
//! the same live owners. Their shared observation marks need one publication,
//! and the resulting certificate must still depend on every traversed owner.

use super::*;
use crate::heap_types::LispString;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};

const OWNER_COUNT: usize = 1024;
const TRAVERSALS: usize = 8;

struct ObservedMode;

impl ObservedMode {
    fn begin() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[derive(Clone, Copy, Debug)]
enum OwnerKind {
    Cons,
    Vector,
    String,
}

impl OwnerKind {
    fn allocate(self, heap: &mut TaggedHeap) -> TaggedValue {
        match self {
            Self::Cons => heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL),
            Self::Vector => heap.alloc_vector(vec![TaggedValue::make_int(97)]),
            Self::String => heap.alloc_string(LispString::from_utf8("abc")),
        }
    }

    fn read(self, owner: TaggedValue) -> TaggedValue {
        match self {
            Self::Cons => owner.cons_car(),
            Self::Vector => owner.as_vector_data().expect("vector")[0],
            Self::String => TaggedValue::make_int(i64::from(
                owner.as_str_owned().expect("string").as_bytes()[0],
            )),
        }
    }

    fn mutate(self, owner: TaggedValue) {
        match self {
            Self::Cons => owner.set_car(TaggedValue::make_int(122)),
            Self::Vector => assert!(crate::tagged::mutate::set_vector_slot(
                owner,
                0,
                TaggedValue::make_int(122),
            )),
            Self::String => assert!(crate::tagged::mutate::set_string_byte_same_char_count(
                owner, 0, b'z',
            )),
        }
    }
}

fn colliding_live_traversals(kind: OwnerKind) {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owners: Vec<_> = (0..OWNER_COUNT).map(|_| kind.allocate(&mut heap)).collect();

    // A real traversal with more distinct owners than the recent-read table
    // evicts its shortcut entries. Keep every object live and perform no GC:
    // repeated misses must retain the first dependency entry without another
    // shared mark publication or duplicate dependency recording.
    let (sum, certificate) = capture(|| {
        OBSERVED_OWNER_PUBLICATIONS.with(|count| count.set(0));
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        let mut sum = 0_i64;
        for _ in 0..TRAVERSALS {
            for &owner in &owners {
                sum += kind.read(owner).as_int().expect("read value");
            }
        }
        assert_eq!(
            OBSERVED_OWNER_PUBLICATIONS.with(Cell::get),
            OWNER_COUNT,
            "colliding reads must publish each live owner once: {kind:?}",
        );
        assert_eq!(
            OBSERVATION_STATE_ACCESSES.with(Cell::get),
            OWNER_COUNT,
            "colliding reads must record each scope dependency once: {kind:?}",
        );
        sum
    });
    assert_eq!(sum, (OWNER_COUNT * TRAVERSALS * 97) as i64);
    let certificate = certificate.expect("all repeated reads are coherent");
    assert!(certificate.unchanged());

    // A later scope also reuses the lifetime publication, while retaining
    // actual read dependencies in its own certificate.
    let (_, next_certificate) = capture(|| {
        OBSERVED_OWNER_PUBLICATIONS.with(|count| count.set(0));
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        for _ in 0..TRAVERSALS {
            for &owner in &owners {
                assert_eq!(kind.read(owner), TaggedValue::make_int(97));
            }
        }
        assert_eq!(OBSERVED_OWNER_PUBLICATIONS.with(Cell::get), 0);
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), OWNER_COUNT);
    });
    let next_certificate = next_certificate.expect("later reads remain coherent");
    assert!(next_certificate.unchanged());
    kind.mutate(owners[OWNER_COUNT / 2]);
    assert_eq!(
        kind.read(owners[OWNER_COUNT / 2]),
        TaggedValue::make_int(122)
    );
    assert!(!certificate.unchanged());
    assert!(!next_certificate.unchanged());
}

#[test]
fn observed_colliding_cons_traversals_publish_each_live_owner_once() {
    colliding_live_traversals(OwnerKind::Cons);
}

#[test]
fn observed_colliding_vector_traversals_publish_each_live_owner_once() {
    colliding_live_traversals(OwnerKind::Vector);
}

#[test]
fn observed_colliding_string_traversals_publish_each_live_owner_once() {
    colliding_live_traversals(OwnerKind::String);
}

#[test]
fn observed_off_certificate_replay_does_not_hide_first_real_owner_read() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owner = OwnerKind::Cons.allocate(&mut heap);

    force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
    let (_, old_certificate) = capture(|| owner.cons_car());
    let old_certificate = old_certificate.expect("coherent Off-mode read");
    assert!(!is_observed(owner.bits()));

    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let (value, certificate) = capture(|| {
        assert!(old_certificate.unchanged_and_observe());
        // Replaying identities cannot dereference their old headers. The
        // following actual owner read must still publish before its snapshot.
        assert!(!is_observed(owner.bits()));
        let value = owner.cons_car();
        assert!(
            is_observed(owner.bits()),
            "an Off-mode replay must not hide the first real Observed read",
        );
        value
    });
    assert_eq!(value, TaggedValue::make_int(97));
    let certificate = certificate.expect("replay and real read remain coherent");
    owner.set_car(TaggedValue::make_int(122));
    assert!(!certificate.unchanged());
}

#[test]
fn observed_mode_change_inside_capture_still_publishes_unmarked_owner() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owner = OwnerKind::Cons.allocate(&mut heap);
    let (value, certificate) = capture(|| {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
        assert_eq!(owner.cons_car(), TaggedValue::make_int(97));
        assert!(!is_observed(owner.bits()));
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        let value = owner.cons_car();
        assert!(is_observed(owner.bits()));
        value
    });
    assert_eq!(value, TaggedValue::make_int(97));
    let certificate = certificate.expect("policy changes retain the first read");
    owner.set_car(TaggedValue::make_int(122));
    assert!(!certificate.unchanged());
}

#[test]
fn observed_certificate_replay_records_colliding_live_dependencies_once() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owners: Vec<_> = (0..OWNER_COUNT)
        .map(|_| OwnerKind::Cons.allocate(&mut heap))
        .collect();
    let (_, original) = capture(|| {
        for &owner in &owners {
            assert_eq!(owner.cons_car(), TaggedValue::make_int(97));
        }
    });
    let original = original.expect("all original live reads are coherent");

    // Layout and scroll captures replay an earlier cache certificate before
    // reading its live dependencies again. More owners than RECENT slots
    // makes real repeated traversals miss that shortcut without any mutation.
    let (sum, replayed) = capture(|| {
        OBSERVED_OWNER_PUBLICATIONS.with(|count| count.set(0));
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        assert!(original.unchanged_and_observe());
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), OWNER_COUNT);
        assert_eq!(OBSERVED_OWNER_PUBLICATIONS.with(Cell::get), 0);
        let mut sum = 0_i64;
        for _ in 0..TRAVERSALS {
            for &owner in &owners {
                sum += owner.cons_car().as_int().expect("live Cons value");
            }
        }
        assert_eq!(OBSERVED_OWNER_PUBLICATIONS.with(Cell::get), 0);
        assert_eq!(
            OBSERVATION_STATE_ACCESSES.with(Cell::get),
            OWNER_COUNT,
            "a valid replay must retain its one dependency record across colliding real reads",
        );
        sum
    });
    assert_eq!(sum, (OWNER_COUNT * TRAVERSALS * 97) as i64);
    let replayed = replayed.expect("replayed and real reads remain coherent");
    assert!(original.unchanged());
    assert!(replayed.unchanged());
    let changed = owners[OWNER_COUNT / 2];
    changed.set_car(TaggedValue::make_int(122));
    assert_eq!(changed.cons_car(), TaggedValue::make_int(122));
    assert!(!original.unchanged());
    assert!(!replayed.unchanged());
}
