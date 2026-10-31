//! The native parser registry owns its tag independently of the Lisp record.
use super::*;
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::value::{HashTableTest, HashTableWeakness, LispHashTable};
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};

#[derive(Clone, Copy)]
enum FirstCollection {
    Full,
    Minor,
}

fn collect_minor_or_full(heap: &mut TaggedHeap, roots: Vec<Value>) {
    if heap.generational_enabled() {
        heap.begin_minor_collection()
            .expect("fresh GEN1 heap admits a minor");
        for root in roots {
            heap.seed_root(root);
        }
        heap.complete_minor_collection();
        heap.finish_incremental_sweep_now();
    } else {
        // GEN0 refuses minors and preserves the edge through a full cycle.
        assert!(heap.begin_minor_collection().is_err());
        heap.collect_exact(roots.into_iter());
    }
}

fn native_parser_tag_survives(first: FirstCollection) {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let mut manager = TreeSitterManager::new();
    let language = Value::symbol("json");
    let tag = Value::from_sym_id(intern_uninterned("native-parser-only-tag"));
    let id = manager.insert_parser(
        Value::NIL,
        BufferId(7),
        BufferId(7),
        language.as_bare_symbol_id().unwrap(),
        tag,
        Parser::new(),
        false,
    );
    let record = make_parser_value(id, language, Value::NIL, tag);
    manager.parser_mut(id).unwrap().value = record;
    // Parser records expose this setter, independently of ParserEntry.tag.
    // No parsing is needed: the canonical native owner is real, and a new
    // Parser does not need a grammar until it parses input.
    assert!(record.set_record_slot(PARSER_SLOT_TAG, Value::NIL));
    assert_eq!(
        record.as_record_data().unwrap()[PARSER_SLOT_TAG],
        Value::NIL
    );
    assert_eq!(manager.parser(id).unwrap().tag, tag);

    // The weak key does not root the tag. Its entry survives a full GC
    // exactly when the independent native tag edge was traced.
    let witness = heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    ));
    let payload = heap.alloc_cons(Value::fixnum(173), Value::NIL);
    crate::tagged::mutate::with_hash_table_mut(witness, |table| {
        table.insert(tag.to_hash_key(&HashTableTest::Eq), tag, payload);
    });
    let roots: Vec<Value> = manager.roots().into_iter().chain([witness]).collect();
    match first {
        FirstCollection::Full => heap.collect_exact(roots.into_iter()),
        FirstCollection::Minor => collect_minor_or_full(&mut heap, roots),
    }
    for _ in 0..3 {
        heap.collect_exact(manager.roots().into_iter().chain([witness]));
        assert_eq!(manager.parser(id).unwrap().tag, tag);
        let table = witness.as_hash_table().unwrap();
        assert_eq!(
            table.data.len(),
            1,
            "the native-only parser tag must stay live after full collection"
        );
        assert_eq!(table.data.entries_in_slot_order().next().unwrap().key, tag);
        assert!(heap.owns_heap_value_for_test(payload));
        assert_eq!(payload.cons_car(), Value::fixnum(173));
    }
}

#[test]
fn native_parser_tag_survives_full_collection_without_record_alias() {
    native_parser_tag_survives(FirstCollection::Full);
}

#[test]
fn native_parser_tag_survives_minor_then_full_collection_without_record_alias() {
    native_parser_tag_survives(FirstCollection::Minor);
}
