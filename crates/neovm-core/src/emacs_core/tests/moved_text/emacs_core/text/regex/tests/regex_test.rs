use neovm_edit::buffer::{Buffer, BufferId, BufferTextBackendKind, EmacsByteRange};

use neovm_text::emacs_core::regex::*;

use neovm_value::heap_types::LispString;

use neovm_value::value::Value;

#[test]
fn re_search_backward_word_begin_respects_search_origin_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(progn (erase-buffer) (insert \"one two three\") (goto-char 9))")
        .expect("prepare public Lisp search fixture");

    let word_begin = eval
        .eval_str("(re-search-backward \" \\\\<\" nil t)")
        .expect("search for a word beginning");
    assert_eq!(word_begin.as_int(), Some(4));

    eval.eval_str("(goto-char 9)")
        .expect("restore search origin");
    let word_boundary = eval
        .eval_str("(re-search-backward \" \\\\b\" nil t)")
        .expect("search for a word boundary");
    assert_eq!(word_boundary.as_int(), Some(8));

    eval.eval_str("(goto-char 9)")
        .expect("restore search origin");
    let symbol_begin = eval
        .eval_str("(re-search-backward \" \\\\_<\" nil t)")
        .expect("search for a symbol beginning");
    assert_eq!(symbol_begin.as_int(), Some(4));
}

/// Interleaved A/B benchmark of the `SyntaxSpecSet` fusion, immune to
/// whole-machine timing noise: for each fused font-lock pattern it times the
/// SAME pattern compiled fused vs unfused, alternating iterations in one
/// process so both experience identical contention, and reports the min for
/// each plus the speedup.  Fused must be at least as fast; match counts must
/// be identical (proving semantics-preserving).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_syntax_fusion_ab() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 15u32;

    // The font-lock patterns that actually carry a `\w\|\s_\|\\.` alternation.
    let fused_patterns = ["el-defs", "sexp-head-kw", "catch-throw", "quoted-symbol"];

    let mut report = String::from("BENCH syntax-fusion A/B (fused vs unfused, interleaved):\n");
    let mut tot_f = std::time::Duration::ZERO;
    let mut tot_u = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        if !fused_patterns.contains(name) {
            continue;
        }
        let cp_f = regex_bench_compile(pat, false);
        let cp_u = regex_emacs::with_syntax_fusion_disabled(|| regex_bench_compile(pat, false));
        assert!(
            cp_f.bytecode().contains(&33u8) && !cp_u.bytecode().contains(&33u8),
            "A/B setup: fused must contain SyntaxSpecSet, unfused must not ({name})"
        );
        let mc_f = regex_bench_engine_scan(&cp_f, bytes);
        let mc_u = regex_bench_engine_scan(&cp_u, bytes);
        assert_eq!(mc_f, mc_u, "fused/unfused match count differs for {name}");

        // Warm.
        regex_bench_engine_scan(&cp_f, bytes);
        regex_bench_engine_scan(&cp_u, bytes);
        let mut best_f = std::time::Duration::MAX;
        let mut best_u = std::time::Duration::MAX;
        for _ in 0..iters {
            let t = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_f, bytes), mc_f);
            best_f = best_f.min(t.elapsed());
            let t = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_u, bytes), mc_u);
            best_u = best_u.min(t.elapsed());
        }
        tot_f += best_f;
        tot_u += best_u;
        report.push_str(&format!(
            "  {name:<14} fused {best_f:>9.1?}  unfused {best_u:>9.1?}  speedup {:.2}x  ({mc_f} matches)\n",
            best_u.as_secs_f64() / best_f.as_secs_f64(),
        ));
    }
    report.push_str(&format!(
        "  TOTAL          fused {tot_f:>9.1?}  unfused {tot_u:>9.1?}  speedup {:.2}x",
        tot_u.as_secs_f64() / tot_f.as_secs_f64(),
    ));
    panic!("{report}");
}

/// (a) Literal-heavy search over ~256 KiB of real elisp:
///   - the `Literal` fast path (`str::find` / naive case-fold window scan),
///   - the same literal forced through the GNU-bytecode engine (`Exactn`),
///   - both case-fold variants.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_literal_100kb() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let needle = "unread-command-events"; // real symbol, occurs in subr.el
    let iters = 15;

    let t_find = regex_bench_min(iters, || {
        assert!(literal_find(&hay, needle, false).is_some());
    });
    let t_find_fold = regex_bench_min(iters, || {
        assert!(literal_find(&hay, "UNREAD-COMMAND-EVENTS", true).is_some());
    });

    let cp = regex_bench_compile(needle, false);
    assert!(cp.fastmap_accurate && !cp.uses_syntax);
    let t_engine = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(regex_emacs::re_search(&cp, bytes, 0, bytes.len() as isize, &syn, 0).is_some());
    });
    let cp_fold = regex_bench_compile("UNREAD-COMMAND-EVENTS", true);
    let t_engine_fold = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(
            regex_emacs::re_search(&cp_fold, bytes, 0, bytes.len() as isize, &syn, 0).is_some()
        );
    });

    // Count full-scan throughput too (needle near start would flatter us):
    // scan for a needle that never occurs, so every byte is visited.
    let cp_miss = regex_bench_compile("neverxyzzyneverxyzzy", false);
    let t_engine_miss = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(
            regex_emacs::re_search(&cp_miss, bytes, 0, bytes.len() as isize, &syn, 0).is_none()
        );
    });
    let t_find_miss = regex_bench_min(iters, || {
        assert!(literal_find(&hay, "neverxyzzyneverxyzzy", false).is_none());
    });

    panic!(
        "BENCH regex literal ({kib:.0} KiB subr.el): \
         hit: str::find {t_find:?} | engine-Exactn {t_engine:?} | \
         fold: window-scan {t_find_fold:?} | engine-translate {t_engine_fold:?} || \
         full-scan miss: str::find {t_find_miss:?} ({:.0} MiB/s) | engine {t_engine_miss:?} ({:.0} MiB/s)",
        kib / 1024.0 / t_find_miss.as_secs_f64(),
        kib / 1024.0 / t_engine_miss.as_secs_f64(),
    );
}

