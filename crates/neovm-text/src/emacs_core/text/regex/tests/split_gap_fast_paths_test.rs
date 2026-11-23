//! Split storage must preserve the matcher and candidate scanner contracts.
//!
//! The reference enumerates character-boundary starts and invokes the whole
//! input's forced backtracker directly. It deliberately does not use either
//! accelerated search scanner. GNU parity remains covered by the oracle rows.

use super::*;

type Captured = Option<(usize, Vec<i64>, Vec<i64>)>;

fn captured(found: Option<(usize, MatchRegisters)>) -> Captured {
    found.map(|(position, registers)| (position, registers.start.to_vec(), registers.end.to_vec()))
}

fn reference_search(
    pattern: &CompiledPattern,
    text: &str,
    start: usize,
    range: isize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Captured {
    with_backtracker_forced(|| {
        let stop = if range >= 0 {
            start.saturating_add(range as usize).min(text.len())
        } else {
            start
        };
        let lower = if range < 0 {
            start.saturating_sub(range.unsigned_abs())
        } else {
            start
        };
        let upper = if range < 0 { start } else { stop };
        let mut positions: Vec<_> = (lower..=upper)
            .filter(|&position| text.is_char_boundary(position))
            .collect();
        if range < 0 {
            positions.reverse();
        }
        positions.into_iter().find_map(|position| {
            re_match(pattern, text.as_bytes(), position, stop, syntax, point)
                .map(|(_, registers)| (position, registers.start.to_vec(), registers.end.to_vec()))
        })
    })
}

#[test]
fn split_gap_anchors_boundaries_and_prefixes_match_a_candidate_by_candidate_reference() {
    let syntax = DefaultSyntaxLookup;
    let fixtures = [
        "aa foobaz\nbbb foo\néfooé!",
        "prefix foo bar foobar foobaz\nlast",
        "\n\n* heading\nbody * emphasis *\n",
    ];
    let expressions = [
        r"^a+\|^b+",
        r"\`a*",
        r"a+$",
        r"last\'",
        r"\bfoo\b",
        r"\_<foo\_>",
        r"\(foo\(?:bar\|baz\)\)",
        r"foo.*?bar\|foobaz",
        r"\(é\)?foo\(é\)?",
        r"^\*+ ",
        r"\=\|foobar",
        r"\(foo\)\1\|foobaz",
    ];
    for text in fixtures {
        let boundaries: Vec<_> = (0..=text.len())
            .filter(|&position| text.is_char_boundary(position))
            .collect();
        let middle = boundaries[boundaries.len() / 2];
        let searches = [
            (0, text.len() as isize),
            (middle, (text.len() - middle) as isize),
            (0, middle as isize),
            (text.len(), -(text.len() as isize)),
            (middle, -(middle as isize)),
        ];
        for expression in expressions {
            let pattern = regex_compile(expression, false, false).expect("valid expression");
            // Exercise the literal seam scanner even on these short fixtures.
            build_search_optimizations(&pattern);
            for (start, range) in searches {
                let expected = reference_search(&pattern, text, start, range, &syntax, middle);
                for &split in &boundaries {
                    let actual = captured(re_search_segments(
                        &pattern,
                        &text.as_bytes()[..split],
                        &text.as_bytes()[split..],
                        start,
                        range,
                        &syntax,
                        middle,
                    ));
                    assert_eq!(
                        actual, expected,
                        "expression={expression:?} text={text:?} split={split} start={start} range={range}"
                    );
                }
            }
        }
    }
}

#[test]
fn split_gap_dfa_cold_and_warm_scans_preserve_hits_misses_and_registers() {
    let syntax = DefaultSyntaxLookup;
    let text = format!("{}aZ", "aabcdefx ".repeat(40));
    for cold in [false, true] {
        for mode in [dfa::DfaMode::Off, dfa::DfaMode::On, dfa::DfaMode::Verify] {
            dfa::with_cold_path(cold, || {
                dfa::with_dfa_mode(mode, || {
                    let pattern =
                        regex_compile(r"\(a[a-z]*\)Z", false, false).expect("valid expression");
                    for (start, range) in [
                        (0, text.len() as isize),
                        (0, (text.len() - 2) as isize),
                        (text.len(), -(text.len() as isize)),
                    ] {
                        let expected = reference_search(&pattern, &text, start, range, &syntax, 0);
                        // Repeat the same slot: first use and a reused DFA must
                        // answer identically, including after a bounded miss.
                        for repetition in 0..4 {
                            for split in [0, 1, text.len() / 2, text.len() - 1, text.len()] {
                                assert_eq!(
                                    captured(re_search_segments(
                                        &pattern,
                                        &text.as_bytes()[..split],
                                        &text.as_bytes()[split..],
                                        start,
                                        range,
                                        &syntax,
                                        0,
                                    )),
                                    expected,
                                    "cold={cold} mode={mode:?} rep={repetition} split={split} start={start} range={range}"
                                );
                            }
                        }
                    }
                });
            });
        }
    }
}

#[test]
fn split_gap_pike_matches_and_misses_preserve_captures_and_stop() {
    let syntax = DefaultSyntaxLookup;
    let pattern = regex_compile(r"\(a*\)a*b", false, false).expect("valid expression");
    assert!(pattern.pike_eligible);
    // A small forced-backtracker reference remains cheap. This checks the
    // catastrophic pattern's result contract independently of routing.
    for ending in ["b", "!"] {
        let text = format!("{}{ending}", "a".repeat(24));
        for stop in [text.len() - 1, text.len()] {
            let expected = with_backtracker_forced(|| {
                captured(re_match(&pattern, text.as_bytes(), 0, stop, &syntax, 0))
            });
            for split in 0..=text.len() {
                let actual = with_pike_forced(|| {
                    captured(re_match_segments(
                        &pattern,
                        &text.as_bytes()[..split],
                        &text.as_bytes()[split..],
                        0,
                        stop,
                        &syntax,
                        0,
                    ))
                });
                assert_eq!(
                    actual, expected,
                    "ending={ending} stop={stop} split={split}"
                );
            }
        }
    }
}

struct DashProperty;

impl SyntaxLookup for DashProperty {
    fn char_syntax(&self, character: char) -> SyntaxClass {
        DefaultSyntaxLookup.char_syntax(character)
    }

    fn char_syntax_at(&self, character: char, position: usize) -> SyntaxClass {
        if position == 1 {
            SyntaxClass::Word
        } else {
            self.char_syntax(character)
        }
    }

    fn char_has_category(&self, character: char, category: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(character, category)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::External {
            id: 0x5350_4c49_54,
            epoch: 0,
        }
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Standard)
    }

    fn position_dependent(&self) -> bool {
        true
    }

    fn plain_syntax_until(&self, position: usize) -> usize {
        match position {
            0 => 1,
            1 => 1,
            _ => usize::MAX,
        }
    }
}

