use super::*;

#[test]
fn anchor_override_restores_nested_and_unwound_state() {
    let original = ANCHOR_ALT_OVERRIDE.with(std::cell::Cell::get);
    with_anchor_alt(false, || {
        assert!(!anchor_alt_enabled());
        with_anchor_alt(true, || assert!(anchor_alt_enabled()));
        assert!(!anchor_alt_enabled());
        let unwind = std::panic::catch_unwind(|| {
            with_anchor_alt(true, || {
                assert!(anchor_alt_enabled());
                panic!("exercise anchor override unwinding");
            });
        });
        assert!(unwind.is_err());
        assert!(!anchor_alt_enabled());
    });
    assert_eq!(ANCHOR_ALT_OVERRIDE.with(std::cell::Cell::get), original);
}

#[test]
fn differential_overrides_restore_nested_and_unwound_state() {
    assert!(!force_backtrack());
    assert!(!force_pike());
    assert!(!fastmap_force_disabled());

    with_regex_engine_override(RegexEngineOverride::Backtracker, || {
        assert!(force_backtrack());
        assert!(!force_pike());

        let panic = std::panic::catch_unwind(|| {
            with_regex_engine_override(RegexEngineOverride::PikeVm, || {
                assert!(!force_backtrack());
                assert!(force_pike());
                panic!("exercise override unwinding");
            });
        });
        assert!(panic.is_err());
        assert!(force_backtrack());
        assert!(!force_pike());
    });

    let panic = std::panic::catch_unwind(|| {
        with_fastmap_disabled(|| {
            assert!(fastmap_force_disabled());
            panic!("exercise optimization-override unwinding");
        });
    });
    assert!(panic.is_err());
    assert!(!force_backtrack());
    assert!(!force_pike());
    assert!(!fastmap_force_disabled());
}

/// `a*a*b` — the classic catastrophic-backtracking pattern — must be handled
/// by the Pike VM in LINEAR time (was cubic in the backtracker).  This test
/// would blow up / time out if it ever fell back to the backtracker.
#[test]
fn pike_no_catastrophic_backtracking() {
    let cp = regex_compile("a*a*b", false, false).expect("compile");
    assert!(cp.pike_eligible, "a*a*b must be pike-eligible");

    // Linear check: a growing run of 'a' with no trailing 'b' fails FAST.
    // The backtracker is O(n^3) here; the Pike VM is O(n*m).  A generous
    // wall-clock ceiling that only the linear engine can meet.
    let syn = DefaultSyntaxLookup;
    let n = 20_000usize;
    let mut text = vec![b'a'; n];
    text.push(b'!');
    let t0 = std::time::Instant::now();
    assert!(re_match(&cp, &text, 0, text.len(), &syn, 0).is_none());
    let elapsed = t0.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "a*a*b over {n} a's took {elapsed:?} — not linear (Pike path not taken?)"
    );

    // And it still finds the real match when a 'b' is present.
    let mut text2 = vec![b'a'; 500];
    text2.push(b'b');
    let r = re_match(&cp, &text2, 0, text2.len(), &syn, 0);
    assert_eq!(r.map(|(e, _)| e), Some(501));
}

/// The dual: a capture INSIDE a nullable loop is Pike-INELIGIBLE (GNU's
/// empty-loop capture semantics), so it must fall back to the backtracker.
#[test]
fn pike_capture_in_nullable_loop_is_ineligible() {
    for p in [
        "\\(a*\\)*",
        "\\(a\\|\\)*",
        "\\(?:\\(x*\\)\\)*",
        "\\(.??\\)+",
    ] {
        let cp = regex_compile(p, false, false).expect("compile");
        assert!(
            !cp.pike_eligible,
            "{p:?} has a capture in a nullable loop; must be Pike-ineligible"
        );
    }
}

/// The rewind view exists for every pattern, not only the Pike-eligible ones:
/// the existence DFA reads it for POSIX patterns and `??`, and it costs
/// nothing to have it for the rest (a backreference here).
#[test]
fn rewind_view_is_built_for_every_pattern() {
    for (pattern, posix, keep_string) in [
        // `[a-z]*` before `:` is exclusive: a keep-string loop to rewrite.
        ("[a-z]*:", false, true),
        ("[a-z]*:", true, true),
        ("\\(x\\)\\1[a-z]*:", false, true),
        ("x??[a-z]*:", false, true),
        ("a*a", false, false),
    ] {
        let compiled = regex_compile(pattern, posix, false).expect("compile");
        let has_keep_string = compiled
            .buffer
            .contains(&(RegexOp::OnFailureKeepStringJump as u8));
        assert_eq!(has_keep_string, keep_string, "{pattern:?}");
        let view = compiled.rewind_bytecode().expect("a rewind view");
        assert_eq!(view.len(), compiled.buffer.len(), "{pattern:?}");
        assert!(
            !view.contains(&(RegexOp::OnFailureKeepStringJump as u8)),
            "{pattern:?}: the view has no keep-string jump left"
        );
        assert_eq!(
            matches!(compiled.rewind, RewindView::Rewritten(_)),
            keep_string,
            "{pattern:?}"
        );
    }
}