/// (b) Font-lock-ish workload, engine level: run each real emacs-lisp-mode
/// font-lock matcher over the haystack in the `(while (re-search-forward))`
/// shape. This is the per-fontification cost with zero elisp overhead.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fontlock_engine() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let iters = 7;

    let mut report = format!(
        "BENCH regex fontlock engine ({kib:.0} KiB subr.el):\n  \
         {:<16} {:>12} {:>12}  {:>5}  prefilter\n",
        "pattern", "fastmap-only", "prefilter", "matches"
    );
    // TOTAL over both configs so the net payoff is visible in one number.
    let mut total_off = std::time::Duration::ZERO; // fastmap-only (baseline)
    let mut total_on = std::time::Duration::ZERO; //  prefilter-enabled
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        // Prefilter-enabled compile (this branch's default).
        let cp = regex_bench_compile(pat, false);
        // Same pattern with the prefilter cleared → the fastmap-only baseline
        // (identical match engine + fastmap; the ONLY difference is the SIMD
        // multi-literal skip), so this is an apples-to-apples before/after.
        let mut cp_off = cp.clone();
        cp_off.prefilter = std::cell::OnceCell::from(None);

        let matches = regex_bench_engine_scan(&cp, bytes);
        // Semantics unchanged: the prefilter must never alter the match count.
        assert_eq!(
            regex_bench_engine_scan(&cp_off, bytes),
            matches,
            "prefilter changed match count for {name}"
        );

        // Interleave the two measurements in ONE loop (and warm both first)
        // so cache/paging state is identical for each — otherwise the second
        // `regex_bench_min` benefits from the first's warm-up, which would
        // fabricate a speedup for the no-prefilter patterns (whose two configs
        // run byte-identical code).  Take the min per config to reject noise.
        regex_bench_engine_scan(&cp_off, bytes);
        regex_bench_engine_scan(&cp, bytes);
        let mut t_off = std::time::Duration::MAX;
        let mut t_on = std::time::Duration::MAX;
        for _ in 0..iters {
            let a = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_off, bytes), matches);
            t_off = t_off.min(a.elapsed());
            let b = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp, bytes), matches);
            t_on = t_on.min(b.elapsed());
        }
        total_off += t_off;
        total_on += t_on;
        report.push_str(&format!(
            "  {name:<16} {t_off:>12.1?} {t_on:>12.1?}  {matches:>5}  {}\n",
            if cp.literal_prefilter().is_some() {
                "YES"
            } else {
                "none"
            },
        ));
    }
    report.push_str(&format!(
        "  {:<16} {total_off:>12.1?} {total_on:>12.1?}   TOTAL one fontify pass  ({:+.1}%)",
        "TOTAL",
        (total_on.as_secs_f64() / total_off.as_secs_f64() - 1.0) * 100.0,
    ));
    panic!("{report}");
}

/// A/B: the same font-lock fontify pass with the non-backtracking Pike VM
/// (default for eligible patterns) vs the backtracker forced on.  Reports
/// per-pattern + TOTAL time for both and asserts the MATCH COUNT is
/// identical (semantics unchanged).  Run explicitly in release.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_pike_vs_backtracker() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let iters = 7;

    let mut report = format!("BENCH pike-vs-backtracker fontlock ({kib:.0} KiB subr.el):\n");
    let mut tot_pike = std::time::Duration::ZERO;
    let mut tot_bt = std::time::Duration::ZERO;
    let mut tot_def = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let cp = regex_bench_compile(pat, false);
        let eligible = cp.pike_eligible;

        // Match counts MUST agree between all three routings.
        let mc_pike = regex_emacs::with_pike_forced(|| regex_bench_engine_scan(&cp, bytes));
        let mc_bt = regex_emacs::with_backtracker_forced(|| regex_bench_engine_scan(&cp, bytes));
        let mc_def = regex_bench_engine_scan(&cp, bytes); // production routing
        assert_eq!(
            mc_pike, mc_bt,
            "pike/backtrack match count differs for {name}"
        );
        assert_eq!(
            mc_def, mc_bt,
            "default/backtrack match count differs for {name}"
        );

        let t_pike = regex_bench_min(iters, || {
            assert_eq!(
                regex_emacs::with_pike_forced(|| regex_bench_engine_scan(&cp, bytes)),
                mc_pike
            );
        });
        let t_bt = regex_bench_min(iters, || {
            assert_eq!(
                regex_emacs::with_backtracker_forced(|| regex_bench_engine_scan(&cp, bytes)),
                mc_bt
            );
        });
        // Production default routing (backtracker + catastrophe budget).
        let t_def = regex_bench_min(iters, || {
            assert_eq!(regex_bench_engine_scan(&cp, bytes), mc_def);
        });
        tot_pike += t_pike;
        tot_bt += t_bt;
        tot_def += t_def;
        report.push_str(&format!(
            "  {name:<16} default {t_def:>9.1?}  backtrack {t_bt:>9.1?}  pike {t_pike:>9.1?}  eligible={eligible} ({mc_pike} matches)\n",
        ));
    }
    report.push_str(&format!(
        "  TOTAL default {tot_def:.1?}  backtrack {tot_bt:.1?}  pike {tot_pike:.1?}  (default/backtrack {:.2}x, pike/backtrack {:.2}x)",
        tot_def.as_secs_f64() / tot_bt.as_secs_f64(),
        tot_pike.as_secs_f64() / tot_bt.as_secs_f64(),
    ));
    panic!("{report}");
}

