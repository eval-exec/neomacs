//! Regression replay of the first semantic divergence the weekly regex
//! differential fuzzer reached (workflow `regex fuzz`, run 41276399804 on
//! main 589873beb1, target `regex_pike_vm`): a backward search with case
//! folding over a multibyte target where the backtracker oracle matched and
//! the Pike VM returned nothing.
//!
//! Both engines were in fact right: the two searches diverged because the
//! existence DFA's candidate filter had cached a transition under
//! previous-character facts that the matcher's backward character scan
//! (`GET_CHAR_BEFORE_2`) does not honor over a run of continuation bytes —
//! see `Facts::NON_HEAD_PREV` in `dfa.rs` and the verdict-history
//! regression in `tests/dfa_test.rs`.
//!
//! The case is replayed exactly as the fuzz harness maps it: the wire
//! `start`/`point` go through `RegexCase`'s `% (len + 1)` mapping, the
//! pattern compiles under `with_anchor_alt`, the target is multibyte, and
//! the backward search is `re_search` from `start` with range `-start`.

use super::*;
use crate::emacs_core::regex_emacs::{
    DefaultSyntaxLookup, RegexEngineOverride, re_search, regex_compile, with_anchor_alt,
    with_regex_engine_override,
};
use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

/// Pattern bytes as the fuzzer generated them: `.` `.` `?` `*` `\` `B` NUL
/// `.` NUL — a greedy `.` followed by `(.?)*`, then `\B`, a literal NUL,
/// any character, and another literal NUL.
const PATTERN: &str = "..?*\\B\0.\0";

/// Text bytes verbatim from the crash report.
const TEXT: &[u8] = &[
    142, 17, 0, 46, 117, 0, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133,
    133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 133, 0, 255, 0, 39, 39, 39,
    39, 39, 39, 39, 39, 39, 122, 241, 0, 149, 249, 0, 0, 42, 39, 39, 0, 13, 33, 192, 1, 255, 255,
    254, 255, 255, 44, 137, 0, 254, 255, 255, 255, 46, 46, 63, 42, 92, 66, 0, 46, 0, 142, 17, 0,
    46, 117, 0, 0, 255, 39, 0, 125, 39, 125, 39, 121, 39, 241, 0, 149, 249, 0, 0, 42, 0, 254, 255,
    255, 255, 0, 0, 0, 47, 39, 39, 39, 39, 39, 39, 39, 0, 13, 33, 245, 245, 245, 245, 245, 245,
    245, 255, 255, 255, 127, 245, 245, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180,
    180, 245, 245, 245, 245, 245, 245, 245, 255, 255, 255, 103, 57, 50, 227, 42, 255, 255, 255,
    255, 0, 251, 0, 0, 10, 255, 196, 122, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 254, 255, 255, 255, 255, 196, 255, 255, 255, 247, 255,
    49, 0, 0, 0,
];

const START: usize = 65534;
const POINT: usize = 65535;

/// Compile the case exactly as the differential harness does.
fn compile_case() -> CompiledPattern {
    let compiled = with_anchor_alt(true, || regex_compile(PATTERN, false, true))
        .expect("fuzzer case compiles");
    let mut compiled = compiled;
    compiled.target_multibyte = true;
    compiled
}

#[test]
fn pike_vm_agrees_with_backtracker_on_backward_fold_case() {
    crate::test_utils::init_test_tracing();

    let compiled = compile_case();
    assert!(
        compiled.pike_eligible,
        "the fuzzer only reaches the comparison on an eligible pattern"
    );

    // The full differential (all three operations) must hold.
    let case =
        RegexCase::new(PATTERN, TEXT, true, START, POINT).with_target(SearchTarget::Multibyte);
    let check = check_regex_differential(case, RegexDifferential::PikeVm)
        .unwrap_or_else(|divergence| panic!("{divergence}"));
    assert_eq!(check, RegexCheck::Equivalent { comparisons: 3 });

    // And the backward search both engines agree on is the oracle's match:
    // [5, 37) from the crash report.
    let syntax = DefaultSyntaxLookup;
    let start = START % (TEXT.len() + 1);
    let point = POINT % (TEXT.len() + 1);
    for engine in [
        RegexEngineOverride::Backtracker,
        RegexEngineOverride::PikeVm,
    ] {
        let (match_start, registers) = with_regex_engine_override(engine, || {
            re_search(&compiled, TEXT, start, -(start as isize), &syntax, point)
        })
        .unwrap_or_else(|| panic!("{engine:?} must match the fuzzer case"));
        assert_eq!(match_start, 5, "{engine:?} match start");
        assert_eq!(registers.start[0], 5, "{engine:?} group 0 start");
        assert_eq!(registers.end[0], 37, "{engine:?} group 0 end");
    }
}
