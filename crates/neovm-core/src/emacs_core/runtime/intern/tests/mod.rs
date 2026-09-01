use super::*;

fn unibyte_name(bytes: &[u8]) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(bytes.to_vec())
}

#[test]
fn name_interner_dedup() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let a = interner.intern("foo");
    let b = interner.intern("foo");
    let c = interner.intern("bar");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(interner.resolve(a), "foo");
    assert_eq!(interner.resolve(c), "bar");
}

#[test]
fn ordinary_symbol_slot_stays_compact() {
    assert!(
        std::mem::size_of::<SymbolSlot>() <= 2 * std::mem::size_of::<u32>(),
        "ordinary symbols must not pay for rare per-symbol metadata"
    );
}

#[test]
fn dense_thread_symbol_cache_stays_within_two_words() {
    assert!(
        std::mem::size_of::<SymbolCacheEntry>() <= 2 * std::mem::size_of::<usize>(),
        "dense symbol cache entries must remain at most two machine words"
    );
}

#[test]
fn dense_name_to_symbol_cache_uses_raw_u32_slots() {
    assert_eq!(std::mem::size_of::<u32>(), 4);
    assert_eq!(std::mem::size_of::<Option<SymId>>(), 8);
}

#[test]
fn immutable_symbol_name_resolution_reads_the_registry_once_per_thread() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("immutable-name-cache-registry-read-probe");

    reset_resolve_sym_lisp_string_registry_reads();
    for _ in 0..64 {
        std::hint::black_box(resolve_sym_lisp_string(symbol));
    }

    assert_eq!(resolve_sym_lisp_string_registry_reads(), 1);
}

/// GNU stores a Lisp string object directly in every symbol and
/// `Fsymbol_name` returns that same object on every call.  Symbols synthesized
/// from Rust text must lazily acquire the same per-heap identity instead of
/// cloning their process-lifetime name atom for each read.
#[test]
fn atom_only_symbol_name_materializes_one_lisp_object_per_heap() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let symbol = crate::emacs_core::value::Value::from_sym_id(intern(
        "atom-only-symbol-name-identity-probe",
    ));

    let first = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should materialize an atom-only name");
    let second = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should reuse the materialized name");

    assert_eq!(
        first.bits(),
        second.bits(),
        "GNU Fsymbol_name returns the symbol's one stored name object"
    );

    crate::emacs_core::builtins::collections::builtin_aset(vec![
        first,
        crate::emacs_core::value::Value::fixnum(0),
        crate::emacs_core::value::Value::fixnum('X' as i64),
    ])
    .expect("GNU permits mutation of the stored symbol-name string");
    let after_mutation =
        crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
            .expect("symbol-name should retain the mutated object");
    assert_eq!(after_mutation.bits(), first.bits());
    assert_eq!(
        after_mutation
            .as_lisp_string()
            .expect("symbol-name remains a string")
            .as_bytes()[0],
        b'X'
    );
}

#[test]
fn atom_only_symbol_names_are_cached_and_rooted_per_heap_identity() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("atom-only-symbol-name-per-heap-probe");

    let mut first_heap = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut first_heap);
    let first_heap_id = crate::tagged::gc::current_tagged_heap_identity().unwrap();
    let first = materialize_symbol_name_value(symbol);
    assert_eq!(materialize_symbol_name_value(symbol).bits(), first.bits());

    let mut second_heap = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut second_heap);
    let second_heap_id = crate::tagged::gc::current_tagged_heap_identity().unwrap();
    let second = materialize_symbol_name_value(symbol);
    assert_ne!(
        first.bits(),
        second.bits(),
        "heap-local Lisp objects must never cross evaluator heaps"
    );

    crate::tagged::gc::set_tagged_heap(&mut first_heap);
    assert_eq!(materialize_symbol_name_value(symbol).bits(), first.bits());
    let mut first_roots = Vec::new();
    collect_symbol_name_gc_roots(&mut first_roots, first_heap_id);
    assert!(first_roots.iter().any(|root| root.bits() == first.bits()));

    crate::tagged::gc::set_tagged_heap(&mut second_heap);
    let mut second_roots = Vec::new();
    collect_symbol_name_gc_roots(&mut second_roots, second_heap_id);
    assert!(second_roots.iter().any(|root| root.bits() == second.bits()));
    assert!(!second_roots.iter().any(|root| root.bits() == first.bits()));
}

