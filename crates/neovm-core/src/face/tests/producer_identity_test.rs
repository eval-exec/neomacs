use super::*;
use crate::emacs_core::intern::{
    intern, intern_calls, intern_lisp_string, intern_uninterned, is_canonical_id,
    reset_intern_calls, unintern_canonical_id,
};
use crate::heap_types::{GnuDeclaredCharacterCount, LispString};

struct ScratchRoots(usize);

impl ScratchRoots {
    fn new() -> Self {
        Self(crate::emacs_core::eval::save_scratch_gc_roots())
    }

    fn keep(&self, value: Value) -> Value {
        crate::emacs_core::eval::push_scratch_gc_root_slot(value);
        value
    }
}

impl Drop for ScratchRoots {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
    }
}

// Literal pre-change named/reference recursion, independent of the new input
// constructors and certificate. Internal native compatibility, not a GNU oracle.
fn legacy_depth(table: &FaceTable, name: &str, depth: usize) -> Face {
    if depth > 10 {
        return Face::new(name);
    }
    let key = face_symbol_value(name);
    let Some(face) = table.faces.map.get(&key) else {
        return Face::new(name);
    };
    let mut result = face.clone();
    if let Some(inherit_ref) = face.inherit {
        let parent = legacy_reference(table, inherit_ref, depth + 1);
        result = parent.merge(&result);
    }
    result
}

fn legacy_reference(table: &FaceTable, face_ref: Value, depth: usize) -> Face {
    if depth > 40 {
        return Face::default();
    }
    if face_ref.is_nil() || face_ref.is_bare_symbol_named("nil") {
        return Face::default();
    }
    if let Some(name) = face_ref.as_bare_symbol_name() {
        return legacy_depth(table, name, depth);
    }
    let Some(items) = crate::emacs_core::value::list_to_vec(&face_ref) else {
        return Face::default();
    };
    if items.is_empty() {
        return Face::default();
    }
    if items[0]
        .as_bare_symbol_name()
        .is_some_and(|name| name.starts_with(':'))
    {
        let own = Face::from_plist("--inline--", &items);
        let parent = match own.inherit {
            Some(inherit_ref) => legacy_reference(table, inherit_ref, depth + 1),
            None => Face::default(),
        };
        parent.merge(&own)
    } else {
        let mut result = Face::default();
        for item in items.iter().rev() {
            result = result.merge(&legacy_reference(table, *item, depth + 1));
        }
        result
    }
}

fn foreground(name: &str, color: Color) -> Face {
    let mut face = Face::new(name);
    face.foreground = Some(color);
    face
}

#[test]
fn canonical_face_producer_uses_identity_without_intern_roundtrip() {
    let name = "cont9-face-producer-canonical";
    let mut table = FaceTable::new();
    let expected = foreground(name, Color::rgb(1, 2, 3));
    table.define(name, expected.clone());
    let value = Value::symbol(name);
    let producer = ProducerSymbolName::from_value(value).unwrap();
    reset_intern_calls();
    assert_eq!(Value::symbol(&producer).bits(), value.bits());
    assert_eq!(table.resolve_reference(value), expected);
    assert_eq!(intern_calls(), 0);
}

#[test]
fn uninterned_face_producer_keeps_legacy_canonical_name_fallback() {
    let name = "cont9-face-producer-uninterned";
    let mut table = FaceTable::new();
    table.define(name, foreground(name, Color::rgb(5, 6, 7)));
    let canonical = Value::symbol(name);
    let value = Value::from_sym_id(intern_uninterned(name));
    assert_ne!(value.bits(), canonical.bits());
    let producer = ProducerSymbolName::from_value(value).unwrap();
    reset_intern_calls();
    assert_eq!(Value::symbol(&producer).bits(), canonical.bits());
    assert_eq!(intern_calls(), 1);
    assert_eq!(
        table.resolve_reference(value),
        legacy_reference(&table, value, 0)
    );
}

