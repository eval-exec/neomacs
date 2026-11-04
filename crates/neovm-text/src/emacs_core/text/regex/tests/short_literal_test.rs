//! Bounded whole-literal searches must avoid per-candidate matcher setup.

use super::*;

type CapturedMatch = Option<(usize, Vec<i64>, Vec<i64>)>;

fn search(
    optimized: bool,
    expression: &str,
    text: &[u8],
    start: usize,
    range: isize,
) -> (CapturedMatch, u64) {
    short_literal::with_enabled(optimized, || {
        let pattern = regex_compile(expression, false, true).expect("valid pattern");
        let before = matcher_entry_count();
        let matched = dfa::with_dfa_mode(dfa::DfaMode::Off, || {
            re_search(&pattern, text, start, range, &DefaultSyntaxLookup, start)
        });
        (
            matched.map(|(position, regs)| (position, regs.start.to_vec(), regs.end.to_vec())),
            matcher_entry_count() - before,
        )
    })
}

#[test]
fn short_folded_literal_avoids_general_matcher_entries() {
    crate::test_utils::init_test_tracing();
    let text = "Ж".repeat(90);
    let legacy = search(false, "жq", text.as_bytes(), 0, text.len() as isize);
    let optimized = search(true, "жq", text.as_bytes(), 0, text.len() as isize);
    assert_eq!(optimized.0, legacy.0);
    assert!(legacy.1 > 0, "the control must enter the matcher");
    assert_eq!(optimized.1, 0, "a whole literal needs no general matcher");
}

#[test]
fn short_folded_literal_preserves_registers_and_bounds() {
    crate::test_utils::init_test_tracing();
    for text in [
        "",
        "ЖqжQ",
        "жЖжq!",
        "xΣqςQσq",
        "µqΜqμQ",
        "Жq\nжQ",
        "éЖq😀Жq",
    ] {
        // Include byte bounds inside encoded characters as well as Lisp-valid
        // character boundaries: the engine must reject matches past STOP.
        let positions: Vec<_> = (0..=text.len()).collect();
        for expression in [
            "жq",
            "σq",
            "μq",
            "ж",
            "éЖq",
            "Жq\\!",
            "Жq\\|σq",
            "\\(Жq\\)",
            "Жq+",
        ] {
            for &start in &positions {
                for &stop in &positions {
                    let range = stop as isize - start as isize;
                    assert_eq!(
                        search(true, expression, text.as_bytes(), start, range).0,
                        search(false, expression, text.as_bytes(), start, range).0,
                        "expression={expression:?} text={text:?} start={start} stop={stop}"
                    );
                }
            }
        }
    }
}

#[test]
fn short_folded_literal_leaves_nonliteral_and_long_searches_on_general_path() {
    crate::test_utils::init_test_tracing();
    for (expression, text) in [
        ("\\(Жq\\)", "Жq".to_string()),
        ("Жq\\|Σq", "Жq".to_string()),
        ("Жq+", "Жq".to_string()),
        ("Жq", format!("{}Жq", "ж".repeat(1000))),
    ] {
        let optimized = search(true, expression, text.as_bytes(), 0, text.len() as isize);
        let legacy = search(false, expression, text.as_bytes(), 0, text.len() as isize);
        assert_eq!(optimized, legacy, "fallback expression={expression:?}");
    }
}