/// Fastmap A/B: the same effective search (find `(defun` heads) with a
/// syntax-free pattern (fastmap ON — skip loop over the first-byte table)
/// vs a `\_>`-terminated pattern (uses_syntax → fastmap disabled →
/// `re_match` attempted at every char position).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fastmap_on_vs_off() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 7;

    let cp_on = regex_bench_compile("(defun[ \t]", false);
    assert!(!cp_on.uses_syntax, "charset-only pattern must keep fastmap");
    let cp_off = regex_bench_compile("(defun\\_>", false);
    assert!(cp_off.uses_syntax, "\\_> must mark uses_syntax");

    let n_on = regex_bench_engine_scan(&cp_on, bytes);
    let n_off = regex_bench_engine_scan(&cp_off, bytes);
    let t_on = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_on, bytes), n_on);
    });
    let t_off = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_off, bytes), n_off);
    });

    panic!(
        "BENCH regex fastmap A/B ((defun heads, {} KiB): \
         fastmap-on \"(defun[ \\t]\" {t_on:?} ({n_on} matches) | \
         fastmap-off \"(defun\\\\_>\" {t_off:?} ({n_off} matches) -> {:.1}x",
        bytes.len() / 1024,
        t_off.as_secs_f64() / t_on.as_secs_f64(),
    );
}

/// (d) Case-fold + multibyte: the `quoted-symbol` matcher (multibyte
/// pattern chars U+2018/U+2019) and a case-folded keyword alternation,
/// both over the multibyte haystack.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_casefold_multibyte() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 7;

    let alt = "(\\(?:defun\\|defmacro\\|defvar\\|defconst\\|defsubst\\)[ \t]";
    let cp_nofold = regex_bench_compile(alt, false);
    let cp_fold = regex_bench_compile(alt, true);
    let n_nofold = regex_bench_engine_scan(&cp_nofold, bytes);
    let n_fold = regex_bench_engine_scan(&cp_fold, bytes);
    let t_nofold = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_nofold, bytes), n_nofold);
    });
    let t_fold = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_fold, bytes), n_fold);
    });

    let (mb_name, mb_pat) = REGEX_BENCH_FONTLOCK_PATTERNS
        .iter()
        .find(|(name, _)| *name == "quoted-symbol")
        .expect("quoted-symbol pattern present");
    let cp_mb = regex_bench_compile(mb_pat, false);
    let n_mb = regex_bench_engine_scan(&cp_mb, bytes);
    let t_mb = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_mb, bytes), n_mb);
    });

    panic!(
        "BENCH regex case-fold/multibyte ({} KiB): def-alternation nofold {t_nofold:?} \
         ({n_nofold}) vs fold {t_fold:?} ({n_fold}) -> {:.1}x | \
         {mb_name} (multibyte chars) {t_mb:?} ({n_mb} matches)",
        bytes.len() / 1024,
        t_fold.as_secs_f64() / t_nofold.as_secs_f64(),
    );
}

