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
    MODE_OVERRIDE.with(|cell| cell.set(Some(mode)));
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
    MODE_OVERRIDE.with(|cell| cell.set(None));
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
        MODE_OVERRIDE.with(|cell| cell.set(Some(ParseCacheMode::Off)));
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
        MODE_OVERRIDE.with(|cell| cell.set(None));
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

/// A buffer whose properties resolve through `category`, or through
/// `char-property-alias-alist`, is never cached (and still answers right).
#[test]
fn category_and_alias_properties_bypass_the_cache() {
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
        MODE_OVERRIDE.with(|cell| cell.set(Some(ParseCacheMode::Off)));
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
        MODE_OVERRIDE.with(|cell| cell.set(None));
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