#[test]
fn split_gap_syntax_properties_keep_absolute_input_positions() {
    let syntax = DashProperty;
    let text = "a-b c-d";
    let word = regex_compile(r"\w+", false, false).expect("valid expression");
    assert_eq!(
        reference_search(&word, text, 0, text.len() as isize, &syntax, 0),
        Some((0, vec![0], vec![3]))
    );
    for expression in [r"\w+", r"\b", r"\B", r"\_<", r"\_>", r"\sw+"] {
        let pattern = regex_compile(expression, false, false).expect("valid expression");
        for (start, range) in [
            (0, text.len() as isize),
            (1, (text.len() - 1) as isize),
            (text.len(), -(text.len() as isize)),
            (3, -3),
        ] {
            let expected = reference_search(&pattern, text, start, range, &syntax, 0);
            for mode in [dfa::DfaMode::Off, dfa::DfaMode::On] {
                for split in 0..=text.len() {
                    let actual = dfa::with_dfa_mode(mode, || {
                        captured(re_search_segments(
                            &pattern,
                            &text.as_bytes()[..split],
                            &text.as_bytes()[split..],
                            start,
                            range,
                            &syntax,
                            0,
                        ))
                    });
                    assert_eq!(
                        actual, expected,
                        "expression={expression:?} mode={mode:?} split={split} start={start}"
                    );
                }
            }
        }
    }
}