/// (b, full stack) Font-lock-ish workload through the real elisp builtins:
/// insert the haystack into a buffer, then per pattern run the exact
/// font-lock loop `(goto-char (point-min)) (while (re-search-forward P nil t))`.
/// Also re-runs one pattern with the gap parked mid-buffer (as after a
/// keystroke at the middle) — `with_buffer_emacs_bytes` then copies the
/// whole accessible region on every `re-search-forward` call (audit #17).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fontlock_elisp() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let hay = regex_bench_haystack();
    let kib = hay.len() as f64 / 1024.0;
    ev.eval_str(&format!(
        "(progn (insert \"{}\") (goto-char (point-max)) (insert \"\\n\") nil)",
        regex_bench_elisp_escape(&hay)
    ))
    .expect("insert haystack");
    // Gap now sits at point-max: the whole accessible region is contiguous,
    // so buffer searches borrow zero-copy.

    let iters = 5;
    let mut report = format!("BENCH regex fontlock elisp ({kib:.0} KiB buffer):\n");
    let mut total = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let form = format!(
            "(progn (goto-char (point-min)) (let ((n 0)) (while (re-search-forward \"{}\" nil t) (setq n (1+ n))) n))",
            regex_bench_elisp_escape(pat)
        );
        let matches = ev
            .eval_str(&form)
            .expect("fontlock pass evaluates")
            .as_int()
            .expect("match count");
        let t = regex_bench_min(iters, || {
            let got = ev.eval_str(&form).expect("fontlock pass evaluates");
            assert_eq!(got.as_int(), Some(matches));
        });
        total += t;
        report.push_str(&format!("  {name:<16} {t:>10.1?}  {matches:>5} matches\n"));
    }
    report.push_str(&format!(
        "  TOTAL one fontify pass over all patterns: {total:.1?}\n"
    ));

    // Park the gap mid-buffer: insert+delete a char at the middle. Every
    // subsequent whole-buffer search must copy the accessible region.
    // (Core builtins only — a bare Context has no subr.el, so no delete-char.)
    ev.eval_str(
        "(progn (goto-char (/ (point-max) 2)) (insert \"z\") (delete-region (- (point) 1) (point)) nil)",
    )
    .expect("park gap mid-buffer");
    let (name, pat) = REGEX_BENCH_FONTLOCK_PATTERNS[1]; // sexp-head-kw
    let form = format!(
        "(progn (goto-char (point-min)) (let ((n 0)) (while (re-search-forward \"{}\" nil t) (setq n (1+ n))) n))",
        regex_bench_elisp_escape(pat)
    );
    let matches = ev
        .eval_str(&form)
        .expect("gap-split pass evaluates")
        .as_int()
        .expect("match count");
    let t_split = regex_bench_min(iters, || {
        let got = ev.eval_str(&form).expect("gap-split pass evaluates");
        assert_eq!(got.as_int(), Some(matches));
    });
    report.push_str(&format!(
        "  {name} with gap mid-buffer (copy per call): {t_split:.1?}  {matches} matches"
    ));
    panic!("{report}");
}

/// `string-match` per-call overhead: a short hot-cache `string-match` on a
/// small string (per-call fixed costs: cache probe, pattern copy, match-data
/// conversion) vs a full-scan miss over the big string (throughput).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_string_match_elisp() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let hay = regex_bench_haystack();
    ev.eval_str(&format!(
        "(progn (defvar regex-bench-big \"{}\") (defvar regex-bench-small (substring regex-bench-big 0 512)) nil)",
        regex_bench_elisp_escape(&hay)
    ))
    .expect("define bench strings");

    let iters = 7;
    let calls = 1000u32;
    // Core builtins only — a bare Context has no subr.el (no dotimes/when).
    let t_small = regex_bench_min(iters, || {
        ev.eval_str(
            "(let ((n 0) (i 0)) (while (< i 1000) (if (string-match \"(\\\\(\\\\(?:\\\\w\\\\|\\\\s_\\\\)+\\\\)\\\\_>\" regex-bench-small) (setq n (1+ n))) (setq i (1+ i))) n)",
        )
        .expect("small string-match loop");
    });
    let t_miss = regex_bench_min(iters, || {
        ev.eval_str("(string-match \"neverxyzzyneverxyzzy\" regex-bench-big)")
            .expect("big string-match miss");
    });
    panic!(
        "BENCH regex string-match elisp: hot small (512B, first match) {:?}/call | \
         literal full-scan miss over {} KiB: {t_miss:?}",
        t_small / calls,
        hay.len() / 1024,
    );
}

/// Fastmap-on/off equivalence over the real font-lock patterns plus
/// syntax/class/boundary/charset patterns, across ASCII, multibyte,
/// case-fold, and match-at-start/middle/none haystacks.
#[test]
fn regex_fastmap_equivalence_fuzz() {
    let mut patterns: Vec<(&str, bool)> = REGEX_BENCH_FONTLOCK_PATTERNS
        .iter()
        .map(|(_, pat)| (*pat, false))
        .collect();
    patterns.extend_from_slice(&[
        ("[[:digit:]]+", false),
        ("[[:word:]]+", false),
        ("[[:space:]]+", false),
        ("x[[:alpha:]]*!", false),
        ("[-[:alnum:]]+\\.el", false),
        ("[^[:word:]]+", false),
        ("[[:word:]]+", true),
        ("\\bdefun", false),
        ("provide\\b", false),
        ("\\<forward\\>", false),
        ("\\_<point-min\\_>", false),
        ("\\w+", false),
        ("\\W\\w", false),
        ("\\s-;;", false),
        ("\\Sw+", false),
        ("[a-f\u{e9}-\u{ef}]+", false),
        ("(defun\\_>", false),
        ("(DEFUN\\_>", true),
        ("^;;;###autoload", false),
        ("\\(setq\\|setf\\)[ \t]", false),
        ("[A-F]+x", true),
    ]);

    let haystacks: [&str; 10] = [
        "",
        "(defun foo-bar (x) \"doc \u{2018}quoted\u{2019}\" nil)",
        ";;;###autoload\n(defun x ())",
        "no-match-here 12345 end",
        "===defun=== \u{2018}sym\u{2019} `q' \\e",
        "\u{e9}\u{e9} (defun \u{e0}ccent-fn ()) ;; \u{2018}x\u{2019}",
        "a b\tc\nd-e_f",
        "defun at start",
        "ends with (defun\t",
        "(SETQ CASE-FOLD T) (setf x)",
    ];

    for &(pat, case_fold) in &patterns {
        let cp = regex_bench_compile(pat, case_fold);
        for hay in &haystacks {
            assert_fastmap_equivalence(
                &format!("{pat:?} (fold={case_fold}) vs {hay:?}"),
                &cp,
                &DefaultSyntaxLookup,
                hay.as_bytes(),
            );
        }
        // Same corpus under a modified syntax table, mirroring the
        // front-end pipeline: `used_syntax` fastmaps are rebaked
        // against the active table before use.
        let mut cp_dash = cp.clone();
        regex_emacs::recompute_fastmap(&mut cp_dash, &DashWordSyntaxLookup);
        for hay in &haystacks {
            assert_fastmap_equivalence(
                &format!("{pat:?} (fold={case_fold}, dash-word table) vs {hay:?}"),
                &cp_dash,
                &DashWordSyntaxLookup,
                hay.as_bytes(),
            );
        }
    }

    // The real font-lock patterns over real elisp text (2 KiB of
    // subr.el, cut at a char boundary).
    let big = regex_bench_haystack();
    let mut end = big.len().min(2 * 1024);
    while !big.is_char_boundary(end) {
        end -= 1;
    }
    let big = &big[..end];
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let cp = regex_bench_compile(pat, false);
        assert_fastmap_equivalence(
            &format!("{name} vs subr.el slice"),
            &cp,
            &DefaultSyntaxLookup,
            big.as_bytes(),
        );
    }
}

