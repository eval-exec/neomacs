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
        SyntaxCacheKey::External {
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
    dfa.begin_search(compiled, syntax);
    let boundaries = char_boundaries(text, compiled.target_multibyte);
    let mut tally = (0, 0, 0);
    let stops = [text.len(), boundaries[boundaries.len() / 2]];
    for &stop in &stops {
        for &point in &[0, boundaries[boundaries.len() / 3], text.len()] {
            // One pass over the candidates is one search.
            dfa.begin_search(compiled, syntax);
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
    assert_eq!(DfaMode::parse(None), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("off")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("0")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some(" OFF ")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("no")), DfaMode::Off);
    assert_eq!(DfaMode::parse(Some("bogus")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("on")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some(" 1 ")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("ON")), DfaMode::On);
    assert_eq!(DfaMode::parse(Some("verify")), DfaMode::Verify);
}

/// A search whose candidates always succeed never borrows a cold DFA slot.
#[test]
fn cold_successful_searches_take_no_dfa_lease() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("(defun \\([-a-z0-9]+\\)", false, true).unwrap();
    let text = b"(defun example-name)";
    let expected = with_dfa_mode(DfaMode::Off, || {
        search(&compiled, text, 0, text.len() as isize, &syntax, 0)
    });
    assert!(expected.0.is_some());
    reset_dfa_stats();
    with_cold_path(true, || {
        with_dfa_mode(DfaMode::On, || {
            for _ in 0..64 {
                assert_eq!(
                    search(&compiled, text, 0, text.len() as isize, &syntax, 0),
                    expected
                );
            }
        });
    });
    assert!(!compiled.dfa.initialized());
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
    let stats = dfa_stats();
    assert_eq!(stats.searches, 0, "{stats:?}");
    assert_eq!(stats.builds, 0, "{stats:?}");
}

/// The threshold failure publishes the slot and leases it for the remaining
/// candidates of the same search, without counting its classic attempt twice.
#[test]
fn cold_failure_threshold_filters_the_rest_of_the_same_search() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("z[0-9]", false, false).unwrap();
    let dense = vec![b'z'; 128];
    reset_dfa_stats();
    with_cold_path(true, || {
        with_dfa_mode(DfaMode::On, || {
            for _ in 0..COLD_THRESHOLD * 2 {
                // An isolated failure followed by a match must not heat the
                // slot. Whole failed searches are admitted on completion.
                let found = search(&compiled, b"zz0", 0, 3, &syntax, 0);
                assert!(!found.1);
                assert_eq!(found.0.as_ref().map(|(at, _, _)| *at), Some(1));
            }
            assert!(!compiled.dfa.initialized());
            assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
            assert_eq!(dfa_stats().searches, 0);
            let before = matcher_entry_count();
            assert_eq!(
                search(&compiled, &dense, 0, dense.len() as isize, &syntax, 0),
                (None, false)
            );
            assert_eq!(matcher_entry_count() - before, COLD_THRESHOLD as u64);
        });
    });
    assert!(compiled.dfa.initialized());
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Live(_)));
    let stats = dfa_stats();
    assert_eq!(stats.builds, 1, "{stats:?}");
    assert_eq!(stats.searches, 1, "{stats:?}");
    assert!(stats.skipped > 100, "{stats:?}");
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

// ---------------------------------------------------------------------------
// `syntax-table` property runs and the propertize frontier (C7)
// ---------------------------------------------------------------------------

/// The syntax a `syntax-table` property run gives its characters.
#[derive(Clone, Copy)]
enum RunSyntax {
    /// A descriptor cons: every character has this class.
    Descriptor(SyntaxClass),
    /// A syntax table as the property: each character's class in it.
    Table(&'static dyn SyntaxLookup),
}

/// A base table plus `syntax-table` property runs (sorted, disjoint
/// `[start, end)` input ranges), and a lazy-propertize frontier that
/// records the lowest syntax read at or past it, as the buffer lookup does.
struct PropertyRunLookup {
    base: &'static dyn SyntaxLookup,
    runs: Vec<(usize, usize, RunSyntax)>,
    /// Whether `plain_syntax_until` reports the runs; otherwise the trait's
    /// default treats every position as propertized.
    reports_runs: bool,
    frontier: usize,
    crossed: std::cell::Cell<Option<usize>>,
}

impl PropertyRunLookup {
    fn new(base: &'static dyn SyntaxLookup, runs: Vec<(usize, usize, RunSyntax)>) -> Self {
        Self {
            base,
            runs,
            reports_runs: true,
            frontier: usize::MAX,
            crossed: std::cell::Cell::new(None),
        }
    }

    fn run_at(&self, pos: usize) -> Option<RunSyntax> {
        self.runs
            .iter()
            .find(|&&(start, end, _)| start <= pos && pos < end)
            .map(|&(_, _, syntax)| syntax)
    }
}

impl SyntaxLookup for PropertyRunLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        self.base.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, pos: usize) -> SyntaxClass {
        if pos >= self.frontier && self.crossed.get().is_none_or(|seen| pos < seen) {
            self.crossed.set(Some(pos));
        }
        match self.run_at(pos) {
            Some(RunSyntax::Descriptor(class)) => class,
            Some(RunSyntax::Table(table)) => table.char_syntax(c),
            None => self.base.char_syntax(c),
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.base.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.base.word_boundary_between(c1, c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        self.base.cache_key()
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        self.base.class_cache_key()
    }

    fn position_dependent(&self) -> bool {
        true
    }

    fn plain_syntax_until(&self, pos: usize) -> usize {
        if !self.reports_runs || self.run_at(pos).is_some() {
            return pos;
        }
        self.runs
            .iter()
            .map(|&(start, _, _)| start)
            .filter(|&start| start > pos)
            .min()
            .unwrap_or(usize::MAX)
    }

    fn syntax_read_limit(&self) -> usize {
        self.frontier
    }
}

const RUN_CLASSES: &[SyntaxClass] = &[
    SyntaxClass::Word,
    SyntaxClass::Symbol,
    SyntaxClass::Whitespace,
    SyntaxClass::Punctuation,
    SyntaxClass::Open,
    SyntaxClass::EndComment,
];

/// Up to three random property runs over `text`'s character boundaries.
fn gen_runs(rng: &mut DfaRng, text: &[u8], multibyte: bool) -> Vec<(usize, usize, RunSyntax)> {
    let boundaries = char_boundaries(text, multibyte);
    let mut cuts: Vec<usize> = (0..2 * rng.below(4))
        .map(|_| boundaries[rng.below(boundaries.len())])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.chunks_exact(2)
        .map(|pair| {
            let syntax = if rng.below(4) == 0 {
                RunSyntax::Table(&CustomTableLookup)
            } else {
                RunSyntax::Descriptor(RUN_CLASSES[rng.below(RUN_CLASSES.len())])
            };
            (pair[0], pair[1], syntax)
        })
        .collect()
}

fn random_base(rng: &mut DfaRng) -> &'static dyn SyntaxLookup {
    if rng.below(2) == 0 {
        &DefaultSyntaxLookup
    } else {
        &CustomTableLookup
    }
}

