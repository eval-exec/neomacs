//! Case-folded candidate scans in `re_search`.
//!
//! A case-folded search may skip a position only where the per-character loop
//! would have skipped it.  These tests pin down the facts that make the fast
//! scans exact, and compare them with the exhaustive scan.

use super::*;
use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

/// `AsciiPreimage::AsciiOnly` is a claim about every character code: the
/// standard translation keeps ASCII in ASCII and non-ASCII out of it.  The
/// folded candidate table and the folded literal prefilter both rest on it.
#[test]
fn standard_translation_keeps_non_ascii_out_of_ascii() {
    crate::test_utils::init_test_tracing();
    let table = CaseTranslation::standard();
    assert_eq!(table.ascii_preimage(), AsciiPreimage::AsciiOnly);
    for c in 0..0x80u32 {
        let translated = table.translate(c);
        assert!(
            translated < 0x80,
            "ASCII {c:#x} translates out of ASCII to {translated:#x}"
        );
    }
    let mut into_ascii = Vec::new();
    for c in 0x80..=emacs_char::MAX_CHAR {
        let translated = table.translate(c);
        if translated < 0x80 {
            into_ascii.push((c, translated));
        }
    }
    assert!(
        into_ascii.is_empty(),
        "non-ASCII characters translate into ASCII: {into_ascii:#x?}"
    );
}

/// A case-canon char-table can fold anything into ASCII, so it makes no
/// claim.
#[test]
fn char_table_translation_makes_no_ascii_preimage_claim() {
    crate::test_utils::init_test_tracing();
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    let translation = CaseTranslation::from_char_table(table);
    assert_eq!(translation.ascii_preimage(), AsciiPreimage::Unknown);
}

/// Compile `pattern` case-folded for a `repr` text and build its folded scan.
fn folded_scan_of(pattern: &str, repr: TextRepr) -> FoldedScan {
    let mut cp = regex_compile(pattern, false, true).expect("compile");
    cp.target_multibyte = repr == TextRepr::Multibyte;
    let table = cp.translate.clone().expect("a case-folded pattern");
    cp.folded_scan(&table, true)
        .expect("built on demand")
        .clone()
}

#[test]
fn folded_scan_kind_matches_the_pattern_shape() {
    crate::test_utils::init_test_tracing();
    use SparseAsciiFastmap::{One, Two};
    let multibyte = |pattern| folded_scan_of(pattern, TextRepr::Multibyte);
    let unibyte = |pattern| folded_scan_of(pattern, TextRepr::Unibyte);
    // One or two ASCII bytes can start a match: memchr.  The Kelvin sign is
    // not among `k`'s spellings, as in GNU's standard case table.
    assert!(matches!(multibyte("(defun"), FoldedScan::Sparse(One(b'('))));
    assert!(matches!(
        multibyte("defun"),
        FoldedScan::Sparse(Two(b'D', b'd'))
    ));
    assert!(matches!(
        multibyte("k"),
        FoldedScan::Sparse(Two(b'K', b'k'))
    ));
    // Six bytes: one table load per byte.
    let FoldedScan::Table(accept) = multibyte("[a-c]x") else {
        panic!("[a-c]x should scan with a table");
    };
    let accepted: Vec<u8> = (0..=u8::MAX).filter(|&b| accept[b as usize]).collect();
    assert_eq!(accepted, b"ABCabc");
    // A leading non-ASCII character needs the decoded character.
    assert!(matches!(multibyte("é"), FoldedScan::PerChar));
    assert!(matches!(multibyte("\\(?:x\\|é\\)"), FoldedScan::PerChar));
    // Unibyte text reads each byte as one character: the table covers them.
    assert!(matches!(unibyte("(defun"), FoldedScan::Sparse(One(b'('))));
    assert!(matches!(unibyte("é"), FoldedScan::Table(_)));

    // A case-canon char-table can change after compile: no tabulation.
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    let cp = regex_compile_lisp_with_translation(
        &crate::heap_types::LispString::from_utf8("k"),
        false,
        Some(CaseTranslation::from_char_table(table)),
    )
    .expect("compile");
    let translation = cp.translate.clone().expect("a case-folded pattern");
    assert!(matches!(
        cp.folded_scan(&translation, true),
        Some(FoldedScan::PerChar)
    ));
}

/// Patterns covering every folded scan shape and the fastmap gates.
const FOLDED_SCAN_PATTERNS: &[&str] = &[
    "(defun",
    "defun",
    "(defun \\([-a-z0-9]+\\)",
    "k",
    "s",
    "i",
    "K",
    "[a-c]x",
    "[k-m]",
    "[^a]",
    "\\_<let\\*?\\_>",
    "\\(catch\\|throw\\)",
    "x*y",
    "\\bfoo",
    "é",
    "\\(?:x\\|é\\)",
    "ß",
    "σ",
    "Ａ",
    ".",
    "^k",
    "",
    "a\\|",
    "\u{0}",
];

