//! Tests of the existence DFA (P3.3): the NFA over the rewind view (C2).

use super::*;

use crate::emacs_core::regex_emacs::{
    DefaultSyntaxLookup, fail_stack_push_sites, re_match, regex_compile,
};

use crate::emacs_core::regex_emacs::{
    CaseTranslation, LookupClassKey, SyntaxCacheKey, match_anychar_at, match_categoryspec_at,
    match_charset_at, match_exactn_char_at, match_syntaxspec_at, match_syntaxspecset_at,
    regex_compile_lisp_with_translation,
};

use crate::emacs_core::syntax::SyntaxClass;

use neovm_value::value::Value;

use crate::emacs_core::regex_emacs::{
    MatchRegisters, MatchScratch, re_match_internal, take_fail_stack_probe, take_matcher_overflow,
    take_pike_fallback,
};

use crate::emacs_core::regex_emacs::{matcher_entry_count, re_search};

#[test]
fn split_gap_cached_prefix_selects_storage_once_after_the_first_byte() {
    use crate::emacs_core::regex_emacs::{Segments, chunk_query_count, segment_byte_query_count};

    let syntax = DefaultSyntaxLookup;
    for prefix_len in [0, 3, 7, 8] {
        let expression = format!("{}Z", "a".repeat(prefix_len));
        let text = format!("{}x", "a".repeat(prefix_len));
        let text = text.as_bytes();
        let compiled = regex_compile(&expression, false, false).unwrap();
        let mut dfa = ExistenceDfa::new(Nfa::build(&compiled).unwrap());
        let context = ClassContext::of_search(&compiled, dfa.nfa(), &syntax).unwrap();
        dfa.classes.sync(context);
        dfa.begin_search(&compiled, &syntax);
        assert_eq!(
            with_first_step(false, || {
                dfa.anchored_exists(&compiled, text, 0, text.len(), 0, &syntax)
            }),
            Exists::No {
                consumed: prefix_len
            }
        );
        let counters = dfa.counters;
        let transitions = dfa.trans.clone();
        for split in 0..=text.len() {
            let segmented = Segments {
                left: &text[..split],
                right: &text[split..],
            };
            let before = (segment_byte_query_count(), chunk_query_count());
            let actual =
                dfa.cached_prefix_rejection(&compiled, &segmented, 0, text.len(), 0, usize::MAX);
            assert_eq!(
                actual,
                (prefix_len < CACHED_PREFIX_BYTES).then_some(prefix_len)
            );
            let byte_queries = segment_byte_query_count() - before.0;
            let chunk_queries = chunk_query_count() - before.1;
            assert_eq!(
                dfa.counters, counters,
                "cached probes must not publish counters"
            );
            assert_eq!(
                dfa.trans, transitions,
                "cached probes must not build transitions"
            );
            if prefix_len == 0 {
                assert_eq!((byte_queries, chunk_queries), (1, 0));
            } else {
                assert!(
                    byte_queries + chunk_queries <= 2,
                    "prefix={prefix_len} split={split}: {byte_queries} individual byte selections + {chunk_queries} chunk selections"
                );
            }
        }
    }
}

#[test]
fn split_gap_cached_first_transition_respects_first_byte_limits() {
    let compiled = regex_compile(r"\bZ", false, false).unwrap();
    let text = b"x-";
    let mut dfa = warmed_dead_prefix(&compiled, text, 1, 0);
    let check = |dfa: &ExistenceDfa, stop, point, span, expected| {
        assert_cached_prefix_at_every_split(dfa, &compiled, text, (1, stop, point, span), expected);
    };
    check(&dfa, 2, 0, 1, Some(0));
    check(&dfa, 2, 3, 1, Some(0));
    check(&dfa, 0, 0, 1, None);
    check(&dfa, 1, 0, 1, None);
    check(&dfa, 2, 0, 0, None);
    dfa.search.read_limit = 1;
    check(&dfa, 2, 0, 1, None);
    dfa.search.read_limit = 2;
    check(&dfa, 2, 0, 1, Some(0));

    // A positioned search may use the base-table cache only when both the
    // current and preceding word-boundary facts are in its known plain run.
    dfa.search.positional = true;
    dfa.plain = 0..2;
    check(&dfa, 2, 0, 1, Some(0));
    dfa.plain = 0..1;
    check(&dfa, 2, 0, 1, None);
    dfa.plain = 1..2;
    check(&dfa, 2, 0, 1, None);
    dfa.plain = 0..2;
    check(&dfa, 2, 0, 1, Some(0));
}

#[test]
fn split_gap_cached_first_transition_respects_dynamic_point() {
    // The Q alternative makes the first byte class and its DEAD transition
    // cacheable while the point assertion is false. At point it matches x.
    let compiled = regex_compile(r"\=x\|Q", false, false).unwrap();
    let text = b"-x";
    let mut dfa = warmed_dead_prefix(&compiled, text, 1, 0);
    assert_cached_prefix_at_every_split(&dfa, &compiled, text, (1, 2, 0, 1), Some(0));
    assert_cached_prefix_at_every_split(&dfa, &compiled, text, (1, 2, 99, 1), Some(0));
    assert_cached_prefix_at_every_split(&dfa, &compiled, text, (1, 2, 1, 1), None);
    assert_eq!(
        dfa.anchored_exists(&compiled, text, 1, 2, 1, &DefaultSyntaxLookup),
        Exists::Yes
    );
    assert!(re_match(&compiled, text, 1, 2, &DefaultSyntaxLookup, 1).is_some());
}

