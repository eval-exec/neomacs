//! The L1 syntax parse cache answers exactly as a plain scan (P3.4 S4).
//!
//! A differential fuzz: random buffers under five comment dialects, random
//! query sessions (chained TOs from one FROM, repeated TOs, OLDSTATEs taken
//! from real parses, every option), interleaved with every kind of change a
//! scan reads -- text edits, `syntax-table` properties, syntax-table entries,
//! narrowing, `comment-end-can-be-escaped`, `parse-sexp-lookup-properties`,
//! multibyteness, and descriptor conses changed in place. Every answer, value
//! and point, must equal the uncached scan's; the cache must actually serve
//! (exact answers and resumes), and each invalidation path must be taken.

use super::*;
use crate::emacs_core::print::print_value;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const TOKENS: &[&str] = &[
    "/*",
    "*/",
    "//",
    "\n",
    "'",
    "\"",
    "\\",
    "(*",
    "*)",
    "{-",
    "-}",
    "!",
    "|",
    "#",
    "%",
    "(",
    ")",
    "[",
    "]",
    ";",
    "it's",
    "a\\(b",
    "é",
    "ü",
    "?\\(",
    "\"a\\\"b\"",
    "(defun f (x) x)",
];

fn random_text(rng: &mut Rng, tokens: usize) -> String {
    let mut out = String::new();
    for _ in 0..tokens {
        if rng.below(3) == 0 {
            for _ in 0..rng.below(4) + 1 {
                out.push([' ', 'a', 'x', 'é', '_', '\''][rng.below(6)]);
            }
        } else {
            out.push_str(TOKENS[rng.below(TOKENS.len())]);
        }
    }
    out
}

fn modify(eval: &mut crate::emacs_core::eval::Context, ch: char, descriptor: &str) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum(ch as i64), Value::string(descriptor)],
    )
    .expect("modify-syntax-entry");
}

fn install_table(eval: &mut crate::emacs_core::eval::Context, kind: usize) {
    eval.eval_str("(set-syntax-table (copy-syntax-table))")
        .expect("own table");
    for ch in [
        '/', '*', '\\', '\'', '"', '!', '#', '%', '(', ')', '{', '-', '}', '|', '\n', ';', '[',
        ']', '?',
    ] {
        modify(eval, ch, ".");
    }
    modify(eval, '\\', "\\");
    modify(eval, '"', "\"");
    modify(eval, '(', "()");
    modify(eval, ')', ")(");
    modify(eval, '[', "(]");
    modify(eval, ']', ")[");
    // Non-ASCII entries the recording scans log (`é` a word, `ü` a symbol).
    modify(eval, 'é', "w");
    modify(eval, 'ü', "_");
    match kind {
        0 => {
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
            modify(eval, '\n', "> b");
            modify(eval, '\'', "\"");
        }
        1 => {
            modify(eval, ';', "<");
            modify(eval, '\n', ">");
            modify(eval, '\'', "'");
            modify(eval, '?', "_ p");
        }
        2 => {
            modify(eval, '(', "()1n");
            modify(eval, ')', ")(4n");
            modify(eval, '*', ". 23n");
            modify(eval, '{', "(}1n");
            modify(eval, '}', "){4n");
            modify(eval, '-', ". 23n");
        }
        3 => {
            modify(eval, '!', "!");
            modify(eval, '|', "|");
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
        }
        _ => {
            modify(eval, '#', "< 3");
            modify(eval, '%', ". 4");
            modify(eval, '\n', ">");
        }
    }
}

fn point_max(eval: &crate::emacs_core::eval::Context) -> usize {
    eval.buffers
        .current_buffer()
        .expect("buffer")
        .point_max_char_pos()
        .get()
        + 1
}

fn point_min(eval: &crate::emacs_core::eval::Context) -> usize {
    eval.buffers
        .current_buffer()
        .expect("buffer")
        .point_min_char_pos()
        .get()
        + 1
}

#[derive(Clone, Copy, Debug)]
struct Query {
    from: usize,
    to: usize,
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: u8,
    oldstate_from_parse: bool,
}

/// One `parse-partial-sexp` under MODE: its printed value and point.
fn answer(
    eval: &mut crate::emacs_core::eval::Context,
    query: Query,
    oldstate: Value,
    mode: ParseCacheMode,
) -> (String, usize) {
    let _guard = ParseCacheTestGuard::modes(mode, canon::CanonMode::Off);
    let commentstop = match query.commentstop {
        0 => Value::NIL,
        1 => Value::T,
        _ => Value::symbol("syntax-table"),
    };
    let result = builtin_parse_partial_sexp_6(
        eval,
        Value::fixnum(query.from as i64),
        Value::fixnum(query.to as i64),
        query.target_depth.map_or(Value::NIL, Value::fixnum),
        Value::bool_val(query.stop_before),
        oldstate,
        commentstop,
    )
    .expect("parse-partial-sexp");
    let point = eval
        .buffers
        .current_buffer()
        .expect("buffer")
        .point_char_pos()
        .get()
        + 1;
    (print_value(&result), point)
}

/// Check one query: the cached answer equals the plain one.
fn check(eval: &mut crate::emacs_core::eval::Context, query: Query, what: &str) {
    let roots = eval.save_specpdl_roots();
    let oldstate = if query.oldstate_from_parse && query.from > point_min(eval) {
        let begv = point_min(eval) as i64;
        let _guard = ParseCacheTestGuard::modes(ParseCacheMode::Off, canon::CanonMode::Off);
        let state = builtin_parse_partial_sexp_6(
            eval,
            Value::fixnum(begv),
            Value::fixnum(query.from as i64),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        )
        .expect("oldstate");
        state
    } else {
        Value::NIL
    };
    eval.push_specpdl_root(oldstate);
    let cached = answer(eval, query, oldstate, ParseCacheMode::On);
    let plain = answer(eval, query, oldstate, ParseCacheMode::Off);
    eval.restore_specpdl_roots(roots);
    assert_eq!(cached, plain, "{what}: {query:?}");
}

fn random_query(rng: &mut Rng, begv: usize, zv: usize, from: usize) -> Query {
    let to = from + rng.below(zv + 1 - from);
    Query {
        from,
        to,
        target_depth: [None, None, None, Some(-1), Some(0), Some(1)][rng.below(6)],
        stop_before: rng.below(6) == 0,
        commentstop: [0, 0, 0, 1, 2][rng.below(5)] as u8,
        oldstate_from_parse: from > begv && rng.below(2) == 0,
    }
}

