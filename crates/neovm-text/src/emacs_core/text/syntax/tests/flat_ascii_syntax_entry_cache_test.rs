use super::*;

#[test]
fn repeated_ascii_lookups_decode_the_table_once_per_write_tick() {
    crate::test_utils::init_test_tracing();
    // The persistent flat ASCII cache must survive across `SyntaxPropByteRun`
    // instances (one is built per `re-search-forward`), so font-lock's
    // thousands of searches do not re-decode every ASCII char through
    // `ct_lookup`. Before: N searches x distinct chars decodes; after: 128.
    let table = SyntaxTable::new_standard();
    // Warm the persistent cache once (fills 0..128).
    let _ = flat_ascii_syntax_entry(&table, b'a');
    reset_syntax_table_decodes_for_test();

    // A fresh per-scan memo per "search", each looking up several chars.
    for _ in 0..50 {
        let run = SyntaxPropByteRun::new(SyntaxProperties::Ignore);
        for &c in b"abc(); \t\ndefun-let" {
            let _ = run.ascii_entry(&table, c as char);
        }
    }
    assert_eq!(
        syntax_table_decodes_for_test(),
        0,
        "a warm persistent cache serves ASCII entries without re-decoding"
    );

    // The entries returned still match a direct table decode.
    for &c in b"a(); \tX9_-" {
        assert_eq!(
            flat_ascii_syntax_entry(&table, c),
            syntax_entry_from_table(&table, c as char),
            "flat ASCII cache must agree with a direct decode for {}",
            c as char
        );
    }
}

#[test]
fn alternating_tables_stay_resident_in_the_flat_ascii_cache() {
    crate::test_utils::init_test_tracing();
    // elb-smie alternates between the buffer's syntax table and a
    // `with-syntax-table` CPP table. A one-slot cache refilled all 128
    // entries on every switch; the LRU must keep both resident.
    let table_a = SyntaxTable::new_standard().copy_syntax_table();
    let mut table_b = table_a.copy_syntax_table();
    table_b.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(
        flat_ascii_syntax_entry(&table_a, b'a').class,
        SyntaxClass::Word
    );
    assert_eq!(
        flat_ascii_syntax_entry(&table_b, b'a').class,
        SyntaxClass::Punctuation
    );
    reset_syntax_table_decodes_for_test();
    for _ in 0..200 {
        assert_eq!(
            flat_ascii_syntax_entry(&table_a, b'a').class,
            SyntaxClass::Word
        );
        assert_eq!(
            flat_ascii_syntax_entry(&table_b, b'a').class,
            SyntaxClass::Punctuation
        );
    }
    assert_eq!(
        syntax_table_decodes_for_test(),
        0,
        "both alternating tables must stay resident instead of refilling"
    );
}

#[test]
fn parser_flat_ascii_keeps_alternating_tables_resident() {
    crate::test_utils::init_test_tracing();
    let table_a = SyntaxTable::new_standard().copy_syntax_table();
    let mut table_b = table_a.copy_syntax_table();
    table_b.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(
        flat_ascii_entries_for_table(&table_a)[b'a' as usize].class,
        SyntaxClass::Word
    );
    assert_eq!(
        flat_ascii_entries_for_table(&table_b)[b'a' as usize].class,
        SyntaxClass::Punctuation
    );
    reset_syntax_table_decodes_for_test();
    for _ in 0..200 {
        assert_eq!(
            flat_ascii_entries_for_table(&table_a)[b'a' as usize].class,
            SyntaxClass::Word
        );
        assert_eq!(
            flat_ascii_entries_for_table(&table_b)[b'a' as usize].class,
            SyntaxClass::Punctuation
        );
    }
    assert_eq!(
        syntax_table_decodes_for_test(),
        0,
        "the parser's flat ASCII classifier must keep both alternating tables resident"
    );
}

#[test]
fn a_char_table_write_invalidates_every_resident_table() {
    crate::test_utils::init_test_tracing();
    let table_a = SyntaxTable::new_standard().copy_syntax_table();
    let mut table_b = table_a.copy_syntax_table();
    let _ = flat_ascii_syntax_entry(&table_a, b'a');
    let _ = flat_ascii_syntax_entry(&table_b, b'a');
    reset_syntax_table_decodes_for_test();
    // Writing to B bumps the global char-table write tick: A's resident
    // entries are keyed on the old tick and must be re-decoded, not served.
    table_b.modify_syntax_entry('b', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(
        flat_ascii_syntax_entry(&table_a, b'a').class,
        SyntaxClass::Word
    );
    assert_eq!(
        syntax_table_decodes_for_test(),
        128,
        "a write must force a full refill of the other resident table"
    );
}

#[test]
fn rewriting_a_resident_table_is_visible() {
    crate::test_utils::init_test_tracing();
    let mut table = SyntaxTable::new_standard().copy_syntax_table();
    assert_eq!(
        flat_ascii_syntax_entry(&table, b'a').class,
        SyntaxClass::Word
    );
    table.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(
        flat_ascii_syntax_entry(&table, b'a').class,
        SyntaxClass::Punctuation
    );
    assert_eq!(
        flat_ascii_entries_for_table(&table)[b'a' as usize].class,
        SyntaxClass::Punctuation
    );
}

#[test]
fn more_tables_than_cache_slots_never_cross_contaminate() {
    crate::test_utils::init_test_tracing();
    let classes = [
        SyntaxClass::Word,
        SyntaxClass::Punctuation,
        SyntaxClass::Symbol,
        SyntaxClass::Open,
        SyntaxClass::Close,
        SyntaxClass::StringDelim,
    ];
    let mut tables = Vec::new();
    for &class in &classes {
        let mut table = SyntaxTable::new_standard().copy_syntax_table();
        table.modify_syntax_entry('a', SyntaxEntry::simple(class));
        tables.push(table);
    }
    // Six tables against four slots forces evictions; every revisit must
    // refill that table's own entries rather than answer from a stale slot.
    for _ in 0..2 {
        for (table, &class) in tables.iter().zip(&classes) {
            assert_eq!(flat_ascii_syntax_entry(table, b'a').class, class);
            for c in 0..128u8 {
                assert_eq!(
                    flat_ascii_syntax_entry(table, c),
                    syntax_entry_from_table(table, c as char),
                    "evicted/refilled slot must match a direct decode for {}",
                    c as char
                );
            }
        }
    }
}