/// Multibyte texts: every character whose Unicode case partner is ASCII (K,
/// İ, ı, ſ), other case pairs, CJK, raw eight-bit characters (C0/C1 forms),
/// and malformed (non-overlong) sequences.
const FOLDED_SCAN_MULTIBYTE_TEXTS: &[&[u8]] = &[
    b"",
    "xK(DEFUN a)(defun b)k Kk".as_bytes(),
    "İıſ Iis ß ẞ SS Σσς Ａａ 中文 k\nK ſ".as_bytes(),
    "\n(deſun x)\n(Defun LET* let)".as_bytes(),
    b"a\xC1\xBFk\xC0\x80K \xC3\xA9\xC3\x89x",
    b"\xE3(Defun\xE3\x80(defun)\xE3\x80\x28\x00k",
];

/// Unibyte texts: one character per byte, Latin-1 case pairs among them.
const FOLDED_SCAN_UNIBYTE_TEXTS: &[&[u8]] = &[
    b"",
    b"x\xC9y\xE9 (DEFUN a) k K \xDF\xFF\xB5\xD7",
    b"\n(defun \xC9)\x00(Defun b) \xE9\xC3\xA9 let*",
];

/// Positions a search may start or stop at: character boundaries.
fn scan_positions(text: &[u8], repr: TextRepr) -> Vec<usize> {
    (0..=text.len())
        .filter(|&pos| repr == TextRepr::Unibyte || pos == text.len() || (text[pos] & 0xC0) != 0x80)
        .collect()
}

/// Which searches `assert_folded_scans_agree` compares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanDirections {
    Forward,
    Both,
}

/// Every folded scan finds exactly what the exhaustive scan finds, before and
/// after the scan is built, for every start and limit.
fn assert_folded_scans_agree(directions: ScanDirections) {
    let syntax = DefaultSyntaxLookup;
    for repr in [TextRepr::Multibyte, TextRepr::Unibyte] {
        let texts = match repr {
            TextRepr::Multibyte => FOLDED_SCAN_MULTIBYTE_TEXTS,
            TextRepr::Unibyte => FOLDED_SCAN_UNIBYTE_TEXTS,
        };
        for &pattern in FOLDED_SCAN_PATTERNS {
            let mut cp = regex_compile(pattern, false, true).expect("compile");
            cp.target_multibyte = repr == TextRepr::Multibyte;
            for &text in texts {
                let positions = scan_positions(text, repr);
                for &start in &positions {
                    for &limit in &positions {
                        if directions == ScanDirections::Forward && limit < start {
                            continue;
                        }
                        let search = || {
                            re_search(
                                &cp,
                                text,
                                start,
                                limit as isize - start as isize,
                                &syntax,
                                start,
                            )
                            .map(|(pos, regs)| (pos, regs.start, regs.end))
                        };
                        let expected = with_fastmap_disabled(search);
                        // Short texts leave the scan unbuilt the first time
                        // round: the per-character loop runs.
                        let unbuilt = search();
                        build_search_optimizations(&cp);
                        let built = search();
                        let context = format!(
                            "{pattern:?} {repr:?} {text:x?} {start} -> {limit}: scan {:?}",
                            cp.folded_scan(cp.translate.as_ref().expect("folded"), false)
                        );
                        assert_eq!(unbuilt, expected, "per-character loop: {context}");
                        assert_eq!(built, expected, "folded scan: {context}");
                    }
                }
            }
        }
    }
}

#[test]
fn folded_scan_agrees_with_exhaustive_candidates_forward() {
    crate::test_utils::init_test_tracing();
    assert_folded_scans_agree(ScanDirections::Forward);
}

/// Backward searches too: a limit below the start searches backward.
#[test]
fn folded_scan_agrees_with_exhaustive_candidates_both_ways() {
    crate::test_utils::init_test_tracing();
    assert_folded_scans_agree(ScanDirections::Both);
}