/// A random change of something a scan reads. Returns its name.
fn mutate(eval: &mut crate::emacs_core::eval::Context, rng: &mut Rng) -> &'static str {
    let begv = point_min(eval);
    let zv = point_max(eval);
    let len = zv - begv;
    let at = begv + rng.below(len + 1);
    match rng.below(12) {
        0 | 1 => {
            let tokens = 1 + rng.below(3);
            let text = random_text(rng, tokens);
            let lisp = format!("{text:?}");
            eval.eval_str(&format!(
                "(save-excursion (goto-char {at}) (insert {lisp}))"
            ))
            .expect("insert");
            "insert"
        }
        2 | 3 if len > 0 => {
            let end = (at + 1 + rng.below(4)).min(zv);
            let start = at.min(end);
            eval.eval_str(&format!("(delete-region {start} {end})"))
                .expect("delete");
            "delete"
        }
        4 | 5 if len > 0 => {
            let end = (at + 1 + rng.below(3)).min(zv);
            let class = [0, 1, 2, 3, 4, 5, 7, 11, 12, 14, 15][rng.below(11)];
            eval.eval_str(&format!(
                "(put-text-property {} {end} 'syntax-table '({class}))",
                at.min(end)
            ))
            .expect("put");
            "put syntax-table"
        }
        6 if len > 0 => {
            eval.eval_str(&format!(
                "(remove-text-properties {} {zv} '(syntax-table nil))",
                at.min(zv)
            ))
            .expect("remove");
            "remove syntax-table"
        }
        7 => {
            // A property descriptor changed in place: every value this test
            // puts is a fresh cons, so find one and flip its class.
            let class = [1, 7, 12][rng.below(3)];
            eval.eval_str(&format!(
                "(let ((pos (next-single-property-change {begv} 'syntax-table nil {zv})))
                   (if (and pos (< pos {zv}))
                       (let ((d (get-text-property pos 'syntax-table)))
                         (if (consp d) (setcar d {class})))))"
            ))
            .expect("setcar property descriptor");
            "setcar property descriptor"
        }
        8 => {
            // A non-ASCII table descriptor changed in place.
            let class = [2, 3, 7][rng.below(3)];
            eval.eval_str(&format!(
                "(let ((d (aref (syntax-table) ?é))) (if (consp d) (setcar d {class})))"
            ))
            .expect("setcar table descriptor");
            "setcar table descriptor"
        }
        9 => {
            let (ch, descriptor) =
                [('\'', "\""), ('\'', "."), ('x', "."), ('x', "w")][rng.below(4)];
            modify(eval, ch, descriptor);
            "modify-syntax-entry"
        }
        10 => {
            if rng.below(2) == 0 && len > 2 {
                let start = begv + rng.below(len / 2 + 1);
                let end = (start + 1 + rng.below(len)).min(zv);
                eval.eval_str(&format!("(narrow-to-region {start} {end})"))
                    .expect("narrow");
                "narrow"
            } else {
                eval.eval_str("(widen)").expect("widen");
                "widen"
            }
        }
        _ => match rng.below(3) {
            0 => {
                eval.eval_str(
                    "(setq comment-end-can-be-escaped (null comment-end-can-be-escaped))",
                )
                .expect("escape");
                "comment-end-can-be-escaped"
            }
            1 => {
                eval.eval_str(
                    "(setq parse-sexp-lookup-properties (null parse-sexp-lookup-properties))",
                )
                .expect("lookup");
                "parse-sexp-lookup-properties"
            }
            _ => {
                eval.eval_str(
                    "(save-restriction (widen) (set-buffer-multibyte nil) (set-buffer-multibyte t))",
                )
                    .expect("multibyte");
                "set-buffer-multibyte"
            }
        },
    }
}

#[test]
fn cached_answers_equal_plain_scans_under_every_change() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    crate::test_utils::init_test_tracing();
    reset_parse_cache_stats();
    let mut rng = Rng(0x5851_f42d_4c95_7f2d);
    let mut queries = 0usize;
    let mut changes = std::collections::BTreeMap::<&'static str, usize>::new();
    for kind in 0..5 {
        for (round, geometry) in [(16, 0), (64, 8), (16, 128), (1000, 0)]
            .into_iter()
            .enumerate()
        {
            GEOMETRY_OVERRIDE.with(|cell| cell.set(Some(geometry)));
            let mut eval = crate::emacs_core::eval::Context::new();
            install_table(&mut eval, kind);
            eval.eval_str("(make-local-variable 'comment-end-can-be-escaped)")
                .expect("local");
            eval.eval_str(&format!(
                "(setq parse-sexp-lookup-properties {})",
                if round % 2 == 0 { "t" } else { "nil" }
            ))
            .expect("lookup");
            let text = random_text(&mut rng, 150 + 100 * round);
            eval.eval_str(&format!("(insert {text:?})")).expect("text");
            for _session in 0..30 {
                let begv = point_min(&eval);
                let zv = point_max(&eval);
                let from = begv + rng.below(zv + 1 - begv);
                let mut query = random_query(&mut rng, begv, zv, from);
                for step in 0..8 {
                    check(&mut eval, query, "query");
                    queries += 1;
                    // Chain (a later TO), repeat, or restart from the same
                    // FROM with other options.
                    let begv = point_min(&eval);
                    let zv = point_max(&eval);
                    match rng.below(4) {
                        0 => {}
                        1 | 2 => query.to = query.to + rng.below(zv + 1 - query.to),
                        _ => query = random_query(&mut rng, begv, zv, query.from),
                    }
                    if step % 3 == 2 {
                        let what = mutate(&mut eval, &mut rng);
                        *changes.entry(what).or_default() += 1;
                        // Positions may have moved: re-check the same query
                        // clamped to the new region, then go on.
                        let begv = point_min(&eval);
                        let zv = point_max(&eval);
                        query.from = query.from.clamp(begv, zv);
                        query.to = query.to.clamp(query.from, zv);
                        check(&mut eval, query, what);
                        queries += 1;
                    }
                }
            }
        }
    }
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
    let stats = parse_cache_stats();
    tracing::info!(queries, ?stats, ?changes, "parse cache fuzz coverage");
    assert_eq!(stats.mismatches, 0);
    assert!(queries > 5_000, "queries {queries}");
    assert!(stats.exact > 500, "exact answers: {stats:?}");
    assert!(stats.resumes > 500, "resumes: {stats:?}");
    assert!(
        stats.skipped_chars > 10_000,
        "resumes skipped too little: {stats:?}"
    );
    assert!(stats.recorded > 500, "recorded: {stats:?}");
    assert!(stats.short > 100, "short scans: {stats:?}");
    assert!(
        stats.descriptor_changes > 10,
        "descriptor changes: {stats:?}"
    );
    for what in [
        "insert",
        "delete",
        "put syntax-table",
        "remove syntax-table",
        "setcar property descriptor",
        "setcar table descriptor",
        "modify-syntax-entry",
        "narrow",
        "widen",
        "comment-end-can-be-escaped",
        "parse-sexp-lookup-properties",
        "set-buffer-multibyte",
    ] {
        assert!(
            changes.get(what).copied().unwrap_or(0) > 5,
            "{what}: {changes:?}"
        );
    }
}

