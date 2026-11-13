//! GNU parity of the backtracker's fail stack (P3.3 section 12, D5-D7): the
//! cycle check of a non-greedy loop over a nullable body, and the "Stack
//! overflow in regexp matcher" that the Pike fallback used to mask.
//!
//! Every expected value below is GNU 31.1's `(string-match RE S)` followed by
//! `(match-data t)` (probe: `tmp/probes/nasty.el` in the U0.5 worktree).

use super::*;

#[test]
fn nongreedy_loops_over_nullable_bodies_match_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cases: &[(&str, &str, Option<&[i64]>)] = &[
        // D5: GNU answers at once; the loop without GNU's marker frames
        // re-entered the empty iteration forever.
        ("\\(?:a?\\)*?b", "aaaac", None),
        ("\\(?:a?\\)*?b", "c", None),
        ("\\(?:a?\\)*?b", "", None),
        ("\\(?:a?\\)*?b", "aaab", Some(&[0, 4])),
        // D6: each empty iteration left a register save behind, so the
        // search ended in a spurious fail-stack overflow.
        ("\\(a\\|\\)+?x", "aaaay", None),
        ("\\(a\\|\\)+?x", "aaaax", Some(&[0, 5, 3, 4])),
        ("\\(a\\|\\)+?x", "x", Some(&[0, 1, 0, 0])),
        // regex-emacs.c: "We want (x?)*?y\1z to match both xxyz and xxyxz."
        ("\\(x?\\)*?y\\1z", "xxyz", Some(&[0, 4, 2, 2])),
        ("\\(x?\\)*?y\\1z", "xxyxz", Some(&[0, 5, 1, 2])),
        ("\\(?:a\\|\\)*?c", "aab", None),
        ("\\(\\(?:ab\\)?\\)*?\\(?:ab\\)c", "ababab", None),
        (
            "\\(\\(?:ab\\)?\\)*?\\(?:ab\\)c",
            "abababc",
            Some(&[0, 7, 2, 4]),
        ),
        ("\\(?:\\(a\\)?\\|b\\)+?\\'", "abba", Some(&[0, 4, 3, 4])),
        ("\\(?:\\(a\\)?\\|b\\)+?\\'", "abbac", Some(&[5, 5])),
        ("x\\(?:a*?\\)*?y", "xaaaz", None),
        ("x\\(?:a*?\\)*?y", "xaay", Some(&[0, 4])),
        ("\\(?:a?b?\\)*?c", "ababx", None),
    ];
    for &(pattern, text, expected) in cases {
        let compiled = regex_compile(pattern, false, false).expect("pattern compiles");
        assert!(
            compiled
                .buffer
                .contains(&(RegexOp::OnFailureJumpNastyloop as u8)),
            "{pattern:?} must exercise on_failure_jump_nastyloop"
        );
        let (found, overflow) = string_match(pattern, text);
        assert!(!overflow, "{pattern:?} on {text:?}: no fail-stack overflow");
        assert_eq!(
            found.as_deref(),
            expected,
            "{pattern:?} on {text:?}: GNU's match data"
        );
    }
}

/// A long text: GNU answers `nil` and `(0 1001)` (its time grows with the
/// square of the length, as each candidate walks the rest of the text).
#[test]
fn nongreedy_loop_over_nullable_body_walks_a_long_text_like_gnu() {
    let text = format!("{}c", "a".repeat(1_000));
    let (found, overflow) = string_match("\\(?:a?\\)*?b", &text);
    assert_eq!(found, None);
    assert!(!overflow);
    let (found, overflow) = string_match("\\(?:a?\\)*?c", &text);
    assert_eq!(found, Some(vec![0, 1_001]));
    assert!(!overflow);
}

/// D7: a candidate the budgeted backtracker hands to the Pike VM still ends
/// in GNU's "Stack overflow in regexp matcher" when GNU's backtracker would
/// run out of fail stack there, and in the Pike VM's answer otherwise.
#[test]
fn pike_fallback_keeps_gnu_fail_stack_overflow() {
    crate::test_utils::init_test_tracing();
    let pairs = |n: usize| "ab".repeat(n);

    // One candidate (only `x` starts a match): 200K characters overflow in GNU.
    let (found, overflow) = string_match("x\\(?:a\\|b\\)*c", &format!("x{}", pairs(100_000)));
    assert_eq!(found, None);
    assert!(overflow, "GNU signals the fail-stack overflow");

    // The 20K-character twin stays under GNU's limit: a plain failure.
    let (found, overflow) = string_match("x\\(?:a\\|b\\)*c", &format!("x{}", pairs(10_000)));
    assert_eq!(found, None);
    assert!(!overflow);

    // The design's D7 case: GNU stops at the first candidate.
    let (found, overflow) = string_match("\\(?:a\\|b\\)*c", &pairs(150_000));
    assert_eq!(found, None);
    assert!(overflow, "GNU signals the fail-stack overflow");

    // A long candidate that matches before the stack fills keeps the Pike
    // VM's (linear, exact) answer.
    let text = format!("x{}c", pairs(3_000));
    let (found, overflow) = string_match("x\\(?:a\\|b\\)*c", &text);
    assert_eq!(found, Some(vec![0, text.len() as i64]));
    assert!(!overflow);
}

/// GNU `(and (string-match PATTERN TEXT) (match-data t))` over ASCII text,
/// plus whether the search stopped on the fail-stack limit.
fn string_match(pattern: &str, text: &str) -> (Option<Vec<i64>>, bool) {
    let compiled = regex_compile(pattern, false, false).expect("pattern compiles");
    let syntax = DefaultSyntaxLookup;
    let _ = take_matcher_overflow();
    let found = re_search(
        &compiled,
        text.as_bytes(),
        0,
        text.len() as isize,
        &syntax,
        0,
    );
    let overflow = take_matcher_overflow();
    (found.map(|(_, registers)| match_data(&registers)), overflow)
}

/// Registers as GNU's `match-data` lists them: trailing unset groups dropped.
fn match_data(registers: &MatchRegisters) -> Vec<i64> {
    let mut data: Vec<i64> = registers
        .start
        .iter()
        .zip(&registers.end)
        .flat_map(|(&start, &end)| [start, end])
        .collect();
    while data.len() > 2 && data[data.len() - 1] < 0 && data[data.len() - 2] < 0 {
        data.truncate(data.len() - 2);
    }
    data
}