#[test]
fn prefilter_none_for_syntax_class_head() {
    // sexp-head-kw: after `(`, the group starts with `\w`/`\s_` — no required
    // literal beyond the common `(`, so the single-byte set is rejected (the
    // fastmap's memchr already covers it).
    let cp =
        regex_compile("(\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)\\_>", false, false).expect("compile");
    assert!(
        cp.literal_prefilter().is_none(),
        "single-byte `(` prefix must not build a prefilter (fastmap suffices)"
    );
}

#[test]
fn prefilter_none_for_leading_nonliteral() {
    for pat in ["\\w+foo", ".*bar", "[a-z]+baz", "\\(?:\\w\\|x\\)y"] {
        let cp = regex_compile(pat, false, false).expect("compile");
        assert!(
            cp.literal_prefilter().is_none(),
            "leading non-literal must have no prefilter: {pat:?}"
        );
    }
}

/// Result counters from one fuzz stream.
#[derive(Default, Clone, Copy)]
struct PfFuzzCounts {
    compiled: usize,
    with_prefilter: usize,
    /// Differential checks (a forward and a backward search each) on the
    /// exhaustively compared cases: those with `prefilter.is_some()`, and every
    /// case-folded case.  This is the number that matters for the soundness
    /// gate — every one asserts optimized scan == skip-off.
    pf_forward_cmp: usize,
}

/// The two-segment search must find exactly what the single-slice search
/// finds over the same logical text. The accelerated candidate scan runs
/// memchr (or a folded accept table) over each gap half; a seam that hid a
/// candidate, or read a first byte from the wrong half, would diverge here.
#[test]
fn gap_segments_search_matches_single_slice_over_random_texts() {
    use crate::emacs_core::regex_emacs::DefaultSyntaxLookup;

    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    let pieces = [
        "a", "b", "c", "d", "e", "f", "l", "n", "u", "x", "z", " ", "\n", "0", "(", ")", "é",
        "defun", "aa", "bb",
    ];
    let patterns = [
        ("d[e]fun", false),
        ("defun", false),
        ("a\\{2\\}", false),
        ("l+", false),
        ("z[z]z", false),
        ("[a-c]+x", false),
        ("\\bdefun\\b", false),
        ("^a", false),
        ("a$", false),
        (".", false),
        ("a*", false),
        ("\\(?:ab\\|cd\\)x", false),
        ("d[e]fun", true),
        ("[A-Z]+", true),
        ("\\`a", false),
        ("a\\'", false),
        ("ab", false),
        ("a.b", false),
        ("[a-c]b", false),
    ];
    let syntax = DefaultSyntaxLookup;

    for _case in 0..480 {
        let mut text = String::new();
        let target = next() % 400;
        while text.len() < target {
            text.push_str(pieces[next() % pieces.len()]);
        }
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut gap = if len == 0 { 0 } else { next() % (len + 1) };
        while !text.is_char_boundary(gap) {
            gap -= 1;
        }
        let start = next() % (len + 1);
        let forward = next() % 2 == 0;
        let range = if forward {
            (next() % (len + 1 - start)) as isize
        } else {
            -((next() % (start + 1)) as isize)
        };

        for (pattern, case_fold) in patterns {
            let compiled = regex_compile(pattern, false, case_fold).expect("compile");
            let whole = re_search(&compiled, bytes, start, range, &syntax, start);
            let halves = re_search_segments(
                &compiled,
                &bytes[..gap],
                &bytes[gap..],
                start,
                range,
                &syntax,
                start,
            );
            assert_eq!(
                whole
                    .as_ref()
                    .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
                halves
                    .as_ref()
                    .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
                "pattern {pattern:?} case_fold={case_fold} text {text:?} gap={gap} start={start} range={range}"
            );
        }
    }
}