/// Existence against the backtracker where `syntax-table` properties change
/// characters' syntax: every verdict at every candidate agrees, and a
/// character inside a run is classified at its position.
#[test]
fn dfa_verdicts_agree_with_the_matcher_inside_property_runs() {
    crate::test_utils::init_test_tracing();
    let mut rng = DfaRng(0x0C7_5EED);
    let mut totals = (0usize, 0usize, 0usize);
    let mut positional_chars = 0u64;
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
            let mut lookup =
                PropertyRunLookup::new(random_base(&mut rng), gen_runs(&mut rng, &text, multibyte));
            lookup.reports_runs = rng.below(5) != 0;
            let tally = check_dfa_against_matcher(
                &compiled,
                &mut dfa,
                &text,
                &lookup,
                &format!(
                    "case {case} {source:?} fold={case_fold} mb={multibyte} reports={}",
                    lookup.reports_runs
                ),
            );
            totals.0 += tally.0;
            totals.1 += tally.1;
            totals.2 += tally.2;
        }
        positional_chars += dfa.counters.positional_chars;
    }
    tracing::info!(
        eligible,
        ?totals,
        positional_chars,
        "existence DFA in property runs"
    );
    assert!(eligible > 500, "{eligible}");
    assert!(totals.0 > 1_000 && totals.1 > 10_000, "{totals:?}");
    assert_eq!(totals.2, 0, "no verdict is left undecided");
    assert!(positional_chars > 10_000, "{positional_chars}");
}

/// A pattern that reads no syntax ignores the property runs: its search is
/// not positional, and no character is classified at its position.
#[test]
fn a_pattern_that_reads_no_syntax_steps_over_property_runs() {
    for (source, reads) in [
        ("[[:alnum:]_@#%:]+x", false),
        ("[[:alpha:]]+[0-9]", false),
        ("[[:space:]]+x", true),
        ("[[:word:]]x", true),
        ("[[:punct:]]x", true),
        ("\\sw+x", true),
        ("\\_<x", true),
        ("\\bx", true),
        ("ab*x", false),
    ] {
        let compiled = regex_compile(source, false, false).unwrap();
        let nfa = Nfa::build(&compiled).unwrap();
        assert_eq!(nfa.reads_syntax, reads, "{source:?}");
        let text = b"ab ab: _a  bx a-b @x ab9 x";
        let lookup = PropertyRunLookup::new(
            &DefaultSyntaxLookup,
            vec![(0, 8, RunSyntax::Descriptor(SyntaxClass::Word))],
        );
        let mut dfa = ExistenceDfa::new(nfa);
        let tally = check_dfa_against_matcher(&compiled, &mut dfa, text, &lookup, source);
        assert_eq!(tally.2, 0, "{source:?}");
        assert_eq!(
            dfa.counters.positional_chars > 0,
            reads,
            "{source:?}: {:?}",
            dfa.counters
        );
    }
}

/// Searches under property runs with the filter on and in verify mode equal
/// the matcher alone (forward, backward, bounded, POSIX), and the filter
/// skips candidates there: the lease is granted.
#[test]
fn searches_inside_property_runs_equal_the_matcher_alone() {
    crate::test_utils::init_test_tracing();
    let mut rng = DfaRng(0x5EA7_0C7E);
    reset_dfa_stats();
    for _ in 0..300 {
        let source = gen_pattern(&mut rng, 2);
        let posix = rng.below(5) == 0;
        let Ok(mut compiled) = regex_compile(&source, posix, rng.below(3) == 0) else {
            continue;
        };
        compiled.target_multibyte = true;
        let _ = prime(&compiled, &DefaultSyntaxLookup);
        for _ in 0..4 {
            let text = gen_text(&mut rng, 12);
            let lookup =
                PropertyRunLookup::new(random_base(&mut rng), gen_runs(&mut rng, &text, true));
            let boundaries = char_boundaries(&text, true);
            let start = boundaries[rng.below(boundaries.len())];
            let point = boundaries[rng.below(boundaries.len())];
            for range in [
                (text.len() - start) as isize,
                -(start as isize),
                ((text.len() - start) / 2) as isize,
            ] {
                let want = with_dfa_mode(DfaMode::Off, || {
                    search(&compiled, &text, start, range, &lookup, point)
                });
                let verified = with_dfa_mode(DfaMode::Verify, || {
                    search(&compiled, &text, start, range, &lookup, point)
                });
                let on = with_dfa_mode(DfaMode::On, || {
                    search(&compiled, &text, start, range, &lookup, point)
                });
                assert_eq!(verified, want, "{source:?} from {start} range {range}");
                assert_eq!(on, want, "{source:?} from {start} range {range}");
            }
        }
    }
    let stats = dfa_stats();
    tracing::info!(?stats, "property-run soak");
    assert_eq!(stats.verify_bad_no, 0, "{stats:?}");
    assert_eq!(stats.verify_bad_yes, 0, "{stats:?}");
    assert!(
        stats.positional > 1_000 && stats.skipped > 1_000 && stats.positional_chars > 1_000,
        "{stats:?}"
    );
}

