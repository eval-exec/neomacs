use super::*;

#[test]
fn repeated_ascii_lookups_decode_the_table_once_per_write_tick() {
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
