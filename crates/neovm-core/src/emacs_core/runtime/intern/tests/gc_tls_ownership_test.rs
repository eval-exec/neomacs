use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_symbol_names_are_rooted_only_in_their_heap() {
    let mut first = Context::new();
    let symbol = first
        .eval_str(r#"(make-symbol (copy-sequence "gc-tls-name"))"#)
        .unwrap();
    let id = symbol.as_symbol_id().unwrap();
    let name = materialize_symbol_name_value(id);
    let mut second = Context::new();
    let mut roots = Vec::new();
    collect_symbol_name_gc_roots(&mut roots, second.tagged_heap.identity());
    assert!(roots.iter().all(|root| root.bits() != name.bits()));
    second.gc_collect_exact();
    first.setup_thread_locals();
    let mut roots = Vec::new();
    collect_symbol_name_gc_roots(&mut roots, first.tagged_heap.identity());
    assert!(roots.iter().any(|root| root.bits() == name.bits()));
    first.gc_collect_exact();
    assert_eq!(name.as_utf8_str(), Some("gc-tls-name"));
}

#[test]
fn gc_tls_ownership_visible_name_cache_revalidates_on_context_switch() {
    let mut first = Context::new();
    let symbol = first
        .eval_str(r#"(make-symbol (copy-sequence "gc-tls-cache"))"#)
        .unwrap();
    let id = symbol.as_symbol_id().unwrap();
    let original = materialize_symbol_name_value(id);
    assert!(
        matches!(resolve_lisp_visible_symbol_name(id), LispVisibleSymbolName::LispObject(value) if value.bits() == original.bits())
    );
    let mut second = Context::new();
    let own = materialize_symbol_name_value(id);
    assert_ne!(original.bits(), own.bits());
    assert!(
        matches!(resolve_lisp_visible_symbol_name(id), LispVisibleSymbolName::LispObject(value) if value.bits() == own.bits())
    );
    second.gc_collect_exact();
    first.setup_thread_locals();
    assert!(
        matches!(resolve_lisp_visible_symbol_name(id), LispVisibleSymbolName::LispObject(value) if value.bits() == original.bits())
    );
    first.gc_collect_exact();
    assert_eq!(original.as_utf8_str(), Some("gc-tls-cache"));
}

#[test]
fn gc_tls_ownership_visible_name_cache_revalidates_after_heap_drop() {
    let id = {
        let mut first = Context::new();
        let symbol = first
            .eval_str(r#"(make-symbol (copy-sequence "gc-tls-dropped-name"))"#)
            .unwrap();
        let id = symbol.as_symbol_id().unwrap();
        assert!(matches!(
            resolve_lisp_visible_symbol_name(id),
            LispVisibleSymbolName::LispObject(_)
        ));
        id
    };
    let mut next = Context::new();
    let own = materialize_symbol_name_value(id);
    assert!(next.tagged_heap.owns_heap_value_for_test(own));
    assert!(
        matches!(resolve_lisp_visible_symbol_name(id), LispVisibleSymbolName::LispObject(value) if value.bits() == own.bits())
    );
    next.gc_collect_exact();
    assert_eq!(own.as_utf8_str(), Some("gc-tls-dropped-name"));
}

#[test]
fn gc_tls_ownership_interned_name_atoms_do_not_carry_heap_properties() {
    let mut first = Context::new();
    let symbol = first
        .eval_str("(make-symbol (propertize \"gc-tls-name-props\" 'gc-tls-property (vector 45)))")
        .unwrap();
    let id = symbol.as_symbol_id().unwrap();
    let exact = materialize_symbol_name_value(id);
    assert!(exact.as_lisp_string().unwrap().has_intervals());
    assert!(
        !resolve_sym_lisp_string(id).has_intervals(),
        "the process-lifetime name atom cloned unrooted heap properties"
    );
    let mut second = Context::new();
    let name = materialize_symbol_name_value(id);
    assert!(!name.as_lisp_string().unwrap().has_intervals());
    second.gc_collect_exact();
    first.setup_thread_locals();
    first.gc_collect_exact();
    assert!(exact.as_lisp_string().unwrap().has_intervals());
}

#[test]
fn name_atom_refs_strip_heap_properties_before_crossing_threads() {
    use crate::buffer::{CharLen, CharPos0, CharRange};
    use crate::tagged::gc::TaggedHeap;

    let mut heap = TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut heap);
    let payload = heap.alloc_string(LispString::from_utf8("name-atom-property-payload"));
    let property = TaggedValue::from_sym_id(intern_uninterned("name-atom-property"));
    let mut name = LispString::from_utf8("λ-name");
    let len = CharLen::new(name.schars());
    let range = CharRange::new(CharPos0::new(0), CharPos0::new(name.schars()));
    assert!(
        name.intervals_mut()
            .put_property_for_object_char_len(range, len, property, payload,)
    );
    assert!(name.has_intervals());

    let mut storage = NameAtomStorage::new();
    let atom = storage.push(name);
    let borrowed: &LispString = atom.borrow();
    assert!(!borrowed.has_intervals());
    let mut map = HashMap::with_hasher(FxBuildHasher);
    map.insert(atom, NameId(0));
    assert_eq!(map.get(&LispString::from_utf8("λ-name")), Some(&NameId(0)));

    // The frozen atom kept only bytes, so its former property cannot keep the
    // heap object alive. Reading the atom on a foreign thread needs no heap.
    heap.collect_exact(std::iter::empty());
    assert!(!heap.owns_heap_value_for_test(payload));
    std::thread::spawn(move || {
        let borrowed: &LispString = atom.borrow();
        assert_eq!(borrowed.as_utf8_str(), Some("λ-name"));
        assert!(!borrowed.has_intervals());
    })
    .join()
    .expect("frozen atom reader exits");
}
