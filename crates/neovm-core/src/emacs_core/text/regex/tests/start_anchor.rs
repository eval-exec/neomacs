//! P3.3 Stage 0 (`NEOVM_REGEX_ANCHOR_ALT`): a pattern whose every
//! alternative begins with `^` searches line starts only.

use super::*;

fn anchor_of(pattern: &str) -> StartAnchor {
    let compiled =
        with_anchor_alt(true, || regex_compile(pattern, false, false)).expect("pattern compiles");
    compiled.start_anchor
}

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

#[test]
fn start_anchor_is_not_computed_with_the_knob_off() {
    let compiled = with_anchor_alt(false, || regex_compile("\\(?:^a\\|^b\\)", false, false))
        .expect("pattern compiles");
    assert_eq!(compiled.start_anchor, StartAnchor::None);
}

/// A forward search finds the same match with the analysis on as with it off
/// and as the exhaustive oracle, and enters the matcher only at line starts.
#[test]
fn alternation_anchored_search_tries_line_starts_only() {
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

/// Random `\(?:^A\|^B\)C` shapes over texts with newlines: the analysis-on
/// search equals the exhaustive oracle, forward and backward, at every start.
#[test]
fn alternation_anchor_fuzz_agrees_with_the_exhaustive_oracle() {
    let syntax = DefaultSyntaxLookup;
    let mut seed = 0x0a17_c0de_u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let atoms = [
        "a",
        "b",
        " ",
        "[ \t]*",
        "[-+]",
        "\\w+",
        ".",
        "\\(a\\|b\\)",
        "x?",
        "",
    ];
    let mut anchored = 0;
    for _ in 0..600 {
        let arms = 1 + next(3);
        let mut pattern = String::from("\\(?:");
        for arm in 0..arms {
            if arm > 0 {
                pattern.push_str("\\|");
            }
            pattern.push_str(if next(8) == 0 { "\\`" } else { "^" });
            for _ in 0..next(3) {
                pattern.push_str(atoms[next(atoms.len())]);
            }
        }
        pattern.push_str("\\)");
        for _ in 0..next(3) {
            pattern.push_str(atoms[next(atoms.len())]);
        }
        let Ok(compiled) = with_anchor_alt(true, || regex_compile(&pattern, false, false)) else {
            continue;
        };
        if compiled.start_anchor != StartAnchor::None {
            anchored += 1;
        }
        let text: Vec<u8> = (0..next(30)).map(|_| b"ab \t-+\n\nx"[next(9)]).collect();
        for start in 0..=text.len() {
            for range in [(text.len() - start) as isize, -(start as isize)] {
                let got = re_search(&compiled, &text, start, range, &syntax, start);
                let want = with_fastmap_disabled(|| {
                    re_search(&compiled, &text, start, range, &syntax, start)
                });
                let norm = |r: &Option<(usize, MatchRegisters)>| {
                    r.as_ref()
                        .map(|(at, regs)| (*at, regs.start.to_vec(), regs.end.to_vec()))
                };
                assert_eq!(
                    norm(&got),
                    norm(&want),
                    "{pattern:?} in {:?} from {start} range {range}",
                    String::from_utf8_lossy(&text)
                );
            }
        }
    }
    assert!(
        anchored > 300,
        "the generator must produce anchored patterns: {anchored}"
    );
}