/// A syntax-table content mutation must not serve a stale fastmap:
/// GNU `Fmodify_syntax_entry` ends with `clear_regexp_cache ()`; our
/// analog is the syntax mutation epoch in the cache key.
#[test]
fn regex_syntax_table_mutation_invalidates_cached_fastmap() {
    let mut ev = Context::new();
    ev.eval_str("(progn (insert \"===abc===\") nil)")
        .expect("insert");
    let probe = "(progn (goto-char (point-min)) \
                 (if (re-search-forward \"[[:word:]]+\" nil t) (match-beginning 0) -1))";
    // Standard table: '=' is Ssymbol, so the first word chars are "abc".
    let first = ev.eval_str(probe).expect("first search");
    assert_eq!(first.as_int(), Some(4), "standard table matches at 'abc'");
    // Make '=' a word constituent.  The cached pattern's fastmap (baked
    // without '=') must not be reused, or the search would skip the
    // '===' prefix and still report position 4.
    ev.eval_str("(modify-syntax-entry ?= \"w\")")
        .expect("modify-syntax-entry");
    let second = ev.eval_str(probe).expect("second search");
    assert_eq!(
        second.as_int(),
        Some(1),
        "mutated table must re-bake the fastmap (no stale cache entry)"
    );
}

/// Cache entries baked for one syntax table must not be served under
/// another (GNU keys the regexp cache by the syntax-table object,
/// search.c:222-224), and table-independent entries survive table
/// switches.
#[test]
fn regex_syntax_table_identity_keys_cached_fastmap() {
    let mut ev = Context::new();
    // Build the modified table BEFORE any compile so the mutation epoch
    // is constant across all three probes — this isolates the identity
    // axis of the cache key.
    ev.eval_str(
        "(progn (defvar regex-fastmap-t2 (copy-syntax-table)) \
                (modify-syntax-entry ?= \"w\" regex-fastmap-t2) \
                (insert \"===abc===\") nil)",
    )
    .expect("setup");
    let probe = "(progn (goto-char (point-min)) \
                 (if (re-search-forward \"[[:word:]]+\" nil t) (match-beginning 0) -1))";

    let under_standard = ev.eval_str(probe).expect("standard search");
    assert_eq!(under_standard.as_int(), Some(4));

    ev.eval_str("(set-syntax-table regex-fastmap-t2)")
        .expect("switch to t2");
    let under_t2 = ev.eval_str(probe).expect("t2 search");
    assert_eq!(
        under_t2.as_int(),
        Some(1),
        "the standard-table cache entry must not satisfy table t2"
    );

    ev.eval_str("(set-syntax-table (standard-syntax-table))")
        .expect("switch back");
    let back = ev.eval_str(probe).expect("standard search again");
    assert_eq!(
        back.as_int(),
        Some(4),
        "the t2 cache entry must not satisfy the standard table"
    );
}

/// The same overflow at the elisp level signals GNU's error
/// (`search.c:matcher_overflow`), even under `noerror`.
#[test]
fn regex_fail_stack_overflow_signals_elisp_error() {
    let mut ev = Context::new();
    ev.eval_str("(progn (insert (make-string 400000 ?a)) (goto-char (point-min)) nil)")
        .expect("insert haystack");
    // `noerror = t` must not swallow the overflow: GNU's
    // matcher_overflow is an `error`, not `search-failed`.
    let caught = ev
        .eval_str(
            "(condition-case err \
                 (progn (re-search-forward \"\\\\(a*\\\\)*b\" nil t) \"no-error\") \
               (error (format \"%S\" err)))",
        )
        .expect("condition-case evaluates");
    let msg = caught
        .as_lisp_string()
        .and_then(|s| s.as_utf8_str())
        .unwrap_or_default()
        .to_string();
    assert!(
        msg.contains("Stack overflow in regexp matcher"),
        "expected GNU matcher_overflow error, got: {msg}"
    );
}

