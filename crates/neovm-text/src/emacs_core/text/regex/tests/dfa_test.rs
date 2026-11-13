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