/// With a lazy-propertize frontier inside the searched span, the filter
/// grants the lease and leaves exactly the candidates that would read syntax
/// at or past the frontier to the matcher: the lowest read it records is the
/// one the matcher alone records, and every result is the matcher's.
#[test]
fn the_frontier_records_what_the_matcher_alone_records() {
    crate::test_utils::init_test_tracing();
    let mut rng = DfaRng(0xF207_71E2);
    reset_dfa_stats();
    let mut crossed_cases = 0usize;
    for _ in 0..600 {
        let source = gen_pattern(&mut rng, 2);
        // The longest random patterns can make the reference matcher
        // exponential on these texts (one took minutes).
        if source.len() > 160 {
            continue;
        }
        let Ok(mut compiled) = regex_compile(&source, false, rng.below(3) == 0) else {
            continue;
        };
        compiled.target_multibyte = true;
        let _ = prime(&compiled, &DefaultSyntaxLookup);
        for _ in 0..4 {
            let text = gen_text(&mut rng, 12);
            let boundaries = char_boundaries(&text, true);
            let runs = gen_runs(&mut rng, &text, true);
            let base = random_base(&mut rng);
            let frontier = boundaries[rng.below(boundaries.len())];
            let start = boundaries[rng.below(boundaries.len())];
            for range in [(text.len() - start) as isize, -(start as isize)] {
                let run = |mode| {
                    let mut lookup = PropertyRunLookup::new(base, runs.clone());
                    lookup.frontier = frontier;
                    let found =
                        with_dfa_mode(mode, || search(&compiled, &text, start, range, &lookup, 0));
                    (found, lookup.crossed.get())
                };
                let off = run(DfaMode::Off);
                crossed_cases += usize::from(off.1.is_some());
                assert_eq!(
                    run(DfaMode::On),
                    off,
                    "{source:?} in {:?} from {start} range {range} frontier {frontier}",
                    String::from_utf8_lossy(&text)
                );
                assert_eq!(run(DfaMode::Verify), off, "{source:?}");
            }
        }
    }
    let stats = dfa_stats();
    tracing::info!(?stats, crossed_cases, "frontier soak");
    assert!(crossed_cases > 100, "{crossed_cases}");
    assert!(
        stats.frontier > 100 && stats.frontier_unknown > 100 && stats.skipped > 1_000,
        "{stats:?}"
    );
    assert_eq!(stats.verify_bad_no + stats.verify_bad_yes, 0, "{stats:?}");
}

/// A candidate at or past the frontier is left undecided before any syntax
/// is read; one decided below it reads nothing at or past it.
#[test]
fn a_candidate_reaching_the_frontier_is_left_to_the_matcher() {
    let compiled = regex_compile("\\_<ab+c", false, false).unwrap();
    let mut dfa = ExistenceDfa::new(Nfa::build(&compiled).unwrap());
    let text = b"xx abbbbbbc ab abbbbd";
    let mut lookup = PropertyRunLookup::new(&DefaultSyntaxLookup, Vec::new());
    lookup.frontier = 8;
    let context = ClassContext::of_search(&compiled, dfa.nfa(), &lookup).unwrap();
    dfa.classes_mut().sync(context);
    dfa.begin_search(&compiled, &lookup);
    let stop = text.len();
    // A candidate at or past the frontier: nothing is read.
    assert_eq!(
        dfa.anchored_exists(&compiled, text, 12, stop, 0, &lookup),
        Exists::Unknown
    );
    // At 3 the thread runs `abbbbb` up to the frontier at 8: undecided.
    assert_eq!(
        dfa.anchored_exists(&compiled, text, 3, stop, 0, &lookup),
        Exists::Unknown
    );
    // At 0 the `a` fails on `x` at once: decided, reading below the frontier.
    assert!(matches!(
        dfa.anchored_exists(&compiled, text, 0, stop, 0, &lookup),
        Exists::No { .. }
    ));
    assert_eq!(lookup.crossed.get(), None);
    assert_eq!(dfa.counters.frontier_unknown, 2);
}

// ---------------------------------------------------------------------------
// Cached first-step rejection (same-binary knob)
// ---------------------------------------------------------------------------

fn first_step_dfa(pattern: &CompiledPattern, syntax: &dyn SyntaxLookup) -> ExistenceDfa {
    let mut dfa = ExistenceDfa::new(Nfa::build(pattern).unwrap());
    let context = ClassContext::of_search(pattern, dfa.nfa(), syntax).unwrap();
    dfa.classes.sync(context);
    dfa.begin_search(pattern, syntax);
    dfa
}