#[test]
fn atom_only_symbol_name_resolution_skips_the_rare_exact_object_table() {
    crate::test_utils::init_test_tracing();
    let _eval = crate::emacs_core::eval::Context::new();
    let symbol = intern("atom-only-symbol-name-storage-probe");

    reset_symbol_name_value_probes();
    let materialized = materialize_symbol_name_value(symbol);
    assert!(materialized.is_string());
    let (exact_probes, materialized_probes) = symbol_name_value_probes();

    assert_eq!(
        exact_probes, 0,
        "an atom-backed symbol must not probe the table reserved for Lisp-supplied name objects"
    );
    assert!(
        materialized_probes >= 1,
        "atom-backed lookup must use the per-heap materialized-name table"
    );
}

#[test]
fn lisp_object_symbol_name_resolution_uses_its_declared_storage_first() {
    crate::test_utils::init_test_tracing();
    let _eval = crate::emacs_core::eval::Context::new();
    let name = crate::emacs_core::value::Value::string("lisp-object-symbol-name-storage-probe");
    let symbol = make_uninterned_symbol_with_name_value(name);

    reset_symbol_name_value_probes();
    assert_eq!(resolve_sym_name_value(symbol).unwrap().bits(), name.bits());
    assert_eq!(
        symbol_name_value_probes(),
        (1, 0),
        "a Lisp-created symbol should find its exact name object without probing the materialized fallback"
    );
}

#[test]
fn name_to_symbol_cache_round_trips_nonzero_ids() {
    let name_id = NameId(7);
    let sym_id = SymId(11);
    thread_local_record_canonical_symbol_for_name(name_id, sym_id);
    assert_eq!(
        thread_local_canonical_symbol_for_name(name_id),
        Some(sym_id)
    );

    thread_local_record_canonical_symbol_for_name(name_id, NIL_SYM_ID);
    assert_eq!(thread_local_canonical_symbol_for_name(name_id), None);
}

#[test]
fn runtime_intern() {
    crate::test_utils::init_test_tracing();
    let a = intern("hello");
    let b = intern("hello");
    let c = intern("world");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(resolve_sym(a), "hello");
    assert_eq!(resolve_sym(c), "world");
}

#[test]
fn runtime_intern_cache_borrows_canonical_name_storage() {
    crate::test_utils::init_test_tracing();
    let input = String::from("runtime-intern-cache-borrowed-name");
    let id = intern(&input);
    let canonical_name = resolve_sym(id);

    INTERN_STR_CACHE.with(|cache| {
        let cache = cache.borrow();
        let (cached_name, cached_id) = cache
            .get_key_value(input.as_str())
            .expect("intern should populate the thread-local string cache");
        assert_eq!(*cached_id, id);
        assert_eq!(cached_name.as_ptr(), canonical_name.as_ptr());
    });
}

#[test]
fn runtime_symbol_name_id_stable_across_growth() {
    crate::test_utils::init_test_tracing();
    let early = intern("early-runtime-name");
    let early_name = symbol_name_id(early);
    for i in 0..500 {
        intern(&format!("growth-runtime-{i}"));
    }
    assert_eq!(symbol_name_id(early), early_name);
    assert_eq!(resolve_name(early_name), "early-runtime-name");
}

#[test]
fn name_interner_empty_string() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let id = interner.intern("");
    assert_eq!(interner.resolve(id), "");
    assert_eq!(interner.intern(""), id);
}

#[test]
fn name_interner_many_strings() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let ids: Vec<NameId> = (0..1000)
        .map(|i| interner.intern(&format!("sym-{i}")))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(interner.resolve(*id), format!("sym-{i}"));
    }
    let unique: std::collections::HashSet<NameId> = ids.iter().copied().collect();
    assert_eq!(unique.len(), 1000);
}

#[test]
fn name_interner_keeps_addresses_stable_across_chunk_growth() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let first = interner.intern("first-chunked-name");
    let first_address = interner.resolve_lisp_string(first) as *const _;

    for i in 1..=NAME_ATOM_CHUNK {
        interner.intern(&format!("chunked-name-{i}"));
    }

    assert_eq!(
        interner.resolve_lisp_string(first) as *const _,
        first_address
    );
    assert_eq!(interner.resolve(first), "first-chunked-name");
    assert_eq!(
        interner.resolve(NameId(NAME_ATOM_CHUNK as u32)),
        format!("chunked-name-{NAME_ATOM_CHUNK}")
    );
}

#[test]
fn name_interner_idempotent() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let first = interner.intern("repeated");
    for _ in 0..100 {
        assert_eq!(interner.intern("repeated"), first);
    }
}