/// Short misses must not prevent a longer run from serving short queries.
#[test]
fn short_queries_keep_long_run_resumes_and_exact_answers() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 128))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str("(insert (apply #'concat (make-list 160 \"(é)\\n\")))")
        .expect("text");
    let short = Query {
        from: 1,
        to: 65,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    };
    reset_parse_cache_stats();
    check(&mut eval, short, "fresh short scan");
    let stats = parse_cache_stats();
    assert_eq!(stats.queries, 1, "short miss counted once");
    assert_eq!(stats.short, 1);
    assert_eq!((stats.recorded, stats.resumes, stats.exact), (0, 0, 0));

    check(&mut eval, Query { to: 401, ..short }, "record long scan");
    check(&mut eval, short, "resume long run for short query");
    assert_eq!(parse_cache_stats().resumes, 1);
    let plain = answer(&mut eval, short, Value::NIL, ParseCacheMode::Off);
    let verified = answer(&mut eval, short, Value::NIL, ParseCacheMode::Verify);
    assert_eq!(verified, plain);
    let stats = parse_cache_stats();
    assert_eq!(stats.queries, 4, "each cache-enabled query counted once");
    assert_eq!(stats.exact, 1);
    assert_eq!(stats.verified, 1);
    assert_eq!(stats.mismatches, 0);
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// Colliding membership bits must survive removal of another run in the bucket.
#[test]
fn from_membership_survives_collisions_eviction_and_invalidation() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 128))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str("(insert (apply #'concat (make-list 1600 \"(a)\\n\")))")
        .expect("text");
    let colliding = (1..1024)
        .find(|from| from_filter_bit(*from) == from_filter_bit(0))
        .expect("a deliberate filter collision");
    let query = |from| Query {
        from: from + 1,
        to: from + 301,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    };
    check(&mut eval, query(0), "first bucket occupant");
    check(&mut eval, query(colliding), "colliding bucket occupant");
    let later: Vec<_> = (colliding + 700..)
        .filter(|from| from_filter_bit(*from) != from_filter_bit(0))
        .take(MAX_RUNS - 1)
        .collect();
    for &from in &later {
        check(&mut eval, query(from), "evict the first colliding run");
    }
    let probes: Vec<_> = [0, colliding].into_iter().chain(later).collect();
    let inspect = |eval: &crate::emacs_core::eval::Context| {
        eval.buffers
            .current_buffer()
            .expect("buffer")
            .with_syntax_parse_cache(|cache, _| {
                for &from in &probes {
                    assert_eq!(
                        cache.has_run_from(from),
                        cache.runs.iter().any(|run| run.key.from_char == from),
                        "membership at {from}"
                    );
                }
                assert!(!cache.has_run_from(0), "oldest run evicted");
                assert!(cache.has_run_from(colliding), "collision survivor kept");
            });
    };
    inspect(&eval);
    eval.eval_str(&format!("(goto-char {}) (insert \"!\")", colliding + 501))
        .expect("partial invalidation after the surviving run");
    inspect(&eval);
    eval.buffers
        .current_buffer()
        .expect("buffer")
        .with_syntax_parse_cache(|cache, _| {
            let from = colliding + 400;
            let mut key = cache.runs[0].key.clone();
            key.from_char = from;
            cache.store(key, Record::new(from, from, from), None);
            assert!(!cache.has_run_from(from), "empty recording not retained");
            assert!(
                cache.has_run_from(colliding),
                "empty removal preserves others"
            );
        });
    eval.eval_str("(erase-buffer)").expect("full invalidation");
    eval.buffers
        .current_buffer()
        .expect("buffer")
        .with_syntax_parse_cache(|cache, _| {
            for from in probes {
                assert!(!cache.has_run_from(from), "all runs invalidated");
            }
        });
    assert!(!SyntaxParseCache::default().has_run_from(colliding));
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// A short miss must leave edits pending until a real cache lookup drains them.
#[test]
fn short_misses_preserve_pending_text_and_property_invalidation() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 128))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str(
        "(setq parse-sexp-lookup-properties t)
         (insert (apply #'concat (make-list 160 \"(é)\\n\")))",
    )
    .expect("text");
    let long = Query {
        from: 1,
        to: 401,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    };
    let miss = Query {
        from: 257,
        to: 289,
        ..long
    };
    for (what, change, expected) in [
        (
            "text edit",
            "(goto-char 1) (insert \"(\")",
            Invalidation::From { byte: 0, char: 0 },
        ),
        (
            "syntax property",
            "(put-text-property 1 2 'syntax-table '(7))",
            Invalidation::From {
                byte: usize::MAX,
                char: 0,
            },
        ),
    ] {
        check(&mut eval, long, "record before mutation");
        eval.eval_str(change).expect(what);
        reset_parse_cache_stats();
        check(&mut eval, miss, what);
        assert_eq!(parse_cache_stats().short, 1);
        let invalidation = eval
            .buffers
            .current_buffer()
            .expect("buffer")
            .with_syntax_parse_cache(|_, invalidation| invalidation);
        assert_eq!(invalidation, expected, "short miss consumed {what}");
        check(&mut eval, Query { to: 65, ..long }, "mutated original FROM");
        assert_eq!(parse_cache_stats().mismatches, 0);
    }
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// The eligibility threshold uses characters and keeps OLDSTATE/options intact.
#[test]
fn short_query_gate_preserves_threshold_zero_span_and_oldstate_options() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 128))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str("(insert (apply #'concat (make-list 160 \"(é) ; note\\n\")))")
        .expect("text");
    let query = Query {
        from: 1,
        to: 1,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    };
    reset_parse_cache_stats();
    check(&mut eval, query, "empty scan");
    check(&mut eval, Query { to: 128, ..query }, "127 characters");
    assert_eq!(parse_cache_stats().recorded, 0);
    check(&mut eval, Query { to: 129, ..query }, "128 characters");
    assert_eq!(parse_cache_stats().recorded, 1);

    for (target_depth, stop_before, commentstop) in [
        (None, false, 0),
        (Some(0), true, 0),
        (Some(1), false, 1),
        (None, false, 2),
    ] {
        check(
            &mut eval,
            Query {
                from: 51,
                to: 80,
                target_depth,
                stop_before,
                commentstop,
                oldstate_from_parse: true,
            },
            "short scan with OLDSTATE and options",
        );
    }
    assert_eq!(parse_cache_stats().recorded, 1);
    assert_eq!(parse_cache_stats().mismatches, 0);
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// A buffer whose properties resolve through `category`, or through
/// `char-property-alias-alist`, is never cached (and still answers right).
#[test]
fn category_and_alias_properties_bypass_the_cache() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    crate::test_utils::init_test_tracing();
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 0))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    eval.eval_str(
        "(progn (setq parse-sexp-lookup-properties t)
                (insert \"a /* b */ (c) \\\"d\\\" e f g h i j k l m n o p\"))",
    )
    .expect("setup");
    let query = Query {
        from: 1,
        to: 30,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    };
    reset_parse_cache_stats();
    check(&mut eval, query, "plain buffer");
    check(&mut eval, query, "plain buffer again");
    assert_eq!(parse_cache_stats().exact, 1, "cached normally");

    eval.eval_str(
        "(progn (put 'my-cat 'syntax-table '(7)) (put-text-property 3 4 'category 'my-cat))",
    )
    .expect("category");
    reset_parse_cache_stats();
    check(&mut eval, query, "category");
    eval.eval_str("(put 'my-cat 'syntax-table '(1))")
        .expect("category plist");
    check(&mut eval, query, "category plist changed");
    assert_eq!(parse_cache_stats().bypassed, 2);

    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    eval.eval_str(
        "(progn (setq parse-sexp-lookup-properties t)
                (setq char-property-alias-alist '((syntax-table my-syntax)))
                (insert \"a /* b */ (c) \\\"d\\\" e f g h i j k l m n o p\")
                (put-text-property 3 4 'my-syntax '(7)))",
    )
    .expect("alias");
    reset_parse_cache_stats();
    check(&mut eval, query, "alias");
    assert_eq!(parse_cache_stats().bypassed, 1);
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// `verify` compares every cached answer with a plain scan and counts none
/// different.
#[test]
fn verify_mode_recomputes_cached_answers() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    crate::test_utils::init_test_tracing();
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 0))));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let text = random_text(&mut rng, 200);
    eval.eval_str(&format!("(insert {text:?})")).expect("text");
    let zv = point_max(&eval);
    reset_parse_cache_stats();
    for to in (1..=zv).step_by(7) {
        for _ in 0..2 {
            let roots = eval.save_specpdl_roots();
            let _ = answer(
                &mut eval,
                Query {
                    from: 1,
                    to,
                    target_depth: None,
                    stop_before: false,
                    commentstop: 0,
                    oldstate_from_parse: false,
                },
                Value::NIL,
                ParseCacheMode::Verify,
            );
            eval.restore_specpdl_roots(roots);
        }
    }
    let stats = parse_cache_stats();
    assert!(stats.verified > 50, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    assert_eq!(parse_cache_mismatches(), 0);
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

/// Every char-table mutation entry point moves `char_table_write_tick`: the
/// run key (and the flat ASCII classifiers, and P3.3's DFA context key) trust
/// it for syntax-table entries (P3.0 §3.10).
#[test]
fn every_char_table_mutation_moves_the_write_tick() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn (setq tick-table (make-syntax-table))
                (setq tick-parent (make-syntax-table))
                (setq tick-extra (make-char-table 'case-table)))",
    )
    .expect("tables");
    for (what, form) in [
        ("make-char-table", "(make-char-table 'syntax-table)"),
        ("make-syntax-table", "(make-syntax-table)"),
        ("copy-syntax-table", "(copy-syntax-table tick-table)"),
        ("aset", "(aset tick-table ?a '(1))"),
        ("aset non-ASCII", "(aset tick-table ?é '(2))"),
        (
            "set-char-table-range",
            "(set-char-table-range tick-table '(?b . ?z) '(3))",
        ),
        (
            "set-char-table-range t",
            "(set-char-table-range tick-table t '(0))",
        ),
        (
            "set-char-table-parent",
            "(set-char-table-parent tick-table tick-parent)",
        ),
        (
            "set-char-table-extra-slot",
            "(set-char-table-extra-slot tick-extra 0 'x)",
        ),
        ("fillarray", "(fillarray tick-table '(0))"),
        (
            "modify-syntax-entry",
            "(modify-syntax-entry ?c \".\" tick-table)",
        ),
        (
            "map-char-table writing",
            "(map-char-table (lambda (k v) (aset tick-table (if (consp k) (car k) k) '(1))) tick-parent)",
        ),
        ("optimize-char-table", "(optimize-char-table tick-table)"),
    ] {
        let before = crate::emacs_core::chartable::char_table_write_tick();
        eval.eval_str(form)
            .unwrap_or_else(|e| panic!("{what}: {e:?}"));
        let after = crate::emacs_core::chartable::char_table_write_tick();
        if what == "optimize-char-table" {
            // Optimizing folds uniform sub-tables into their common value: no
            // lookup changes, so it need not move the tick.
            continue;
        }
        assert_ne!(before, after, "{what} must move the char-table write tick");
    }
}

/// Record a query, change one thing the key holds, and ask again: the answer
/// must be the plain one, and must differ from the recorded one (so the
/// change mattered).
fn after_env_change(
    eval: &mut crate::emacs_core::eval::Context,
    query: Query,
    change: &str,
    what: &str,
) {
    let roots = eval.save_specpdl_roots();
    let oldstate = if query.oldstate_from_parse {
        let _guard = ParseCacheTestGuard::modes(ParseCacheMode::Off, canon::CanonMode::Off);
        let state = builtin_parse_partial_sexp_6(
            eval,
            Value::fixnum(1),
            Value::fixnum(query.from as i64),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        )
        .expect("oldstate");
        state
    } else {
        Value::NIL
    };
    eval.push_specpdl_root(oldstate);
    let before = answer(eval, query, oldstate, ParseCacheMode::On);
    let again = answer(eval, query, oldstate, ParseCacheMode::On);
    assert_eq!(before, again, "{what}: the second answer is cached");
    eval.eval_str(change).expect(what);
    let cached = answer(eval, query, oldstate, ParseCacheMode::On);
    let plain = answer(eval, query, oldstate, ParseCacheMode::Off);
    eval.restore_specpdl_roots(roots);
    assert_eq!(cached, plain, "{what}");
    assert_ne!(plain, before, "{what} must change the answer");
}

/// Each environment field of the run key, changed with nothing else.
#[test]
fn every_key_field_separates_runs() {
    let _guard = ParseCacheTestGuard::isolated(GEOMETRY_OVERRIDE.with(|cell| cell.get()));
    crate::test_utils::init_test_tracing();
    GEOMETRY_OVERRIDE.with(|cell| cell.set(Some((16, 0))));
    let query = |from, to, oldstate_from_parse| Query {
        from,
        to,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse,
    };

    // `parse-sexp-lookup-properties`: a property turns a quote into
    // punctuation.
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    eval.eval_str(
        "(progn (insert \"a \\\"b c\\\" d e f g h i j\")
                (put-text-property 3 4 'syntax-table '(1))
                (setq parse-sexp-lookup-properties t))",
    )
    .expect("setup");
    after_env_change(
        &mut eval,
        query(1, 6, false),
        "(setq parse-sexp-lookup-properties nil)",
        "parse-sexp-lookup-properties",
    );

    // `comment-end-can-be-escaped`: an escaped newline does not end a Lisp
    // comment.
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str(
        "(progn (insert \"; a \\\\\\nb c d e f\\n g\")
                (make-local-variable 'comment-end-can-be-escaped)
                (setq comment-end-can-be-escaped t))",
    )
    .expect("setup");
    after_env_change(
        &mut eval,
        query(1, 9, false),
        "(setq comment-end-can-be-escaped nil)",
        "comment-end-can-be-escaped",
    );

    // BEGV: a comment resumed after an end-first `*` closes with the `/` at
    // FROM, unless FROM is BEGV.
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    eval.eval_str("(insert \"/* a */ b c d e f g h\")")
        .expect("setup");
    after_env_change(
        &mut eval,
        query(7, 12, true),
        "(narrow-to-region 7 22)",
        "BEGV",
    );

    // The syntax table's identity: two tables, switched without allocating
    // or writing either.
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    eval.eval_str(
        "(progn (setq table-a (syntax-table))
                (setq table-b (copy-syntax-table table-a))
                (modify-syntax-entry ?a \"\\\"\" table-b)
                (insert \"x a y z w v u t s r q\"))",
    )
    .expect("setup");
    after_env_change(
        &mut eval,
        query(1, 12, false),
        "(set-syntax-table table-b)",
        "syntax table",
    );
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
}

// Append to syntax/tests/parse_cache.rs. Uses that module's existing Query,
// Rng, install_table, mutate, point_min/point_max, and print_value helpers.

/// Scalar test overrides owned by this test invocation, restored on unwind.
/// They contain no Lisp state and never serve as runtime thread-local caches.
struct ParseCacheTestGuard {
    mode: Option<ParseCacheMode>,
    canon: Option<canon::CanonMode>,
    geometry: Option<(usize, usize)>,
}

impl ParseCacheTestGuard {
    fn new(
        mode: Option<ParseCacheMode>,
        canonical: Option<canon::CanonMode>,
        geometry: Option<(usize, usize)>,
    ) -> Self {
        Self {
            mode: MODE_OVERRIDE.with(|cell| cell.replace(mode)),
            canon: canon::CANON_MODE_OVERRIDE.with(|cell| cell.replace(canonical)),
            geometry: GEOMETRY_OVERRIDE.with(|cell| cell.replace(geometry)),
        }
    }

    fn modes(mode: ParseCacheMode, canonical: canon::CanonMode) -> Self {
        let geometry = GEOMETRY_OVERRIDE.with(|cell| cell.get());
        Self::new(Some(mode), Some(canonical), geometry)
    }

    /// Cache setup scans stay plain; answer_value enables each path explicitly.
    fn isolated(geometry: Option<(usize, usize)>) -> Self {
        Self::new(
            Some(ParseCacheMode::Off),
            Some(canon::CanonMode::Off),
            geometry,
        )
    }
}

impl Drop for ParseCacheTestGuard {
    fn drop(&mut self) {
        MODE_OVERRIDE.with(|cell| cell.set(self.mode));
        canon::CANON_MODE_OVERRIDE.with(|cell| cell.set(self.canon));
        GEOMETRY_OVERRIDE.with(|cell| cell.set(self.geometry));
    }
}

fn plain_query(from: usize, to: usize) -> Query {
    Query {
        from,
        to,
        target_depth: None,
        stop_before: false,
        commentstop: 0,
        oldstate_from_parse: false,
    }
}
// ---------------------------------------------------------------------------
// L2: the canonical run
// ---------------------------------------------------------------------------

/// Lisp-like text: nested lists, strings, comments, escapes and quotes,
/// with top-level forms (so that `syntax-ppss`-style chains cross them).
fn random_lisp_text(rng: &mut Rng, forms: usize) -> String {
    const PIECES: &[&str] = &[
        "(",
        "(",
        "(",
        ")",
        ")",
        ")",
        " ",
        " ",
        "\n",
        "foo",
        "bar-baz",
        "'x",
        "`(a ,b)",
        "\"str\"",
        "\"a\\\"b\"",
        "; c\n",
        "?\\(",
        "?\\)",
        "#|x|#",
        "/* y */",
        "// z\n",
        "é",
        "[",
        "]",
        "\\",
        "{- w -}",
        "(* v *)",
        "!q!",
        "|p|",
        "# h\n",
    ];
    let mut out = String::new();
    for _ in 0..forms {
        out.push('(');
        for _ in 0..rng.below(40) + 1 {
            out.push_str(PIECES[rng.below(PIECES.len())]);
        }
        out.push_str(")\n");
    }
    out
}

/// `parse-partial-sexp` under MODE with L2 as given: the value and point.
fn answer_value(
    eval: &mut crate::emacs_core::eval::Context,
    query: Query,
    oldstate: Value,
    mode: ParseCacheMode,
    l2: canon::CanonMode,
) -> (Value, usize) {
    let _guard = ParseCacheTestGuard::modes(mode, l2);
    let result = builtin_parse_partial_sexp_6(
        eval,
        Value::fixnum(query.from as i64),
        Value::fixnum(query.to as i64),
        query.target_depth.map_or(Value::NIL, Value::fixnum),
        Value::bool_val(query.stop_before),
        oldstate,
        match query.commentstop {
            0 => Value::NIL,
            1 => Value::T,
            _ => Value::symbol("syntax-table"),
        },
    )
    .expect("parse-partial-sexp");
    let point = eval
        .buffers
        .current_buffer()
        .expect("buffer")
        .point_char_pos()
        .get()
        + 1;
    (result, point)
}

/// One query with an explicit OLDSTATE, through L1 + L2 and plainly: the two
/// must agree. Returns the plain answer, rooted by the caller's frame.
fn check_l2(
    eval: &mut crate::emacs_core::eval::Context,
    query: Query,
    oldstate: Value,
    what: &str,
) -> Value {
    let (cached, cached_point) = answer_value(
        eval,
        query,
        oldstate,
        ParseCacheMode::On,
        canon::CanonMode::On,
    );
    eval.push_specpdl_root(cached);
    let (plain, plain_point) = answer_value(
        eval,
        query,
        oldstate,
        ParseCacheMode::Off,
        canon::CanonMode::Off,
    );
    eval.push_specpdl_root(plain);
    assert_eq!(
        (print_value(&cached), cached_point),
        (print_value(&plain), plain_point),
        "{what}: {query:?} oldstate {}",
        print_value(&oldstate)
    );
    plain
}

/// The canonical state at FROM, as `syntax-ppss` would pass it: a plain
/// parse from BEGV (rooted by the caller's frame).
fn canonical_oldstate(eval: &mut crate::emacs_core::eval::Context, from: usize) -> Value {
    let begv = point_min(eval);
    let (state, _) = answer_value(
        eval,
        plain_query(begv, from),
        Value::NIL,
        ParseCacheMode::Off,
        canon::CanonMode::Off,
    );
    eval.push_specpdl_root(state);
    state
}

/// L2 answers exactly as a plain scan: `syntax-ppss`-shaped streams --
/// absolute queries, chains that start each query where the last one
/// stopped with its answer as OLDSTATE, queries from canonical states below,
/// inside and past the run's end, and OLDSTATEs that do not agree with the
/// run -- interleaved with every change a scan reads.
#[test]
fn canonical_answers_equal_plain_scans_under_every_change() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(None);
    reset_parse_cache_stats();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut queries = 0usize;
    let mut changes = std::collections::BTreeMap::<&'static str, usize>::new();
    for kind in 0..5 {
        for (round, geometry) in [(16, 0), (64, 0), (16, 64), (512, 0)]
            .into_iter()
            .enumerate()
        {
            GEOMETRY_OVERRIDE.with(|cell| cell.set(Some(geometry)));
            let mut eval = crate::emacs_core::eval::Context::new();
            install_table(&mut eval, kind);
            // modify-syntax-entry's class descriptor may be shared with ASCII
            // entries. Keep this fuzz's non-ASCII setcar mutation independent
            // of the inherited flat-ASCII descriptor mutation divergence.
            eval.eval_str(
                "(let ((table (syntax-table)) (chars '(?é ?ü)))
                   (while chars
                     (let* ((ch (car chars)) (descriptor (aref table ch)))
                       (set-char-table-range table ch
                         (cons (car descriptor) (cdr descriptor))))
                     (setq chars (cdr chars))))",
            )
            .expect("private non-ASCII descriptors");
            eval.eval_str("(make-local-variable 'comment-end-can-be-escaped)")
                .expect("local");
            eval.eval_str(&format!(
                "(setq parse-sexp-lookup-properties {})",
                if round % 2 == 0 { "t" } else { "nil" }
            ))
            .expect("lookup");
            let text = random_lisp_text(&mut rng, 20 + 10 * round);
            eval.eval_str(&format!("(insert {text:?})")).expect("text");
            for session in 0..24 {
                let roots = eval.save_specpdl_roots();
                let begv = point_min(&eval);
                let zv = point_max(&eval);
                match session % 4 {
                    // Absolute queries, rising and falling.
                    0 => {
                        for _ in 0..6 {
                            let to = begv + rng.below(zv + 1 - begv);
                            check_l2(&mut eval, plain_query(begv, to), Value::NIL, "absolute");
                            queries += 1;
                        }
                    }
                    // A chain: each query from the last one's TO and answer.
                    1 | 2 => {
                        let mut from = begv + rng.below(zv + 1 - begv);
                        let mut state = if from == begv {
                            Value::NIL
                        } else {
                            canonical_oldstate(&mut eval, from)
                        };
                        for _ in 0..8 {
                            let zv = point_max(&eval);
                            if from >= zv {
                                break;
                            }
                            let to = from + 1 + rng.below((zv - from).min(300));
                            state = check_l2(&mut eval, plain_query(from, to), state, "chain");
                            queries += 1;
                            from = to;
                        }
                    }
                    // Repeated FROMs with canonical OLDSTATEs and random TOs,
                    // and a random well-formed OLDSTATE that need not agree.
                    _ => {
                        let from = begv + rng.below(zv + 1 - begv);
                        let state = canonical_oldstate(&mut eval, from);
                        for _ in 0..4 {
                            let to = from + rng.below(zv + 1 - from);
                            check_l2(&mut eval, plain_query(from, to), state, "relative");
                            queries += 1;
                        }
                        let odd = eval
                            .eval_str(&format!(
                                "(list {} nil nil nil nil nil 0 nil nil nil nil)",
                                rng.below(3) as i64 - 1
                            ))
                            .expect("odd state");
                        eval.push_specpdl_root(odd);
                        let to = from + rng.below(zv + 1 - from);
                        check_l2(&mut eval, plain_query(from, to), odd, "odd oldstate");
                        queries += 1;
                    }
                }
                eval.restore_specpdl_roots(roots);
                if session % 3 == 2 {
                    let what = mutate(&mut eval, &mut rng);
                    *changes.entry(what).or_default() += 1;
                }
            }
        }
    }
    GEOMETRY_OVERRIDE.with(|cell| cell.set(None));
    let stats = parse_cache_stats();
    tracing::info!(queries, ?stats, ?changes, "canonical run fuzz coverage");
    assert_eq!(stats.mismatches, 0);
    assert!(queries > 2_000, "queries {queries}");
    assert!(stats.canon_absolute > 300, "absolute: {stats:?}");
    assert!(stats.canon_adopted > 500, "adopted: {stats:?}");
    assert!(stats.canon_declined > 20, "declined: {stats:?}");
    assert!(
        stats.canon_skipped_chars > 20_000,
        "skipped too little: {stats:?}"
    );
    for what in [
        "insert",
        "delete",
        "put syntax-table",
        "remove syntax-table",
        "setcar property descriptor",
        "modify-syntax-entry",
        "narrow",
    ] {
        assert!(
            changes.get(what).copied().unwrap_or(0) > 1,
            "{what}: {changes:?}"
        );
    }
    assert!(stats.canon_resets > 5, "environment changes: {stats:?}");
}

