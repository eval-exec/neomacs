//! Tests of the existence DFA (P3.3): the NFA over the rewind view (C2).

use super::*;
use crate::emacs_core::regex_emacs::{
    DefaultSyntaxLookup, fail_stack_push_sites, re_match, regex_compile,
};

fn nfa_of(pattern: &str) -> Nfa {
    let compiled = regex_compile(pattern, false, false).expect("pattern compiles");
    Nfa::build(&compiled).unwrap_or_else(|why| panic!("{pattern:?} is ineligible: {why:?}"))
}

fn place<'a>(text: &'a [u8], d: usize) -> Place<'a> {
    Place {
        text,
        d,
        stop: text.len(),
        point: 0,
        target_multibyte: true,
        syntax: &DefaultSyntaxLookup,
    }
}

/// The predicates of the positions the start closure reaches at `d`.
fn start_closure(nfa: &Nfa, text: &[u8], d: usize) -> (Vec<Predicate>, bool) {
    let mut scratch = ClosureScratch::default();
    let mut out = Vec::new();
    let accept = nfa.closure(
        &[kernel_item(0, 0)],
        &place(text, d),
        &mut scratch,
        &mut out,
    );
    out.sort_unstable();
    let predicates = out
        .iter()
        .map(|&p| nfa.predicates[nfa.positions[p as usize].predicate as usize])
        .collect();
    (predicates, accept)
}

#[test]
fn literal_positions_step_one_pattern_character_at_a_time() {
    let nfa = nfa_of("abca");
    assert_eq!(nfa.positions.len(), 4);
    // `a` twice: one predicate.
    assert_eq!(nfa.predicates.len(), 3);
    assert_eq!(nfa.positions[0].predicate, nfa.positions[3].predicate);
    let pc = nfa.positions[0].pc as usize;
    for (i, position) in nfa.positions.iter().enumerate() {
        assert_eq!(position.pc as usize, pc);
        assert_eq!(position.lit_off as usize, i);
    }
    assert_eq!(nfa.positions[0].next, kernel_item(pc, 1));
    assert_eq!(nfa.positions[2].next, kernel_item(pc, 3));
    // The last character resumes at the opcode after the literal.
    assert_eq!(nfa.positions[3].next, kernel_item(pc + 2 + 4, 0));
    assert_eq!(nfa.position_at(kernel_item(pc, 2)), Some(2));
    assert_eq!(nfa.position_at(kernel_item(pc, 5)), None);
}

#[test]
fn multibyte_literal_positions_follow_character_lengths() {
    let nfa = nfa_of("é中x");
    let offsets: Vec<u8> = nfa.positions.iter().map(|p| p.lit_off).collect();
    assert_eq!(offsets, vec![0, 2, 5]);
    let pc = nfa.positions[0].pc as usize;
    assert_eq!(nfa.positions[0].next, kernel_item(pc, 2));
    assert_eq!(nfa.positions[1].next, kernel_item(pc, 5));
    assert_eq!(nfa.positions[2].next, kernel_item(pc + 2 + 6, 0));
}

#[test]
fn every_consuming_opcode_is_one_position_with_its_test() {
    let nfa = nfa_of(".[a-z][^0-9]\\w\\W\\s-\\cg\\Cg");
    let kinds: Vec<Predicate> = nfa
        .positions
        .iter()
        .map(|p| nfa.predicates[p.predicate as usize])
        .collect();
    assert!(matches!(kinds[0], Predicate::AnyChar));
    assert!(matches!(kinds[1], Predicate::Charset { .. }));
    assert!(matches!(kinds[2], Predicate::Charset { .. }));
    assert!(matches!(kinds[3], Predicate::Syntax { negate: false, .. }));
    assert!(matches!(kinds[4], Predicate::Syntax { negate: true, .. }));
    assert!(matches!(kinds[5], Predicate::Syntax { negate: false, .. }));
    assert!(matches!(
        kinds[6],
        Predicate::Category {
            category: b'g',
            negate: false
        }
    ));
    assert!(matches!(
        kinds[7],
        Predicate::Category {
            category: b'g',
            negate: true
        }
    ));
    assert!(nfa.uses_categories);
    // The fused `\w\|\s_` is one position with a mask.
    let fused = nfa_of("\\(?:\\w\\|\\s_\\)+");
    assert!(
        fused
            .predicates
            .iter()
            .any(|p| matches!(p, Predicate::SyntaxSet { .. }))
    );
}

#[test]
fn closure_follows_both_edges_of_every_split() {
    let nfa = nfa_of("\\(?:ab\\|c\\)\\|d*e");
    let (predicates, accept) = start_closure(&nfa, b"", 0);
    assert!(!accept);
    // `a`, `c`, `d` and `e` (the empty `d*`).
    assert_eq!(predicates.len(), 4);
}

#[test]
fn closure_reads_the_rewind_view_of_keep_string_loops() {
    let compiled = regex_compile("[a-z]*:", false, false).unwrap();
    assert!(
        compiled
            .buffer
            .contains(&(RegexOp::OnFailureKeepStringJump as u8))
    );
    let nfa = Nfa::build(&compiled).unwrap();
    assert!(
        !nfa.bytecode
            .contains(&(RegexOp::OnFailureKeepStringJump as u8))
    );
    let (predicates, _) = start_closure(&nfa, b"", 0);
    assert_eq!(predicates.len(), 2, "the loop body and the continuation");
}

#[test]
fn closure_evaluates_assertions_at_the_real_position() {
    let nfa = nfa_of("^a\\|\\bb\\|c$");
    let text = b"xa b\nc";
    // At 0: `^` and `\b` hold (buffer start); `$` is tested after `c` only.
    assert_eq!(start_closure(&nfa, text, 0).0.len(), 3);
    // At 1 ("x|a"): no line start, no word boundary.
    assert_eq!(start_closure(&nfa, text, 1).0.len(), 1);
    // At 3 (" |b"): a word boundary.
    assert_eq!(start_closure(&nfa, text, 3).0.len(), 2);
    // At 5 ("\n|c"): a line start and a word boundary.
    assert_eq!(start_closure(&nfa, text, 5).0.len(), 3);
    let empty = nfa_of("x\\|^$");
    assert!(
        start_closure(&empty, b"a\n\nb", 2).1,
        "`^$` matches at an empty line"
    );
    assert!(!start_closure(&empty, b"a\n\nb", 1).1);
}