#[test]
fn name_interner_canonicalizes_ascii_multibyte_names_to_unibyte_atoms() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let multibyte = crate::heap_types::LispString::from_utf8("batch-byte-compile");
    let unibyte = crate::heap_types::LispString::from_unibyte(b"batch-byte-compile".to_vec());

    let from_multibyte = interner.intern_lisp_string(&multibyte);
    let from_unibyte = interner.intern_lisp_string(&unibyte);

    assert_eq!(from_multibyte, from_unibyte);
    let resolved = interner.resolve_lisp_string(from_multibyte);
    assert_eq!(resolved.as_bytes(), b"batch-byte-compile");
    assert!(!resolved.is_multibyte());
}

#[test]
fn name_interner_lookup_reuses_ascii_multibyte_canonical_atom() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let multibyte = crate::heap_types::LispString::from_utf8("symbol-name");

    let id = interner.intern_lisp_string(&multibyte);

    assert_eq!(interner.lookup("symbol-name"), Some(id));
    assert_eq!(interner.intern("symbol-name"), id);
}

#[test]
fn name_interner_borrowed_lookup_preserves_non_ascii_representation() {
    crate::test_utils::init_test_tracing();
    let mut interner = StringInterner::new();
    let text = "lambda-λ";
    let multibyte = crate::heap_types::LispString::from_utf8(text);
    let unibyte = crate::heap_types::LispString::from_unibyte(text.as_bytes().to_vec());

    let from_str = interner.intern(text);
    assert_eq!(interner.lookup(text), Some(from_str));
    assert_eq!(interner.intern_lisp_string(&multibyte), from_str);

    let from_unibyte = interner.intern_lisp_string(&unibyte);
    assert_ne!(from_unibyte, from_str);
    assert_eq!(interner.lookup_lisp_string(&unibyte), Some(from_unibyte));
}

#[test]
fn symid_copy_eq_hash() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let a = registry.intern("x");
    let b = a;
    assert_eq!(a, b);

    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
}

#[test]
fn resolve_sym_stable_across_growth() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let early = registry.intern("early");
    assert_eq!(registry.resolve(early), "early");
    for i in 0..500 {
        registry.intern(&format!("growth-{i}"));
    }
    assert_eq!(registry.resolve(early), "early");
}

#[test]
fn canonical_id_distinguishes_interned_from_uninterned_duplicates() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let canonical = registry.intern("dup");
    let uninterned = registry.intern_uninterned("dup");

    assert!(registry.is_canonical_id(canonical));
    assert!(!registry.is_canonical_id(uninterned));
    assert_eq!(registry.lookup("dup"), Some(canonical));
}

#[test]
fn runtime_registry_canonicalizes_ascii_multibyte_and_unibyte_names() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let multibyte = crate::heap_types::LispString::from_utf8("foo");
    let unibyte = crate::heap_types::LispString::from_unibyte(b"foo".to_vec());

    let from_multibyte = registry.intern_lisp_string(&multibyte);
    let from_unibyte = registry.intern_lisp_string(&unibyte);

    assert_eq!(from_multibyte, from_unibyte);
    let resolved = registry.resolve_lisp_string(from_multibyte);
    assert_eq!(resolved.as_bytes(), b"foo");
    assert!(!resolved.is_multibyte());
}

#[test]
fn canonical_id_survives_dump_style_reconstruction() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let remap = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"dup"),
            ],
            &[0, 1, 2, 2],
            None,
        )
        .expect("dump symbol table should restore");

    assert!(registry.is_canonical_id(remap.symbols[2]));
    assert!(!registry.is_canonical_id(remap.symbols[3]));
    assert_eq!(registry.lookup("dup"), Some(remap.symbols[2]));
}

#[test]
fn restore_dump_slots_remaps_reordered_layout() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let runtime_bar = registry.intern("bar");
    let runtime_foo = registry.intern("foo");

    let remap = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"bar"),
                unibyte_name(b"foo"),
            ],
            &[0, 1, 3, 2],
            Some(&[true, true, true, true]),
        )
        .expect("dump symbol table should restore");

    assert_eq!(
        remap.symbols,
        vec![NIL_SYM_ID, T_SYM_ID, runtime_foo, runtime_bar]
    );
}

#[test]
fn restore_dump_slots_preserves_lone_uninterned_slot() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let remap = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"solo"),
            ],
            &[0, 1, 2],
            Some(&[true, true, false]),
        )
        .expect("dump symbol table should restore");

    assert_eq!(registry.resolve(remap.symbols[2]), "solo");
    assert!(!registry.is_canonical_id(remap.symbols[2]));
    assert_eq!(registry.lookup("solo"), None);
}