/// Deeply nested text, long spans and a small chunk: adopted queries jump
/// over many canonical states, and their minimum depth and per-level
/// positions come from the correction, not from a scan.
#[test]
fn adopted_answers_correct_depth_and_levels_across_jumps() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    // Rising and falling depth, unbalanced closes (negative depth), and
    // atoms and strings at every level.
    let text = "(a (b (c d) \"s\" e) (f (g (h i) j) k) l)\n)) (m (n) o\n\
                ((((p)))) q (r \"t\" (s)) u) v (w (x (y (z))))\n(1 (2 (3 (4 (5)))))";
    let text = text.repeat(6);
    eval.eval_str(&format!("(insert {text:?})")).expect("text");
    let zv = point_max(&eval);
    reset_parse_cache_stats();
    // Warm the run over the whole buffer.
    let roots = eval.save_specpdl_roots();
    check_l2(&mut eval, plain_query(1, zv), Value::NIL, "warm");
    for from in (2..zv).step_by(5) {
        let state = canonical_oldstate(&mut eval, from);
        for to in [from + 1, from + 7, from + 40, from + 150, zv] {
            if to <= zv {
                check_l2(&mut eval, plain_query(from, to), state, "jump");
            }
        }
    }
    eval.restore_specpdl_roots(roots);
    let stats = parse_cache_stats();
    assert_eq!(stats.mismatches, 0);
    assert!(stats.canon_adopted > 200, "{stats:?}");
    assert!(stats.canon_skipped_chars > 20_000, "{stats:?}");
}