#[test]
fn eligibility_rejects_what_the_dfa_does_not_model() {
    for (pattern, why) in [
        ("\\(a\\)\\1", DfaIneligible::Backreference),
        ("a\\{2,3\\}", DfaIneligible::IntervalCounter),
        ("\\(?:a?\\)*?b", DfaIneligible::NullableNonGreedyLoop),
        ("a*", DfaIneligible::MatchesEmptyEverywhere),
        ("\\(?:a\\|\\)", DfaIneligible::MatchesEmptyEverywhere),
        ("", DfaIneligible::MatchesEmptyEverywhere),
    ] {
        let compiled = regex_compile(pattern, false, false).unwrap();
        assert_eq!(Nfa::build(&compiled).err(), Some(why), "{pattern:?}");
    }
    // An empty match behind a test is fine: the test can fail.
    for pattern in ["^a*", "\\(?:a\\|\\)\\b", "\\=", "x??y", "\\(a*\\)*b"] {
        let compiled = regex_compile(pattern, false, false).unwrap();
        assert!(Nfa::build(&compiled).is_ok(), "{pattern:?}");
    }
    // POSIX patterns are eligible: existence ignores which match is chosen.
    let posix = regex_compile("\\(a\\|ab\\)c", true, false).unwrap();
    assert!(Nfa::build(&posix).is_ok());
}

#[test]
fn nfa_counts_the_fail_stack_push_sites() {
    for pattern in ["a\\|b", "\\(a*\\)b", "[a-z]*:", "x\\(?:a\\|b\\)*c"] {
        let compiled = regex_compile(pattern, false, false).unwrap();
        let nfa = Nfa::build(&compiled).unwrap();
        assert_eq!(nfa.push_sites, fail_stack_push_sites(&compiled.buffer));
        assert!(nfa.push_sites > 0, "{pattern:?}");
    }
}