fn warmed_dead_prefix(
    compiled: &CompiledPattern,
    text: &[u8],
    position: usize,
    point: usize,
) -> ExistenceDfa {
    let syntax = DefaultSyntaxLookup;
    let mut dfa = ExistenceDfa::new(Nfa::build(compiled).unwrap());
    let context = ClassContext::of_search(compiled, dfa.nfa(), &syntax).unwrap();
    dfa.classes.sync(context);
    dfa.begin_search(compiled, &syntax);
    assert_eq!(
        with_first_step(false, || {
            dfa.anchored_exists(compiled, text, position, text.len(), point, &syntax)
        }),
        Exists::No { consumed: 0 }
    );
    dfa
}

fn assert_cached_prefix_at_every_split(
    dfa: &ExistenceDfa,
    compiled: &CompiledPattern,
    text: &[u8],
    (position, stop, point, span): (usize, usize, usize, usize),
    expected: Option<usize>,
) {
    use crate::emacs_core::regex_emacs::Segments;

    assert_eq!(
        dfa.cached_prefix_rejection(compiled, text, position, stop, point, span),
        expected
    );
    for split in 0..=text.len() {
        let text = Segments {
            left: &text[..split],
            right: &text[split..],
        };
        assert_eq!(
            dfa.cached_prefix_rejection(compiled, &text, position, stop, point, span),
            expected,
            "split={split} position={position} stop={stop} point={point} span={span}"
        );
    }
}

#[test]
fn dfa_overrides_restore_nested_and_unwound_state() {
    let original_mode = DFA_MODE_OVERRIDE.with(std::cell::Cell::get);
    with_dfa_mode(DfaMode::Off, || {
        assert_eq!(dfa_mode(), DfaMode::Off);
        with_dfa_mode(DfaMode::On, || assert_eq!(dfa_mode(), DfaMode::On));
        assert_eq!(dfa_mode(), DfaMode::Off);
        let unwind = std::panic::catch_unwind(|| {
            with_dfa_mode(DfaMode::Verify, || {
                assert_eq!(dfa_mode(), DfaMode::Verify);
                panic!("exercise DFA mode override unwinding");
            });
        });
        assert!(unwind.is_err());
        assert_eq!(dfa_mode(), DfaMode::Off);
    });
    assert_eq!(DFA_MODE_OVERRIDE.with(std::cell::Cell::get), original_mode);

    let original_cold = DFA_COLD_OVERRIDE.with(std::cell::Cell::get);
    with_cold_path(false, || {
        assert!(!cold_path_enabled());
        let unwind = std::panic::catch_unwind(|| {
            with_cold_path(true, || {
                assert!(cold_path_enabled());
                panic!("exercise DFA cold-path override unwinding");
            });
        });
        assert!(unwind.is_err());
        assert!(!cold_path_enabled());
    });
    assert_eq!(DFA_COLD_OVERRIDE.with(std::cell::Cell::get), original_cold);

    let original_first_step = DFA_FIRST_STEP_OVERRIDE.with(std::cell::Cell::get);
    with_first_step(false, || {
        assert!(!first_step_enabled());
        let unwind = std::panic::catch_unwind(|| {
            with_first_step(true, || {
                assert!(first_step_enabled());
                panic!("exercise DFA first-step override unwinding");
            });
        });
        assert!(unwind.is_err());
        assert!(!first_step_enabled());
    });
    assert_eq!(
        DFA_FIRST_STEP_OVERRIDE.with(std::cell::Cell::get),
        original_first_step
    );
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

fn nfa_of(pattern: &str) -> Nfa {
    let compiled = regex_compile(pattern, false, false).expect("pattern compiles");
    Nfa::build(&compiled).unwrap_or_else(|why| panic!("{pattern:?} is ineligible: {why:?}"))
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

/// The same unibyte byte has two GNU syntax identities: symbol opcodes
/// inspect the raw current byte, while every previous/word read promotes it
/// to byte8 (regex-emacs.c:55-56,67-95,5111,5163).
struct RawCurrentSymbolLookup;

impl SyntaxLookup for RawCurrentSymbolLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        if c == '\u{a9}' {
            SyntaxClass::Punctuation
        } else {
            DefaultSyntaxLookup.char_syntax(c)
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::External {
            id: usize::MAX - 1,
            epoch: 0,
        }
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: usize::MAX - 1,
            category: 0,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

/// A syntax lookup whose `WORD_BOUNDARY_P` separates CJK from other word
/// constituents, as GNU's char-script-table does.
struct ScriptBoundaryLookup;

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

fn random_base(rng: &mut DfaRng) -> &'static dyn SyntaxLookup {
    if rng.below(2) == 0 {
        &DefaultSyntaxLookup
    } else {
        &CustomTableLookup
    }
}

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