/// A property-supplied syntax table is deliberately unvalidatable: setcar on
/// one of its entries changes syntax without the char-table write tick moving.
/// An L2 answer crossing it must never become an unchecked L1 exact result.
#[test]
fn canonical_exact_memo_rejects_property_syntax_tables() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    for adopted in [false, true] {
        let mut eval = crate::emacs_core::eval::Context::new();
        install_table(&mut eval, 1);
        let prefix = if adopted {
            "(a) ".repeat(20)
        } else {
            String::new()
        };
        let property_at = prefix.chars().count() + 1;
        let from = if adopted { property_at - 16 } else { 1 };
        let text = format!("{prefix}é{}", " ".repeat(16));
        eval.eval_str(&format!("(insert {text:?})")).expect("text");
        eval.eval_str(&format!(
            r#"(progn
                 (setq parse-sexp-lookup-properties t)
                 (setq l2-property-table (copy-syntax-table)
                       l2-property-descriptor (cons 4 ?\)))
                 (set-char-table-range l2-property-table ?é l2-property-descriptor)
                 (put-text-property {property_at} {} 'syntax-table l2-property-table))"#,
            property_at + 1,
        ))
        .expect("property syntax table");
        let roots = eval.save_specpdl_roots();
        reset_parse_cache_stats();
        if adopted {
            // The canonical run ends just before the unvalidatable table;
            // the adopted answer jumps through it and scans that table in its tail.
            check_l2(
                &mut eval,
                plain_query(1, property_at),
                Value::NIL,
                "warm prefix",
            );
        }
        let oldstate = if adopted {
            canonical_oldstate(&mut eval, from)
        } else {
            Value::NIL
        };
        let query = plain_query(from, point_max(&eval));
        let before = check_l2(&mut eval, query, oldstate, "warm property table");
        eval.eval_str("(setcar l2-property-descriptor 0)")
            .expect("mutate property table entry in place");
        let after = check_l2(&mut eval, query, oldstate, "changed property table");
        assert_ne!(
            print_value(&before),
            print_value(&after),
            "mutation must matter; adopted={adopted}"
        );
        let stats = parse_cache_stats();
        assert_eq!(
            stats.exact, 0,
            "unvalidatable syntax must not be memoized; adopted={adopted}: {stats:?}"
        );
        if adopted {
            assert_eq!(stats.canon_adopted, 2, "{stats:?}");
        } else {
            assert_eq!(stats.canon_absolute, 2, "{stats:?}");
        }
        eval.restore_specpdl_roots(roots);
    }
}

