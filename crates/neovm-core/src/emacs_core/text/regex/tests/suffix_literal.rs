//! Bounded suffix scans compared with the exhaustive character matcher.

use super::*;
use crate::heap_types::LispString;

fn canon_table() -> Value {
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    for (source, canonical) in [
        ('Ж', 'ж'),
        ('Q', 'q'),
        ('Σ', 'σ'),
        ('ς', 'σ'),
        ('µ', 'μ'),
        ('Μ', 'μ'),
        ('中', 'é'),
    ] {
        fold(table, source, canonical);
    }
    table
}

fn fold(table: Value, source: char, canonical: char) {
    crate::emacs_core::chartable::ct_set_single(
        &table,
        source as i64,
        Value::fixnum(canonical as i64),
    );
}

fn compile_on(source: &LispString, posix: bool, table: Value) -> CompiledPattern {
    let mut cp = regex_compile_lisp_with_translation(
        source,
        posix,
        Some(CaseTranslation::from_char_table(table)),
    )
    .unwrap();
    cp.suffix_literal = suffix_literal::derive(&cp, true);
    cp
}

fn snapshot(found: Option<(usize, MatchRegisters)>) -> Option<(usize, Vec<i64>, Vec<i64>)> {
    found.map(|(at, regs)| (at, regs.start.into_vec(), regs.end.into_vec()))
}

fn assert_exhaustive(cp: &CompiledPattern, text: &[u8], start: usize, stop: usize) {
    let run = || {
        re_search(
            cp,
            text,
            start,
            (stop - start) as isize,
            &DefaultSyntaxLookup,
            start,
        )
    };
    let fast = snapshot(run());
    let exhaustive = snapshot(with_fastmap_disabled(run));
    assert_eq!(fast, exhaustive, "text {text:?}, {start}..{stop}");
}

