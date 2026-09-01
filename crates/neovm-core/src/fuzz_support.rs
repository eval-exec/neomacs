//! Typed differential-fuzzing seam for NeoVM's Emacs-compatible regexp engine.
//!
//! This module deliberately exposes cases and observable comparison results,
//! not compiler bytecode or engine-control switches. The implementation keeps
//! forced routing scoped and panic-safe inside the regexp module.

use std::fmt;

use strum::IntoEnumIterator;

use crate::emacs_core::regex_emacs::{
    self, DefaultSyntaxLookup, MatchRegisters, RegexEngineOverride,
};

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
        }
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
    let compiled = match regex_emacs::regex_compile(case.pattern, false, case.case_fold) {
        Ok(compiled) => compiled,
        Err(_) => {
            return Ok(RegexCheck::NotApplicable(
                RegexNotApplicable::CompileRejected,
            ));
        }
    };

    match differential {
        RegexDifferential::PikeVm if !compiled.pike_eligible => {
            return Ok(RegexCheck::NotApplicable(
                RegexNotApplicable::PikeIneligible,
            ));
        }
        RegexDifferential::PikeVm | RegexDifferential::SearchOptimizations => {}
    }

    let mut comparisons = 0;
    for operation in RegexOperation::iter() {
        if differential == RegexDifferential::SearchOptimizations
            && operation != RegexOperation::SearchForward
        {
            continue;
        }

        let comparison = match differential {
            RegexDifferential::PikeVm => compare_engines(&compiled, case, operation),
            RegexDifferential::SearchOptimizations => compare_search_optimizations(&compiled, case),
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
) -> Option<(MatchResult, MatchResult)> {
    let _ = regex_emacs::take_matcher_overflow();
    let oracle = normalize(regex_emacs::with_fastmap_disabled(|| {
        run_operation(compiled, case, RegexOperation::SearchForward)
    }));
    if regex_emacs::take_matcher_overflow() {
        return None;
    }

    let candidate = normalize(run_operation(compiled, case, RegexOperation::SearchForward));
    if regex_emacs::take_matcher_overflow() {
        return None;
    }
    Some((oracle, candidate))
}

fn run_operation(
    compiled: &regex_emacs::CompiledPattern,
    case: RegexCase<'_>,
    operation: RegexOperation,
) -> Option<(usize, MatchRegisters)> {
    let syntax = DefaultSyntaxLookup;
    let start = case.start();
    let point = case.point();
    let text_len = case.text.len();

    match operation {
        RegexOperation::Match => {
            regex_emacs::re_match(compiled, case.text, start, text_len, &syntax, point)
        }
        RegexOperation::SearchForward => regex_emacs::re_search(
            compiled,
            case.text,
            start,
            isize::try_from(text_len - start).unwrap_or(isize::MAX),
            &syntax,
            point,
        ),
        RegexOperation::SearchBackward => regex_emacs::re_search(
            compiled,
            case.text,
            start,
            -isize::try_from(start).unwrap_or(isize::MAX),
            &syntax,
            point,
        ),
    }
}