/// The descriptor after the dictionary's capacity remains mutable too. Its
/// absence from the validation dictionary must disable the full exact memo.
#[test]
fn canonical_exact_memo_rejects_descriptor_log_overflow() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    for existing_capacity in [false, true] {
        let mut eval = crate::emacs_core::eval::Context::new();
        let count = DESCRIPTOR_LOG_CAP + 1;
        eval.eval_str(&format!(
            "(progn
               (setq parse-sexp-lookup-properties t)
               (insert (make-string {count} ?x))
               (let ((i 0))
                 (while (< i {count})
                   (put-text-property (1+ i) (+ i 2) 'syntax-table (cons 0 nil))
                   (setq i (1+ i)))))"
        ))
        .expect("distinct property descriptors");
        let roots = eval.save_specpdl_roots();
        reset_parse_cache_stats();
        if existing_capacity {
            // Fill the canonical dictionary first. The extending scan logs only
            // the last known descriptor and the new one, below its local cap.
            check_l2(
                &mut eval,
                plain_query(1, count),
                Value::NIL,
                "fill dictionary",
            );
        }
        let query = plain_query(1, point_max(&eval));
        let before = check_l2(&mut eval, query, Value::NIL, "warm descriptor overflow");
        let absolutes_before = parse_cache_stats().canon_absolute;
        eval.eval_str(&format!(
            "(setcar (get-text-property {count} 'syntax-table) 4)"
        ))
        .expect("mutate first unlogged descriptor");
        let after = check_l2(&mut eval, query, Value::NIL, "changed unlogged descriptor");
        assert_ne!(
            print_value(&before),
            print_value(&after),
            "mutation must matter; existing_capacity={existing_capacity}"
        );
        let stats = parse_cache_stats();
        assert_eq!(
            stats.exact, 0,
            "overflow must not create an exact memo; existing_capacity={existing_capacity}: {stats:?}"
        );
        assert_eq!(stats.canon_absolute, absolutes_before + 1, "{stats:?}");
        eval.restore_specpdl_roots(roots);
    }
}

/// More than GNU's 100-level stack ceiling: this pins cache equivalence with
/// the current plain scanner, without claiming to fix its inherited GNU cap
/// divergence. All level positions and minima are compared across long jumps.
#[test]
fn canonical_adoption_matches_plain_above_one_hundred_levels() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    let text = format!(
        "{}{}{}",
        "(".repeat(128),
        "x ".repeat(160),
        ") ".repeat(140)
    );
    eval.eval_str(&format!("(insert {text:?})"))
        .expect("deep text");
    let zv = point_max(&eval);
    let roots = eval.save_specpdl_roots();
    reset_parse_cache_stats();
    check_l2(&mut eval, plain_query(1, zv), Value::NIL, "warm deep run");
    for from in [2, 64, 100, 101, 120, 129, 131, 201, 401] {
        let oldstate = canonical_oldstate(&mut eval, from);
        for to in [from + 1, from + 17, from + 80, zv - 1, zv] {
            if from < to && to <= zv {
                check_l2(&mut eval, plain_query(from, to), oldstate, "deep adoption");
            }
        }
    }
    let stats = parse_cache_stats();
    assert!(stats.canon_adopted > 0, "{stats:?}");
    assert!(stats.canon_skipped_chars > 100, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    eval.restore_specpdl_roots(roots);
}

/// Prefix closes establish a lower global minimum than the relative query
/// encounters. Extending an existing run must keep those prefix closes out of
/// the query's element 6 while preserving surviving level positions.
#[test]
fn canonical_extension_ignores_negative_minima_before_from() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    let prefix = format!("{}{}", ") ".repeat(20), "(".repeat(25));
    let from = prefix.chars().count() + 1;
    let text = format!("{prefix}{}{}", "x ".repeat(160), ") ".repeat(10));
    eval.eval_str(&format!("(insert {text:?})"))
        .expect("negative prefix");
    let zv = point_max(&eval);
    let roots = eval.save_specpdl_roots();
    reset_parse_cache_stats();
    check_l2(
        &mut eval,
        plain_query(1, from + 128),
        Value::NIL,
        "warm run below end",
    );
    let oldstate = canonical_oldstate(&mut eval, from);
    check_l2(
        &mut eval,
        plain_query(from, zv),
        oldstate,
        "extend past frontier",
    );
    for to in [from + 129, zv - 1, zv] {
        check_l2(
            &mut eval,
            plain_query(from, to),
            oldstate,
            "reuse extended minima",
        );
    }
    let stats = parse_cache_stats();
    assert!(stats.canon_adopted > 0, "{stats:?}");
    assert!(stats.canon_skipped_chars > 100, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    eval.restore_specpdl_roots(roots);
}

/// Current L1 records optioned queries too. L2 must leave their stop positions
/// and every state element to that path, even with a warm canonical run.
#[test]
fn canonical_run_leaves_optioned_queries_to_l1() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    eval.eval_str("(insert \"(a (b c) \\\"d\\\" ; e\\n f) (g)\")")
        .expect("option text");
    let zv = point_max(&eval);
    let roots = eval.save_specpdl_roots();
    reset_parse_cache_stats();
    check_l2(
        &mut eval,
        plain_query(1, zv),
        Value::NIL,
        "warm for options",
    );
    let before = parse_cache_stats();
    for from in [1, 4] {
        let oldstate = if from == 1 {
            Value::NIL
        } else {
            canonical_oldstate(&mut eval, from)
        };
        for (target_depth, stop_before, commentstop) in [
            (Some(0), false, 0),
            (Some(1), false, 0),
            (Some(2), false, 0),
            (None, true, 0),
            (None, false, 1),
            (None, false, 2),
        ] {
            let query = Query {
                target_depth,
                stop_before,
                commentstop,
                ..plain_query(from, zv)
            };
            check_l2(&mut eval, query, oldstate, "options first query");
            check_l2(&mut eval, query, oldstate, "options exact query");
        }
    }
    let after = parse_cache_stats();
    assert_eq!(after.canon_absolute, before.canon_absolute, "{after:?}");
    assert_eq!(after.canon_adopted, before.canon_adopted, "{after:?}");
    assert_eq!(after.canon_declined, before.canon_declined, "{after:?}");
    eval.restore_specpdl_roots(roots);
}

/// The L2 verify knob must recompute both absolute and adopted answers even
/// when L1 is merely On. The caller supplies a fresh rooted OLDSTATE each time.
#[test]
fn canonical_verify_mode_recomputes_absolute_and_adopted_answers() {
    crate::test_utils::init_test_tracing();
    let _guard = ParseCacheTestGuard::isolated(Some((16, 0)));
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 1);
    let text = "(a) ".repeat(180);
    eval.eval_str(&format!("(insert {text:?})"))
        .expect("verify text");
    let zv = point_max(&eval);
    let roots = eval.save_specpdl_roots();
    reset_parse_cache_stats();
    for from in [1, 17, 33] {
        let oldstate = if from == 1 {
            Value::NIL
        } else {
            canonical_oldstate(&mut eval, from)
        };
        let query = plain_query(from, zv);
        let (verified, verified_point) = answer_value(
            &mut eval,
            query,
            oldstate,
            ParseCacheMode::On,
            canon::CanonMode::Verify,
        );
        eval.push_specpdl_root(verified);
        let (plain, plain_point) = answer_value(
            &mut eval,
            query,
            oldstate,
            ParseCacheMode::Off,
            canon::CanonMode::Off,
        );
        eval.push_specpdl_root(plain);
        assert_eq!(
            (print_value(&verified), verified_point),
            (print_value(&plain), plain_point),
            "verify: {query:?}",
        );
    }
    let stats = parse_cache_stats();
    assert!(stats.canon_absolute > 0, "{stats:?}");
    assert!(stats.canon_adopted > 0, "{stats:?}");
    assert!(stats.verified >= 3, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    eval.restore_specpdl_roots(roots);
}