#[test]
fn unintern_reintern_invalidates_previously_certified_face_producer() {
    let name = "cont9-face-producer-reintern";
    let old_id = intern(name);
    let old_value = Value::from_sym_id(old_id);
    let producer = ProducerSymbolName::from_value(old_value).unwrap();
    let mut table = FaceTable::new();
    table.define(name, foreground(name, Color::rgb(10, 20, 30)));
    assert_eq!(Value::symbol(&producer).bits(), old_value.bits());
    assert!(unintern_canonical_id(old_id));
    let new_value = Value::symbol(name);
    assert_ne!(new_value.bits(), old_value.bits());
    table.define(name, foreground(name, Color::rgb(40, 50, 60)));
    assert_eq!(Value::symbol(&producer).bits(), new_value.bits());
    assert_eq!(
        table.resolve_reference(old_value),
        legacy_reference(&table, old_value, 0)
    );
    assert_eq!(
        table.resolve_reference(old_value).foreground,
        Some(Color::rgb(40, 50, 60))
    );
    reset_intern_calls();
    assert_eq!(
        Value::symbol(&ProducerSymbolName::from_value(new_value).unwrap()).bits(),
        new_value.bits()
    );
    assert_eq!(intern_calls(), 0);
}

#[test]
fn utf8_looking_unibyte_and_declared_count_names_keep_legacy_fallback() {
    for atom in [
        LispString::from_unibyte("cont9-face-é-raw".as_bytes().to_vec()),
        LispString::from_emacs_bytes_with_declared_chars(
            b"cont9-face-declared".to_vec(),
            GnuDeclaredCharacterCount::try_from(1).unwrap(),
        ),
    ] {
        let id = intern_lisp_string(&atom);
        assert!(is_canonical_id(id));
        let value = Value::from_sym_id(id);
        let producer = ProducerSymbolName::from_value(value).unwrap();
        let expected = face_symbol_value(producer.spelling());
        assert_ne!(expected.bits(), value.bits());
        let mut table = FaceTable::new();
        table.define(
            producer.spelling(),
            foreground(producer.spelling(), Color::rgb(11, 22, 33)),
        );
        table.update_faces(|faces| {
            faces.insert(value, foreground("raw-identity", Color::rgb(88, 99, 100)));
        });
        reset_intern_calls();
        assert_eq!(Value::symbol(&producer).bits(), expected.bits());
        assert_eq!(intern_calls(), 1);
        assert_eq!(
            table.resolve_reference(value),
            legacy_reference(&table, value, 0)
        );
    }
}

#[test]
fn ascii_multibyte_and_unicode_face_producers_keep_exact_identity() {
    for atom in [
        LispString::new("cont9-face-multibyte-ascii".to_owned(), true),
        LispString::from_utf8("cont9-face-λé"),
    ] {
        let id = intern_lisp_string(&atom);
        let value = Value::from_sym_id(id);
        let producer = ProducerSymbolName::from_value(value).unwrap();
        reset_intern_calls();
        assert_eq!(Value::symbol(&producer).bits(), value.bits());
        assert_eq!(intern_calls(), 0);
    }
}

#[test]
fn non_utf8_face_name_stays_outside_named_producer_admission() {
    let raw = LispString::from_unibyte(vec![b'c', b'9', 0xff]);
    let value = Value::from_sym_id(intern_lisp_string(&raw));
    assert!(ProducerSymbolName::from_value(value).is_none());
    let mut table = FaceTable::new();
    table.update_faces(|faces| {
        faces.insert(value, foreground("raw", Color::rgb(7, 8, 9)));
    });
    assert_eq!(
        table.resolve_reference(value),
        legacy_reference(&table, value, 0)
    );
    assert_eq!(table.resolve_reference(value), Face::default());
}