/// A real candidate rejected from the cached start transition avoids the
/// outlined matcher, while a successful fallback and verify keep all registers.
#[test]
fn inline_first_step_rejects_and_success_fallback_agree_with_verify() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\<\\(foo\\)\\([0-9]\\);", false, false).unwrap();
    let text = b"xfoo7; foo8;";
    let expected = with_dfa_mode(DfaMode::Off, || {
        search(&compiled, text, 0, text.len() as isize, &syntax, 0)
    });
    assert!(expected.0.is_some());
    prime(&compiled, &syntax).unwrap();
    // Warm an actual DEAD start transition and the successful continuation.
    with_first_step(false, || {
        with_dfa_mode(DfaMode::On, || {
            assert_eq!(
                search(&compiled, text, 0, text.len() as isize, &syntax, 0),
                expected
            );
        });
    });
    reset_dfa_stats();
    with_first_step(true, || {
        with_dfa_mode(DfaMode::On, || {
            let mut lease = DfaLease::acquire(&compiled, &syntax, text.len()).unwrap();
            assert!(lease.inline_first_step_enabled());
            let (before_no, before_decisions) = match &*lease.slot {
                DfaSlot::Live(live) => (live.dfa.counters.no, live.decisions),
                _ => panic!("primed slot should be live"),
            };
            let entries = matcher_entry_count();
            assert!(lease.try_inline_first_step_skip(&compiled, text, 1, text.len(), 0));
            assert_eq!(matcher_entry_count(), entries);
            let DfaSlot::Live(live) = &*lease.slot else {
                panic!("a cached rejection keeps the slot live");
            };
            assert_eq!(live.dfa.counters.no, before_no + 1);
            assert_eq!(live.decisions, before_decisions + 1);
            assert_eq!(lease.skipped, 1);
            // The real match falls through exactly once, with full registers.
            assert!(!lease.try_inline_first_step_skip(&compiled, text, 7, text.len(), 0));
            let mut scratch = MatchScratch::default();
            let mut registers = MatchRegisters::default();
            let end = lease.candidate::<false>(
                &mut scratch,
                &compiled,
                text,
                7,
                text.len(),
                &syntax,
                0,
                &mut registers,
            );
            assert_eq!(end, Some(text.len()));
            assert_eq!(matcher_entry_count() - entries, 1);
            assert_eq!(
                Some((7, registers.start.to_vec(), registers.end.to_vec())),
                expected.0
            );
        });
        assert_eq!(dfa_stats().skipped, 1);
        assert_eq!(dfa_stats().no, 1);
        assert_eq!(dfa_stats().yes, 1);
        // Exercise the actual re_search macro, then the verify-only path.
        reset_dfa_stats();
        let entries = matcher_entry_count();
        let on = with_dfa_mode(DfaMode::On, || {
            search(&compiled, text, 0, text.len() as isize, &syntax, 0)
        });
        assert_eq!(on, expected);
        assert_eq!(matcher_entry_count() - entries, 1);
        assert_eq!(dfa_stats().skipped, 1);
        let entries = matcher_entry_count();
        let verified = with_dfa_mode(DfaMode::Verify, || {
            let lease = DfaLease::acquire(&compiled, &syntax, text.len()).unwrap();
            assert!(!lease.inline_first_step_enabled());
            drop(lease);
            search(&compiled, text, 0, text.len() as isize, &syntax, 0)
        });
        assert_eq!(verified, expected);
        assert_eq!(matcher_entry_count() - entries, 2);
        let stats = dfa_stats();
        assert_eq!(stats.verify_bad_no + stats.verify_bad_yes, 0, "{stats:?}");
    });
}

#[test]
fn cached_first_step_rejections_equal_the_matcher() {
    let syntax = DefaultSyntaxLookup;
    for source in ["\\<foo", "^foo", "\\`foo", "\\(?:bar\\|baz\\)"] {
        let compiled = regex_compile(source, false, false).unwrap();
        let mut dfa = first_step_dfa(&compiled, &syntax);
        let text = b"xfoo";
        let p = 1;
        assert_eq!(
            with_first_step(false, || {
                dfa.anchored_exists(&compiled, text, p, text.len(), 0, &syntax)
            }),
            Exists::No { consumed: 0 },
            "{source}"
        );
        assert!(dfa.cached_first_step_dead(&compiled, text, p, text.len(), 0));
        let before = dfa.counters;
        assert_eq!(
            with_first_step(true, || {
                dfa.anchored_exists(&compiled, text, p, text.len(), 0, &syntax)
            }),
            Exists::No { consumed: 0 }
        );
        assert_eq!(dfa.counters.no, before.no + 1);
        assert_eq!(dfa.counters.bytes, before.bytes);
        assert_eq!(dfa.counters.states, before.states);
        assert!(re_match(&compiled, text, p, text.len(), &syntax, 0).is_none());
    }
}

#[test]
fn cached_first_step_respects_point_stop_and_unknown_entries() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\=foo", false, false).unwrap();
    let mut dfa = first_step_dfa(&compiled, &syntax);
    let text = b"xfoo";
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &syntax),
            Exists::No { consumed: 0 }
        );
    });
    assert!(dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    // The same candidate now satisfies point: the cached failure is invalid.
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 1));
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 1, text.len(), 1, &syntax)
        }),
        Exists::Yes
    );
    assert!(re_match(&compiled, text, 1, text.len(), &syntax, 1).is_some());
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, 1, 0));
    assert!(!dfa.cached_first_step_dead(&compiled, text, text.len(), text.len(), 0));
    // A byte that has never been classified must use the normal loop.
    assert!(!dfa.cached_first_step_dead(&compiled, b"xqoo", 1, text.len(), 0));
    let class = dfa.classes.byte_class[b'f' as usize];
    let start = dfa.start[0] << dfa.stride_shift;
    let at = start as usize + class as usize;
    let original = dfa.trans[at];
    for unresolved in [UNKNOWN, SLOW, MATCH, start] {
        dfa.trans[at] = unresolved;
        assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    }
    dfa.trans[at] = original;
    dfa.clear_states();
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
}

#[test]
fn cached_first_step_keeps_memory_and_give_up_guards() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("z[0-9]", false, false).unwrap();
    let mut dfa = first_step_dfa(&compiled, &syntax);
    let text = b"y";
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &syntax),
            Exists::No { consumed: 0 }
        );
    });
    assert!(dfa.cached_first_step_dead(&compiled, text, 0, text.len(), 0));
    dfa.memory = MEMORY_CAP + 1;
    let clears = dfa.counters.clears;
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &syntax)
        }),
        Exists::No { consumed: 0 }
    );
    assert_eq!(dfa.counters.clears, clears + 1);
    dfa.gave_up = Some(DfaGaveUp::StateExplosion);
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &syntax)
        }),
        Exists::Unknown
    );
}