/// Compare every returned list field and point with both verification knobs
/// enabled. The existing caller-owned root frame owns the returned value.
fn check_live_verified(
    eval: &mut crate::emacs_core::eval::Context,
    query: Query,
    oldstate: Value,
    what: &str,
) -> Value {
    let (cached, cached_point) = answer_value(
        eval,
        query,
        oldstate,
        ParseCacheMode::Verify,
        canon::CanonMode::Verify,
    );
    eval.push_specpdl_root(cached);
    let (plain, plain_point) = answer_value(
        eval,
        query,
        oldstate,
        ParseCacheMode::Off,
        canon::CanonMode::Off,
    );
    eval.push_specpdl_root(plain);
    assert_eq!(
        (print_value(&cached), cached_point),
        (print_value(&plain), plain_point),
        "{what}: {query:?}, OLDSTATE {}",
        print_value(&oldstate),
    );
    plain
}

fn live_canonical_frontier(eval: &crate::emacs_core::eval::Context) -> usize {
    eval.buffers
        .current_buffer()
        .expect("buffer")
        .with_syntax_parse_cache(|cache, _| {
            cache.canonical.as_ref().expect("canonical run").frontier()
        })
}

/// The negative prefix gives the absolute scan a different minimum. Nested
/// opens preserve reported levels without creating completed parent atoms.
fn live_negative_prefix() -> String {
    format!("{}{}", ")".repeat(10), "(".repeat(10))
}

#[derive(Clone, Copy, Debug)]
enum LiveBoundary {
    String,
    Quoted,
    PendingAtom,
    ElementTen,
    ReportingLevels,
}

impl LiveBoundary {
    fn outer(self) -> &'static str {
        match self {
            Self::String => "(head \"quoted text\" tail)",
            Self::Quoted => "(head a\\(b after)",
            Self::PendingAtom | Self::ReportingLevels => "(head longatomname after)",
            Self::ElementTen => "(head /* comment */ after)",
        }
    }

    fn from(self, text: &str) -> usize {
        let (needle, offset) = match self {
            Self::String => ("quoted", 2),
            Self::Quoted => ("\\(", 1),
            Self::PendingAtom | Self::ReportingLevels => ("longatomname", 4),
            Self::ElementTen => ("/*", 1),
        };
        text[..text.find(needle).expect("FROM marker")]
            .chars()
            .count()
            + offset
            + 1
    }
}

/// String/escape/atom/pending two-character syntax cannot be adopted at FROM.
/// Actual parsing must consume that prefix and wait for all level metadata to
/// converge, while the relative query retains its own minimum depth.
#[test]
fn live_sync_waits_for_full_state_then_skips_with_verify() {
    crate::test_utils::init_test_tracing();
    for chunk in [1, 3, 16, 64, 2048] {
        let _guard = ParseCacheTestGuard::isolated(Some((chunk, 0)));
        for boundary in [
            LiveBoundary::String,
            LiveBoundary::Quoted,
            LiveBoundary::PendingAtom,
            LiveBoundary::ElementTen,
            LiveBoundary::ReportingLevels,
        ] {
            let mut eval = crate::emacs_core::eval::Context::new();
            install_table(
                &mut eval,
                if matches!(boundary, LiveBoundary::ElementTen) {
                    0
                } else {
                    1
                },
            );
            let text = format!(
                "{}{} (reset gate) {}",
                live_negative_prefix(),
                boundary.outer(),
                "(tail x) ".repeat(1400),
            );
            let from = boundary.from(&text);
            eval.eval_str(&format!("(insert {text:?})")).expect("text");
            let roots = eval.save_specpdl_roots();
            let zv = point_max(&eval);
            check_live_verified(&mut eval, plain_query(1, zv), Value::NIL, "warm");
            let mut oldstate = canonical_oldstate(&mut eval, from);
            if matches!(boundary, LiveBoundary::ReportingLevels) {
                // Same depth and number of reported levels, different inner
                // containing-position metadata. Child/outer completion can
                // eventually overwrite it, but it is not an agreement now.
                let mut fields = list_to_vec(&oldstate).expect("OLDSTATE list");
                let mut levels = list_to_vec(&fields[9]).expect("levels list");
                let last = levels.last_mut().expect("nested level");
                *last = Value::fixnum(last.as_fixnum().expect("level position") + 1);
                fields[9] = Value::list(levels);
                oldstate = Value::list(fields);
                eval.push_specpdl_root(oldstate);
                reset_parse_cache_stats();
                check_live_verified(
                    &mut eval,
                    plain_query(from, from + 1),
                    oldstate,
                    "different reporting levels before completion",
                );
                assert_eq!(parse_cache_stats().canon_synced, 0);
            }
            reset_parse_cache_stats();
            let query = plain_query(from, zv);
            let first = check_live_verified(&mut eval, query, oldstate, "live boundary");
            let stats = parse_cache_stats();
            assert!(
                stats.canon_synced > 0,
                "chunk={chunk}, {boundary:?}: {stats:?}"
            );
            assert!(
                stats.canon_skipped_chars > 0,
                "chunk={chunk}, {boundary:?}: {stats:?}"
            );
            assert!(stats.verified > 0, "{stats:?}");
            assert_eq!(stats.mismatches, 0, "{stats:?}");
            let again = check_live_verified(&mut eval, query, oldstate, "live exact repeat");
            assert_eq!(print_value(&first), print_value(&again));
            assert!(parse_cache_stats().exact > 0, "chunk={chunk}, {boundary:?}");
            eval.restore_specpdl_roots(roots);
        }
    }
}

/// Start with a tiny canonical prefix ending inside a string. Each subsequent
/// query carries the prior answer as OLDSTATE, extends the canonical frontier,
/// and requires live agreement without a new absolute warm-up.
#[test]
fn live_sync_grows_partial_frontier_for_oldstate_string_chains() {
    crate::test_utils::init_test_tracing();
    for chunk in [1, 3, 16, 64, 2048] {
        let _guard = ParseCacheTestGuard::isolated(Some((chunk, 0)));
        let mut eval = crate::emacs_core::eval::Context::new();
        install_table(&mut eval, 1);
        let prefix = live_negative_prefix();
        let prefix_len = prefix.chars().count();
        let text = format!("{prefix}{}", "(\"a\") ".repeat(3000));
        eval.eval_str(&format!("(insert {text:?})"))
            .expect("chain text");
        let roots = eval.save_specpdl_roots();
        // In each six-character form, TO is the closing quote position, so
        // the returned state is still inside the string.
        let from = prefix_len + 4;
        let middle = prefix_len + 6 * 1000 + 4;
        let zv = point_max(&eval);
        let state = check_live_verified(
            &mut eval,
            plain_query(1, from),
            Value::NIL,
            "tiny partial warm",
        );
        assert!(!list_to_vec(&state).expect("state")[3].is_nil());
        let first_frontier = live_canonical_frontier(&eval);
        assert!(first_frontier < from, "only a partial warm");
        reset_parse_cache_stats();
        let next = check_live_verified(
            &mut eval,
            plain_query(from, middle),
            state,
            "first string chain",
        );
        let second_frontier = live_canonical_frontier(&eval);
        let stats = parse_cache_stats();
        assert!(
            stats.canon_synced > 0,
            "first chain, chunk={chunk}: {stats:?}"
        );
        assert!(
            second_frontier > first_frontier,
            "first chain, chunk={chunk}"
        );
        assert_eq!(stats.canon_absolute, 0, "chain must not reseed: {stats:?}");
        assert!(!list_to_vec(&next).expect("state")[3].is_nil());
        reset_parse_cache_stats();
        check_live_verified(
            &mut eval,
            plain_query(middle, zv),
            next,
            "second string chain",
        );
        let stats = parse_cache_stats();
        assert!(
            stats.canon_synced > 0,
            "second chain, chunk={chunk}: {stats:?}"
        );
        assert!(live_canonical_frontier(&eval) > second_frontier);
        assert_eq!(stats.canon_absolute, 0, "chain must not reseed: {stats:?}");
        assert_eq!(stats.mismatches, 0, "{stats:?}");
        eval.restore_specpdl_roots(roots);
    }
}