/// A closure that accepts at `p` is a zero-length match the backtracker must
/// find too (it may prefer a longer one).
#[test]
fn an_accepting_start_closure_is_a_match() {
    let syntax = DefaultSyntaxLookup;
    let text = b"ab \n cd\n\nx_y ";
    for pattern in [
        "^$",
        "\\b",
        "\\_>",
        "a?$",
        "\\(?:x\\|\\<\\)",
        "\\B\\|q",
        "\\'",
    ] {
        let compiled = regex_compile(pattern, false, false).unwrap();
        let nfa = Nfa::build(&compiled).unwrap();
        for p in 0..=text.len() {
            let (_, accept) = start_closure(&nfa, text, p);
            let matched = re_match(&compiled, text, p, text.len(), &syntax, 0);
            if accept {
                assert!(matched.is_some(), "{pattern:?} at {p}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Character classes (C3)
// ---------------------------------------------------------------------------

use crate::emacs_core::regex_emacs::{
    CaseTranslation, LookupClassKey, SyntaxCacheKey, match_anychar_at, match_categoryspec_at,
    match_charset_at, match_exactn_char_at, match_syntaxspec_at, match_syntaxspecset_at,
    regex_compile_lisp_with_translation,
};
use crate::emacs_core::syntax::SyntaxClass;
use crate::emacs_core::value::Value;

/// Patterns covering every predicate kind and fact.
const CLASS_PATTERNS: &[&str] = &[
    "abcABC019 _-!:\n",
    "éüß中日K\u{212A}ǅΣσςЖж😀",
    ".",
    "[a-z]",
    "[^a-z0-9]",
    "[A-Za-zé-üα-ω]",
    "[^中]",
    "[][^-]",
    "[[:alpha:]]",
    "[[:alnum:]_]",
    "[[:upper:]]",
    "[[:lower:]]",
    "[[:space:]]",
    "[[:word:]]",
    "[[:punct:]]",
    "[[:digit:][:xdigit:]]",
    "[[:ascii:]]",
    "[[:nonascii:]]",
    "[[:multibyte:]]",
    "[[:unibyte:]]",
    "[[:cntrl:][:blank:]]",
    "[[:graph:]]",
    "[[:print:]]",
    "[^[:space:]\n]",
    "\\w\\W",
    "\\s-\\s_\\s.\\sw\\S-",
    "\\(?:\\w\\|\\s_\\)+",
    "\\cg\\Cl\\c|\\ca\\cc",
    "\\bx\\B\\<y\\>",
    "\\_<z\\_>",
    "^a$",
    // org font-lock (P3.3 census)
    "\\(?:^[ \t]*[-+]\\|^[ \t]+[*]\\)[ \t]+\\(.*?[ \t]+::\\)\\([ \t]+\\|$\\)",
    "^\\*+ \\(?:.*[ \t]\\)?\\(:\\([[:alnum:]_@#%:]+\\):\\)[ \t]*$",
    "^[ \t]*|\\(?:.*?|\\)? *\\(:?=[^|\n]*\\)",
];

/// Every character code the class tests visit in multibyte text.
fn multibyte_sample() -> Vec<u32> {
    let mut codes: Vec<u32> = (0..0x100).collect();
    codes.extend([
        0xDF, 0x212A, 0x1C5, 0x3B1, 0x3A3, 0x3C2, 0x416, 0x436, 0x4E2D, 0x65E5, 0x1F600, 0x2018,
    ]);
    codes.extend([0x80u8, 0xA9, 0xC0, 0xFF].map(emacs_char::byte8_to_char));
    codes
}

fn encode(code: u32) -> Vec<u8> {
    let mut buf = [0u8; 8];
    let len = emacs_char::char_string(code, &mut buf);
    buf[..len].to_vec()
}

/// A syntax table unlike the standard one: `-` is a word constituent, the
/// newline ends comments, `é` and `中` are symbol constituents.
struct CustomTableLookup;

impl SyntaxLookup for CustomTableLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        match c {
            '-' => SyntaxClass::Word,
            '\n' => SyntaxClass::EndComment,
            'é' | '中' => SyntaxClass::Symbol,
            _ => crate::emacs_core::syntax::standard_syntax_class_for_char(c),
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Table {
            id: usize::MAX,
            epoch: 0,
        }
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: usize::MAX,
            category: 0,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

fn case_table(folds: &[(char, char)]) -> Value {
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    for &(from, to) in folds {
        crate::emacs_core::chartable::ct_set_single(&table, from as i64, Value::fixnum(to as i64));
    }
    table
}

/// The case settings of the class tests: none, the standard table, a custom
/// table, and one that folds non-ASCII characters into ASCII.
fn case_settings() -> Vec<Option<CaseTranslation>> {
    vec![
        None,
        Some(CaseTranslation::standard()),
        Some(CaseTranslation::from_char_table(case_table(&[
            ('A', 'a'),
            ('É', 'é'),
            ('Σ', 'σ'),
            ('ς', 'σ'),
        ]))),
        Some(CaseTranslation::from_char_table(case_table(&[
            ('\u{212A}', 'k'),
            ('K', 'k'),
            ('é', 'e'),
            ('É', 'e'),
        ]))),
    ]
}

/// The matcher's own test for `predicate` at the real position, with the
/// real lookup and the whole text as the limit.
fn direct_accepts(
    nfa: &Nfa,
    predicate: Predicate,
    pattern: &CompiledPattern,
    text: &[u8],
    d: usize,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    let stop = text.len();
    let tm = pattern.target_multibyte;
    match predicate {
        Predicate::Literal { pc, lit_off } => {
            let pc = pc as usize;
            let count = nfa.bytecode[pc + 1] as usize;
            match_exactn_char_at(
                &nfa.bytecode[pc + 2..pc + 2 + count],
                lit_off as usize,
                pattern.multibyte,
                tm,
                &pattern.translate,
                text,
                d,
                stop,
            )
            .map(|(_, len)| len)
        }
        Predicate::AnyChar => match_anychar_at(text, d, stop, tm, &pattern.translate),
        Predicate::Charset { pc } => match_charset_at(
            pattern,
            pc as usize,
            text,
            d,
            stop,
            tm,
            &pattern.translate,
            syntax,
        ),
        Predicate::Syntax { class, negate } => {
            match_syntaxspec_at(class, negate, text, d, stop, tm, syntax)
        }
        Predicate::SyntaxSet { mask } => match_syntaxspecset_at(mask, text, d, stop, tm, syntax),
        Predicate::Category { category, negate } => {
            match_categoryspec_at(category, negate, text, d, stop, tm, syntax)
        }
    }
}

/// Exhaustive class equivalence: for every predicate of every class pattern,
/// every character of the samples, 4 case settings and 2 syntax tables, in
/// both representations, the class bit is the matcher's own test of that
/// character inside a real text, and one character gets one class wherever
/// it appears.
#[test]
fn classes_agree_with_the_matcher_tests_on_every_sample_character() {
    let lookups: [&dyn SyntaxLookup; 2] = [&DefaultSyntaxLookup, &CustomTableLookup];
    let mut checked = 0usize;
    for source in CLASS_PATTERNS {
        for translate in case_settings() {
            let lisp = crate::heap_types::LispString::from_utf8(source);
            let mut compiled =
                regex_compile_lisp_with_translation(&lisp, false, translate.clone()).unwrap();
            for multibyte in [true, false] {
                compiled.target_multibyte = multibyte;
                let Ok(nfa) = Nfa::build(&compiled) else {
                    continue;
                };
                let samples: Vec<Vec<u8>> = if multibyte {
                    multibyte_sample().into_iter().map(encode).collect()
                } else {
                    (0..=255u8).map(|b| vec![b]).collect()
                };
                for &syntax in &lookups {
                    let base = BaseTableView(syntax);
                    let mut classes = CharClasses::new(nfa.fact_mask());
                    for sample in &samples {
                        let mut ids = Vec::new();
                        for (before, after) in [(&b"x"[..], &b"y"[..]), (&b""[..], &b" "[..])] {
                            let text = [before, sample.as_slice(), after].concat();
                            let d = before.len();
                            let (class, len) =
                                classes.class_at(&nfa, &compiled, &text, d, &base).unwrap();
                            assert_eq!(len, sample.len());
                            ids.push(class);
                            let key = classes.key(class).clone();
                            for (i, &predicate) in nfa.predicates.iter().enumerate() {
                                let direct =
                                    direct_accepts(&nfa, predicate, &compiled, &text, d, syntax);
                                assert_eq!(
                                    key.accepts(i as u16),
                                    direct.is_some(),
                                    "{source:?} {predicate:?} on {sample:x?} (multibyte={multibyte})"
                                );
                                checked += 1;
                            }
                            let (code, _) = re_text_char(&text, d, multibyte).unwrap();
                            let ch = regex_syntax_char(code);
                            let class_syntax = syntax.char_syntax_at(ch, d);
                            let mask = nfa.fact_mask();
                            let word = class_syntax == SyntaxClass::Word;
                            let expected = [
                                (Facts::NEWLINE, text[d] == b'\n'),
                                (Facts::WORD, word),
                                (
                                    Facts::WORD_OR_SYMBOL,
                                    word || class_syntax == SyntaxClass::Symbol,
                                ),
                                (Facts::WIDE_WORD, word && ch as u32 > 0xFF),
                            ];
                            for (fact, holds) in expected {
                                if mask.contains(fact) {
                                    assert_eq!(key.facts.contains(fact), holds, "{fact:?}");
                                }
                            }
                        }
                        assert_eq!(ids[0], ids[1], "one class per character");
                    }
                }
            }
        }
    }
    assert!(checked > 100_000, "{checked}");
}

/// A context change empties the character maps and nothing else: the
/// interned classes, and so the transitions keyed by them, stay.
#[test]
fn a_context_change_empties_only_the_character_maps() {
    let compiled = regex_compile("\\w+x", false, false).unwrap();
    let nfa = Nfa::build(&compiled).unwrap();
    let mut classes = CharClasses::new(nfa.fact_mask());
    let context = ClassContext::of_search(&compiled, &nfa, &DefaultSyntaxLookup).unwrap();
    classes.sync(context);
    let base = BaseTableView(&DefaultSyntaxLookup);
    let (a, _) = classes.class_at(&nfa, &compiled, b"a-", 0, &base).unwrap();
    let (dash, _) = classes.class_at(&nfa, &compiled, b"a-", 1, &base).unwrap();
    assert_ne!(a, dash);
    classes.sync(context);
    assert_eq!(classes.resets, 0);
    let custom = ClassContext::of_search(&compiled, &nfa, &CustomTableLookup).unwrap();
    assert_ne!(custom, context);
    classes.sync(custom);
    assert_eq!(classes.resets, 1);
    assert_eq!(classes.byte_class[b'-' as usize], UNKNOWN_CLASS);
    // Under the custom table `-` is a word constituent: `a`'s class.
    let custom_base = BaseTableView(&CustomTableLookup);
    let (dash_now, _) = classes
        .class_at(&nfa, &compiled, b"a-", 1, &custom_base)
        .unwrap();
    assert_eq!(dash_now, a);
    assert_eq!(classes.len(), 2);
}

/// A pattern that reads no syntax keys its classes by neither the lookup nor
/// the char-table tick.
#[test]
fn class_context_reads_only_what_the_classes_depend_on() {
    let plain = regex_compile("ab[cd]", false, false).unwrap();
    let nfa = Nfa::build(&plain).unwrap();
    assert_eq!(
        ClassContext::of_search(&plain, &nfa, &DefaultSyntaxLookup),
        ClassContext::of_search(&plain, &nfa, &CustomTableLookup)
    );
    let syntax = regex_compile("a\\w", false, false).unwrap();
    let nfa = Nfa::build(&syntax).unwrap();
    assert_ne!(
        ClassContext::of_search(&syntax, &nfa, &DefaultSyntaxLookup),
        ClassContext::of_search(&syntax, &nfa, &CustomTableLookup)
    );
    // A lookup with no cache identity gets no context: the DFA stays off.
    struct NoIdentity;
    impl SyntaxLookup for NoIdentity {
        fn char_syntax(&self, c: char) -> SyntaxClass {
            DefaultSyntaxLookup.char_syntax(c)
        }
        fn char_has_category(&self, c: char, cat: u8) -> bool {
            DefaultSyntaxLookup.char_has_category(c, cat)
        }
        fn cache_key(&self) -> SyntaxCacheKey {
            SyntaxCacheKey::Standard
        }
    }
    assert_eq!(ClassContext::of_search(&syntax, &nfa, &NoIdentity), None);
}

// ---------------------------------------------------------------------------
// The lazy DFA (C4)
// ---------------------------------------------------------------------------

use crate::emacs_core::regex_emacs::{
    MatchRegisters, MatchScratch, re_match_internal, take_fail_stack_probe, take_matcher_overflow,
    take_pike_fallback,
};

struct DfaRng(u64);

impl DfaRng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const GEN_ATOMS: &[&str] = &[
    "a",
    "b",
    "c",
    "x",
    "A",
    " ",
    "-",
    "_",
    ":",
    "\n",
    "é",
    "中",
    "Ж",
    ".",
    "[a-c]",
    "[^a-c\n]",
    "[[:alpha:]]",
    "[[:space:]]",
    "[[:upper:]]",
    "[[:word:]]",
    "[[:punct:]]",
    "[é-ü]",
    "\\w",
    "\\W",
    "\\s-",
    "\\s_",
    "\\sw",
    "\\S-",
    "\\cg",
    "\\C|",
    "\\(?:\\w\\|\\s_\\)",
];
const GEN_ZERO_WIDTH: &[&str] = &[
    "^", "$", "\\`", "\\'", "\\b", "\\B", "\\<", "\\>", "\\_<", "\\_>", "\\=",
];
const GEN_QUANTIFIERS: &[&str] = &["", "", "", "*", "+", "?", "*?", "+?", "??"];

/// A random pattern over the whole eligible vocabulary, quantifiers nested.
fn gen_pattern(rng: &mut DfaRng, depth: usize) -> String {
    let mut out = String::new();
    let arms = 1 + rng.below(3);
    for arm in 0..arms {
        if arm > 0 {
            out.push_str("\\|");
        }
        for _ in 0..1 + rng.below(4) {
            let atom = match rng.below(if depth == 0 { 6 } else { 9 }) {
                0..=3 => rng.pick(GEN_ATOMS).to_string(),
                4 | 5 => rng.pick(GEN_ZERO_WIDTH).to_string(),
                6 | 7 => format!("\\(?:{}\\)", gen_pattern(rng, depth - 1)),
                _ => format!("\\({}\\)", gen_pattern(rng, depth - 1)),
            };
            out.push_str(&atom);
            if !GEN_ZERO_WIDTH.contains(&atom.as_str()) {
                out.push_str(rng.pick(GEN_QUANTIFIERS));
            }
        }
    }
    out
}

/// A random valid multibyte text (raw bytes encoded as Emacs does).
fn gen_text(rng: &mut DfaRng, max_len: usize) -> Vec<u8> {
    const PIECES: &[&str] = &[
        "a", "b", "c", "x", "A", "B", " ", " ", "\n", "-", "_", ":", "!", "é", "É", "中", "日",
        "Ж", "α", "K", "ab", "cab",
    ];
    let mut text = Vec::new();
    for _ in 0..rng.below(max_len) {
        if rng.below(12) == 0 {
            text.extend(encode(emacs_char::byte8_to_char(
                0x80 + rng.below(0x80) as u8,
            )));
        } else {
            text.extend_from_slice(rng.pick(PIECES).as_bytes());
        }
    }
    text
}

fn char_boundaries(text: &[u8], multibyte: bool) -> Vec<usize> {
    let mut out = Vec::new();
    let mut d = 0;
    while d < text.len() {
        out.push(d);
        d += re_text_char(text, d, multibyte).map_or(1, |(_, len)| len);
    }
    out.push(text.len());
    out
}

/// Every verdict of the DFA against the matcher at every candidate of
/// `text`, for a few stops and points.  Returns (yes, no, unknown).
fn check_dfa_against_matcher(
    compiled: &CompiledPattern,
    dfa: &mut ExistenceDfa,
    text: &[u8],
    syntax: &dyn SyntaxLookup,
    label: &str,
) -> (usize, usize, usize) {
    let context = ClassContext::of_search(compiled, dfa.nfa(), syntax)
        .expect("test lookups have a class identity");
    dfa.classes_mut().sync(context);
    dfa.begin_search();
    let boundaries = char_boundaries(text, compiled.target_multibyte);
    let mut tally = (0, 0, 0);
    let stops = [text.len(), boundaries[boundaries.len() / 2]];
    for &stop in &stops {
        for &point in &[0, boundaries[boundaries.len() / 3], text.len()] {
            // One pass over the candidates is one search.
            dfa.begin_search();
            for &p in boundaries.iter().filter(|&&p| p <= stop) {
                let verdict = dfa.anchored_exists(compiled, text, p, stop, point, syntax);
                let _ = take_matcher_overflow();
                let matched = re_match(compiled, text, p, stop, syntax, point);
                if take_matcher_overflow() {
                    continue;
                }
                let where_ = || {
                    format!(
                        "{label}: at {p} stop {stop} point {point} in {:?}: {verdict:?} vs {:?}",
                        String::from_utf8_lossy(text),
                        matched.as_ref().map(|m| m.0)
                    )
                };
                match verdict {
                    Exists::Yes => {
                        assert!(matched.is_some(), "{}", where_());
                        tally.0 += 1;
                    }
                    Exists::No { .. } => {
                        assert!(matched.is_none(), "{}", where_());
                        tally.1 += 1;
                    }
                    Exists::Unknown => tally.2 += 1,
                }
            }
        }
    }
    tally
}

/// Existence against the backtracker: a random pattern's verdict at every
/// candidate of random texts, in both representations, case-folded or not.
#[test]
fn dfa_verdicts_agree_with_the_matcher_on_random_patterns() {
    let mut rng = DfaRng(0xDFA0_5EED);
    let mut totals = (0usize, 0usize, 0usize);
    let mut eligible = 0usize;
    for case in 0..1_500 {
        let source = gen_pattern(&mut rng, 2);
        let case_fold = rng.below(3) == 0;
        let Ok(mut compiled) = regex_compile(&source, rng.below(5) == 0, case_fold) else {
            continue;
        };
        let multibyte = rng.below(4) != 0;
        compiled.target_multibyte = multibyte;
        let Ok(nfa) = Nfa::build(&compiled) else {
            continue;
        };
        eligible += 1;
        let mut dfa = ExistenceDfa::new(nfa);
        for _ in 0..3 {
            let text = if multibyte {
                gen_text(&mut rng, 14)
            } else {
                (0..rng.below(14))
                    .map(|_| b"ab \n-_:x\xe9\xa9"[rng.below(10)])
                    .collect()
            };
            let lookup: &dyn SyntaxLookup = if rng.below(2) == 0 {
                &DefaultSyntaxLookup
            } else {
                &CustomTableLookup
            };
            let tally = check_dfa_against_matcher(
                &compiled,
                &mut dfa,
                &text,
                lookup,
                &format!("case {case} {source:?} fold={case_fold} mb={multibyte}"),
            );
            totals.0 += tally.0;
            totals.1 += tally.1;
            totals.2 += tally.2;
        }
    }
    tracing::info!(eligible, ?totals, "existence DFA vs matcher");
    assert!(eligible > 500, "{eligible}");
    assert!(totals.0 > 1_000 && totals.1 > 10_000, "{totals:?}");
    assert_eq!(totals.2, 0, "no verdict is left undecided");
}

/// The org font-lock patterns of the P3.3 census over an org-like text.
#[test]
fn dfa_verdicts_agree_on_org_font_lock_patterns() {
    let text = "* TODO [#A] Heading :tag:work:\n  - item :: description\n  + deep item\n\
                | a | b |\n|---+---|\n| =x= | *y* |\n:PROPERTIES:\n:ID: 42\n:END:\n\
                Some [[https://example.org][link]] and <mailto:x@y> text.\n\
                ** DONE Sub :ARCHIVE:\n#+BEGIN_SRC elisp\n(defun f () 1)\n#+END_SRC\n";
    for source in [
        "^[ \t]*|\\(?:.*?|\\)? *\\(:?=[^|\n]*\\)",
        "\\(?:^[ \t]*[-+]\\|^[ \t]+[*]\\)[ \t]+\\(.*?[ \t]+::\\)\\([ \t]+\\|$\\)",
        "^\\*+.*?\\(\\[#\\([A-Z]\\|[0-9]\\|[1-5][0-9]\\)\\] ?\\)",
        "| *\\(<[lrc]?[0-9]*>\\)",
        "^\\*+ \\(.*:ARCHIVE:.*\\)",
        "^\\*+ \\(?:.*[ \t]\\)?\\(:\\([[:alnum:]_@#%:]+\\):\\)[ \t]*$",
        "^[ \t]*|\\( *\\([$!_^/]\\) *\\|.*\\)|",
        "^[ \t]*| *\\([#*]\\) *|",
        "^[ \t]*\\(:\\(?: .*\\|$\\)\n?\\)",
        "^\\(\\*+\\)\\(?: +\\(?:DONE\\)\\)\\(?: +\\(.*?\\)\\)?[ \t]*$",
        "\\(\\[\\[\\([^]]+\\)\\]\\(?:\\[\\([^]]+\\)\\]\\)?\\]\\|<\\(mailto\\|https?\\):\\([^>]+\\)>\\|\\<\\(https?\\|mailto\\):\\([^ \t\n]+\\)\\)",
        "(\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)\\_>",
    ] {
        for case_fold in [false, true] {
            let compiled = regex_compile(source, false, case_fold).unwrap();
            let nfa = Nfa::build(&compiled).unwrap_or_else(|why| panic!("{source:?}: {why:?}"));
            let mut dfa = ExistenceDfa::new(nfa);
            let tally = check_dfa_against_matcher(
                &compiled,
                &mut dfa,
                text.as_bytes(),
                &DefaultSyntaxLookup,
                source,
            );
            assert_eq!(tally.2, 0);
            assert!(tally.1 > 0, "{source:?} rejects some candidates");
        }
    }
}

/// A syntax lookup whose `WORD_BOUNDARY_P` separates CJK from other word
/// constituents, as GNU's char-script-table does.
struct ScriptBoundaryLookup;

impl SyntaxLookup for ScriptBoundaryLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        DefaultSyntaxLookup.char_syntax(c)
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        if c1 as u32 <= 0xFF && c2 as u32 <= 0xFF {
            return false;
        }
        let cjk = |c: char| ('\u{3000}'..='\u{9FFF}').contains(&c);
        cjk(c1) != cjk(c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Standard
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: 1,
            category: 1,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

/// `\b` between CJK and Latin word constituents is decided per character
/// pair (the SLOW transitions), never from a cached class.
#[test]
fn word_boundaries_between_scripts_are_decided_per_pair() {
    let text = "ab中文cd日本x αβ中".as_bytes();
    for source in [
        "\\b",
        "\\B",
        "\\<",
        "\\>",
        "\\w\\b\\w",
        "\\w\\B\\w+",
        "中\\b",
    ] {
        let compiled = regex_compile(source, false, false).unwrap();
        let Ok(nfa) = Nfa::build(&compiled) else {
            continue;
        };
        let mut dfa = ExistenceDfa::new(nfa);
        for _ in 0..2 {
            let tally =
                check_dfa_against_matcher(&compiled, &mut dfa, text, &ScriptBoundaryLookup, source);
            assert_eq!(tally.2, 0);
        }
        assert!(
            dfa.counters.slow_transitions > 0 || !source.contains('w'),
            "{source:?}"
        );
    }
}

/// The overflow bound's premise for a rejected candidate: the backtracker's
/// deepest fail stack there stays below `(consumed + 1) * 2 * push sites`.
#[test]
fn a_rejected_candidate_stays_within_the_overflow_bound() {
    let mut rng = DfaRng(0x0B0_0D);
    let syntax = DefaultSyntaxLookup;
    let mut measured = 0usize;
    for _ in 0..1_500 {
        let source = gen_pattern(&mut rng, 2);
        let Ok(compiled) = regex_compile(&source, rng.below(4) == 0, false) else {
            continue;
        };
        let Ok(nfa) = Nfa::build(&compiled) else {
            continue;
        };
        let sites = nfa.push_sites;
        let mut dfa = ExistenceDfa::new(nfa);
        let context = ClassContext::of_search(&compiled, dfa.nfa(), &syntax).unwrap();
        dfa.classes_mut().sync(context);
        let text = gen_text(&mut rng, 24);
        for p in char_boundaries(&text, true) {
            let Exists::No { consumed } =
                dfa.anchored_exists(&compiled, &text, p, text.len(), 0, &syntax)
            else {
                continue;
            };
            let _ = take_fail_stack_probe();
            let _ = take_matcher_overflow();
            // The backtrack budget caps the exponential shapes; the depth
            // bound holds at every step of the run, however it ends.
            let mut scratch = MatchScratch::default();
            let mut registers = MatchRegisters::default();
            let found = re_match_internal(
                &mut scratch,
                &compiled,
                &text,
                p,
                text.len(),
                &syntax,
                0,
                true,
                &mut registers,
            );
            let gave_up = take_pike_fallback();
            assert!(gave_up || found.is_none(), "{source:?} at {p}");
            let probe = take_fail_stack_probe();
            assert!(
                probe.max_depth <= (consumed + 1) * 2 * sites,
                "{source:?} at {p}: depth {} consumed {consumed} sites {sites}",
                probe.max_depth
            );
            measured += 1;
        }
    }
    assert!(measured > 5_000, "{measured}");
}

/// The cache is cleared past its cap and relearned, with the same verdicts.
#[test]
fn a_full_cache_is_cleared_and_relearned() {
    // Many distinct literal prefixes: one state per prefix position.
    let words: Vec<String> = (0..700).map(|i| format!("w{i:03}q")).collect();
    let source = words.join("\\|");
    let compiled = regex_compile(&source, false, false).unwrap();
    let nfa = Nfa::build(&compiled).unwrap();
    let mut dfa = ExistenceDfa::new(nfa);
    let text: String = (0..700).map(|i| format!("w{i:03}x ")).collect();
    let tally = check_dfa_against_matcher(
        &compiled,
        &mut dfa,
        text.as_bytes(),
        &DefaultSyntaxLookup,
        "many prefixes",
    );
    assert_eq!(tally.0, 0);
    assert!(tally.1 > 0);
    assert!(dfa.counters.clears > 0, "{:?}", dfa.counters);
    assert_eq!(dfa.gave_up(), None);
}

// ---------------------------------------------------------------------------
// The candidate filter in `re_search` (C5)
// ---------------------------------------------------------------------------

use crate::emacs_core::regex_emacs::{matcher_entry_count, re_search};

type SearchResult = Option<(usize, Vec<i64>, Vec<i64>)>;

fn search(
    compiled: &CompiledPattern,
    text: &[u8],
    start: usize,
    range: isize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> (SearchResult, bool) {
    let _ = take_matcher_overflow();
    let found = re_search(compiled, text, start, range, syntax, point)
        .map(|(at, regs)| (at, regs.start.to_vec(), regs.end.to_vec()));
    (found, take_matcher_overflow())
}

#[test]
fn the_knob_reads_off_on_and_verify() {
    assert_eq!(DfaMode::parse(None), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("off")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("0")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("bogus")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("on")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some(" 1 ")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("ON")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("verify")), DfaMode::Verify);
}

/// Off: the slot is never touched.  On: after 16 failed entries the DFA is
/// built, then skips candidates; the results are those of the matcher alone.
#[test]
fn searches_skip_rejected_candidates_once_the_dfa_is_built() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\(?:foo\\|bar\\)[0-9]+;", false, false).unwrap();
    let text = b"foo bar fo1 foox barbar1 foo2 bar33x foo9; tail bar7 end";
    with_dfa_mode(DfaMode::Off, || {
        for start in 0..text.len() {
            let _ = search(
                &compiled,
                text,
                start,
                (text.len() - start) as isize,
                &syntax,
                0,
            );
        }
    });
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
    let reference: Vec<_> = (0..=text.len())
        .map(|start| {
            with_dfa_mode(DfaMode::Off, || {
                search(
                    &compiled,
                    text,
                    start,
                    (text.len() - start) as isize,
                    &syntax,
                    0,
                )
            })
        })
        .collect();
    reset_dfa_stats();
    let entries_before = matcher_entry_count();
    for _ in 0..3 {
        for start in 0..=text.len() {
            let got = with_dfa_mode(DfaMode::On, || {
                search(
                    &compiled,
                    text,
                    start,
                    (text.len() - start) as isize,
                    &syntax,
                    0,
                )
            });
            assert_eq!(got, reference[start], "from {start}");
        }
    }
    let entries = matcher_entry_count() - entries_before;
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Live(_)));
    let stats = dfa_stats();
    assert_eq!(stats.builds, 1);
    assert!(stats.skipped > 100, "{stats:?}");
    assert!(entries < 3 * reference.len() as u64 * 4, "{entries}");
}

/// Verify mode runs the matcher on every candidate and finds no verdict it
/// contradicts, over forward, backward, bounded and POSIX searches.
#[test]
fn verify_mode_finds_no_mismatch_on_random_searches() {
    let mut rng = DfaRng(0x5EA7_C4ED);
    reset_dfa_stats();
    for _ in 0..800 {
        let source = gen_pattern(&mut rng, 2);
        let posix = rng.below(5) == 0;
        let Ok(mut compiled) = regex_compile(&source, posix, rng.below(3) == 0) else {
            continue;
        };
        compiled.target_multibyte = true;
        let lookup: &dyn SyntaxLookup = if rng.below(2) == 0 {
            &DefaultSyntaxLookup
        } else {
            &CustomTableLookup
        };
        for _ in 0..4 {
            // Short texts: the reference searches run the unbudgeted
            // backtracker on POSIX and capture-in-empty-loop patterns.
            let text = gen_text(&mut rng, 12);
            let boundaries = char_boundaries(&text, true);
            let start = boundaries[rng.below(boundaries.len())];
            let point = boundaries[rng.below(boundaries.len())];
            for range in [
                (text.len() - start) as isize,
                -(start as isize),
                ((text.len() - start) / 2) as isize,
            ] {
                let want = with_dfa_mode(DfaMode::Off, || {
                    search(&compiled, &text, start, range, lookup, point)
                });
                let verified = with_dfa_mode(DfaMode::Verify, || {
                    search(&compiled, &text, start, range, lookup, point)
                });
                let on = with_dfa_mode(DfaMode::On, || {
                    search(&compiled, &text, start, range, lookup, point)
                });
                assert_eq!(verified, want, "{source:?} from {start} range {range}");
                assert_eq!(on, want, "{source:?} from {start} range {range}");
            }
        }
    }
    let stats = dfa_stats();
    tracing::info!(?stats, "verify-mode soak");
    assert_eq!(stats.verify_bad_no, 0, "{stats:?}");
    assert_eq!(stats.verify_bad_yes, 0, "{stats:?}");
    assert!(
        stats.builds > 100 && stats.no > 1_000 && stats.skipped > 1_000,
        "{stats:?}"
    );
}

/// A lookup where `syntax-table` properties may apply leaves a
/// syntax-reading pattern to the matcher; so does a frontier inside the
/// positions the search can read.
#[test]
fn positional_syntax_and_the_propertize_frontier_turn_the_filter_off() {
    struct Positional;
    impl SyntaxLookup for Positional {
        fn char_syntax(&self, c: char) -> SyntaxClass {
            DefaultSyntaxLookup.char_syntax(c)
        }
        fn char_has_category(&self, c: char, cat: u8) -> bool {
            DefaultSyntaxLookup.char_has_category(c, cat)
        }
        fn cache_key(&self) -> SyntaxCacheKey {
            SyntaxCacheKey::Standard
        }
        fn class_cache_key(&self) -> Option<LookupClassKey> {
            Some(LookupClassKey::Standard)
        }
    }
    struct Frontier(usize);
    impl SyntaxLookup for Frontier {
        fn char_syntax(&self, c: char) -> SyntaxClass {
            DefaultSyntaxLookup.char_syntax(c)
        }
        fn char_has_category(&self, c: char, cat: u8) -> bool {
            DefaultSyntaxLookup.char_has_category(c, cat)
        }
        fn cache_key(&self) -> SyntaxCacheKey {
            SyntaxCacheKey::Standard
        }
        fn class_cache_key(&self) -> Option<LookupClassKey> {
            Some(LookupClassKey::Standard)
        }
        fn position_dependent(&self) -> bool {
            false
        }
        fn syntax_read_limit(&self) -> usize {
            self.0
        }
    }
    let text = b"one two three four five";
    let reads_syntax = regex_compile("\\_<zz", false, false).unwrap();
    let plain = regex_compile("zz", false, false).unwrap();
    reset_dfa_stats();
    with_dfa_mode(DfaMode::On, || {
        let _ = search(&reads_syntax, text, 0, text.len() as isize, &Positional, 0);
        let _ = search(&plain, text, 0, text.len() as isize, &Positional, 0);
        let _ = search(&reads_syntax, text, 0, 10, &Frontier(10), 0);
        let _ = search(&reads_syntax, text, 0, 10, &Frontier(11), 0);
    });
    let stats = dfa_stats();
    assert_eq!(stats.positional_off, 1, "{stats:?}");
    assert_eq!(stats.frontier_off, 1, "{stats:?}");
    // The plain pattern and the search short of the frontier took leases.
    assert_eq!(stats.searches, 2, "{stats:?}");
}

/// A rejection whose consumed span could have filled GNU's fail stack runs
/// the matcher, which signals the overflow as GNU does.
#[test]
fn a_rejection_that_could_overflow_runs_the_matcher() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("x\\(?:a\\|b\\)*c", false, false).unwrap();
    let short = b"xab xba xabab xb xa xx xab xb xa xabb xa xbb xa xb xab xba xab xa";
    let long = [&b"x"[..], &b"ab".repeat(100_000)].concat();
    reset_dfa_stats();
    with_dfa_mode(DfaMode::On, || {
        // Warm the slot: 16 failed entries build the DFA.
        for _ in 0..3 {
            let _ = search(&compiled, short, 0, short.len() as isize, &syntax, 0);
        }
        assert!(matches!(*compiled.dfa.slot(), DfaSlot::Live(_)));
        let (found, overflow) = search(&compiled, &long, 0, long.len() as isize, &syntax, 0);
        assert_eq!(found, None);
        assert!(overflow, "GNU's fail-stack overflow");
    });
    assert!(dfa_stats().overflow_guarded >= 1);
}

/// A pattern whose candidates mostly match goes on holiday.
#[test]
fn mostly_matching_candidates_send_the_pattern_on_holiday() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("[a-z]+", false, false).unwrap();
    let failing = b"1234567890 1234567890 12345";
    let matching = b"ab cd ef gh ij kl";
    reset_dfa_stats();
    with_dfa_mode(DfaMode::On, || {
        // Build it on failures (the fastmap admits no digit, so fail with a
        // pattern-shaped text: every candidate fails at its second char).
        let failing_pattern = regex_compile("[a-z][0-9]", false, false).unwrap();
        for _ in 0..2 {
            let _ = search(
                &failing_pattern,
                failing,
                0,
                failing.len() as isize,
                &syntax,
                0,
            );
        }
        let _ = failing_pattern;
        // `[a-z]+` searched from each letter: every candidate matches.
        for _ in 0..10 {
            for start in 0..matching.len() {
                let _ = search(
                    &compiled,
                    matching,
                    start,
                    (matching.len() - start) as isize,
                    &syntax,
                    0,
                );
            }
        }
    });
    // `[a-z]+` never fails, so it never builds: no holiday needed.
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
    // A pattern that fails enough to build, then matches: holiday.
    let compiled = regex_compile("[a-z]+;", false, false).unwrap();
    let mixed_fail = b"ab cd ef gh ij kl mn op qr st uv wx yz";
    let mixed_match = b"a; b; c; d; e; f; g; h; i; j; k; l; m;";
    with_dfa_mode(DfaMode::On, || {
        let _ = search(
            &compiled,
            mixed_fail,
            0,
            mixed_fail.len() as isize,
            &syntax,
            0,
        );
        assert!(matches!(*compiled.dfa.slot(), DfaSlot::Live(_)));
        for _ in 0..8 {
            for start in (0..mixed_match.len()).step_by(3) {
                let _ = search(
                    &compiled,
                    mixed_match,
                    start,
                    (mixed_match.len() - start) as isize,
                    &syntax,
                    0,
                );
            }
        }
    });
    assert!(dfa_stats().holiday_off > 0, "{:?}", dfa_stats());
}

/// A pending quit leaves the candidate to the matcher, which quits.
#[test]
fn a_pending_quit_leaves_the_candidate_undecided() {
    let compiled = regex_compile("zq", false, false).unwrap();
    let nfa = Nfa::build(&compiled).unwrap();
    let mut dfa = ExistenceDfa::new(nfa);
    let context = ClassContext::of_search(&compiled, dfa.nfa(), &DefaultSyntaxLookup).unwrap();
    dfa.classes_mut().sync(context);
    // A long text of `z`s: the DFA keeps walking until a poll.
    let text = vec![b'z'; 200_000];
    let flag = crate::emacs_core::eval::install_quit_requested_for_test(true);
    let verdict = dfa.anchored_exists(&compiled, &text, 0, text.len(), 0, &DefaultSyntaxLookup);
    crate::emacs_core::eval::clear_quit_requested_for_test();
    drop(flag);
    // `zq` dies at the second `z`: decided before any poll.
    assert!(matches!(verdict, Exists::No { .. }));
    let compiled = regex_compile("z+q", false, false).unwrap();
    let nfa = Nfa::build(&compiled).unwrap();
    let mut dfa = ExistenceDfa::new(nfa);
    dfa.classes_mut().sync(context);
    let _flag = crate::emacs_core::eval::install_quit_requested_for_test(true);
    let verdict = dfa.anchored_exists(&compiled, &text, 0, text.len(), 0, &DefaultSyntaxLookup);
    crate::emacs_core::eval::clear_quit_requested_for_test();
    assert_eq!(verdict, Exists::Unknown);
}

// ---------------------------------------------------------------------------
// Differential fuzz smoke (C6)
// ---------------------------------------------------------------------------

use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

/// The `ExistenceDfa` differential on every `cargo nextest` run: random
/// patterns and texts, both representations, case-folded or not; every
/// forward and backward search equal with the filter on, and `verify`
/// finding nothing.
#[test]
fn dfa_fuzz_smoke() {
    crate::test_utils::init_test_tracing();
    let mut rng = DfaRng(0xF022_DFA0);
    let mut compared = 0usize;
    for _ in 0..2_000 {
        let source = gen_pattern(&mut rng, 2);
        let case_fold = rng.below(3) == 0;
        let target = if rng.below(4) == 0 {
            SearchTarget::Unibyte
        } else {
            SearchTarget::Multibyte
        };
        let text = gen_text(&mut rng, 16);
        let start = rng.below(text.len() + 1);
        let point = rng.below(text.len() + 1);
        let case = RegexCase::new(&source, &text, case_fold, start, point).with_target(target);
        match check_regex_differential(case, RegexDifferential::ExistenceDfa) {
            Ok(RegexCheck::Equivalent { comparisons }) => compared += comparisons,
            Ok(RegexCheck::NotApplicable(_)) => {}
            Err(divergence) => panic!(
                "{divergence}\npattern={source:?} case_fold={case_fold} target={target} \
                 text={:?} start={start} point={point}",
                String::from_utf8_lossy(&text)
            ),
        }
    }
    assert!(compared > 1_000, "{compared}");
}

/// The fixed regression cases of the fuzz target: shapes where a rejection
/// is easy to get wrong.
#[test]
fn dfa_differential_regressions() {
    for (pattern, text, start) in [
        // A candidate whose only match is empty, at the stop.
        ("x*$", &b"ab\ncd"[..], 0),
        // `\=` at point in the middle of the text.
        ("a\\=b\\|c", b"xxab", 2),
        // `\b` against the text edges.
        ("\\bq\\|z\\b", b"q z", 3),
        // A multibyte character across the bound.
        ("é+", "aéé".as_bytes(), 0),
        // A keep-string loop (rewind view) followed by its exit.
        ("[a-z]*:x", b"abc:y abc:x", 0),
        // POSIX-style alternation where only the longer arm continues.
        ("\\(a\\|ab\\)c", b"abd abc", 0),
    ] {
        for target in [SearchTarget::Multibyte, SearchTarget::Unibyte] {
            for case_fold in [false, true] {
                let case =
                    RegexCase::new(pattern, text, case_fold, start, start).with_target(target);
                let check = check_regex_differential(case, RegexDifferential::ExistenceDfa);
                assert!(
                    matches!(check, Ok(RegexCheck::Equivalent { comparisons: 2 })),
                    "{pattern:?}: {check:?}"
                );
            }
        }
    }
}

/// Lisp-level searches with the filter on across syntax-table edits,
/// `with-syntax-table`, a new char-table and a buffer switch answer exactly
/// as with it off.
#[test]
fn lisp_searches_follow_syntax_table_changes_with_the_filter_on() {
    // Primitives only: `Context::new()` loads no Lisp.
    let program = r#"
(let ((out nil) (round 0) (st (copy-syntax-table)))
  (set-buffer (get-buffer-create "dfa-a"))
  (insert (apply 'concat (make-list 30 "ab-x cd_x ef x-y gh-x ")))
  (set-syntax-table st)
  (while (< round 4)
    (goto-char (point-min))
    (let ((hits nil))
      (while (re-search-forward "\\w+-x\\|\\_<\\w+_x\\_>" nil t)
        (setq hits (cons (match-beginning 0) hits)))
      (setq out (cons (list round (length hits) (car hits)) out)))
    (cond ((= round 0) (modify-syntax-entry ?- "w" st))
          ((= round 1) (modify-syntax-entry ?_ "." st))
          ((= round 2) (make-char-table 'syntax-table)))
    (setq round (1+ round)))
  (set-syntax-table (standard-syntax-table))
  (goto-char (point-min))
  (setq out (cons (list 'standard (re-search-forward "\\w+-x" nil t)) out))
  (set-buffer (get-buffer-create "dfa-b"))
  (insert "zz-x " (make-string 50 ?q) " qq-x")
  (goto-char (point-min))
  (let ((hits nil))
    (while (re-search-forward "\\w+-x" nil t) (setq hits (cons (point) hits)))
    (setq out (cons (list 'other-buffer hits) out)))
  (nreverse out))
"#;
    let run = |mode| {
        with_dfa_mode(mode, || {
            let mut ev = crate::emacs_core::eval::Context::new();
            let value = ev.eval_str(program).expect("program evaluates");
            crate::emacs_core::print::print_value(&value)
        })
    };
    reset_dfa_stats();
    let off = run(DfaMode::Off);
    let on = run(DfaMode::On);
    let verify = run(DfaMode::Verify);
    assert_eq!(on, off);
    assert_eq!(verify, off);
    let stats = dfa_stats();
    assert!(stats.builds > 0 && stats.skipped > 0, "{stats:?}");
    assert!(stats.context_resets > 0, "{stats:?}");
}
