//! Typed differential-fuzzing seam for NeoVM's Emacs-compatible regexp engine.
//!
//! This module deliberately exposes cases and observable comparison results,
//! not compiler bytecode or engine-control switches. The implementation keeps
//! forced routing scoped and panic-safe inside the regexp module.

use std::fmt;

use strum::IntoEnumIterator;

use crate::emacs_core::emacs_char;
use crate::emacs_core::regex_emacs::{
    self, DefaultSyntaxLookup, LookupClassKey, MatchRegisters, RegexEngineOverride, SyntaxCacheKey,
    SyntaxLookup,
};
use crate::emacs_core::syntax::SyntaxClass;

/// Representation of the searched text.
///
/// A multibyte target is a Lisp string or buffer holding Emacs's internal
/// multibyte form; a unibyte target is one byte per character (a unibyte
/// string or buffer), which case folding and the fastmap read differently.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, strum::Display, strum::EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum SearchTarget {
    #[default]
    Multibyte,
    Unibyte,
}

/// One regexp differential case.
///
/// Offsets are mapped into `0..=text.len()` rather than rejected, so arbitrary
/// inputs exercise the beginning, middle, and end of every generated text.
#[derive(Clone, Copy, Debug)]
pub struct RegexCase<'a> {
    pattern: &'a str,
    text: &'a [u8],
    case_fold: bool,
    start: usize,
    point: usize,
    target: SearchTarget,
}

impl<'a> RegexCase<'a> {
    pub const fn new(
        pattern: &'a str,
        text: &'a [u8],
        case_fold: bool,
        start: usize,
        point: usize,
    ) -> Self {
        Self {
            pattern,
            text,
            case_fold,
            start,
            point,
            target: SearchTarget::Multibyte,
        }
    }

    /// The same case searched in a text of representation `target`.
    #[must_use]
    pub const fn with_target(self, target: SearchTarget) -> Self {
        Self { target, ..self }
    }

    fn start(self) -> usize {
        offset_in_text(self.start, self.text.len())
    }

    fn point(self) -> usize {
        offset_in_text(self.point, self.text.len())
    }
}

fn offset_in_text(offset: usize, text_len: usize) -> usize {
    offset % text_len.saturating_add(1)
}

/// Independent implementations that can serve as differential oracles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::Display, strum::EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum RegexDifferential {
    /// Pure backtracker versus the eligible non-backtracking Pike VM.
    PikeVm,
    /// Exhaustive candidate scanning versus production fastmap/prefilter skips.
    SearchOptimizations,
    /// Searches with the existence DFA filtering candidates
    /// (`NEOVM_REGEX_DFA=on`, and `verify` finding no contradicted verdict)
    /// versus the matcher alone: position, registers and the fail-stack
    /// overflow flag.  Each search runs under the standard syntax and again
    /// under `syntax-table` property runs derived from the text
    /// ([`PropertyRunSyntax`]).
    ExistenceDfa,
}

/// Observable regexp operations compared by the differential checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::Display, strum::EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum RegexOperation {
    Match,
    SearchForward,
    SearchBackward,
}

/// Successful outcome from one differential check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegexCheck {
    Equivalent { comparisons: usize },
    NotApplicable(RegexNotApplicable),
}

/// Why a generated case could not exercise the selected differential.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum RegexNotApplicable {
    CompileRejected,
    PikeIneligible,
    OracleOverflow,
    DfaIneligible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedMatch {
    match_start: usize,
    group_starts: Vec<i64>,
    group_ends: Vec<i64>,
}

type MatchResult = Option<NormalizedMatch>;

fn normalize(result: Option<(usize, MatchRegisters)>) -> MatchResult {
    result.map(|(match_start, registers)| NormalizedMatch {
        match_start,
        group_starts: registers.start.to_vec(),
        group_ends: registers.end.to_vec(),
    })
}

/// A semantic disagreement between the selected regexp implementations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegexDivergence {
    differential: RegexDifferential,
    operation: RegexOperation,
    oracle: MatchResult,
    candidate: MatchResult,
}

impl fmt::Display for RegexDivergence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} divergence during {}: oracle={:?}, candidate={:?}",
            self.differential, self.operation, self.oracle, self.candidate
        )
    }
}

impl std::error::Error for RegexDivergence {}