#[test]
fn cached_first_step_context_changes_clear_character_maps() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\<foo", false, false).unwrap();
    let mut dfa = first_step_dfa(&compiled, &syntax);
    let text = b"xfoo";
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &syntax),
            Exists::No { consumed: 0 }
        );
    });
    assert!(dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    let prev_class = dfa.classes.byte_class[b'x' as usize];
    let prev_facts = dfa.classes.byte_facts[b'x' as usize];
    dfa.classes.byte_class[b'x' as usize] = UNKNOWN_CLASS;
    dfa.classes.byte_facts[b'x' as usize] = NO_FACTS;
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    dfa.classes.byte_class[b'x' as usize] = prev_class;
    dfa.classes.byte_facts[b'x' as usize] = prev_facts;
    // A multibyte previous character cannot borrow an ASCII byte's facts.
    assert!(!dfa.cached_first_step_dead(&compiled, "中foo".as_bytes(), 3, 6, 0));
    let mut changed = ClassContext::of_search(&compiled, dfa.nfa(), &syntax).unwrap();
    changed.tick = changed.tick.wrapping_add(1);
    dfa.classes.sync(changed);
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    let actual = with_first_step(true, || {
        dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &syntax)
    });
    assert_eq!(actual, Exists::No { consumed: 0 });
    assert!(re_match(&compiled, text, 1, text.len(), &syntax, 0).is_none());

    // A real table switch changes this candidate from a cached rejection
    // to a match: `-` is Word in CustomTableLookup, Symbol in standard.
    let text = b"-foo";
    let mut dfa = first_step_dfa(&compiled, &CustomTableLookup);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &CustomTableLookup),
            Exists::No { consumed: 0 }
        );
    });
    assert!(dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    assert!(re_match(&compiled, text, 1, text.len(), &CustomTableLookup, 0).is_none());
    let standard = ClassContext::of_search(&compiled, dfa.nfa(), &syntax).unwrap();
    dfa.classes.sync(standard);
    dfa.begin_search(&compiled, &syntax);
    assert_eq!(dfa.classes.byte_facts[b'-' as usize], NO_FACTS);
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &syntax)
        }),
        Exists::Yes
    );
    assert!(re_match(&compiled, text, 1, text.len(), &syntax, 0).is_some());
}

#[test]
fn cached_first_step_properties_and_frontier_leave_stale_rejects_unused() {
    let compiled = regex_compile("\\<foo", false, false).unwrap();
    let text = b"xfoo";
    let mut dfa = first_step_dfa(&compiled, &DefaultSyntaxLookup);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &DefaultSyntaxLookup),
            Exists::No { consumed: 0 }
        );
    });
    // A property on the previous character turns this rejected candidate
    // into a real word beginning. Covering only p is insufficient.
    let lookup = PropertyRunLookup::new(
        &DefaultSyntaxLookup,
        vec![(0, 1, RunSyntax::Descriptor(SyntaxClass::Punctuation))],
    );
    dfa.begin_search(&compiled, &lookup);
    dfa.plain = 1..usize::MAX;
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &lookup)
        }),
        Exists::Yes
    );
    assert!(re_match(&compiled, text, 1, text.len(), &lookup, 0).is_some());
    // A frontier at p must be handled by the matcher, which records its read.
    let mut frontier = PropertyRunLookup::new(&DefaultSyntaxLookup, Vec::new());
    frontier.frontier = 1;
    dfa.begin_search(&compiled, &frontier);
    dfa.plain = 0..usize::MAX;
    assert!(!dfa.cached_first_step_dead(&compiled, text, 1, text.len(), 0));
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 1, text.len(), 0, &frontier)
        }),
        Exists::Unknown
    );
    assert_eq!(frontier.crossed.get(), None);
    assert!(re_match(&compiled, text, 1, text.len(), &frontier, 0).is_none());
    assert_eq!(frontier.crossed.get(), Some(1));
    // A property on the current character invalidates its base-table class.
    let compiled = regex_compile("\\s-q", false, false).unwrap();
    let text = b"yq";
    let mut dfa = first_step_dfa(&compiled, &DefaultSyntaxLookup);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &DefaultSyntaxLookup),
            Exists::No { consumed: 0 }
        );
    });
    let lookup = PropertyRunLookup::new(
        &DefaultSyntaxLookup,
        vec![(0, 1, RunSyntax::Descriptor(SyntaxClass::Whitespace))],
    );
    dfa.begin_search(&compiled, &lookup);
    assert!(!dfa.cached_first_step_dead(&compiled, text, 0, text.len(), 0));
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &lookup)
        }),
        Exists::Yes
    );
    assert!(re_match(&compiled, text, 0, text.len(), &lookup, 0).is_some());
}