#[test]
fn depth_refusal_precedes_noncanonical_face_name_fallback() {
    let value = Value::from_sym_id(intern_uninterned("cont9-face-depth-refusal"));
    let producer = ProducerSymbolName::from_value(value).unwrap();
    let table = FaceTable::new();
    reset_intern_calls();
    let result = table.resolve_producer_depth(&producer, 11);
    assert_eq!(intern_calls(), 0);
    assert_eq!(result, Face::new(producer.spelling()));
}

#[test]
fn face_producer_resolution_preserves_lists_plists_depth_and_source_identity() {
    let _context = crate::emacs_core::Context::new();
    let roots = ScratchRoots::new();
    let mut table = FaceTable::new();
    let mut parent = foreground("cont9-face-parent", Color::rgb(12, 34, 56));
    parent.family = Some(roots.keep(Value::string("family λ")));
    parent.doc = Some(roots.keep(Value::string("doc")));
    parent.foundry = Some(roots.keep(Value::string("foundry")));
    parent.stipple = Some(roots.keep(Value::string("stipple")));
    table.define("cont9-face-parent", parent.clone());
    let mut child = Face::new("cont9-face-child");
    child.inherit = Some(Value::symbol("cont9-face-parent"));
    child.weight = Some(FontWeight::BOLD);
    table.define("cont9-face-child", child);
    for reference in [
        Value::symbol("cont9-face-child"),
        roots.keep(Value::list(vec![
            Value::symbol("cont9-face-child"),
            Value::symbol("italic"),
        ])),
        roots.keep(Value::list(vec![
            Value::keyword("inherit"),
            Value::symbol("cont9-face-child"),
            Value::keyword("overline"),
            Value::T,
        ])),
        Value::NIL,
        Value::from_sym_id(intern_uninterned("nil")),
        Value::symbol("cont9-face-missing"),
        Value::fixnum(23),
    ] {
        assert_eq!(
            table.resolve_reference(reference),
            legacy_reference(&table, reference, 0)
        );
    }
    let result = table.resolve("cont9-face-child");
    for (got, original) in [
        (result.family, parent.family),
        (result.doc, parent.doc),
        (result.foundry, parent.foundry),
        (result.stipple, parent.stipple),
    ] {
        assert_eq!(got.map(Value::bits), original.map(Value::bits));
    }
    let mut cyclic = Face::new("cont9-face-cycle");
    cyclic.inherit = Some(Value::symbol("cont9-face-cycle"));
    table.define("cont9-face-cycle", cyclic);
    assert_eq!(
        table.resolve("cont9-face-cycle"),
        legacy_depth(&table, "cont9-face-cycle", 0)
    );
}

#[test]
fn producer_constructor_preserves_symbol_accounting_tags_and_fallback() {
    use crate::tagged::gc::MemoryUseCountSlot;
    let _context = crate::emacs_core::Context::new();
    for value in [
        Value::symbol("cont9-account-ordinary"),
        Value::symbol(":cont9-account-keyword"),
        Value::NIL,
        Value::T,
        Value::from_sym_id(intern_uninterned("cont9-account-fallback")),
        Value::from_sym_id(intern_uninterned(":cont9-account-keyword-fallback")),
        Value::from_sym_id(intern_uninterned("nil")),
        Value::from_sym_id(intern_uninterned("t")),
    ] {
        let producer = ProducerSymbolName::from_value(value).unwrap();
        let before = Value::memory_use_counts_snapshot();
        let legacy = Value::symbol(producer.spelling());
        let after = Value::memory_use_counts_snapshot();
        let legacy_symbols = after[MemoryUseCountSlot::Symbols.index()]
            .wrapping_sub(before[MemoryUseCountSlot::Symbols.index()]);
        let before = Value::memory_use_counts_snapshot();
        let actual = Value::symbol(&producer);
        let after = Value::memory_use_counts_snapshot();
        assert_eq!(actual.bits(), legacy.bits());
        assert_eq!(actual.is_bare_keyword(), legacy.is_bare_keyword());
        assert_eq!(
            after[MemoryUseCountSlot::Symbols.index()]
                .wrapping_sub(before[MemoryUseCountSlot::Symbols.index()]),
            legacy_symbols,
        );
    }
}

