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