/// Audit #17: a regexp search across a mid-buffer gap must produce the
/// same result as over contiguous text (the search path now moves the
/// gap out of the accessible range instead of copying it per call).
#[test]
fn regex_search_across_mid_buffer_gap() {
    let mut ev = Context::new();
    // Insert text, then park the gap in the middle of "defun" via an
    // insert+delete at position 7.
    ev.eval_str(
        "(progn (insert \"aaa defun bbb defun ccc\") \
                (goto-char 7) (insert \"z\") (delete-region 7 8) \
                (goto-char (point-min)) nil)",
    )
    .expect("build gap-split buffer");
    let hit = ev
        .eval_str("(if (re-search-forward \"defun\\\\_>\" nil t) (match-beginning 0) -1)")
        .expect("first search");
    assert_eq!(hit.as_int(), Some(5), "match spanning the parked gap");
    let hit2 = ev
        .eval_str("(if (re-search-forward \"defun\\\\_>\" nil t) (match-beginning 0) -1)")
        .expect("second search");
    assert_eq!(hit2.as_int(), Some(15));
    // Backward from the end re-finds the second occurrence.
    let back = ev
        .eval_str(
            "(progn (goto-char (point-max)) \
                    (if (re-search-backward \"defun\\\\_>\" nil t) (point) -1))",
        )
        .expect("backward search");
    assert_eq!(back.as_int(), Some(15));
}

/// Measurement probe for the lazy-match-data (Tier-2) sizing question: of
/// the group endpoints eagerly byte->char converted at publish time, how
/// many are ever read back by Lisp? Run explicitly:
///   cargo nextest run -p neovm-core -E 'test(match_publish_read_stats_probe)' --run-ignored all --no-capture
#[test]
#[cfg(debug_assertions)]
#[cfg(feature = "perf-probes")]
fn match_publish_read_stats_probe() {
    use std::sync::atomic::Ordering;
    let mut eval = crate::test_utils::runtime_startup_context();
    super::match_stats::reset();
    eval.eval_str(
        r#"(with-temp-buffer
             (insert-file-contents (locate-library "bytecomp.el" t))
             (emacs-lisp-mode)
             (font-lock-ensure (point-min) (point-max))
             t)"#,
    )
    .expect("fontify bytecomp.el");
    let s = |c: &std::sync::atomic::AtomicUsize| c.load(Ordering::Relaxed);
    println!(
        "MATCH-STATS publishes={} published_g0={} published_sub={} \
         read_g0={} read_sub={} full_exports={} \
         distinct_some_g0={} distinct_some_sub={}",
        s(&super::match_stats::PUBLISHES),
        s(&super::match_stats::PUBLISHED_GROUP0),
        s(&super::match_stats::PUBLISHED_SUB),
        s(&super::match_stats::READ_GROUP0),
        s(&super::match_stats::READ_SUB),
        s(&super::match_stats::FULL_EXPORTS),
        s(&super::match_stats::FIRST_SOME_G0),
        s(&super::match_stats::FIRST_SOME_SUB),
    );
}

/// GNU 31.1: a buffer search publishes its registers as character positions
/// (`search_buffer_re`'s `BYTE_TO_CHAR` per register) -- in a buffer whose
/// characters are all one byte and in one with multibyte characters, with
/// unmatched groups, a match at the end of the text, a narrowing and a
/// backward search.
#[test]
fn buffer_search_match_data_is_in_characters_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (if (bufferp x) 'buf x)) (match-data t)))))
          (list
           (with-temp-buffer
             (insert "alpha beta* gamma\nzeta: end")
             (list
              (progn (goto-char (point-min)) (re-search-forward "\\(b\\(e\\)\\)\\|\\(x\\)ta\\(\\*\\)?" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(en\\)\\(d\\)\\'" nil t) (funcall md))
              (progn (goto-char (point-min)) (looking-at "\\(al\\)\\(ph\\)?\\(q\\)?") (funcall md))
              (save-restriction (narrow-to-region 7 12) (goto-char (point-min)) (re-search-forward "\\(ta\\)\\(\\*\\)" nil t) (funcall md))
              (progn (goto-char (point-max)) (re-search-backward "\\(z\\)\\(e\\)" nil t) (funcall md))
              (progn (goto-char (point-min)) (search-forward "gamma" nil t) (funcall md))))
           (with-temp-buffer
             (insert "αλφα beta* γάμμα\nζήτα: end")
             (list
              (progn (goto-char (point-min)) (re-search-forward "\\(b\\(e\\)\\)\\|\\(x\\)ta\\(\\*\\)?" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(γά\\)\\(μ+\\)" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(en\\)\\(d\\)\\'" nil t) (funcall md))
              (progn (goto-char (point-min)) (looking-at "\\(αλ\\)\\(φ\\)?\\(q\\)?") (funcall md))
              (save-restriction (narrow-to-region 7 12) (goto-char (point-min)) (re-search-forward "\\(ta\\)\\(\\*\\)" nil t) (funcall md))))))
        "#,
    );
    assert_eq!(
        result,
        "OK (((7 9 7 9 8 9 buf) (25 28 25 27 27 28 buf) (1 5 1 3 3 5 buf) (9 12 9 11 11 12 buf) (19 21 19 20 20 21 buf) (13 18 buf)) ((6 8 6 8 7 8 buf) (12 16 12 14 14 16 buf) (24 27 24 26 26 27 buf) (1 4 1 3 3 4 buf) (8 11 8 10 10 11 buf)))"
    );
}