#[test]
fn split_gap_required_literal_miss_keeps_the_sparse_candidate_work_bound() {
    let syntax = DefaultSyntaxLookup;
    let text = "plain paragraph prose p body\n".repeat(200);
    let pattern = regex_compile(r"prefix[0-9]*Z", false, false).expect("valid expression");
    dfa::with_dfa_mode(dfa::DfaMode::Off, || {
        // Warm the derived literal prefilter on the same compiled expression.
        assert!(
            re_search(
                &pattern,
                text.as_bytes(),
                0,
                text.len() as isize,
                &syntax,
                0
            )
            .is_none()
        );
        let before = matcher_entry_count();
        assert!(
            re_search(
                &pattern,
                text.as_bytes(),
                0,
                text.len() as isize,
                &syntax,
                0
            )
            .is_none()
        );
        let contiguous_entries = matcher_entry_count() - before;
        assert!(
            contiguous_entries <= 2,
            "the literal-free control must skip body candidates"
        );
        for split in [1, text.len() / 2, text.len() - 1] {
            let before = matcher_entry_count();
            assert!(
                re_search_segments(
                    &pattern,
                    &text.as_bytes()[..split],
                    &text.as_bytes()[split..],
                    0,
                    text.len() as isize,
                    &syntax,
                    0,
                )
                .is_none()
            );
            let split_entries = matcher_entry_count() - before;
            assert!(
                split_entries <= contiguous_entries + 2,
                "split={split}: candidate entries {split_entries} exceed contiguous {contiguous_entries} plus two seam/EOF checks"
            );
        }
    });
}

#[test]
fn split_gap_maximum_literal_prefix_keeps_earliest_candidates_across_the_seam() {
    let syntax = DefaultSyntaxLookup;
    let needle = format!("A{}Z", "b".repeat(62));
    assert_eq!(needle.len(), 64);
    let near_miss = format!("A{}Y", "b".repeat(62));
    let expressions = [
        format!(r"\({needle}\)[0-9]*"),
        // A contained short alternative starts after the crossing long one.
        // Both alternative orders must choose the earlier logical start.
        format!(r"\({needle}\|bbb\)"),
        format!(r"\(bbb\|{needle}\)"),
    ];
    for expression in expressions {
        let pattern = regex_compile(&expression, false, false).expect("valid expression");
        build_search_optimizations(&pattern);
        let prefilter = pattern.literal_prefilter().unwrap_or_else(|| {
            panic!("required literal prefilter absent: expression={expression:?}")
        });
        assert_eq!(
            prefilter.pf.max_needle_len(),
            64,
            "exercise the maximum literal seam overlap"
        );
        for text in [
            format!("x{needle}12 {needle}3"),
            format!("x{near_miss}12 {needle}3"),
            format!("x{near_miss}12"),
        ] {
            let searches = [
                (0, text.len() as isize),
                // STOP cuts the first potential long literal one byte short.
                (0, 64),
                (text.len(), -(text.len() as isize)),
            ];
            for (start, range) in searches {
                let expected = reference_search(&pattern, &text, start, range, &syntax, 0);
                if text.as_bytes()[64] == b'Z' && start == 0 && range > 64 {
                    assert_eq!(
                        expected.as_ref().map(|found| found.0),
                        Some(1),
                        "the complete long literal begins before its interior short alternative"
                    );
                }
                // The first needle starts at byte1; these splits leave one
                // or63 bytes of that64-byte needle to its left.
                for offset in [1, 63] {
                    let split = 1 + offset;
                    let actual = captured(re_search_segments(
                        &pattern,
                        &text.as_bytes()[..split],
                        &text.as_bytes()[split..],
                        start,
                        range,
                        &syntax,
                        0,
                    ));
                    assert_eq!(
                        actual, expected,
                        "expression={expression:?} text={text:?} offset={offset} start={start} range={range}"
                    );
                }
            }
        }
    }
}