/// A long backward search builds the scan and walks it with memrchr: each
/// candidate is tried from the nearest one down, as GNU steps backward.
#[test]
fn folded_scan_backward_finds_the_nearest_candidate_first() {
    crate::test_utils::init_test_tracing();
    let cp = regex_compile("(defun \\([a-z]+\\)", false, true).expect("compile");
    let table = cp.translate.clone().expect("folded");
    let mut text = b"(DeFun one) (defun two) (DEFUN (".to_vec();
    text.extend(std::iter::repeat_n(b'x', 400));
    text.extend_from_slice(b"(Defun three)");
    let search = |start: usize, bound: usize| {
        re_search(
            &cp,
            &text,
            start,
            bound as isize - start as isize,
            &DefaultSyntaxLookup,
            start,
        )
        .map(|(pos, regs)| (pos, regs.end[0], regs.start[1]))
    };
    // (start, bound): the match may not extend past `start`.
    let probes = [
        (text.len(), 0),
        (431, 0),
        (20, 0),
        (text.len(), 13),
        (30, 13),
    ];
    let exhaustive: Vec<_> = with_fastmap_disabled(|| {
        probes
            .iter()
            .map(|&(start, bound)| search(start, bound))
            .collect()
    });
    assert_eq!(
        exhaustive,
        [
            Some((432, 444, 439)),
            Some((12, 22, 19)),
            // `[a-z]+` stops at the search start: "(defun t".
            Some((12, 20, 19)),
            Some((432, 444, 439)),
            None,
        ]
    );
    let scanned: Vec<_> = probes
        .iter()
        .map(|&(start, bound)| search(start, bound))
        .collect();
    assert_eq!(scanned, exhaustive);
    assert!(
        matches!(
            cp.folded_scan(&table, false),
            Some(FoldedScan::Sparse(SparseAsciiFastmap::One(b'(')))
        ),
        "a long backward search builds the scan"
    );
}

/// A folded scan is built with the fastmap it folds: a rebaked fastmap never
/// reuses a stale one.
#[test]
fn folded_scan_is_rebuilt_with_the_fastmap() {
    crate::test_utils::init_test_tracing();
    let syntax = DefaultSyntaxLookup;
    let mut compiled = regex_compile("a", false, true).expect("compile");
    let search = |pattern: &CompiledPattern, text: &[u8]| {
        re_search(pattern, text, 0, text.len() as isize, &syntax, 0)
            .map(|(pos, regs)| (pos, regs.end[0]))
    };
    build_search_optimizations(&compiled);
    let table = compiled.translate.clone().expect("folded");
    assert!(matches!(
        compiled.folded_scan(&table, false),
        Some(FoldedScan::Sparse(SparseAsciiFastmap::Two(b'A', b'a')))
    ));
    assert_eq!(search(&compiled, b"xA"), Some((1, 2)));
    let original = compiled.clone();
    // Change the literal operand without changing bytecode boundaries, so a
    // stale scan would still skip to `A`/`a` and miss the new byte.
    assert_eq!(&compiled.buffer[..3], &[RegexOp::Exactn as u8, 1, b'a']);
    compiled.buffer[2] = b'b';
    recompute_fastmap(&mut compiled, &syntax);
    assert!(
        compiled.folded_scan(&table, false).is_none(),
        "recomputing the fastmap drops the folded scan"
    );
    build_search_optimizations(&compiled);
    assert_eq!(search(&compiled, b"xB"), Some((1, 2)));
    assert_eq!(search(&compiled, b"xA"), None);
    assert_eq!(search(&original, b"xA"), Some((1, 2)));
    assert_eq!(search(&original, b"xB"), None);
}

/// Like the literal prefilter, the folded scan is built by the first search
/// long enough to use it; a short search runs the per-character loop.  (The
/// pattern's only literal is one byte, so no prefilter takes over.)
#[test]
fn folded_scan_is_built_by_the_first_long_search() {
    crate::test_utils::init_test_tracing();
    let cp = regex_compile("(\\w+", false, true).expect("compile");
    let table = cp.translate.clone().expect("folded");
    let short = b"xx (DeFun";
    let found = re_search(&cp, short, 0, short.len() as isize, &DefaultSyntaxLookup, 0);
    assert_eq!(found.map(|(pos, _)| pos), Some(3));
    assert!(
        cp.folded_scan(&table, false).is_none(),
        "a short search builds no scan"
    );
    let mut long = vec![b'x'; 1000];
    long.extend_from_slice(b"(DEFUN");
    let found = re_search(&cp, &long, 0, long.len() as isize, &DefaultSyntaxLookup, 0);
    assert_eq!(found.map(|(pos, _)| pos), Some(1000));
    assert!(
        matches!(
            cp.folded_scan(&table, false),
            Some(FoldedScan::Sparse(SparseAsciiFastmap::One(b'(')))
        ),
        "a long search builds and uses the scan"
    );
}

/// The folded prefilter's search equals the exhaustive search at every start,
/// forward and backward, in both representations.
fn assert_folded_prefilter_equiv(pattern: &str, text: &[u8]) {
    for target in [SearchTarget::Multibyte, SearchTarget::Unibyte] {
        for start in 0..=text.len() {
            let case = RegexCase::new(pattern, text, true, start, start).with_target(target);
            assert_eq!(
                check_regex_differential(case, RegexDifferential::SearchOptimizations),
                Ok(RegexCheck::Equivalent { comparisons: 2 }),
                "{pattern:?} {target} start={start}"
            );
        }
    }
}