/// GNU 31.1: a successful buffer search replaces the match data whatever it
/// held (string match data, another buffer's), `COUNT` publishes the last
/// match, and a failed search or `inhibit-changing-match-data` leaves it as
/// it was -- the in-place publication must reuse nothing stale.
#[test]
fn buffer_search_replaces_match_data_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (cond ((bufferp x) (buffer-name x)) ((markerp x) (list 'm (marker-position x) (buffer-name (marker-buffer x)))) (t x))) (match-data)))))
          (let ((a (get-buffer-create "md-a")) (b (get-buffer-create "md-b")) out)
            (with-current-buffer a (erase-buffer) (insert "one two three two one"))
            (with-current-buffer b (erase-buffer) (insert "αβ two γ two"))
            (string-match "\\(t\\)wo" "xx two")
            (push (match-data) out)
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "\\(t\\)\\(w\\)o" nil t) (push (funcall md) out))
            (with-current-buffer b (goto-char (point-min)) (search-forward "two" nil t 2) (push (funcall md) out) (push (point) out))
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "zzz" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-max)) (re-search-backward "\\(o\\)ne" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min))
              (let ((inhibit-changing-match-data t)) (re-search-forward "three" nil t)) (push (point) out) (push (funcall md) out))
            (with-current-buffer b (goto-char (point-max)) (search-backward "two" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "\\(two\\)\\|\\(xyz\\)" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min)) (condition-case e (re-search-forward "nope") (error (push (car e) out))) (push (funcall md) out))
            (kill-buffer a) (kill-buffer b)
            (nreverse out)))
        "#,
    );
    assert_eq!(
        result,
        r#"OK ((3 6 3 4) ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 6 "md-a") (m 6 "md-a") (m 7 "md-a")) ((m 10 "md-b") (m 13 "md-b")) 13 ((m 10 "md-b") (m 13 "md-b")) ((m 19 "md-a") (m 22 "md-a") (m 19 "md-a") (m 20 "md-a")) 14 ((m 19 "md-a") (m 22 "md-a") (m 19 "md-a") (m 20 "md-a")) ((m 10 "md-b") (m 13 "md-b")) ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 8 "md-a")) search-failed ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 8 "md-a")))"#
    );
}

