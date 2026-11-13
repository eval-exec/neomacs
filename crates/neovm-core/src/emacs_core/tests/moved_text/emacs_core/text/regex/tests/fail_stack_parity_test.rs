//! GNU parity of the backtracker's fail stack (P3.3 section 12, D5-D7): the
//! cycle check of a non-greedy loop over a nullable body, and the "Stack
//! overflow in regexp matcher" that the Pike fallback used to mask.
//!
//! Every expected value below is GNU 31.1's `(string-match RE S)` followed by
//! `(match-data t)` (probe: `tmp/probes/nasty.el` in the U0.5 worktree).

use super::*;

use neovm_text::emacs_core::syntax::SyntaxClass;
use neovm_value::tagged::SymbolMode;
use neovm_value::{emacs_char, value::Value};
use regex_automata::{MatchKind, Span};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::regex_emacs::*;

#[test]
fn nullable_nongreedy_loop_answers_nil_from_lisp() {
    let mut ev = crate::test_utils::new_context();
    let caught = ev
        .eval_str(
            "(list (condition-case err (string-match \"\\\\(a\\\\|\\\\)+?x\" \"aaaay\") \
                     (error err)) \
                   (string-match \"\\\\(?:a?\\\\)*?b\" \"aaaac\"))",
        )
        .expect("condition-case evaluates");
    assert_eq!(crate::emacs_core::print::print_value(&caught), "(nil nil)");
}

#[test]
fn pike_fallback_overflow_signals_the_gnu_error_from_lisp() {
    let mut ev = crate::test_utils::new_context();
    let caught = ev
        .eval_str(
            "(condition-case err \
                 (string-match \"x\\\\(?:a\\\\|b\\\\)*c\" \
                               (concat \"x\" (apply #'concat (make-list 100000 \"ab\")))) \
               (error err))",
        )
        .expect("condition-case evaluates");
    assert_eq!(
        crate::emacs_core::print::print_value(&caught),
        "(error \"Stack overflow in regexp matcher\")"
    );
}

/// The bound behind the D7 fix: a backtracker path's fail stack never holds
/// more than `2 * push sites` entries per position it pushed at.  Measured
/// over random patterns with nested and nullable loops, including the
/// patterns the Pike VM cannot run (POSIX patterns are the C4 test's).
#[test]
fn fail_stack_depth_stays_within_the_overflow_bound() {
    crate::test_utils::init_test_tracing();
    let syntax = DefaultSyntaxLookup;
    let mut rng = BoundRng(0x5eed_f00d);
    let mut measured = 0usize;
    let mut worst_ratio = 0.0f64;
    for case in 0..20_000 {
        let pattern = bound_pattern(&mut rng, 2);
        let Ok(compiled) = regex_compile(&pattern, false, false) else {
            continue;
        };
        if compiled.bytecode().iter().any(|&b| {
            matches!(
                RegexOp::from_byte(b),
                Some(RegexOp::SucceedN | RegexOp::JumpN | RegexOp::SetNumberAt)
            )
        }) {
            continue;
        }
        let text = bound_text(&mut rng, 24);
        let sites = fail_stack_push_sites(&compiled.bytecode());
        for pos in 0..=text.len() {
            let _ = take_fail_stack_probe();
            let _ = take_matcher_overflow();
            // The backtrack budget caps the exponential shapes (it only
            // stops the run early; the depth bound holds at every step).
            let mut scratch = MatchScratch::default();
            let mut registers = MatchRegisters::default();
            let _ = re_match_internal(
                &mut scratch,
                &compiled,
                &text,
                pos,
                text.len(),
                &MatchSyntax::new(&syntax),
                pos,
                true,
                &mut registers,
            );
            let _ = take_pike_fallback();
            assert!(!take_matcher_overflow());
            let probe = take_fail_stack_probe();
            if probe.max_depth() == 0 {
                continue;
            }
            measured += 1;
            let consumed = probe.max_push_pos() - pos;
            let bound = (consumed + 1) * 2 * sites;
            worst_ratio =
                worst_ratio.max(probe.max_depth() as f64 / ((consumed + 1) * sites) as f64);
            assert!(
                probe.max_depth() <= bound,
                "case {case}: {pattern:?} at {pos} in {text:?}: depth {} > bound {bound} \
                 (consumed {consumed}, sites {sites})",
                probe.max_depth()
            );
        }
    }
    tracing::info!(
        measured,
        worst_ratio,
        "fail-stack depth per push site and position"
    );
    assert!(measured > 50_000, "too few measured runs: {measured}");
}

struct BoundRng(u64);

impl BoundRng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Random patterns over `a`/`b` with groups, alternations, empty arms and
/// every quantifier nested freely, so nullable loops inside loops appear.
fn bound_pattern(rng: &mut BoundRng, depth: usize) -> String {
    let mut out = String::new();
    let arms = 1 + rng.below(3);
    for arm in 0..arms {
        if arm > 0 {
            out.push_str("\\|");
        }
        for _ in 0..rng.below(4) {
            let atom = match rng.below(if depth == 0 { 4 } else { 7 }) {
                0 => "a".to_string(),
                1 => "b".to_string(),
                2 => ".".to_string(),
                3 => "\\b".to_string(),
                4 => format!("\\(?:{}\\)", bound_pattern(rng, depth - 1)),
                5 => format!("\\({}\\)", bound_pattern(rng, depth - 1)),
                _ => format!("[ab]{}", ["", "*"][rng.below(2)]),
            };
            out.push_str(&atom);
            out.push_str(["", "", "*", "+", "?", "*?", "+?", "??"][rng.below(8)]);
        }
    }
    out
}

fn bound_text(rng: &mut BoundRng, max_len: usize) -> Vec<u8> {
    (0..rng.below(max_len))
        .map(|_| b"aab c"[rng.below(5)])
        .collect()
}