/// Compare one case through the selected independent implementations.
///
/// Rejected or ineligible generated cases are data, not failures. A returned
/// error is always a semantic mismatch and should crash the fuzz target.
pub fn check_regex_differential(
    case: RegexCase<'_>,
    differential: RegexDifferential,
) -> Result<RegexCheck, RegexDivergence> {
    // Every search optimization is on for the candidate side, the opt-in
    // ones included (they are read when the pattern compiles).
    let compiled = regex_emacs::with_anchor_alt(true, || {
        regex_emacs::regex_compile(case.pattern, false, case.case_fold)
    });
    let mut compiled = match compiled {
        Ok(compiled) => compiled,
        Err(_) => {
            return Ok(RegexCheck::NotApplicable(
                RegexNotApplicable::CompileRejected,
            ));
        }
    };
    // The search front end fixes the target representation after compiling
    // (`compile_lisp_pattern_with_posix_translation`); do the same.
    compiled.target_multibyte = case.target == SearchTarget::Multibyte;

    // Every Lisp string and buffer holds VALID internal multibyte text, and
    // the candidate scans are exact only there: GNU's own byte-indexed
    // `fastmap[*d]` loop tests the lead byte of an overlong `E0 81 81`, which
    // the matcher decodes to `A`.  Give the scans the text a Lisp string made
    // of these bytes would hold (GNU `str_as_multibyte`: an invalid sequence
    // becomes raw-byte characters).  The engine differential keeps the raw
    // bytes: both engines decode the text the same way.
    let valid_text;
    let case = match (differential, case.target) {
        (
            RegexDifferential::SearchOptimizations | RegexDifferential::ExistenceDfa,
            SearchTarget::Multibyte,
        ) => {
            valid_text = emacs_char::str_as_multibyte(case.text);
            RegexCase {
                text: &valid_text,
                ..case
            }
        }
        _ => case,
    };

    match differential {
        RegexDifferential::PikeVm if !compiled.pike_eligible => {
            return Ok(RegexCheck::NotApplicable(
                RegexNotApplicable::PikeIneligible,
            ));
        }
        RegexDifferential::ExistenceDfa if regex_emacs::dfa::Nfa::build(&compiled).is_err() => {
            return Ok(RegexCheck::NotApplicable(RegexNotApplicable::DfaIneligible));
        }
        RegexDifferential::PikeVm
        | RegexDifferential::SearchOptimizations
        | RegexDifferential::ExistenceDfa => {}
    }

    let mut comparisons = 0;
    for operation in RegexOperation::iter() {
        if matches!(
            differential,
            RegexDifferential::SearchOptimizations | RegexDifferential::ExistenceDfa
        ) && operation == RegexOperation::Match
        {
            // An anchored match has no candidate scan to optimize.
            continue;
        }

        let comparison = match differential {
            RegexDifferential::PikeVm => compare_engines(&compiled, case, operation),
            RegexDifferential::SearchOptimizations => {
                compare_search_optimizations(&compiled, case, operation)
            }
            RegexDifferential::ExistenceDfa => {
                compare_existence_dfa(&compiled, case, operation, &DefaultSyntaxLookup)
            }
        };
        let Some((oracle, candidate)) = comparison else {
            return Ok(RegexCheck::NotApplicable(
                RegexNotApplicable::OracleOverflow,
            ));
        };
        comparisons += 1;

        if oracle != candidate {
            return Err(RegexDivergence {
                differential,
                operation,
                oracle,
                candidate,
            });
        }

        if differential == RegexDifferential::ExistenceDfa {
            let properties = PropertyRunSyntax::for_text(case.text, compiled.target_multibyte);
            let Some((oracle, candidate)) =
                compare_existence_dfa(&compiled, case, operation, &properties)
            else {
                return Ok(RegexCheck::NotApplicable(
                    RegexNotApplicable::OracleOverflow,
                ));
            };
            comparisons += 1;
            if oracle != candidate {
                return Err(RegexDivergence {
                    differential,
                    operation,
                    oracle,
                    candidate,
                });
            }
        }
    }

    Ok(RegexCheck::Equivalent { comparisons })
}

fn compare_engines(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
) -> Option<(MatchResult, MatchResult)> {
    let _ = regex_emacs::take_matcher_overflow();
    let oracle = normalize(regex_emacs::with_regex_engine_override(
        RegexEngineOverride::Backtracker,
        || run_operation(compiled, case, operation),
    ));
    if regex_emacs::take_matcher_overflow() {
        return None;
    }

    let candidate = normalize(regex_emacs::with_regex_engine_override(
        RegexEngineOverride::PikeVm,
        || run_operation(compiled, case, operation),
    ));
    Some((oracle, candidate))
}

fn compare_search_optimizations(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
) -> Option<(MatchResult, MatchResult)> {
    let _ = regex_emacs::take_matcher_overflow();
    let oracle = normalize(regex_emacs::with_fastmap_disabled(|| {
        run_operation(compiled, case, operation)
    }));
    if regex_emacs::take_matcher_overflow() {
        return None;
    }

    // A search builds its lazily derived scanners only once the text is long
    // enough to repay them; build them now so a short generated text still
    // runs every optimized scan, not only the per-character loops.
    regex_emacs::build_search_optimizations(compiled);
    let candidate = normalize(run_operation(compiled, case, operation));
    if regex_emacs::take_matcher_overflow() {
        return None;
    }
    Some((oracle, candidate))
}