/// The window dispatch must agree with the single-slice searcher at its three
/// kinds of boundary: a window wholly left or right of the gap, a required
/// literal that crosses the seam, and end-sensitive assertions on the byte
/// just beside it.
#[test]
fn gap_window_dispatch_handles_one_sided_and_seam_windows() {
    use crate::emacs_core::regex_emacs::DefaultSyntaxLookup;

    let syntax = DefaultSyntaxLookup;
    let forward = [
        ("aaaabbbbcccc", 8, 0, 6, "ab"),     // whole left half
        ("aaaabbbbcccc", 8, 9, 12, "cc"),    // whole right half
        ("aaaabbbbcccc", 8, 0, 12, "bbbb"),  // literal crossing the seam
        ("aaaabbbbcccc", 8, 0, 12, "[bc]+"), // seam plus both sides
        ("aaa\nbbb", 3, 0, 7, "a$"),         // end-of-line just left of the gap
        ("aaa\nbbb", 4, 0, 7, "^b"),         // line start just right of the gap
        ("aaa\nbbb", 4, 0, 7, "\\`a"),       // buffer-start anchor
        ("xyz", 1, 0, 3, "xy"),              // one-byte seam
        ("xyz", 1, 1, 3, "yz"),              // candidate at the gap itself
    ];
    let backward = [
        ("aaaabbbbcccc", 8, 12, 0, "bb"),    // right region then seam
        ("aaaabbbbcccc", 6, 12, 0, "bbbb"),  // literal crossing the seam
        ("aaaabbbbcccc", 8, 12, 0, "a\\|c"), // empty-ish alternatives
        ("aaa\nbbb", 4, 7, 0, "^b"),         // line start just right of the gap
        ("aaa\nbbb", 3, 7, 0, "a$"),         // end-of-line just left of the gap
        ("xyz", 1, 3, 0, "xy"),              // one-byte seam
    ];

    for (text, gap, start, end, pattern) in forward {
        let compiled = regex_compile(pattern, false, false).expect("compile");
        let range = (end - start) as isize;
        let whole = re_search(&compiled, text.as_bytes(), start, range, &syntax, start);
        let halves = re_search_segments(
            &compiled,
            &text.as_bytes()[..gap],
            &text.as_bytes()[gap..],
            start,
            range,
            &syntax,
            start,
        );
        assert_eq!(
            whole
                .as_ref()
                .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
            halves
                .as_ref()
                .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
            "forward text {text:?} gap={gap} start={start} end={end} pattern {pattern:?}"
        );
    }
    for (text, gap, start, end, pattern) in backward {
        let compiled = regex_compile(pattern, false, false).expect("compile");
        let range = -((start - end) as isize);
        let whole = re_search(&compiled, text.as_bytes(), start, range, &syntax, start);
        let halves = re_search_segments(
            &compiled,
            &text.as_bytes()[..gap],
            &text.as_bytes()[gap..],
            start,
            range,
            &syntax,
            start,
        );
        assert_eq!(
            whole
                .as_ref()
                .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
            halves
                .as_ref()
                .map(|(pos, regs)| (*pos, &regs.start, &regs.end)),
            "backward text {text:?} gap={gap} start={start} end={end} pattern {pattern:?}"
        );
    }
}

/// Org's outline expression has an unbounded heading prefix. Parking an
/// edit's gap inside the buffer must not turn every inline emphasis star
/// into a matcher invocation: physical storage is not a search condition.
#[test]
fn split_gap_org_heading_search_keeps_candidate_work_bounded() {
    let text = "ordinary * emphasis * and *inline* text\n".repeat(1024);
    let pattern = regex_compile(r"^\*+ ", false, false).unwrap();
    let syntax = DefaultSyntaxLookup;
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
    let contiguous = matcher_entry_count() - before;
    let split = text.len() / 2;
    let before = matcher_entry_count();
    assert!(
        re_search_segments(
            &pattern,
            &text.as_bytes()[..split],
            &text.as_bytes()[split..],
            0,
            text.len() as isize,
            &syntax,
            0
        )
        .is_none()
    );
    let segmented = matcher_entry_count() - before;
    assert!(
        segmented <= contiguous * 2 + 2,
        "a mid-buffer edit must preserve bounded candidate work: contiguous={contiguous}, split={segmented}"
    );
}

/// The whole forward scan borrows the physical chunks once. A matcher-entry
/// bound alone misses per-line setup when the first-byte fastmap rejects all
/// body lines. Count the actual Haystack queries, independent of line count.
#[test]
fn split_gap_org_heading_scan_borrows_chunks_once() {
    let pattern = regex_compile(r"^\*+ ", false, false).unwrap();
    let syntax = DefaultSyntaxLookup;
    for lines in [1024, 2048] {
        let text = "ordinary * emphasis * and *inline* text in journal\n".repeat(lines);
        for split in [0, text.len() / 2, text.len()] {
            let before = chunk_query_count();
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
            let queries = chunk_query_count() - before;
            assert!(
                queries <= 1,
                "one scan must borrow chunks once: lines={lines}, split={split}, queries={queries}"
            );
        }
    }
}
