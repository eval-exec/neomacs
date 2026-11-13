//! P3.3 Stage 0 (`NEOVM_REGEX_ANCHOR_ALT`): a pattern whose every
//! alternative begins with `^` searches line starts only.

use neovm_text::emacs_core::syntax::SyntaxClass;
use neovm_value::tagged::SymbolMode;
use neovm_value::{emacs_char, value::Value};
use regex_automata::{MatchKind, Span};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;

#[test]
fn start_anchor_sees_through_alternations_groups_and_jumps() {
    for (pattern, expected) in [
        ("^a", StartAnchor::Line),
        ("\\(?:^a\\|^b\\)c", StartAnchor::Line),
        ("\\(^a\\|^b\\)", StartAnchor::Line),
        (
            "\\(?:^[ \t]*[-+]\\|^[ \t]+[*]\\)[ \t]+\\(.*?[ \t]+::\\)",
            StartAnchor::Line,
        ),
        ("\\(?:\\(?:^a\\)\\|\\(^b\\|^c\\)\\)", StartAnchor::Line),
        ("\\(?:\\`a\\|^b\\)", StartAnchor::Line),
        ("\\`a\\|\\`b", StartAnchor::Buffer),
        ("\\(?:\\`a\\|\\(\\`b\\)\\)", StartAnchor::Buffer),
        // Some path consumes, succeeds or tests something else first.
        ("\\(?:^a\\|b\\)", StartAnchor::None),
        ("\\(?:^a\\)?b", StartAnchor::None),
        ("\\(?:^a\\)*b", StartAnchor::None),
        ("\\(?:^a\\)*?b", StartAnchor::None),
        ("\\(?:^\\|x\\)", StartAnchor::None),
        ("\\(?:^a\\|\\)", StartAnchor::None),
        ("\\(?:^a\\|$\\)", StartAnchor::None),
        ("\\(?:^a\\|\\=b\\)", StartAnchor::None),
        ("\\(?:^a\\|\\bb\\)", StartAnchor::None),
        ("a^", StartAnchor::None),
        ("", StartAnchor::None),
    ] {
        assert_eq!(anchor_of(pattern), expected, "{pattern:?}");
    }
}

/// A forward search finds the same match with the analysis on as with it off
/// and as the exhaustive oracle, and enters the matcher only at line starts.
/// (The existence DFA's filter stays off: entries count the scan.)
#[test]
fn alternation_anchored_search_tries_line_starts_only() {
    dfa::with_dfa_mode(dfa::DfaMode::Off, alternation_anchored_search_entries);
}

fn anchor_of(pattern: &str) -> StartAnchor {
    let compiled =
        with_anchor_alt(true, || regex_compile(pattern, false, false)).expect("pattern compiles");
    compiled.start_anchor
}

fn alternation_anchored_search_entries() {
    let syntax = DefaultSyntaxLookup;
    let text = b"  - item\n  plain line\n * star\n- first\n+ plus\n  + deep\n";
    for pattern in [
        "\\(?:^[ \t]*[-+]\\|^[ \t]+[*]\\)[ \t]+\\(.*\\)",
        "\\(?:^ +\\*\\|^-\\) \\(\\w+\\)",
        "\\(^zzz\\|^yyy\\)",
    ] {
        let on = with_anchor_alt(true, || regex_compile(pattern, false, false)).unwrap();
        let off = with_anchor_alt(false, || regex_compile(pattern, false, false)).unwrap();
        assert_eq!(on.start_anchor, StartAnchor::Line, "{pattern:?}");
        for start in 0..=text.len() {
            let range = (text.len() - start) as isize;
            let before = matcher_entry_count();
            let with = re_search(&on, text, start, range, &syntax, 0);
            let entries_with = matcher_entry_count() - before;
            let before = matcher_entry_count();
            let without = re_search(&off, text, start, range, &syntax, 0);
            let entries_without = matcher_entry_count() - before;
            let oracle = with_fastmap_disabled(|| re_search(&on, text, start, range, &syntax, 0));
            let norm = |r: &Option<(usize, MatchRegisters)>| {
                r.as_ref()
                    .map(|(at, regs)| (*at, regs.start.to_vec(), regs.end.to_vec()))
            };
            assert_eq!(norm(&with), norm(&without), "{pattern:?} from {start}");
            assert_eq!(norm(&with), norm(&oracle), "{pattern:?} from {start}");
            assert!(entries_with <= entries_without);
        }
        // From 0, only position 0 and the byte after each newline qualify.
        let before = matcher_entry_count();
        let _ = re_search(&on, text, 0, text.len() as isize, &syntax, 0);
        let line_starts = 1 + text.iter().filter(|&&b| b == b'\n').count();
        assert!(
            matcher_entry_count() - before <= line_starts as u64,
            "{pattern:?}: at most one matcher entry per line start"
        );
    }
}
