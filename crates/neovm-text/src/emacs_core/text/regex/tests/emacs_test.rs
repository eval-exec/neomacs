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
