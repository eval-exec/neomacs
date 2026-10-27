//! Seed-prefix position mapping in `restore_dump_symbol_table` (pdump v12).
//!
//! The dump table is the dumper's full interner in id order, so its leading
//! slots are the constructor seeds (nil, t, the NON-canonical `unbound`
//! sentinel, then the well-known symbol table in declaration order).
//! Interning the sentinel by name cannot reproduce its id —
//! uninterned symbols never unify by name — which shifted every later slot
//! by one and forced the 127K-entry symbol fixup walk on every load. The
//! position map restores identity on a fresh registry; these tests pin the
//! identity math and the fallback behaviors the adversarial review required.

use super::*;

fn unibyte_name(bytes: &[u8]) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(bytes.to_vec())
}

/// The shape a real dump table has: the live constructor prefix in id order
/// (nil, t, unbound, then every well-known symbol), followed by ordinary
/// canonical symbols and non-canonical (`make-symbol`-style) residents.
fn seed_prefixed_table() -> (Vec<crate::heap_types::LispString>, Vec<u32>, Vec<bool>) {
    let mut table = SymbolRegistry::new().dump_symbol_table();
    for (name, canonical) in [
        (&b"seedtest-foo"[..], true),
        (&b"seedtest-residual"[..], false),
        (&b"seedtest-bar"[..], true),
    ] {
        table.names.push(unibyte_name(name));
        table.symbol_names.push((table.names.len() - 1) as u32);
        table.canonical.push(canonical);
    }
    (table.names, table.symbol_names, table.canonical)
}

#[test]
fn fresh_registry_restore_of_a_seed_prefixed_table_is_identity() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    let (names, symbol_names, canonical) = seed_prefixed_table();
    let remap = registry
        .restore_dump_symbol_table(&names, &symbol_names, Some(&canonical))
        .expect("seed-prefixed table should restore");
    let ids: Vec<u32> = remap.symbols.iter().map(|id| id.0).collect();
    let expected: Vec<u32> = (0..(SEED_SYMBOL_COUNT + 3) as u32).collect();
    assert_eq!(
        ids, expected,
        "a fresh registry must map a seed-prefixed dump table to the identity"
    );
    // The prefix mapped by position, not by allocation: the non-canonical
    // sentinel unified with the live seed instead of shifting the tail.
    assert!(!registry.is_canonical_id(remap.symbols[2]));
    assert!(registry.is_canonical_id(remap.symbols[SEED_SYMBOL_COUNT]));
    assert_eq!(
        registry.lookup("seedtest-foo"),
        Some(remap.symbols[SEED_SYMBOL_COUNT])
    );
}

#[test]
fn pre_populated_registry_takes_the_fallback_and_stays_correct() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    // One pre-load intern — the situation of every in-process test load and
    // the bootstrap cache-miss reload. Identity is impossible; resolution
    // must still be right.
    let intruder = registry.intern("pre-load-intruder");
    let (names, symbol_names, canonical) = seed_prefixed_table();
    let remap = registry
        .restore_dump_symbol_table(&names, &symbol_names, Some(&canonical))
        .expect("fallback restore should succeed");
    // The whole constructor prefix still position-maps (the intruder sits
    // above it)...
    for (slot, id) in remap.symbols.iter().take(SEED_SYMBOL_COUNT).enumerate() {
        assert_eq!(id.0 as usize, slot);
    }
    // ...but the tail is shifted past the intruder: NOT identity.
    let foo = SEED_SYMBOL_COUNT;
    assert!(remap.symbols[foo].0 as usize > foo);
    assert_ne!(remap.symbols[foo], intruder);
    assert_eq!(registry.lookup("seedtest-foo"), Some(remap.symbols[foo]));
    assert!(!registry.is_canonical_id(remap.symbols[foo + 1]));
    assert_eq!(
        registry.lookup("seedtest-bar"),
        Some(remap.symbols[foo + 2])
    );
}

#[test]
fn legacy_canonical_unbound_slot_skips_the_position_map() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    // Legacy derived-flag images mark slot 2's "unbound" CANONICAL; the live
    // seed is non-canonical, so the position map must refuse (canonicality
    // mismatch) and the slot falls through to the ordinary canonical path.
    let names = vec![
        unibyte_name(b"nil"),
        unibyte_name(b"t"),
        unibyte_name(b"unbound"),
    ];
    let remap = registry
        .restore_dump_symbol_table(&names, &[0, 1, 2], Some(&[true, true, true]))
        .expect("legacy table should restore");
    assert_eq!(remap.symbols[0].0, 0);
    assert_eq!(remap.symbols[1].0, 1);
    // A fresh canonical `unbound` is allocated; the non-canonical sentinel
    // at id 2 is not unified with it.
    assert_ne!(remap.symbols[2].0, 2);
    assert!(registry.is_canonical_id(remap.symbols[2]));
}

#[test]
fn duplicate_canonical_in_and_after_the_prefix_is_still_rejected() {
    crate::test_utils::init_test_tracing();
    let mut registry = SymbolRegistry::new();
    // A malformed image carrying a SECOND canonical `nil` after the
    // position-mapped one must keep the hard duplicate-canonical error —
    // the obarray-level duplicate check is debug-only, so degrading this to
    // last-wins would be a silent clobber in release.
    let names = vec![
        unibyte_name(b"nil"),
        unibyte_name(b"t"),
        unibyte_name(b"unbound"),
    ];
    let err = registry
        .restore_dump_symbol_table(&names, &[0, 1, 2, 0], Some(&[true, true, false, true]))
        .expect_err("duplicate canonical nil must be rejected");
    assert!(err.contains("canonical symbol slots"), "got: {err}");
}

#[test]
fn hand_built_tables_without_the_seed_shape_still_restore() {
    crate::test_utils::init_test_tracing();
    // The lenient per-slot fallback: tables that do not start with the seed
    // prefix (the existing hand-built test shapes) restore exactly as before.
    // `alpha` and `bar` are well-known spellings now (frame parameters), so
    // the hand-built probe uses names outside the constructor prefix.
    let mut registry = SymbolRegistry::new();
    let names = vec![
        unibyte_name(b"seedtest-alpha"),
        unibyte_name(b"seedtest-beta"),
    ];
    let remap = registry
        .restore_dump_symbol_table(&names, &[0, 1], Some(&[true, true]))
        .expect("non-seed table should restore");
    assert_eq!(registry.lookup("seedtest-alpha"), Some(remap.symbols[0]));
    assert_eq!(registry.lookup("seedtest-beta"), Some(remap.symbols[1]));
    assert!(
        remap.symbols[0].0 >= SEED_SYMBOL_COUNT as u32,
        "hand-built names must allocate above the constructor prefix"
    );
}
