use super::{plist_get_swp, plist_put_swp};
use crate::emacs_core::{Value, eval::Context};
use crate::tagged::{collection_reads::capture, mutate::LispCollectionRevision};

#[test]
fn plist_capture_keeps_positioned_key_and_property_dependencies() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let bare = Value::symbol("plist-capture-positioned-property");
    let key = eval
        .tagged_heap
        .alloc_symbol_with_pos(bare, Value::fixnum(3));
    let prop = eval
        .tagged_heap
        .alloc_symbol_with_pos(bare, Value::fixnum(7));
    let plist = Value::list(vec![key, Value::fixnum(11)]);

    for source in [key, prop] {
        let (answer, reads) = capture(|| plist_get_swp(plist, &prop, true));
        assert_eq!(answer, Some(Value::fixnum(11)));
        let reads = reads.expect("a pure plist query retains a coherent capture");
        assert!(reads.unchanged());
        // Positioned symbols have no mutating Lisp API. A central journal
        // notification still proves that the compared operand was observed.
        LispCollectionRevision::changed(source);
        assert!(!reads.unchanged());
    }
}

#[test]
fn plist_capture_replacement_still_invalidates_the_query() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let bare = Value::symbol("plist-capture-replacement");
    let key = eval
        .tagged_heap
        .alloc_symbol_with_pos(bare, Value::fixnum(3));
    let prop = eval
        .tagged_heap
        .alloc_symbol_with_pos(bare, Value::fixnum(7));
    let bare_value = Value::symbol("plist-capture-replaced-value");
    let old_value = eval
        .tagged_heap
        .alloc_symbol_with_pos(bare_value, Value::fixnum(11));
    let plist = Value::list(vec![key, old_value]);
    let (answer, reads) = capture(|| plist_get_swp(plist, &prop, true));
    assert_eq!(answer, Some(old_value));

    let (updated, changed) = plist_put_swp(plist, prop, bare_value, true)
        .expect("positioned properties match the existing bare-symbol binding");
    assert_eq!(updated.bits(), plist.bits());
    // GNU's EQ unwraps the old positioned value in this mode, so the
    // effective binding is unchanged while its collection write is retained.
    assert!(!changed);
    assert!(!reads.unwrap().unchanged());
    assert_eq!(plist_get_swp(plist, &bare, true), Some(bare_value));

    let (_, reads) = capture(|| plist_put_swp(plist, prop, Value::fixnum(17), true));
    assert!(
        reads.is_none(),
        "replacement follows an observed value read"
    );
}
