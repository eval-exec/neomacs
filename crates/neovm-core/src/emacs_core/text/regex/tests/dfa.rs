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