/// The filter's one-compare overflow bound answers exactly as the bound (for
/// every span a text can have: `consumed` is below `usize::MAX`).
#[test]
fn the_overflow_free_span_is_the_fail_stack_bound() {
    use crate::emacs_core::regex_emacs::fail_stack_may_overflow_with;
    for push_sites in [
        0,
        1,
        2,
        3,
        7,
        64,
        100,
        4_096,
        133_332,
        133_333,
        266_666,
        usize::MAX / 3,
    ] {
        let span = fail_stack_overflow_free_span(push_sites);
        for consumed in [
            0,
            1,
            2,
            5,
            999,
            1_300,
            1_332,
            1_333,
            1_334,
            133_332,
            133_333,
            usize::MAX - 1,
        ]
        .into_iter()
        .chain([span.saturating_sub(1), span, span.saturating_add(1)])
        .map(|consumed| consumed.min(usize::MAX - 1))
        {
            assert_eq!(
                consumed < span,
                !fail_stack_may_overflow_with(push_sites, consumed),
                "push sites {push_sites}, consumed {consumed}, span {span}"
            );
        }
    }
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
                    // Forward and backward, each under the standard syntax
                    // and under property runs.
                    matches!(check, Ok(RegexCheck::Equivalent { comparisons: 4 })),
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

/// Lisp-level searches in a buffer and over a string whose `syntax-table`
/// properties change characters' syntax, with `parse-sexp-lookup-properties`
/// on: the filter takes the lease, classifies characters inside the property
/// runs at their positions, and answers exactly as with it off.
#[test]
fn lisp_searches_over_syntax_table_properties_with_the_filter_on() {
    // Primitives only: `Context::new()` loads no Lisp.
    let program = r#"
(let ((out nil) (parse-sexp-lookup-properties t) (pos 1) (res nil))
  (set-buffer (get-buffer-create "dfa-props"))
  (insert (apply 'concat (make-list 40 "foo-bar (baz) qux_x foo.x é-x ")))
  (while (< pos (point-max))
    (let ((c (char-after pos)))
      (cond ((and (= c ?-) (= 0 (% pos 3)))
             (put-text-property pos (1+ pos) 'syntax-table '(2)))
            ((and (= c ?\() (= 0 (% pos 2)))
             (put-text-property pos (1+ pos) 'syntax-table '(1)))
            ((= c ?.)
             (put-text-property pos (1+ pos) 'syntax-table '(3)))
            ((and (= c ?q) (= 0 (% pos 5)))
             (put-text-property pos (+ pos 3) 'syntax-table '(0)))))
    (setq pos (1+ pos)))
  (setq res '("\\_<foo\\_>" "\\bbar" "\\w+-bar" "\\s(baz" "\\_<foo[.]x\\_>"
              "[[:space:]]ux" "\\sw+x\\b" "\\<x" "é\\w" "[[:word:]]-x"))
  (while res
    (let ((re (car res)) (round 0))
      (while (< round 3)
        (goto-char (point-min))
        (let ((hits nil))
          (while (re-search-forward re nil t)
            (setq hits (cons (match-beginning 0) hits)))
          (setq out (cons (list re round (length hits) hits) out)))
        (setq round (1+ round)))
      (goto-char (point-max))
      (let ((hits nil))
        (while (re-search-backward re nil t) (setq hits (cons (point) hits)))
        (setq out (cons (list re 'back (length hits) hits) out))))
    (setq res (cdr res)))
  (let ((s (apply 'concat (make-list 30 "ab-cd ef.gh "))) (round 0))
    (put-text-property 0 40 'syntax-table '(2) s)
    (put-text-property 100 130 'syntax-table '(3) s)
    (while (< round 3)
      (let ((i 0) (hits nil))
        (while (string-match "\\_<\\w+-cd\\_>\\|\\bef\\.\\w" s i)
          (setq hits (cons (match-beginning 0) hits) i (match-end 0)))
        (setq out (cons (list 'string round hits) out)))
      (setq round (1+ round))))
  (nreverse out))
"#;
    let run = |mode| {
        with_dfa_mode(mode, || {
            let mut ev = crate::emacs_core::eval::Context::new();
            let value = ev.eval_str(program).expect("program evaluates");
            crate::emacs_core::print::print_value(&value)
        })
    };
    let off = run(DfaMode::Off);
    reset_dfa_stats();
    let on = run(DfaMode::On);
    let stats = dfa_stats();
    let verify = run(DfaMode::Verify);
    assert_eq!(on, off);
    assert_eq!(verify, off);
    assert!(
        stats.positional > 0
            && stats.skipped > 0
            && stats.positional_chars > 0
            && stats.plain_runs > 0,
        "{stats:?}"
    );
    assert_eq!(stats.verify_bad_no + stats.verify_bad_yes, 0, "{stats:?}");
}

/// The sparse scan still runs its classic EOF attempt, but repeated
/// successful passes ending there cannot heat a never-useful cold DFA.
#[test]
fn cold_successful_passes_with_eof_failures_never_build() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\(z\\)[0-9]", false, false).unwrap();
    let text = b"z1 z2";
    let pass = || {
        [
            search(&compiled, text, 0, text.len() as isize, &syntax, 0),
            search(&compiled, text, 2, (text.len() - 2) as isize, &syntax, 0),
            search(&compiled, text, text.len(), 0, &syntax, 0),
        ]
    };
    let expected = with_dfa_mode(DfaMode::Off, pass);
    assert!(expected[0].0.is_some() && expected[1].0.is_some());
    assert_eq!(expected[2], (None, false));
    reset_dfa_stats();
    with_cold_path(true, || {
        with_dfa_mode(DfaMode::On, || {
            for _ in 0..COLD_THRESHOLD * 4 {
                let before = matcher_entry_count();
                assert_eq!(pass(), expected);
                assert_eq!(
                    matcher_entry_count() - before,
                    3,
                    "two matches and the unchanged classic EOF attempt"
                );
            }
        });
    });
    assert!(!compiled.dfa.initialized());
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
    let stats = dfa_stats();
    assert_eq!(stats.searches, 0, "{stats:?}");
    assert_eq!(stats.builds, 0, "{stats:?}");
}

/// The board's backquoted `(defun . ,f)` is a real isolated miss before
/// another true header. Repeating that scan cannot heat the mostly matching
/// pattern, even after the old cumulative threshold would have built it.
#[test]
fn cold_isolated_literal_failures_never_build() {
    let syntax = DefaultSyntaxLookup;
    let mut text = vec![b'x'; 4096];
    text.extend_from_slice(b"\n(defun first)\n`(defun . ,f)\n(defun second)\n");
    for fold in [false, true] {
        let compiled = regex_compile("(defun \\([-a-z0-9]+\\)", false, fold).unwrap();
        let pass = || {
            let mut start = 0;
            let mut results = Vec::new();
            loop {
                let result = search(
                    &compiled,
                    &text,
                    start,
                    (text.len() - start) as isize,
                    &syntax,
                    start,
                );
                let next = result.0.as_ref().map(|(_, _, ends)| ends[0] as usize);
                results.push(result);
                let Some(end) = next else {
                    break;
                };
                start = end;
            }
            results
        };
        let expected = with_dfa_mode(DfaMode::Off, pass);
        assert_eq!(
            expected.len(),
            3,
            "two captures and the terminal failed search"
        );
        assert!(expected[0].0.is_some() && expected[1].0.is_some());
        assert_eq!(expected[2], (None, false));
        reset_dfa_stats();
        with_cold_path(true, || {
            with_dfa_mode(DfaMode::On, || {
                for _ in 0..COLD_THRESHOLD * 4 {
                    let before = matcher_entry_count();
                    assert_eq!(pass(), expected);
                    assert_eq!(
                        matcher_entry_count() - before,
                        3,
                        "two matches, one real failure, no prefilter EOF attempt"
                    );
                }
            });
        });
        assert!(!compiled.dfa.initialized());
        assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
        let stats = dfa_stats();
        assert_eq!(stats.searches, 0, "{stats:?}");
        assert_eq!(stats.builds, 0, "{stats:?}");
        let verify = with_cold_path(true, || with_dfa_mode(DfaMode::Verify, pass));
        assert_eq!(verify, expected);
        assert_eq!(dfa_stats().verify_bad_no + dfa_stats().verify_bad_yes, 0);
    }
}

#[test]
fn cached_prefix_rejections_respect_every_consumed_boundary() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("zz[0-9]", false, false).unwrap();
    let text = b"zzz";
    let mut dfa = first_step_dfa(&compiled, &syntax);
    assert_eq!(
        with_first_step(false, || {
            dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &syntax)
        }),
        Exists::No { consumed: 2 }
    );
    assert!(!dfa.cached_first_step_dead(&compiled, text, 0, text.len(), 0));
    let probe = |dfa: &ExistenceDfa, stop, safe_span| {
        dfa.cached_prefix_rejection(&compiled, text, 0, stop, 0, safe_span)
    };
    assert_eq!(probe(&dfa, text.len(), usize::MAX), Some(2));
    assert_eq!(probe(&dfa, 2, usize::MAX), None);
    assert_eq!(probe(&dfa, text.len(), 2), None);
    assert_eq!(probe(&dfa, text.len(), 3), Some(2));
    assert_eq!(
        dfa.cached_prefix_rejection(&compiled, b"zzq", 0, 3, 0, usize::MAX),
        None,
        "an unknown byte must resume the classifier"
    );
    dfa.search.read_limit = 2;
    assert_eq!(probe(&dfa, text.len(), usize::MAX), None);
    dfa.search.read_limit = usize::MAX;
    dfa.search.positional = true;
    dfa.plain = 0..2;
    assert_eq!(probe(&dfa, text.len(), usize::MAX), None);
    dfa.plain = 0..3;
    assert_eq!(probe(&dfa, text.len(), usize::MAX), Some(2));

    // A later point assertion invalidates a cached failure after a live byte.
    let at_point = regex_compile("zz\\=q", false, false).unwrap();
    let mut dfa = first_step_dfa(&at_point, &syntax);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&at_point, b"zzq", 0, 3, 99, &syntax),
            Exists::No { consumed: 2 }
        );
    });
    assert_eq!(
        dfa.cached_prefix_rejection(&at_point, b"zzq", 0, 3, 99, usize::MAX),
        Some(2)
    );
    assert_eq!(
        dfa.cached_prefix_rejection(&at_point, b"zzq", 0, 3, 2, usize::MAX),
        None
    );
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&at_point, b"zzq", 0, 3, 2, &syntax)
        }),
        Exists::Yes
    );
    assert!(re_match(&at_point, b"zzq", 0, 3, &syntax, 2).is_some());

    // A syntax property in the middle of a warmed prefix changes No to Yes.
    let word = regex_compile("zz\\w", false, false).unwrap();
    let mut dfa = first_step_dfa(&word, &syntax);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&word, b"zz-", 0, 3, 0, &syntax),
            Exists::No { consumed: 2 }
        );
    });
    assert_eq!(
        dfa.cached_prefix_rejection(&word, b"zz-", 0, 3, 0, usize::MAX),
        Some(2)
    );
    let property = PropertyRunLookup::new(
        &DefaultSyntaxLookup,
        vec![(2, 3, RunSyntax::Descriptor(SyntaxClass::Word))],
    );
    dfa.begin_search(&word, &property);
    dfa.plain = 0..2;
    assert_eq!(
        dfa.cached_prefix_rejection(&word, b"zz-", 0, 3, 0, usize::MAX),
        None
    );
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&word, b"zz-", 0, 3, 0, &property)
        }),
        Exists::Yes
    );
    assert!(re_match(&word, b"zz-", 0, 3, &property, 0).is_some());

    // A frontier after live bytes must still leave its read to the matcher.
    let mut dfa = first_step_dfa(&word, &syntax);
    with_first_step(false, || {
        assert_eq!(
            dfa.anchored_exists(&word, b"zz-", 0, 3, 0, &syntax),
            Exists::No { consumed: 2 }
        );
    });
    let mut frontier = PropertyRunLookup::new(&DefaultSyntaxLookup, Vec::new());
    frontier.frontier = 2;
    dfa.begin_search(&word, &frontier);
    dfa.plain = 0..usize::MAX;
    assert_eq!(
        dfa.cached_prefix_rejection(&word, b"zz-", 0, 3, 0, usize::MAX),
        None
    );
    assert_eq!(
        with_first_step(true, || {
            dfa.anchored_exists(&word, b"zz-", 0, 3, 0, &frontier)
        }),
        Exists::Unknown
    );
    assert_eq!(frontier.crossed.get(), None);
    assert!(re_match(&word, b"zz-", 0, 3, &frontier, 0).is_none());
    assert_eq!(frontier.crossed.get(), Some(2));
}