/// GNU 31.1: a search made zero times (`COUNT` 0) does not move point and
/// makes the match data an empty match at point -- `search_buffer`'s
/// `set_search_regs (pos_byte, 0)` -- whatever it held before, unless
/// `inhibit-changing-match-data' is set.
#[test]
fn a_zero_count_search_matches_the_empty_string_at_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (if (bufferp x) 'buf x)) (match-data t)))))
          (with-temp-buffer
            (insert "one two three αβ two")
            (list
             (progn (goto-char 5) (string-match "\\(x\\)" "zzx") (list (re-search-forward "t\\(w\\)o" nil nil 0) (point) (funcall md)))
             (progn (goto-char 5) (re-search-forward "\\(o\\)" nil t) (list (search-forward "two" nil nil 0) (point) (funcall md)))
             (progn (goto-char 18) (list (search-backward "one" nil nil 0) (point) (funcall md)))
             (progn (goto-char 18) (string-match "\\(x\\)" "zzx") (list (posix-search-backward "o+" nil nil 0) (point) (funcall md)))
             (progn (goto-char 7) (string-match "\\(x\\)" "zzx")
                    (let ((inhibit-changing-match-data t)) (list (re-search-forward "x" nil nil 0) (point) (funcall md)))))))
        "#,
    );
    assert_eq!(
        result,
        "OK ((5 5 (5 5 buf)) (8 8 (8 8 buf)) (18 18 (18 18 buf)) (18 18 (18 18 buf)) (7 7 (2 3 2 3)))"
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MatchDataSnapshot {
    groups: Vec<Option<MatchGroup>>,
    searched_buffer: Option<BufferId>,
    searched_string_is_some: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferSearchSnapshot<T> {
    result: Result<T, String>,
    pt_byte: usize,
    pt: usize,
    full_bytes: Vec<u8>,
    multibyte: bool,
    match_data: Option<MatchDataSnapshot>,
}

/// Real GNU Emacs 31 emacs-lisp-mode font-lock matcher regexps.
const REGEX_BENCH_FONTLOCK_PATTERNS: &[(&str, &str)] = &[
    (
        "el-defs",
        "(\\(cl-def\\(?:generic\\|m\\(?:acro\\|ethod\\)\\|s\\(?:\\(?:truc\\|ubs\\)t\\)\\|type\\|un\\)\\|def\\(?:a\\(?:dvice\\|lias\\)\\|c\\(?:lass\\|onst\\|ustom\\)\\|face\\|g\\(?:eneric\\|roup\\)\\|ine-\\(?:advice\\|derived-mode\\|error\\|g\\(?:\\(?:eneric\\|lobalized-minor\\)-mode\\)\\|inline\\|minor-mode\\|skeleton\\|widget\\)\\|m\\(?:acro\\|ethod\\)\\|subst\\|theme\\|un\\|var\\(?:-local\\|alias\\)?\\)\\|ert-deftest\\)\\_>[ \t']*\\(([ \t']*\\)?\\(\\(setf\\)[ \t]+\\(?:\\w\\|\\s_\\|\\\\.\\)+\\|\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    ("sexp-head-kw", "(\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)\\_>"),
    (
        "autoload-cookie",
        "^;;;###\\(\\([-[:alnum:]]+?\\)-\\)?\\(autoload\\)",
    ),
    (
        "el-errors",
        "(\\(cl-\\(?:assert\\|check-type\\)\\|error\\|signal\\|user-error\\|warn\\)\\_>",
    ),
    (
        "catch-throw",
        "(\\(catch\\|throw\\|featurep\\|provide\\|require\\)\\_>[ \t']*\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    (
        "quoted-symbol",
        "[`\u{2018}']\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)['\u{2019}]",
    ),
    ("keyword-colon", "\\_<:\\(?:\\w\\|\\s_\\|\\\\.\\)+\\_>"),
    ("backslash-esc", "\\(\\\\\\)\\([^\"\\]\\)"),
];

/// First ~256 KiB of `lisp/subr.el`, cut at a char boundary.
fn regex_bench_haystack() -> String {
    let path = crate::test_utils::workspace_root()
        .join("lisp/subr.el")
        .to_string_lossy()
        .into_owned();
    let text = std::fs::read_to_string(path).expect("read lisp/subr.el haystack");
    let mut end = text.len().min(256 * 1024);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn regex_bench_min(iters: u32, mut f: impl FnMut()) -> std::time::Duration {
    // Warm once (compile caches, page in the haystack).
    f();
    let mut best = std::time::Duration::MAX;
    for _ in 0..iters {
        let t = std::time::Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

fn regex_bench_compile(pattern: &str, case_fold: bool) -> regex_emacs::CompiledPattern {
    regex_emacs::regex_compile_lisp(&LispString::from_utf8(pattern), false, case_fold)
        .expect("bench pattern should compile")
}

/// Emulate the font-lock `(while (re-search-forward P nil t))` loop over
/// `text` with the already-compiled pattern; returns the match count.
fn regex_bench_engine_scan(cp: &regex_emacs::CompiledPattern, text: &[u8]) -> usize {
    let syn = DefaultSyntaxLookup;
    let mut n = 0usize;
    let mut at = 0usize;
    while at <= text.len() {
        let Some((_pos, regs)) =
            regex_emacs::re_search(cp, text, at, (text.len() - at) as isize, &syn, at)
        else {
            break;
        };
        n += 1;
        let end = regs.end[0].max(0) as usize;
        if end > at {
            at = end;
        } else {
            // Zero-width match: advance one char like `re-search-forward`.
            match next_search_char_boundary(text, end) {
                Some(next) if next > at => at = next,
                _ => break,
            }
        }
    }
    n
}

fn regex_bench_elisp_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1024);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

/// Syntax lookup with `-` promoted to a word constituent (as lisp modes
/// do via `modify-syntax-entry`) — exercises the buffer-table rebake
/// path of the fastmap without needing a full evaluator.
struct DashWordSyntaxLookup;

impl regex_emacs::SyntaxLookup for DashWordSyntaxLookup {
    fn char_syntax(&self, c: char) -> neovm_text::emacs_core::syntax::SyntaxClass {
        if c == '-' {
            neovm_text::emacs_core::syntax::SyntaxClass::Word
        } else {
            neovm_text::emacs_core::syntax::standard_syntax_class_for_char(c)
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> regex_emacs::SyntaxCacheKey {
        // Test-only: distinct from `Standard` so it can never satisfy a
        // standard-baked cache entry.
        regex_emacs::SyntaxCacheKey::External {
            id: usize::MAX - 1,
            epoch: 0,
        }
    }
}

/// Run `re_search` twice — fastmap enabled vs force-disabled — and
/// assert identical results (match position + full match-data
/// registers) over forward-from-0, forward-from-middle, and backward
/// spans.
fn assert_fastmap_equivalence(
    label: &str,
    cp: &regex_emacs::CompiledPattern,
    syn: &dyn regex_emacs::SyntaxLookup,
    text: &[u8],
) {
    let mid = text.len() / 2;
    let spans: [(usize, isize); 3] = [
        (0, text.len() as isize),
        (mid, (text.len() - mid) as isize),
        (text.len(), -(text.len() as isize)),
    ];
    for &(start, range) in &spans {
        let normal = regex_emacs::re_search(cp, text, start, range, syn, start);
        let forced = regex_emacs::with_fastmap_disabled(|| {
            regex_emacs::re_search(cp, text, start, range, syn, start)
        });
        match (&normal, &forced) {
            (None, None) => {}
            (Some((p1, r1)), Some((p2, r2))) => {
                assert_eq!(
                    p1, p2,
                    "{label}: match position diverged at start={start} range={range}"
                );
                assert_eq!(
                    r1.start, r2.start,
                    "{label}: register starts diverged at start={start} range={range}"
                );
                assert_eq!(
                    r1.end, r2.end,
                    "{label}: register ends diverged at start={start} range={range}"
                );
            }
            (n, f) => panic!(
                "{label}: fastmap changed match existence at start={start} range={range}: \
                 with-fastmap={:?} without-fastmap={:?}",
                n.as_ref().map(|(p, _)| *p),
                f.as_ref().map(|(p, _)| *p),
            ),
        }
    }
}