#[test]
fn split_gap_catastrophic_miss_reaches_the_production_pike_budget_fallback() {
    let syntax = DefaultSyntaxLookup;
    // The literal a after the loops overlaps their body, so the compiler
    // cannot turn the inner loop into an exclusive keep-string loop.
    let pattern = regex_compile("a*a*ab", false, false).expect("valid expression");
    assert!(pattern.pike_eligible);
    // The missing terminal byte independently establishes failure. The small
    // forced-backtracker row above checks capture parity; this larger row
    // specifically guards the real production budget-to-Pike handoff.
    let miss = format!("{}!", "a".repeat(256));
    let hit = format!("{}b", "a".repeat(256));
    with_regex_engine_override(RegexEngineOverride::Production, || {
        for split in [1, 128, 255] {
            let before = pike_fallback_count();
            assert!(
                re_match_segments(
                    &pattern,
                    &miss.as_bytes()[..split],
                    &miss.as_bytes()[split..],
                    0,
                    miss.len(),
                    &syntax,
                    0,
                )
                .is_none()
            );
            assert!(
                pike_fallback_count() > before,
                "split={split}: the catastrophic miss must exhaust the backtrack budget and use Pike"
            );
            assert_eq!(
                captured(re_match_segments(
                    &pattern,
                    &hit.as_bytes()[..split],
                    &hit.as_bytes()[split..],
                    0,
                    hit.len(),
                    &syntax,
                    0,
                )),
                Some((hit.len(), vec![0], vec![hit.len() as i64])),
                "split={split}: the same pattern must still consume the terminal b"
            );
        }
    });
}

#[test]
fn split_gap_one_byte_alternative_preserves_the_fallback_search_order() {
    let syntax = DefaultSyntaxLookup;
    let needle = format!("A{}Z", "b".repeat(62));
    let text = format!("x{needle}12");
    for expression in [format!(r"\({needle}\|b\)"), format!(r"\(b\|{needle}\)")] {
        let pattern = regex_compile(&expression, false, false).expect("valid expression");
        build_search_optimizations(&pattern);
        // regex-automata's Teddy is fast only for minimum needle length3;
        // the Aho-Corasick fallback is not adopted as a fast prefilter.
        assert!(pattern.literal_prefilter().is_none());
        for (start, range) in [
            (0, text.len() as isize),
            (text.len(), -(text.len() as isize)),
        ] {
            let expected = reference_search(&pattern, &text, start, range, &syntax, 0);
            for split in [2, 64] {
                assert_eq!(
                    captured(re_search_segments(
                        &pattern,
                        &text.as_bytes()[..split],
                        &text.as_bytes()[split..],
                        start,
                        range,
                        &syntax,
                        0,
                    )),
                    expected,
                    "expression={expression:?} split={split} start={start} range={range}"
                );
            }
        }
    }
}

#[test]
fn split_gap_required_prefix_longer_than_the_cap_has_a_bounded_seam() {
    let syntax = DefaultSyntaxLookup;
    for length in [65, 128, 255, 318] {
        let needle = format!("Q{}Z", "a".repeat(length - 2));
        let expression = format!(r"\({needle}\)[0-9]*");
        let pattern = regex_compile(&expression, false, false).expect("valid expression");
        build_search_optimizations(&pattern);
        let prefilter = pattern
            .literal_prefilter()
            .unwrap_or_else(|| panic!("long literal prefilter absent: length={length}"));
        // A maximum-length Exactn instruction must be clipped before adding
        // bytes to the required prefix, including when several Exactns join.
        assert_eq!(
            prefilter.pf.max_needle_len(),
            64,
            "length={length}: the stored prefix must honor the64-byte seam cap"
        );
        let text = format!("x{needle}12 {needle}3");
        for (start, range) in [
            (0, text.len() as isize),
            (0, length as isize),
            (text.len(), -(text.len() as isize)),
        ] {
            let expected = reference_search(&pattern, &text, start, range, &syntax, 0);
            for split in [2, 64, 65, length] {
                assert_eq!(
                    captured(re_search_segments(
                        &pattern,
                        &text.as_bytes()[..split],
                        &text.as_bytes()[split..],
                        start,
                        range,
                        &syntax,
                        0,
                    )),
                    expected,
                    "length={length} split={split} start={start} range={range}"
                );
            }
        }
    }
}

