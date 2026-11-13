use super::*;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[test]
fn empty_compiled_window_does_not_initialize_collection_history() {
    crate::test_utils::init_test_tracing();
    assert_eq!(STATE_INITIALIZATIONS.load(Ordering::Relaxed), 0);
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    assert_eq!(compiled_observation_window(), (usize::MAX, 0));
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    assert_eq!(STATE_INITIALIZATIONS.load(Ordering::Relaxed), 0);

    let owner = heap.alloc_cons(TaggedValue::make_int(1), TaggedValue::NIL);
    let (_, certificate) = capture(|| owner.cons_car());
    let certificate = certificate.expect("first observed read has a certificate");
    assert_eq!(STATE_INITIALIZATIONS.load(Ordering::Relaxed), 1);
    let window = compiled_observation_window();
    let address = owner.bits() & !super::super::value::TAG_MASK;
    assert!(window.0 <= address && address < window.1);
    assert!(certificate.unchanged_and_observe());
    set_tagged_heap(&mut heap);
    assert_eq!(compiled_observation_window(), window);
    assert_eq!(STATE_INITIALIZATIONS.load(Ordering::Relaxed), 1);
    owner.set_car(TaggedValue::make_int(2));
    assert!(!certificate.unchanged_and_observe());
}