#[test]
fn casefold_prefilter_is_built_and_sound() {
    crate::test_utils::init_test_tracing();
    let text = "x(DEFUN a) (Defun b) (defun c) (deſun d) (d\u{130}fun e) (defu \
                BYTE-COMPILE byte-compile xbyte-compile Byte-Compilex \
                let LET* Let*x (CATCH (Throw (rEqUiRe (\u{212A}ey) (key) (KEY) \
                ſ\u{212A}\u{130}\u{131} ß ẞ 中文 (defun"
        .as_bytes();
    for pattern in [
        "(defun \\([-a-z0-9]+\\)",
        "\\_<byte-compile\\_>",
        "\\_<let\\*?\\_>",
        "(\\(catch\\|throw\\|featurep\\|provide\\|require\\)\\_>",
        "(\\(key\\|KEY\\)",
        "Defun",
    ] {
        let cp = regex_compile(pattern, false, true).expect("compile");
        assert!(
            cp.literal_prefilter().is_some(),
            "{pattern:?} should get a folded prefilter"
        );
        assert_folded_prefilter_equiv(pattern, text);
    }
    // No prefilter: a leading non-ASCII literal, and single-byte heads.
    for pattern in ["\u{e9}t\u{e9}", "k", "(\\w+"] {
        let cp = regex_compile(pattern, false, true).expect("compile");
        assert!(
            cp.literal_prefilter().is_none(),
            "{pattern:?} should get no prefilter"
        );
        assert_folded_prefilter_equiv(pattern, text);
    }
}

/// Only the ASCII spellings of a literal are needles; a literal past its
/// variant cap is cut short, and a long keyword set drops to a smaller cap.
#[test]
fn folded_literals_are_the_ascii_spellings_of_a_required_prefix() {
    crate::test_utils::init_test_tracing();
    let table = CaseTranslation::standard();
    let fold = |literals: &[&str]| {
        let literals: Vec<Vec<u8>> = literals.iter().map(|l| l.as_bytes().to_vec()).collect();
        fold_prefix_literals(&table, &literals).map(|mut needles| {
            needles.sort();
            needles
        })
    };
    assert_eq!(
        fold(&["(k-1"]),
        Some(vec![b"(K-1".to_vec(), b"(k-1".to_vec()])
    );
    // 2^4 spellings of "(defu"; the "n" would make 32.
    let defun = fold(&["(defun "]).expect("folded");
    assert_eq!(defun.len(), 16);
    assert!(defun.iter().all(|needle| needle.len() == 5));
    assert!(defun.contains(&b"(DeFu".to_vec()));
    // Five keywords at 16 spellings each pass 64 needles: cap 8 instead.
    let keywords =
        fold(&["(catch", "(throw", "(featurep", "(provide", "(require"]).expect("folded");
    assert_eq!(keywords.len(), 40);
    // A non-ASCII byte ends the prefix; a leading one leaves none.
    assert_eq!(
        fold(&["ab\u{e9}c"]),
        Some(vec![
            b"AB".to_vec(),
            b"Ab".to_vec(),
            b"aB".to_vec(),
            b"ab".to_vec()
        ])
    );
    assert_eq!(fold(&["\u{e9}t\u{e9}"]), None);
    // Nothing translates to an upper-case letter: unmatchable, no prefilter.
    assert_eq!(fold(&["D"]), None);
}

/// With the folded prefilter a case-folded search enters the matcher only
/// where a spelling of the literal starts; the folded memchr scan enters it
/// at every `(` and at the end of the text.
#[test]
fn casefold_candidate_entries_drop_to_literal_hits() {
    crate::test_utils::init_test_tracing();
    let text = b"( ( (DeFun a) ( (Defun b)";
    let search = |cp: &CompiledPattern| {
        let before = matcher_entry_count();
        let found = re_search(cp, text, 0, text.len() as isize, &DefaultSyntaxLookup, 0);
        (found.map(|(pos, _)| pos), matcher_entry_count() - before)
    };
    let cp = regex_compile("(defun x", false, true).expect("compile");
    build_search_optimizations(&cp);
    assert!(cp.literal_prefilter().is_some());
    assert_eq!(
        search(&cp),
        (None, 2),
        "prefilter: the two `(defun` spellings"
    );

    let mut memchr_only = regex_compile("(defun x", false, true).expect("compile");
    memchr_only.prefilter = std::cell::OnceCell::from(None);
    build_search_optimizations(&memchr_only);
    assert_eq!(
        search(&memchr_only),
        (None, 6),
        "folded memchr: five `(` and the end of the text"
    );
    let exhaustive = with_fastmap_disabled(|| search(&memchr_only));
    assert_eq!(exhaustive, (None, text.len() as u64 + 1));
}