#[test]
fn split_gap_literal_prefilter_respects_pattern_and_target_byte_encodings() {
    use neovm_value::heap_types::LispString;

    let syntax = DefaultSyntaxLookup;
    let cases = [
        (
            LispString::from_unibyte(b"AA\xff[0-9]*Z".to_vec()),
            true,
            b"AA\xc1\xbf12Z".as_slice(),
        ),
        (
            LispString::from_emacs_bytes(b"AA\xc1\xbf[0-9]*Z".to_vec()),
            false,
            b"AA\xff12Z".as_slice(),
        ),
    ];
    for (source, target_multibyte, suffix) in cases {
        let mut pattern = regex_compile_lisp(&source, false, false).expect("valid expression");
        pattern.target_multibyte = target_multibyte;
        assert_ne!(pattern.multibyte, pattern.target_multibyte);
        build_search_optimizations(&pattern);
        let mut text = vec![b'x'; 300];
        text.extend_from_slice(suffix);
        let expected = Some((300, vec![300], vec![text.len() as i64]));
        let reference = with_backtracker_forced(|| {
            re_match(&pattern, &text, 300, text.len(), &syntax, 0)
                .map(|(_, registers)| (300, registers.start.to_vec(), registers.end.to_vec()))
        });
        assert_eq!(
            reference, expected,
            "the raw-byte character must match across the pattern/target representation change"
        );
        for (start, range) in [
            (0, text.len() as isize),
            (text.len(), -(text.len() as isize)),
        ] {
            let split = 150;
            assert_eq!(
                captured(re_search_segments(
                    &pattern,
                    &text[..split],
                    &text[split..],
                    start,
                    range,
                    &syntax,
                    0,
                )),
                expected,
                "pattern_multibyte={} target_multibyte={target_multibyte} start={start} range={range}: accelerated split search must honor character equality",
                pattern.multibyte,
            );
            assert_eq!(
                captured(re_search(&pattern, &text, start, range, &syntax, 0)),
                expected,
                "pattern_multibyte={} target_multibyte={target_multibyte} start={start} range={range}: accelerated whole search must honor character equality",
                pattern.multibyte,
            );
        }
    }
}

/// Warming must not make every candidate re-select both halves. The probe is
/// charged by actual Segments byte/get/chunk operations inside the prefix.
#[test]
fn split_gap_warm_prefix_storage_selection_is_bounded_per_search_half() {
    let syntax = DefaultSyntaxLookup;
    dfa::with_first_step(true, || {
        dfa::with_cold_path(false, || {
            dfa::with_dfa_mode(dfa::DfaMode::On, || {
                for repeats in [256usize, 1024] {
                    let text = "abcdefx ".repeat(repeats);
                    let bytes = text.as_bytes();
                    let split = bytes.len() / 2;
                    let pattern = regex_compile("a[bcdef]*Z", false, false).unwrap();
                    assert!(
                        pattern.literal_prefilter().is_none(),
                        "one-byte prefix must not bypass the DFA"
                    );
                    for range in [bytes.len() as isize, -(bytes.len() as isize)] {
                        let start = if range >= 0 { 0 } else { bytes.len() };
                        // Warm the real slot in the same direction, then compare the
                        // contiguous and segmented invocation's work and outcomes.
                        for _ in 0..2 {
                            assert!(
                                re_search(&pattern, bytes, start, range, &syntax, start).is_none()
                            );
                        }
                        let before = prefix_storage_probe();
                        assert!(re_search(&pattern, bytes, start, range, &syntax, start).is_none());
                        let contiguous = prefix_storage_probe().since(before);
                        let before = prefix_storage_probe();
                        assert!(
                            re_search_segments(
                                &pattern,
                                &bytes[..split],
                                &bytes[split..],
                                start,
                                range,
                                &syntax,
                                start
                            )
                            .is_none()
                        );
                        let segmented = prefix_storage_probe().since(before);
                        assert!(
                            segmented.calls >= repeats as u64,
                            "fixture must enter cached prefixes: {segmented:?}"
                        );
                        assert!(
                            segmented.rejected >= repeats as u64,
                            "fixture must use warmed rejections: {segmented:?}"
                        );
                        assert_eq!(
                            (
                                segmented.calls,
                                segmented.rejected,
                                segmented.undecided,
                                segmented.rejected_bytes,
                                segmented.scalar_reads
                            ),
                            (
                                contiguous.calls,
                                contiguous.rejected,
                                contiguous.undecided,
                                contiguous.rejected_bytes,
                                contiguous.scalar_reads
                            ),
                            "the storage route must preserve actual prefix work and outcomes"
                        );
                        eprintln!(
                            "cached prefix storage: repeats={repeats} range={range} {segmented:?}"
                        );
                        assert!(
                            segmented.segment_bytes
                                + segmented.segment_gets
                                + segmented.segment_chunks
                                <= 64,
                            "prefix geometry must be bounded by the seam, not candidates: repeats={repeats} range={range} {segmented:?}"
                        );
                        assert!(
                            segmented.batch_checks >= repeats as u64,
                            "candidate batch checks must remain visible: {segmented:?}"
                        );
                        assert_eq!(
                            segmented.batch_prepares, 1,
                            "one real storage preparation per warmed search"
                        );
                        assert!(
                            segmented.batch_advances <= 2,
                            "monotonic candidates can cross at most two batch boundaries: {segmented:?}"
                        );
                        assert!(
                            segmented.selections <= 2,
                            "borrow prefix windows once per physical half: repeats={repeats} range={range} {segmented:?}"
                        );
                    }
                }
            })
        })
    });
}