/// Canonical escapes do not classify their quoted bodies. A supplied comment
/// OLDSTATE with escaping disabled does classify them, so these descriptors
/// exist exclusively in the actual prefix's dependency log.
fn live_descriptor_fixture(
    eval: &mut crate::emacs_core::eval::Context,
    prefix_descriptors: usize,
    skipped_descriptors: usize,
) -> (usize, Vec<usize>, Vec<usize>) {
    install_table(eval, 1);
    let head = format!("{}(head ", live_negative_prefix());
    let from = head.chars().count() + 1;
    let actual = "\\é".repeat(prefix_descriptors);
    let first_tail = "(tail x) ".repeat(700);
    let skipped = "é ".repeat(skipped_descriptors);
    let second_tail = "(tail x) ".repeat(700);
    let text = format!("{head}{actual}\n(reset gate)) {first_tail}{skipped}{second_tail}");
    let prefix_positions: Vec<_> = (0..prefix_descriptors).map(|i| from + 2 * i + 1).collect();
    let skipped_from = text[..text
        .find(&format!("{skipped}{second_tail}"))
        .expect("tail marker")]
        .chars()
        .count()
        + 1;
    // An empty skipped block would find the identical first tail early; its
    // start is unused in that case, so only nonempty blocks need the marker.
    let skipped_positions: Vec<_> = (0..skipped_descriptors)
        .map(|i| skipped_from + 2 * i)
        .collect();
    eval.eval_str(&format!("(insert {text:?})"))
        .expect("dependency text");
    eval.eval_str("(setq parse-sexp-lookup-properties t comment-end-can-be-escaped nil)")
        .expect("property and comment policy");
    for &at in prefix_positions.iter().chain(&skipped_positions) {
        eval.eval_str(&format!(
            "(put-text-property {at} {} 'syntax-table (cons 0 nil))",
            at + 1,
        ))
        .expect("fresh property descriptor");
    }
    (from, prefix_positions, skipped_positions)
}

fn live_comment_oldstate(eval: &mut crate::emacs_core::eval::Context, from: usize) -> Value {
    let canonical = canonical_oldstate(eval, from);
    let mut fields = list_to_vec(&canonical).expect("OLDSTATE list");
    fields[4] = Value::T;
    fields[7] = Value::NIL;
    fields[8] = Value::fixnum(from as i64);
    let state = Value::list(fields);
    eval.push_specpdl_root(state);
    state
}

/// The result's dependency union can exceed capacity even when the actual
/// prefix and canonical dictionary are each individually below capacity.
/// Neither that case nor actual-prefix overflow may produce an exact memo.
#[test]
fn live_sync_rejects_exact_memos_for_prefix_and_union_descriptor_overflow() {
    crate::test_utils::init_test_tracing();
    for chunk in [1, 3, 16, 64, 2048] {
        let _guard = ParseCacheTestGuard::isolated(Some((chunk, 0)));
        for (prefix_count, skipped_count) in [(DESCRIPTOR_LOG_CAP + 1, 0), (20, 13)] {
            let mut eval = crate::emacs_core::eval::Context::new();
            let (from, prefix_positions, skipped_positions) =
                live_descriptor_fixture(&mut eval, prefix_count, skipped_count);
            let roots = eval.save_specpdl_roots();
            let zv = point_max(&eval);
            check_live_verified(
                &mut eval,
                plain_query(1, zv),
                Value::NIL,
                "warm descriptors",
            );
            let oldstate = live_comment_oldstate(&mut eval, from);
            let query = plain_query(from, zv);
            reset_parse_cache_stats();
            let first = check_live_verified(&mut eval, query, oldstate, "overflow live prefix");
            assert!(
                parse_cache_stats().canon_synced > 0,
                "chunk={chunk}, prefix={prefix_count}, skipped={skipped_count}: {:?}",
                parse_cache_stats(),
            );
            check_live_verified(&mut eval, query, oldstate, "overflow repeat");
            assert_eq!(
                parse_cache_stats().exact,
                0,
                "overflow must not yield exact memo"
            );
            if let Some(&at) = skipped_positions.last() {
                // This descriptor is skipped by the actual query after the
                // sync point. Making it an unmatched opening delimiter must
                // change the full answer and invalidate the canonical suffix.
                eval.eval_str(&format!(
                    "(setcar (get-text-property {at} 'syntax-table) 4)",
                ))
                .expect("mutate skipped canonical dependency");
                let changed = check_live_verified(
                    &mut eval,
                    query,
                    oldstate,
                    "mutated skipped descriptor after union overflow",
                );
                assert_ne!(print_value(&first), print_value(&changed));
                assert_eq!(parse_cache_stats().exact, 0);
                assert!(parse_cache_stats().descriptor_changes > 0);
            } else {
                // Mutate the first descriptor that did not fit the actual
                // prefix's dictionary. A shorter query exposes the resulting
                // comment exit before later forms overwrite the report state.
                let at = *prefix_positions.last().expect("overflow descriptor");
                let short = plain_query(from, at + 1);
                let before =
                    check_live_verified(&mut eval, short, oldstate, "prefix before mutation");
                eval.eval_str(&format!(
                    "(setcar (get-text-property {at} 'syntax-table) 12)",
                ))
                .expect("mutate unlogged actual dependency");
                let after =
                    check_live_verified(&mut eval, short, oldstate, "prefix after mutation");
                assert_ne!(print_value(&before), print_value(&after));
                check_live_verified(
                    &mut eval,
                    query,
                    oldstate,
                    "full answer after prefix mutation",
                );
                assert_eq!(parse_cache_stats().exact, 0);
            }
            assert_eq!(parse_cache_stats().mismatches, 0);
            eval.restore_specpdl_roots(roots);
        }
    }
}

/// Below capacity, both kinds of dependencies must remain validated when the
/// first synced result becomes an L1 exact memo. In-place cons mutation moves
/// no syntax tick, so the descriptor dictionary must detect it explicitly.
#[test]
fn live_sync_exact_memo_validates_actual_and_skipped_dependencies() {
    crate::test_utils::init_test_tracing();
    for chunk in [1, 3, 16, 64, 2048] {
        let _guard = ParseCacheTestGuard::isolated(Some((chunk, 0)));
        for mutate_prefix in [true, false] {
            let mut eval = crate::emacs_core::eval::Context::new();
            let (from, prefix_positions, skipped_positions) =
                live_descriptor_fixture(&mut eval, 1, 1);
            let roots = eval.save_specpdl_roots();
            let zv = point_max(&eval);
            check_live_verified(
                &mut eval,
                plain_query(1, zv),
                Value::NIL,
                "warm small dictionaries",
            );
            let oldstate = live_comment_oldstate(&mut eval, from);
            let query = plain_query(from, zv);
            reset_parse_cache_stats();
            let before = check_live_verified(&mut eval, query, oldstate, "record dependency union");
            assert!(
                parse_cache_stats().canon_synced > 0,
                "chunk={chunk}: {:?}",
                parse_cache_stats()
            );
            check_live_verified(&mut eval, query, oldstate, "exact dependency union");
            assert!(parse_cache_stats().exact > 0);
            let at = if mutate_prefix {
                prefix_positions[0]
            } else {
                skipped_positions[0]
            };
            let class = if mutate_prefix { 12 } else { 4 };
            eval.eval_str(&format!(
                "(setcar (get-text-property {at} 'syntax-table) {class})",
            ))
            .expect("mutate descriptor without a tick");
            let exact_before = parse_cache_stats().exact;
            let after =
                check_live_verified(&mut eval, query, oldstate, "changed exact dependency union");
            assert_eq!(
                parse_cache_stats().exact,
                exact_before,
                "old exact result rejected"
            );
            assert!(parse_cache_stats().descriptor_changes > 0);
            if !mutate_prefix {
                assert_ne!(print_value(&before), print_value(&after));
            } else {
                // A full answer can converge again; this shorter answer pins
                // the actual-only descriptor's observable effect explicitly.
                let short = plain_query(from, at + 1);
                let changed =
                    check_live_verified(&mut eval, short, oldstate, "changed short prefix");
                assert!(list_to_vec(&changed).expect("short state")[4].is_nil());
            }
            assert_eq!(parse_cache_stats().mismatches, 0);
            eval.restore_specpdl_roots(roots);
        }
    }
}