/// The matcher alone against the DFA-filtered search.  The overflow flag is
/// part of the result: a rejection must never hide GNU's fail-stack overflow.
fn compare_existence_dfa(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
    syntax: &dyn SyntaxLookup,
) -> Option<(MatchResult, MatchResult)> {
    use regex_emacs::dfa::{DfaMode, with_dfa_mode};
    let observe = |mode: DfaMode| {
        let _ = regex_emacs::take_matcher_overflow();
        let found = normalize(with_dfa_mode(mode, || {
            run_operation_with(compiled, case, operation, syntax)
        }));
        (found, regex_emacs::take_matcher_overflow())
    };
    let (oracle, oracle_overflow) = observe(DfaMode::Off);
    let _ = regex_emacs::dfa::prime(compiled, &DefaultSyntaxLookup);
    let before = regex_emacs::dfa::dfa_stats();
    let (verified, verified_overflow) = observe(DfaMode::Verify);
    let after = regex_emacs::dfa::dfa_stats();
    let (filtered, filtered_overflow) = observe(DfaMode::On);
    let contradicted =
        after.verify_bad_no + after.verify_bad_yes > before.verify_bad_no + before.verify_bad_yes;
    // Fold the overflow flag and the verify verdicts into the compared
    // value: any difference is a divergence.
    let mark = |found: MatchResult, overflow: bool, contradicted: bool| {
        if overflow || contradicted {
            Some(NormalizedMatch {
                match_start: usize::MAX,
                group_starts: vec![i64::from(overflow), i64::from(contradicted)],
                group_ends: Vec::new(),
            })
        } else {
            found
        }
    };
    let oracle = mark(oracle, oracle_overflow, false);
    let verified = mark(verified, verified_overflow, contradicted);
    let filtered = mark(filtered, filtered_overflow, false);
    if verified != oracle {
        return Some((oracle, verified));
    }
    Some((oracle, filtered))
}

fn run_operation(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
) -> Option<(usize, MatchRegisters)> {
    run_operation_with(compiled, case, operation, &DefaultSyntaxLookup)
}

fn run_operation_with(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
    syntax: &dyn SyntaxLookup,
) -> Option<(usize, MatchRegisters)> {
    let start = case.start();
    let point = case.point();
    let text_len = case.text.len();

    match operation {
        RegexOperation::Match => {
            regex_emacs::re_match(compiled, case.text, start, text_len, syntax, point)
        }
        RegexOperation::SearchForward => regex_emacs::re_search(
            compiled,
            case.text,
            start,
            isize::try_from(text_len - start).unwrap_or(isize::MAX),
            syntax,
            point,
        ),
        RegexOperation::SearchBackward => regex_emacs::re_search(
            compiled,
            case.text,
            start,
            -isize::try_from(start).unwrap_or(isize::MAX),
            syntax,
            point,
        ),
    }
}

/// Syntax classes `syntax-table` properties give in [`PropertyRunSyntax`].
const PROPERTY_CLASSES: [SyntaxClass; 6] = [
    SyntaxClass::Word,
    SyntaxClass::Symbol,
    SyntaxClass::Whitespace,
    SyntaxClass::Punctuation,
    SyntaxClass::Open,
    SyntaxClass::EndComment,
];

/// The standard syntax plus `syntax-table` properties derived from the text:
/// the character starting with a byte `b` where `b % 5 == 0` begins a run of
/// `1 + b % 3` characters of class `PROPERTY_CLASSES[b % 6]`, as a descriptor
/// property gives it.  Arbitrary texts thus mix property runs and plain
/// stretches, so the existence DFA's property-run handling is fuzzed with
/// the rest of it.
struct PropertyRunSyntax {
    /// The property class at each character's first byte.
    classes: Vec<Option<SyntaxClass>>,
}

impl PropertyRunSyntax {
    fn for_text(text: &[u8], multibyte: bool) -> Self {
        let mut classes = vec![None; text.len()];
        let mut run: Option<(SyntaxClass, u8)> = None;
        let mut d = 0;
        while d < text.len() {
            // The matcher's `re_text_char` step.
            let len = if multibyte {
                emacs_char::string_char(&text[d..]).1
            } else {
                1
            };
            let byte = text[d];
            if run.is_none() && byte % 5 == 0 {
                run = Some((PROPERTY_CLASSES[byte as usize % 6], 1 + byte % 3));
            }
            if let Some((class, left)) = run {
                classes[d] = Some(class);
                run = (left > 1).then(|| (class, left - 1));
            }
            d += len.max(1);
        }
        Self { classes }
    }
}

impl SyntaxLookup for PropertyRunSyntax {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        DefaultSyntaxLookup.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, pos: usize) -> SyntaxClass {
        self.classes
            .get(pos)
            .copied()
            .flatten()
            .unwrap_or_else(|| self.char_syntax(c))
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
        true
    }

    fn plain_syntax_until(&self, pos: usize) -> usize {
        if self.classes.get(pos).is_some_and(Option::is_some) {
            return pos;
        }
        (pos..self.classes.len())
            .find(|&at| self.classes[at].is_some())
            .unwrap_or(usize::MAX)
    }
}