#[test]
fn suffix_literal_scan_matches_exhaustive_bounds_and_case_tables() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    for posix in [false, true] {
        for source in ["жq", "σq", "µq", "éq"] {
            let cp = compile_on(&LispString::from_utf8(source), posix, table);
            assert!(cp.suffix_literal.is_some());
            for tail in [
                "ЖQ q жq ΣQ ςq μQ Μq µq 中Q éq",
                "qЖqQЖQqжqQжQ",
                "中qЖqΣq",
                "",
            ] {
                let text = format!("{}{}{}", "ж".repeat(160), tail, "q".repeat(160));
                let boundaries = text.char_indices().map(|(at, _)| at).collect::<Vec<_>>();
                for start in [0, boundaries[10], boundaries[150], boundaries[160]] {
                    for stop in [boundaries[155], boundaries[160], text.len() - 1, text.len()] {
                        if stop >= start {
                            assert_exhaustive(&cp, text.as_bytes(), start, stop);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn suffix_literal_scan_rechecks_wide_ascii_sources_after_mutation() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    let cp = compile_on(&LispString::from_utf8("жq"), false, table);
    let literal = cp.suffix_literal.as_ref().unwrap();
    let before = format!("{}ЖQ", "ж".repeat(160));
    assert!(matches!(
        literal.search(&cp, before.as_bytes(), 0, before.len()),
        suffix_literal::SuffixSearch::Finished(Some(_))
    ));
    fold(table, '中', 'q');
    let after = format!("{}Ж中", "ж".repeat(160));
    assert!(matches!(
        literal.search(&cp, after.as_bytes(), 0, after.len()),
        suffix_literal::SuffixSearch::Unavailable
    ));
    assert_exhaustive(&cp, after.as_bytes(), 0, after.len());
    fold(table, '中', 'é');
    assert!(matches!(
        literal.search(&cp, before.as_bytes(), 0, before.len()),
        suffix_literal::SuffixSearch::Finished(Some(_))
    ));
}

#[test]
fn suffix_literal_scan_keeps_unencountered_ascii_translations_unfilled() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    let cp = compile_on(&LispString::from_utf8(r"\(?:жq\)"), false, table);
    let mut baseline = cp.clone();
    baseline.suffix_literal = None;
    let run = |cp: &CompiledPattern, text: &str| {
        snapshot(re_search(
            cp,
            text.as_bytes(),
            0,
            text.len() as isize,
            &DefaultSyntaxLookup,
            0,
        ))
    };
    let warm = "ж".repeat(300);
    let baseline_warm = run(&baseline, &warm);
    assert_eq!(run(&cp, &warm), baseline_warm);
    assert_eq!(
        cp.translate.as_ref().unwrap().byte[b'Q' as usize].get(),
        CASE_TRANSLATION_UNFILLED,
        "a failed suffix scan must not freeze Q"
    );
    fold(table, 'Q', 'z');
    let changed = format!("{warm}ЖQ{}", "q".repeat(300));
    assert_eq!(run(&cp, &changed), run(&baseline, &changed));
    fold(table, 'Q', 'q');
    // The baseline has now encountered Q and frozen its old translation;
    // this assertion uses the exhaustive matcher of this pattern instead.
    assert_exhaustive(&cp, changed.as_bytes(), 0, changed.len());
}

#[test]
fn suffix_literal_scan_does_not_rewind_long_or_ascii_prefixes() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    for source in [
        "жжq",
        "жq+",
        "ж[q]",
        r"\(ж\)q",
        r"^жq",
        r"жq\|σq",
        "qq",
        "ж",
        "",
    ] {
        let cp = compile_on(&LispString::from_utf8(source), false, table);
        assert!(cp.suffix_literal.is_none(), "{source:?}");
    }
    let cp = compile_on(&LispString::from_utf8("жq"), false, table);
    assert!(suffix_literal::derive(&cp, false).is_none());
    let dense = format!("{}中{}ЖQ", "q".repeat(8_000), "q".repeat(8_000));
    assert_exhaustive(&cp, dense.as_bytes(), 0, dense.len());
}

#[test]
fn suffix_literal_scan_raw_bytes_and_representation_fallbacks() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    let mut raw = emacs_char::str_to_multibyte(&[0xff]);
    raw.push(b'q');
    let source = LispString::from_emacs_bytes(raw.clone());
    let cp = compile_on(&source, false, table);
    assert!(cp.suffix_literal.is_some());
    let mut text = "ж".repeat(160).into_bytes();
    text.extend_from_slice(&raw);
    assert_exhaustive(&cp, &text, 0, text.len());
    let unibyte = compile_on(&LispString::from_unibyte(vec![0xff, b'q']), false, table);
    assert!(unibyte.suffix_literal.is_none());
    let mut unibyte_target = cp.clone();
    unibyte_target.target_multibyte = false;
    let mut raw_text = vec![0xff; 320];
    raw_text.push(b'Q');
    assert_exhaustive(&unibyte_target, &raw_text, 0, raw_text.len());
}

#[test]
fn suffix_literal_scan_admits_whole_character_exactn_chunks_only() {
    crate::test_utils::init_test_tracing();
    let table = canon_table();
    let mut cp = compile_on(&LispString::from_utf8("жq"), false, table);
    let first = cp.buffer[2..4].to_vec();
    cp.buffer = vec![RegexOp::Exactn as u8, 2];
    cp.buffer.extend_from_slice(&first);
    cp.buffer
        .extend_from_slice(&[RegexOp::Exactn as u8, 1, b'q', RegexOp::Succeed as u8]);
    assert!(suffix_literal::derive(&cp, true).is_some());
    cp.buffer = vec![
        RegexOp::Exactn as u8,
        1,
        first[0],
        RegexOp::Exactn as u8,
        2,
        first[1],
        b'q',
        RegexOp::Succeed as u8,
    ];
    assert!(suffix_literal::derive(&cp, true).is_none());
}