#[test]
fn inline_cached_prefix_keeps_consumed_accounting_and_captures() {
    let syntax = DefaultSyntaxLookup;
    let compiled = regex_compile("\\(zz\\)[0-9]", false, false).unwrap();
    let text = b"zzz zz7";
    let expected = with_dfa_mode(DfaMode::Off, || {
        search(&compiled, text, 0, text.len() as isize, &syntax, 0)
    });
    assert!(expected.0.is_some());
    prime(&compiled, &syntax).unwrap();
    with_first_step(false, || {
        with_dfa_mode(DfaMode::On, || {
            assert_eq!(
                search(&compiled, text, 0, text.len() as isize, &syntax, 0),
                expected
            );
        });
    });
    with_first_step(true, || {
        with_dfa_mode(DfaMode::On, || {
            let mut lease = DfaLease::acquire(&compiled, &syntax, text.len()).unwrap();
            let DfaSlot::Live(live) = &mut *lease.slot else {
                panic!("primed slot should be live");
            };
            let safe_span = live.overflow_free_span;
            live.overflow_free_span = 2;
            assert!(!lease.try_inline_first_step_skip(&compiled, text, 0, text.len(), 0));
            let DfaSlot::Live(live) = &mut *lease.slot else {
                panic!("guarded rejection should stay live");
            };
            live.overflow_free_span = safe_span;
            let before = live.dfa.counters;
            let entries = matcher_entry_count();
            assert!(lease.try_inline_first_step_skip(&compiled, text, 0, text.len(), 0));
            assert_eq!(matcher_entry_count(), entries);
            let DfaSlot::Live(live) = &*lease.slot else {
                panic!("cached rejection should stay live");
            };
            assert_eq!(live.dfa.counters.no, before.no + 1);
            assert_eq!(live.dfa.counters.bytes, before.bytes + 2);
            assert_eq!(lease.skipped, 1);
        });
        let entries = matcher_entry_count();
        let on = with_dfa_mode(DfaMode::On, || {
            search(&compiled, text, 0, text.len() as isize, &syntax, 0)
        });
        assert_eq!(on, expected);
        assert_eq!(matcher_entry_count() - entries, 1);
        let entries = matcher_entry_count();
        let verified = with_dfa_mode(DfaMode::Verify, || {
            search(&compiled, text, 0, text.len() as isize, &syntax, 0)
        });
        assert_eq!(verified, expected);
        assert_eq!(matcher_entry_count() - entries, 4);
        let stats = dfa_stats();
        assert_eq!(stats.verify_bad_no + stats.verify_bad_yes, 0, "{stats:?}");
    });
}