#[test]
fn dump_symbol_table_separates_name_atoms_from_symbol_slots() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let canonical = registry.intern("shared-name");
    let uninterned = registry.intern_uninterned("shared-name");

    let dumped = registry.dump_symbol_table();

    let shared_name_id = registry.name_id(canonical);
    assert_eq!(registry.name_id(uninterned), shared_name_id);
    assert_eq!(
        dumped.names[shared_name_id.0 as usize],
        unibyte_name(b"shared-name")
    );
    assert_eq!(dumped.symbol_names[canonical.0 as usize], shared_name_id.0);
    assert_eq!(dumped.symbol_names[uninterned.0 as usize], shared_name_id.0);
    assert!(dumped.canonical[canonical.0 as usize]);
    assert!(!dumped.canonical[uninterned.0 as usize]);
}

#[test]
fn restore_dump_symbol_table_reuses_existing_name_atoms() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let existing = registry.intern("shared-name");
    let existing_name = registry.name_id(existing);

    let remap = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"shared-name"),
            ],
            &[0, 1, 2, 2],
            Some(&[true, true, true, false]),
        )
        .expect("dump symbol table should restore");

    assert_eq!(registry.name_id(remap.symbols[2]), existing_name);
    assert_eq!(registry.name_id(remap.symbols[3]), existing_name);
    assert!(registry.is_canonical_id(remap.symbols[2]));
    assert!(!registry.is_canonical_id(remap.symbols[3]));
}

#[test]
fn restore_dump_symbol_table_supports_multiple_independent_layouts() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();

    let first = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"foo"),
                unibyte_name(b"bar"),
            ],
            &[0, 1, 2, 3],
            Some(&[true, true, true, true]),
        )
        .expect("first dump symbol table should restore");

    let second = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"bar"),
                unibyte_name(b"foo"),
            ],
            &[0, 1, 2, 3],
            Some(&[true, true, true, true]),
        )
        .expect("second dump symbol table should restore");

    assert_eq!(registry.resolve(first.symbols[2]), "foo");
    assert_eq!(registry.resolve(first.symbols[3]), "bar");
    assert_eq!(registry.resolve(second.symbols[2]), "bar");
    assert_eq!(registry.resolve(second.symbols[3]), "foo");
    assert_eq!(first.symbols[2], second.symbols[3]);
    assert_eq!(first.symbols[3], second.symbols[2]);
}

#[test]
fn restore_dump_symbol_table_rejects_duplicate_canonical_names() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();

    let err = registry
        .restore_dump_symbol_table(
            &[
                unibyte_name(b"nil"),
                unibyte_name(b"t"),
                unibyte_name(b"dup"),
            ],
            &[0, 1, 2, 2],
            Some(&[true, true, true, true]),
        )
        .expect_err("duplicate canonical names should be rejected");

    assert!(
        err.contains("canonical symbol slots"),
        "unexpected error: {err}"
    );
    assert!(err.contains("dup"), "unexpected error: {err}");
}

#[test]
fn symbol_registry_exposes_name_ids_separately() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let canonical = registry.intern("shared-name");
    let uninterned = registry.intern_uninterned("shared-name");

    let canonical_name = registry.name_id(canonical);
    let uninterned_name = registry.name_id(uninterned);

    assert_eq!(canonical_name, uninterned_name);
    assert_eq!(registry.resolve_name(canonical_name), "shared-name");
    assert_ne!(canonical, uninterned);
}

#[test]
fn uninterned_symbol_name_value_roots_are_heap_owned() {
    crate::test_utils::init_test_tracing();

    let mut heap1 = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut heap1);
    let heap1_id = crate::tagged::gc::current_tagged_heap_identity().unwrap();
    let name = crate::emacs_core::value::Value::string("owned-symbol-name");
    let sym = make_uninterned_symbol_with_name_value(name);

    assert_eq!(resolve_sym_name_value(sym), Some(name));
    let mut roots = Vec::new();
    collect_symbol_name_gc_roots(&mut roots, heap1_id);
    assert!(roots.contains(&name));

    let mut heap2 = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut heap2);
    let heap2_id = crate::tagged::gc::current_tagged_heap_identity().unwrap();
    let mut roots = Vec::new();
    collect_symbol_name_gc_roots(&mut roots, heap2_id);

    assert!(!roots.contains(&name));
    assert_eq!(resolve_sym_name_value(sym), None);
    assert_eq!(
        resolve_sym_lisp_string(sym).as_utf8_str(),
        Some("owned-symbol-name")
    );
}