#[test]
fn prepared_prefix_batches_keep_the_seam_bounded_in_both_directions() {
    let bytes = b"0123456789abcdefABCDEFGHIJKLMNOP";
    for split in 0..=bytes.len() {
        let text = Segments {
            left: &bytes[..split],
            right: &bytes[split..],
        };
        for forward in [true, false] {
            let before = prefix_storage_probe();
            let mut batches = PrefixBatches::new(forward);
            assert_eq!(
                prefix_storage_probe().since(before).selections,
                0,
                "constructing a scan must not query storage eagerly"
            );
            let positions: Vec<_> = if forward {
                (0..=bytes.len()).collect()
            } else {
                (0..=bytes.len()).rev().collect()
            };
            for position in positions {
                let input = batches.input(&text, position);
                let interior = position < bytes.len()
                    && (split == 0
                        || split == bytes.len()
                        || position < split.saturating_sub(dfa::PREFIX_READ_CAP - 1)
                        || position > split);
                assert_eq!(
                    input.is_some(),
                    interior,
                    "split={split} forward={forward} position={position}"
                );
            }
            let counts = prefix_storage_probe().since(before);
            assert_eq!(counts.batch_prepares, 1);
            assert_eq!(
                counts.selections, 1,
                "the counted query must include real preparation outside the prefix scope"
            );
            assert!(counts.batch_advances <= 2, "{counts:?}");
            assert_eq!(
                counts.calls, 0,
                "view preparation is not a cached prefix call"
            );
        }
    }
}

#[test]
fn prepared_prefix_slice_keeps_a_global_dynamic_point_and_bounded_stop() {
    let syntax = DefaultSyntaxLookup;
    let text = b"abcdefgh x";
    let pattern = regex_compile(r"\=x\|Q", false, false).unwrap();
    dfa::with_first_step(true, || {
        dfa::with_cold_path(false, || {
            dfa::with_dfa_mode(dfa::DfaMode::On, || {
                for _ in 0..2 {
                    assert!(
                        re_search_segments(&pattern, &text[..8], &text[8..], 0, 10, &syntax, 0)
                            .is_none()
                    );
                }
                assert!(
                    re_search_segments(&pattern, &text[..8], &text[8..], 0, 9, &syntax, 9)
                        .is_none()
                );
                let actual = captured(re_search_segments(
                    &pattern,
                    &text[..8],
                    &text[8..],
                    0,
                    10,
                    &syntax,
                    9,
                ));
                assert_eq!(
                    actual,
                    reference_search(&pattern, "abcdefgh x", 0, 10, &syntax, 9)
                );
                assert_eq!(actual.as_ref().map(|found| found.0), Some(9));
            })
        })
    });
}
