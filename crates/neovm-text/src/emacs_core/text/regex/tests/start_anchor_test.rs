//! P3.3 Stage 0 (`NEOVM_REGEX_ANCHOR_ALT`): a pattern whose every
//! alternative begins with `^` searches line starts only.

use super::*;

#[test]
fn start_anchor_is_not_computed_with_the_knob_off() {
    let compiled = with_anchor_alt(false, || regex_compile("\\(?:^a\\|^b\\)", false, false))
        .expect("pattern compiles");
    assert_eq!(compiled.start_anchor, StartAnchor::None);
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