#[test]
fn shared_symbol_name_object_is_one_gc_root_per_heap() {
    crate::test_utils::init_test_tracing();

    let mut heap = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut heap);
    let heap_id = crate::tagged::gc::current_tagged_heap_identity().unwrap();
    let name = crate::emacs_core::value::Value::string("shared-symbol-name-root");

    let first = make_uninterned_symbol_with_name_value(name);
    let second = make_uninterned_symbol_with_name_value(name);
    assert_ne!(first, second, "make-symbol identity must remain unique");
    assert_eq!(resolve_sym_name_value(first), Some(name));
    assert_eq!(resolve_sym_name_value(second), Some(name));

    let equal_but_distinct_name =
        crate::emacs_core::value::Value::string("shared-symbol-name-root");
    assert_ne!(
        equal_but_distinct_name.bits(),
        name.bits(),
        "the regression fixture needs two equal strings with distinct identities"
    );
    let third = make_uninterned_symbol_with_name_value(equal_but_distinct_name);
    assert_eq!(resolve_sym_name_value(third), Some(equal_but_distinct_name));

    let mut roots = Vec::new();
    collect_symbol_name_gc_roots(&mut roots, heap_id);
    let shared_occurrences = roots
        .iter()
        .filter(|root| root.bits() == name.bits())
        .count();
    assert_eq!(
        shared_occurrences, 1,
        "one shared Lisp name object should occupy one GC root slot"
    );
    let distinct_occurrences = roots
        .iter()
        .filter(|root| root.bits() == equal_but_distinct_name.bits())
        .count();
    assert_eq!(
        distinct_occurrences, 1,
        "equal-but-distinct name objects must retain separate GC roots"
    );
}

#[test]
fn sym_id_debug_resolves_the_symbol_name() {
    crate::test_utils::init_test_tracing();
    // `Debug` must name the symbol (readable bug reports), not print a bare id.
    let id = intern("peculiar-debug-probe-symbol");
    let rendered = format!("{id:?}");
    assert!(
        rendered.starts_with("SymId(") && rendered.contains("peculiar-debug-probe-symbol"),
        "SymId Debug should include the resolved name, got {rendered}"
    );
    // A struct embedding SymId (e.g. a signal) inherits the readable form.
    let embedded = format!("{:?}", (id, 7u8));
    assert!(
        embedded.contains("peculiar-debug-probe-symbol"),
        "got {embedded}"
    );
}

#[test]
fn sym_id_debug_handles_raw_unibyte_symbol_names() {
    crate::test_utils::init_test_tracing();
    let id = intern_lisp_string(&unibyte_name(&[0xff]));
    let rendered = format!("{id:?}");

    assert!(
        rendered == format!("SymId({})", id.0) || rendered == format!(r"SymId({} \xFF)", id.0),
        "got {rendered}"
    );
}

#[test]
fn symbol_diagnostics_preserve_unibyte_name_identity() {
    crate::test_utils::init_test_tracing();
    let raw_utf8_bytes = intern_lisp_string(&unibyte_name(&[0xc3, 0xa9]));
    let raw_high_byte = intern_lisp_string(&unibyte_name(&[0xff]));
    let unicode = intern_lisp_string(&crate::heap_types::LispString::from_utf8("é"));
    let literal_escape = intern(r"\303\251");
    let numeric = intern("377");

    assert_eq!(
        format_symbol_name_for_diagnostic(raw_utf8_bytes),
        r"\xC3\xA9"
    );
    assert_eq!(format_symbol_name_for_diagnostic(raw_high_byte), r"\xFF");
    assert_eq!(format_symbol_name_for_diagnostic(unicode), "é");
    assert_eq!(
        format_symbol_name_for_diagnostic(literal_escape),
        r"\\303\\251"
    );
    assert_eq!(format_symbol_name_for_diagnostic(numeric), r"\377");
    assert_ne!(
        format_symbol_name_for_diagnostic(raw_high_byte),
        format_symbol_name_for_diagnostic(numeric)
    );
    assert_eq!(format_symbol_name_for_diagnostic(intern("?")), r"\?");
}

#[test]
fn symbol_diagnostics_use_the_lisp_visible_name_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let symbol_id = intern("diagnostic-live-name-object-probe");
    let symbol = crate::emacs_core::value::Value::from_sym_id(symbol_id);
    let name = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should materialize the Lisp-visible name object");

    crate::emacs_core::builtins::collections::builtin_aset(vec![
        name,
        crate::emacs_core::value::Value::fixnum(0),
        crate::emacs_core::value::Value::fixnum('X' as i64),
    ])
    .expect("GNU permits mutation of the stored symbol-name string");

    assert_eq!(
        format_symbol_name_for_diagnostic(symbol_id),
        "Xiagnostic-live-name-object-probe"
    );
}