/// A lone nonempty failure heats the slot when the complete search fails,
/// including direct anchored exits and backward sparse-scan exhaustion.
#[test]
fn cold_isolated_whole_search_failures_admit_at_threshold() {
    let syntax = DefaultSyntaxLookup;
    for (source, start, range) in [
        ("z\\([0-9]\\)", 0, 1),
        ("z\\([0-9]\\)", 1, -1),
        ("\\`z\\([0-9]\\)", 0, 1),
        ("^z\\([0-9]\\)", 0, 1),
    ] {
        let compiled = regex_compile(source, false, false).unwrap();
        let before = matcher_entry_count();
        let expected = with_dfa_mode(DfaMode::Off, || {
            search(&compiled, b"z", start, range, &syntax, start)
        });
        let classic_entries = matcher_entry_count() - before;
        assert_eq!(expected, (None, false));
        assert!(classic_entries > 0);
        reset_dfa_stats();
        with_cold_path(true, || {
            with_dfa_mode(DfaMode::On, || {
                for failed_searches in 1..COLD_THRESHOLD {
                    let before = matcher_entry_count();
                    assert_eq!(
                        search(&compiled, b"z", start, range, &syntax, start),
                        expected,
                        "{source:?} from {start}"
                    );
                    assert_eq!(matcher_entry_count() - before, classic_entries);
                    assert!(!compiled.dfa.initialized());
                    assert!(matches!(
                        *compiled.dfa.slot(),
                        DfaSlot::Cold { failed } if failed == failed_searches
                    ));
                }
                assert_eq!(dfa_stats().searches, 0);
                let before = matcher_entry_count();
                assert_eq!(
                    search(&compiled, b"z", start, range, &syntax, start),
                    expected
                );
                assert_eq!(matcher_entry_count() - before, classic_entries);
                assert!(compiled.dfa.initialized());
                assert!(matches!(*compiled.dfa.slot(), DfaSlot::Live(_)));
                assert_eq!(dfa_stats().builds, 1);
                assert_eq!(dfa_stats().searches, 1);

                // The completed threshold search ran only classic; the next
                // search uses its published DFA and skips the same failure.
                let before = matcher_entry_count();
                assert_eq!(
                    search(&compiled, b"z", start, range, &syntax, start),
                    expected
                );
                assert_eq!(matcher_entry_count() - before, 0);
                assert!(dfa_stats().skipped > 0);
            });
            let match_start = if range < 0 { 2 } else { 0 };
            let match_range = if range < 0 { -2 } else { 2 };
            let expected_match = with_dfa_mode(DfaMode::Off, || {
                search(
                    &compiled,
                    b"z7",
                    match_start,
                    match_range,
                    &syntax,
                    match_start,
                )
            });
            assert!(expected_match.0.is_some());
            assert_eq!(
                with_dfa_mode(DfaMode::On, || {
                    search(
                        &compiled,
                        b"z7",
                        match_start,
                        match_range,
                        &syntax,
                        match_start,
                    )
                }),
                expected_match
            );
            let before = matcher_entry_count();
            assert_eq!(
                with_dfa_mode(DfaMode::Verify, || {
                    search(&compiled, b"z", start, range, &syntax, start)
                }),
                expected
            );
            assert_eq!(matcher_entry_count() - before, classic_entries);
            assert_eq!(dfa_stats().verify_bad_no + dfa_stats().verify_bad_yes, 0);
        });
    }

    // A normal first miss followed by overflow must leave its heat local:
    // the candidate macro's overflow return bypasses completion admission.
    let compiled = regex_compile("x\\(?:a\\|b\\)*c", false, false).unwrap();
    let text = [&b"xq x"[..], &b"ab".repeat(100_000)].concat();
    let expected = with_dfa_mode(DfaMode::Off, || {
        search(&compiled, &text, 0, text.len() as isize, &syntax, 0)
    });
    assert_eq!(expected, (None, true));
    assert_eq!(
        with_cold_path(true, || {
            with_dfa_mode(DfaMode::On, || {
                search(&compiled, &text, 0, text.len() as isize, &syntax, 0)
            })
        }),
        expected
    );
    assert!(!compiled.dfa.initialized());
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));

    // A matcher quit also returns None, but it does not complete the scan.
    // It must not turn a pending isolated failure into admitted heat.
    let compiled = regex_compile("z[0-9]", false, false).unwrap();
    let flag = crate::emacs_core::eval::install_quit_requested_for_test(true);
    let results = with_cold_path(true, || {
        with_dfa_mode(DfaMode::On, || {
            (0..COLD_THRESHOLD * 2)
                .map(|_| search(&compiled, b"z", 0, 1, &syntax, 0))
                .collect::<Vec<_>>()
        })
    });
    crate::emacs_core::eval::clear_quit_requested_for_test();
    drop(flag);
    assert!(results.into_iter().all(|result| result == (None, false)));
    assert!(!compiled.dfa.initialized());
    assert!(matches!(*compiled.dfa.slot(), DfaSlot::Cold { failed: 0 }));
}