#[test]
fn public_face_producer_resolver_matches_existing_name_entry() {
    let mut table = FaceTable::new();
    for name in ["cont9-public-producer", ":cont9-public-keyword", "nil", "t"] {
        table.define(name, foreground(name, Color::rgb(13, 24, 35)));
        let value = Value::symbol(name);
        assert_eq!(table.resolve_producer(value), Some(table.resolve(name)));
        let uninterned = Value::from_sym_id(intern_uninterned(name));
        assert_eq!(
            table.resolve_producer(uninterned),
            Some(table.resolve(name))
        );
    }
    assert!(table.resolve_producer(Value::fixnum(2)).is_none());
}

#[test]
fn fixed_face_resolver_preserves_name_tags_and_symbol_accounting() {
    use crate::emacs_core::intern::WellKnown;
    use crate::tagged::gc::MemoryUseCountSlot;
    let _context = crate::emacs_core::Context::new();
    let mut table = FaceTable::new();
    for handle in [WellKnown::Default, WellKnown::ModeLine, WellKnown::Fringe] {
        let name = handle.name();
        table.define(name, foreground(name, Color::rgb(31, 42, 53)));
        let before = Value::memory_use_counts_snapshot();
        let expected = table.resolve(name);
        let after = Value::memory_use_counts_snapshot();
        let expected_symbols = after[MemoryUseCountSlot::Symbols.index()]
            .wrapping_sub(before[MemoryUseCountSlot::Symbols.index()]);
        let before = Value::memory_use_counts_snapshot();
        reset_intern_calls();
        let actual = table.resolve_fixed_symbol(&handle);
        let after = Value::memory_use_counts_snapshot();
        assert_eq!(intern_calls(), 0);
        assert_eq!(actual, expected);
        assert_eq!(
            after[MemoryUseCountSlot::Symbols.index()]
                .wrapping_sub(before[MemoryUseCountSlot::Symbols.index()]),
            expected_symbols,
        );
        // The fixed table handle and the printed spelling are one identity.
        let ordinary = Value::symbol(name);
        for fixed in [Value::symbol(handle), Value::symbol(&handle)] {
            assert_eq!(fixed.bits(), ordinary.bits());
            assert_eq!(fixed.is_bare_keyword(), ordinary.is_bare_keyword());
        }
    }
}

#[test]
fn producer_constructor_observes_completed_cross_thread_unintern_reintern() {
    use crate::tagged::gc::MemoryUseCountSlot;
    // This Context and its accounting stay on the reader thread. The worker
    // receives only a native SymId and static spelling, never heap/Context data.
    let _reader_context = crate::emacs_core::Context::new();
    let name = "cont9-producer-cross-thread-reintern";
    let old_id = intern(name);
    let old_value = Value::from_sym_id(old_id);
    let producer = ProducerSymbolName::from_value(old_value).unwrap();
    assert_eq!(Value::symbol(&producer).bits(), old_value.bits());
    assert!(is_canonical_id(old_id)); // warm the reader's membership cache
    let fresh_id = std::thread::spawn(move || {
        assert!(unintern_canonical_id(old_id));
        let fresh = intern(name);
        assert_ne!(fresh, old_id);
        fresh
    })
    .join()
    .expect("native registry invalidator completed");
    reset_intern_calls();
    let before = Value::memory_use_counts_snapshot();
    let actual = Value::symbol(&producer);
    let after = Value::memory_use_counts_snapshot();
    assert_eq!(actual.bits(), Value::from_sym_id(fresh_id).bits());
    assert_ne!(actual.bits(), old_value.bits());
    assert_eq!(
        intern_calls(),
        1,
        "old producer must take the live spelling fallback"
    );
    assert_eq!(
        after[MemoryUseCountSlot::Symbols.index()]
            .wrapping_sub(before[MemoryUseCountSlot::Symbols.index()]),
        1,
    );
}
