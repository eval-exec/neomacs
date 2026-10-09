//! S6 canonical reuse preserves the existing plain-BEGV scanner's answers.
//!
//! Register this standalone module in `parse_cache.rs`, whose private `canon`
//! child and test overrides it uses. Expected values come from scans with both
//! cache layers off and the legacy safe-position index bypassed. These are
//! differential cache tests; they do not assert independent GNU expectations.

use super::*;
use crate::emacs_core::print::print_value;

type Context = crate::emacs_core::eval::Context;

/// Restore every thread-local test override even when an assertion unwinds.
struct Overrides {
    l1: Option<ParseCacheMode>,
    l2: Option<canon::CanonMode>,
    geometry: Option<(usize, usize)>,
    safe_bypass: bool,
}

impl Overrides {
    fn new(l1: ParseCacheMode, l2: canon::CanonMode, chunk: usize, bypass: bool) -> Self {
        Self {
            l1: MODE_OVERRIDE.with(|cell| cell.replace(Some(l1))),
            l2: canon::CANON_MODE_OVERRIDE.with(|cell| cell.replace(Some(l2))),
            geometry: GEOMETRY_OVERRIDE.with(|cell| cell.replace(Some((chunk, 0)))),
            safe_bypass: BACK_COMMENT_SAFE_BYPASS.with(|cell| cell.replace(bypass)),
        }
    }
}

impl Drop for Overrides {
    fn drop(&mut self) {
        MODE_OVERRIDE.with(|cell| cell.set(self.l1));
        canon::CANON_MODE_OVERRIDE.with(|cell| cell.set(self.l2));
        GEOMETRY_OVERRIDE.with(|cell| cell.set(self.geometry));
        BACK_COMMENT_SAFE_BYPASS.with(|cell| cell.set(self.safe_bypass));
    }
}

#[derive(Clone, Copy, Debug)]
enum Dialect {
    C,
    Smie,
    Nested,
    Fences,
    Overlapping,
    StyleC,
    StyleBc,
}

const DIALECTS: &[Dialect] = &[
    Dialect::C,
    Dialect::Smie,
    Dialect::Nested,
    Dialect::Fences,
    Dialect::Overlapping,
    Dialect::StyleC,
    Dialect::StyleBc,
];

fn modify(eval: &mut Context, ch: char, descriptor: &str) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum(ch as i64), Value::string(descriptor)],
    )
    .expect("modify-syntax-entry");
}

fn install_table(eval: &mut Context, dialect: Dialect) {
    eval.eval_str("(set-syntax-table (copy-syntax-table))")
        .expect("own syntax table");
    eval.eval_str("(setq parse-sexp-ignore-comments t)")
        .expect("scan-sexps must use its backward comment scanner");
    for ch in [
        '/', '*', '\\', '\'', '"', '!', '#', '%', '(', ')', '{', '-', '}', '|', '\n',
    ] {
        modify(eval, ch, ".");
    }
    modify(eval, '\\', "\\");
    modify(eval, '"', "\"");
    modify(eval, '\'', "\"");
    modify(eval, 'é', "w");
    modify(eval, 'ü', "_");
    // Bare syntax descriptors are canonical conses shared with ASCII entries.
    // Keep the non-ASCII setcar probes independent of that inherited flat-ASCII
    // descriptor-mutation divergence, and of later contexts on this thread.
    eval.eval_str(
        "(let ((table (syntax-table)) (chars '(?é ?ü)))
           (while chars
             (let* ((ch (car chars)) (descriptor (aref table ch)))
               (set-char-table-range table ch
                 (cons (car descriptor) (cdr descriptor))))
             (setq chars (cdr chars))))",
    )
    .expect("private non-ASCII descriptors");
    match dialect {
        Dialect::C => {
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
            modify(eval, '\n', "> b");
        }
        Dialect::Smie => {
            modify(eval, '/', ". 124");
            modify(eval, '*', ". 23b");
            modify(eval, '\n', ">");
        }
        Dialect::Nested => {
            modify(eval, '(', "()1n");
            modify(eval, ')', ")(4n");
            modify(eval, '*', ". 23n");
            modify(eval, '{', "(}1n");
            modify(eval, '}', "){4n");
            // A distinct nested style, interleaved with the style-a markers.
            modify(eval, '-', ". 23bn");
        }
        Dialect::Fences => {
            modify(eval, '!', "!");
            modify(eval, '|', "|");
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
        }
        Dialect::Overlapping => {
            modify(eval, '#', "< 3");
            modify(eval, '%', ". 4");
            modify(eval, '-', ". 1234");
            modify(eval, '\n', ">");
        }
        Dialect::StyleC => {
            modify(eval, '/', ". 124c");
            modify(eval, '*', ". 23");
            modify(eval, '\n', "> c");
        }
        Dialect::StyleBc => {
            modify(eval, '/', ". 124bc");
            modify(eval, '*', ". 23b");
            modify(eval, '\n', "> bc");
        }
    }
}

fn set_text(eval: &mut Context, text: &str) {
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.widen();
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert(text);
}

fn bounds(eval: &Context) -> (usize, usize) {
    let buf = eval.buffers.current_buffer().expect("current buffer");
    (
        buf.point_min_char_pos().get() + 1,
        buf.point_max_char_pos().get() + 1,
    )
}

/// Seed from actual absolute parse queries, including the run's near-TO tops.
fn seed_canonical(eval: &mut Context, chunk: usize) {
    let _overrides = Overrides::new(ParseCacheMode::On, canon::CanonMode::On, chunk, false);
    let (begv, zv) = bounds(eval);
    for to in [begv + (zv - begv) / 3, begv + (zv - begv) * 2 / 3, zv] {
        builtin_parse_partial_sexp_6(
            eval,
            Value::fixnum(begv as i64),
            Value::fixnum(to as i64),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        )
        .expect("seed canonical parse");
    }
}

#[derive(Clone, Copy, Debug)]
enum Query {
    BackwardComment,
    BackwardSexps,
}

#[derive(Clone, Copy, Debug)]
enum Configuration {
    Plain,
    Cached,
    VerifyL1,
    VerifyL2,
}

fn answer(
    eval: &mut Context,
    pos: usize,
    query: Query,
    configuration: Configuration,
    chunk: usize,
) -> (String, usize) {
    let (l1, l2, bypass) = match configuration {
        Configuration::Plain => (ParseCacheMode::Off, canon::CanonMode::Off, true),
        Configuration::Cached => (ParseCacheMode::On, canon::CanonMode::On, false),
        Configuration::VerifyL1 => (ParseCacheMode::Verify, canon::CanonMode::On, false),
        Configuration::VerifyL2 => (ParseCacheMode::On, canon::CanonMode::Verify, false),
    };
    let _overrides = Overrides::new(l1, l2, chunk, bypass);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        let byte = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(pos - 1));
        buf.goto_emacs_byte_pos(byte);
    }
    // Compare errors too: arbitrary positions can produce scan-error.
    let source = match query {
        Query::BackwardComment => {
            "(condition-case err (forward-comment -1) (error (cons 'error err)))".to_owned()
        }
        Query::BackwardSexps => {
            format!("(condition-case err (scan-sexps {pos} -1) (error (cons 'error err)))")
        }
    };
    let value = eval
        .eval_str(&source)
        .expect("scanner result or caught error");
    let printed = print_value(&value);
    let point = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get()
        + 1;
    (printed, point)
}

fn check(
    eval: &mut Context,
    pos: usize,
    query: Query,
    configuration: Configuration,
    chunk: usize,
    context: &str,
) {
    let plain = answer(eval, pos, query, Configuration::Plain, chunk);
    let cached = answer(eval, pos, query, configuration, chunk);
    assert_eq!(
        cached, plain,
        "{context}: {query:?} at {pos}, {configuration:?}, chunk {chunk}"
    );
}

fn check_every_position(eval: &mut Context, chunk: usize, context: &str) -> usize {
    let (begv, zv) = bounds(eval);
    for pos in begv..=zv {
        for query in [Query::BackwardComment, Query::BackwardSexps] {
            check(eval, pos, query, Configuration::Cached, chunk, context);
        }
    }
    (zv - begv + 1) * 2
}

/// Mixed strings, comment styles, nesting, fences, overlaps and escaped ends.
/// Every position includes the middle of each two-character delimiter.
#[test]
fn canonical_back_comments_match_plain_scans_across_dialects() {
    crate::test_utils::init_test_tracing();
    reset_parse_cache_stats();
    let samples = [
        "é ü a a /* it's */ // don't\n /* a ' b \\\" c */ ",
        "é a (* outer ' (* inner *) tail *) {- x ' (* y *) -} ",
        "a ! fence ' \\! x ! | string \\| x | /* it's */ ",
        "a # it's #% -- c -- --- c -- ------ c --\n",
        "a /* ' \\*/ x */ /\"*\"/ /*it's*/ /* ' \\\\*/ ",
        "\" { \" a { \" } { a (* \" *) |*| *||* /// /* it's */ ",
    ];
    let mut queries = 0;
    for &dialect in DIALECTS {
        for escapable in [false, true] {
            for chunk in [1, 2, 3, 64] {
                let mut eval = Context::new();
                install_table(&mut eval, dialect);
                eval.eval_str("(make-local-variable 'comment-end-can-be-escaped)")
                    .expect("local escape policy");
                eval.eval_str(&format!(
                    "(setq comment-end-can-be-escaped {})",
                    if escapable { "t" } else { "nil" }
                ))
                .expect("escape policy");
                for sample in samples {
                    set_text(&mut eval, &format!("{}{}", "a é ".repeat(20), sample));
                    seed_canonical(&mut eval, chunk);
                    queries += check_every_position(
                        &mut eval,
                        chunk,
                        &format!("{dialect:?}, escapable {escapable}, {sample:?}"),
                    );
                }
            }
        }
    }
    let stats = parse_cache_stats();
    tracing::info!(queries, ?stats, "canonical back-comment dialect coverage");
    assert!(queries > 10_000, "too few scanner queries: {queries}");
    assert!(
        stats.canon_back_comments > 100,
        "canonical helper unused: {stats:?}"
    );
    assert_eq!(stats.mismatches, 0, "{stats:?}");
}

/// Reset statistics after seeding: skipped characters must come from the
/// backward calls themselves, including the second call to the same endpoint.
#[test]
fn repeated_back_comments_reuse_canonical_states_in_both_verify_modes() {
    crate::test_utils::init_test_tracing();
    for configuration in [
        Configuration::Cached,
        Configuration::VerifyL1,
        Configuration::VerifyL2,
    ] {
        let mut eval = Context::new();
        install_table(&mut eval, Dialect::C);
        set_text(&mut eval, &format!("{}/* it's */", "a é ü ".repeat(300)));
        seed_canonical(&mut eval, 16);
        reset_parse_cache_stats();
        let (_, zv) = bounds(&eval);
        for query in [Query::BackwardComment, Query::BackwardSexps] {
            check(
                &mut eval,
                zv,
                query,
                configuration,
                16,
                "first backward query",
            );
            let before = parse_cache_stats();
            check(
                &mut eval,
                zv,
                query,
                configuration,
                16,
                "repeated backward query",
            );
            let after = parse_cache_stats();
            assert!(
                after.canon_back_comments > before.canon_back_comments,
                "repeat did not reach canonical back-comment helper: {configuration:?}, {query:?}, {before:?} -> {after:?}"
            );
            assert!(
                after.canon_skipped_chars > before.canon_skipped_chars,
                "repeat skipped no canonical prefix: {configuration:?}, {query:?}, {before:?} -> {after:?}"
            );
            assert_eq!(after.mismatches, 0, "{after:?}");
        }
    }
}

fn partial_parse_answer(
    eval: &mut Context,
    from: usize,
    to: usize,
    oldstate: Value,
    configuration: Configuration,
    chunk: usize,
) -> (String, usize) {
    let (l1, l2) = match configuration {
        Configuration::Plain => (ParseCacheMode::Off, canon::CanonMode::Off),
        Configuration::Cached => (ParseCacheMode::On, canon::CanonMode::On),
        Configuration::VerifyL1 => (ParseCacheMode::Verify, canon::CanonMode::On),
        Configuration::VerifyL2 => (ParseCacheMode::On, canon::CanonMode::Verify),
    };
    let _overrides = Overrides::new(l1, l2, chunk, false);
    let value = builtin_parse_partial_sexp_6(
        eval,
        Value::fixnum(from as i64),
        Value::fixnum(to as i64),
        Value::NIL,
        Value::NIL,
        oldstate,
        Value::NIL,
    )
    .expect("relative parse after back-comment extension");
    let point = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get()
        + 1;
    (print_value(&value), point)
}

fn canonical_snapshots(eval: &Context) -> Vec<(LoopState, i64)> {
    eval.buffers
        .current_buffer()
        .expect("current buffer")
        .with_syntax_parse_cache(|cache, _| {
            cache
                .canonical
                .as_ref()
                .expect("a seeded canonical run")
                .snaps
                .iter()
                .map(|snap| (snap.state.clone(), snap.closes_min))
                .collect()
        })
}

/// A backward lossage parse inside the old frontier must not publish segment
/// minima; one beyond it must begin its new segments exactly at that frontier.
/// Negative depth before FROM must not leak into the later relative answer.
#[test]
fn canonical_back_comment_extension_preserves_segment_minima_for_adoption() {
    crate::test_utils::init_test_tracing();
    for configuration in [
        Configuration::Cached,
        Configuration::VerifyL1,
        Configuration::VerifyL2,
    ] {
        for chunk in [1, 3, 16, 64] {
            let mut eval = Context::new();
            install_table(&mut eval, Dialect::C);
            modify(&mut eval, '(', "()");
            modify(&mut eval, ')', ")(");
            let prefix = format!("{}{}", ")".repeat(20), "(".repeat(25));
            // Even chunk 64 has a recorded state before this first comment.
            let early = format!("{} /* early ' */ ", "a ".repeat(40));
            let initial_closes = ") ".repeat(3);
            let before_late = format!("{}{}{}", "a ".repeat(80), ") ".repeat(2), "a ".repeat(80));
            let late = "/* it's */";
            let text = format!(
                "{prefix}{early}{initial_closes}{before_late}{late}{}",
                ") ".repeat(10)
            );
            set_text(&mut eval, &text);
            let (begv, zv) = bounds(&eval);
            let from = begv + prefix.chars().count();
            let early_end = from + early.trim_end().chars().count();
            let warm_to = from + early.chars().count() + initial_closes.chars().count() + 64;
            let late_end = begv + text.chars().count() - 20;
            partial_parse_answer(
                &mut eval,
                begv,
                warm_to,
                Value::NIL,
                Configuration::Cached,
                chunk,
            );
            let before = canonical_snapshots(&eval);
            let frontier = before.last().expect("nonempty partial seed").0.char_pos;
            assert!(frontier < late_end - 3, "late lossage must extend the run");
            assert!(
                before.iter().any(|(_, minimum)| *minimum < 0),
                "seed must contain the pre-FROM negative minima"
            );
            reset_parse_cache_stats();
            check(
                &mut eval,
                early_end,
                Query::BackwardComment,
                configuration,
                chunk,
                "lossage inside the existing frontier",
            );
            assert_eq!(
                canonical_snapshots(&eval),
                before,
                "an inner plain tail must not republish old segments"
            );
            let within = parse_cache_stats();
            assert!(within.canon_back_comments > 0, "{within:?}");
            check(
                &mut eval,
                late_end,
                Query::BackwardComment,
                configuration,
                chunk,
                "lossage beyond the existing frontier",
            );
            let extended = canonical_snapshots(&eval);
            let after = parse_cache_stats();
            assert!(
                after.canon_back_comments > within.canon_back_comments,
                "{after:?}"
            );
            assert!(extended.last().expect("extended run").0.char_pos > frontier);
            assert!(extended.len() > before.len());
            assert_eq!(&extended[..before.len()], before.as_slice());
            assert!(
                extended[before.len()..]
                    .iter()
                    .all(|(_, minimum)| *minimum >= 0),
                "new segment minima must exclude closes below the old frontier"
            );

            let roots = eval.save_specpdl_roots();
            let oldstate = {
                let _overrides =
                    Overrides::new(ParseCacheMode::Off, canon::CanonMode::Off, chunk, false);
                builtin_parse_partial_sexp_6(
                    &mut eval,
                    Value::fixnum(begv as i64),
                    Value::fixnum(from as i64),
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                )
                .expect("OLDSTATE before the early comment")
            };
            eval.push_specpdl_root(oldstate);
            let plain =
                partial_parse_answer(&mut eval, from, zv, oldstate, Configuration::Plain, chunk);
            let adopted_before = parse_cache_stats().canon_adopted;
            let cached = partial_parse_answer(&mut eval, from, zv, oldstate, configuration, chunk);
            let final_stats = parse_cache_stats();
            assert_eq!(cached, plain, "{configuration:?}, chunk {chunk}");
            assert!(
                final_stats.canon_adopted > adopted_before,
                "{final_stats:?}"
            );
            assert_eq!(final_stats.mismatches, 0, "{final_stats:?}");
            if matches!(
                configuration,
                Configuration::VerifyL1 | Configuration::VerifyL2
            ) {
                assert!(
                    final_stats.verified >= 3,
                    "both backward raw states and the adopted answer must be verified: {final_stats:?}"
                );
            }
            eval.restore_specpdl_roots(roots);
        }
    }
}

/// The new helper may consume an existing canonical run only while both
/// layers are enabled. Cold calls retain the legacy safe-position path.
#[test]
fn canonical_back_comment_cold_and_disabled_paths_preserve_the_legacy_index() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_table(&mut eval, Dialect::C);
    set_text(&mut eval, &format!("{}/* it's */", "a é ".repeat(80)));
    let (_, zv) = bounds(&eval);
    reset_parse_cache_stats();
    let reparses = BACK_COMMENT_SAFE_REPARSES.with(Cell::get);
    check(
        &mut eval,
        zv,
        Query::BackwardComment,
        Configuration::Cached,
        16,
        "cold back-comment",
    );
    eval.buffers
        .current_buffer()
        .expect("current buffer")
        .with_syntax_parse_cache(|cache, _| assert!(cache.canonical.is_none()));
    assert_eq!(parse_cache_stats().canon_back_comments, 0);
    assert!(BACK_COMMENT_SAFE_REPARSES.with(Cell::get) > reparses);

    seed_canonical(&mut eval, 16);
    let before = canonical_snapshots(&eval);
    for (l1, l2) in [
        (ParseCacheMode::Off, canon::CanonMode::On),
        (ParseCacheMode::On, canon::CanonMode::Off),
        (ParseCacheMode::Off, canon::CanonMode::Off),
    ] {
        let _overrides = Overrides::new(l1, l2, 16, false);
        let buf = eval.buffers.current_buffer().expect("current buffer");
        let table = SyntaxTable::for_buffer(buf);
        let state = back_comment_canonical_state(
            buf,
            &table,
            zv as i64 - 2,
            SyntaxProperties::Ignore,
            CommentEndEscapePolicy::default(),
        );
        assert!(
            state.is_none(),
            "disabled helper served a state: {l1:?}, {l2:?}"
        );
        assert_eq!(canonical_snapshots(&eval), before);
    }
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Insert,
    Delete,
    Replace,
    PropertyPut,
    PropertyRemove,
    PropertyDescriptor,
    TableDescriptor,
    TableEntry,
    Narrow,
    Widen,
    Multibyte,
    EscapePolicy,
}

const MUTATIONS: &[Mutation] = &[
    Mutation::Insert,
    Mutation::Delete,
    Mutation::Replace,
    Mutation::PropertyPut,
    Mutation::PropertyRemove,
    Mutation::PropertyDescriptor,
    Mutation::TableDescriptor,
    Mutation::TableEntry,
    Mutation::Narrow,
    Mutation::Widen,
    Mutation::Multibyte,
    Mutation::EscapePolicy,
];

#[test]
fn canonical_back_comments_see_edits_properties_descriptors_and_narrowing() {
    crate::test_utils::init_test_tracing();
    for &mutation in MUTATIONS {
        for chunk in [1, 3, 64] {
            let mut eval = Context::new();
            install_table(&mut eval, Dialect::C);
            eval.eval_str("(setq parse-sexp-lookup-properties t)")
                .expect("honor properties");
            set_text(&mut eval, &format!("é {}/* it's */", "a ü ".repeat(60)));
            let (_, zv) = bounds(&eval);
            match mutation {
                Mutation::PropertyRemove => {
                    eval.eval_str("(put-text-property 2 3 'syntax-table '(7))")
                        .expect("initial quote property");
                }
                Mutation::PropertyDescriptor => {
                    eval.eval_str("(put-text-property 2 3 'syntax-table (cons 1 nil))")
                        .expect("initial mutable descriptor");
                }
                Mutation::Widen => {
                    eval.eval_str(&format!("(narrow-to-region 3 {zv})"))
                        .expect("initial narrowing");
                }
                _ => {}
            }
            seed_canonical(&mut eval, chunk);
            check_every_position(&mut eval, chunk, "before mutation");
            let source = match mutation {
                Mutation::Insert => "(save-excursion (goto-char 1) (insert \"\\\"\"))".to_owned(),
                Mutation::Delete => "(delete-region 1 3)".to_owned(),
                Mutation::Replace => {
                    "(progn (goto-char 2) (looking-at \" \") (replace-match \"\\\"\"))".to_owned()
                }
                Mutation::PropertyPut => "(put-text-property 2 3 'syntax-table '(7))".to_owned(),
                Mutation::PropertyRemove => {
                    "(remove-text-properties 2 3 '(syntax-table nil))".to_owned()
                }
                Mutation::PropertyDescriptor => {
                    "(setcar (get-text-property 2 'syntax-table) 7)".to_owned()
                }
                Mutation::TableDescriptor => "(setcar (aref (syntax-table) ?é) 7)".to_owned(),
                Mutation::TableEntry => "(modify-syntax-entry ?' \".\")".to_owned(),
                Mutation::Narrow => format!("(narrow-to-region 3 {zv})"),
                Mutation::Widen => "(widen)".to_owned(),
                Mutation::Multibyte => {
                    "(progn (set-buffer-multibyte nil) (set-buffer-multibyte t))".to_owned()
                }
                Mutation::EscapePolicy => {
                    "(setq comment-end-can-be-escaped (null comment-end-can-be-escaped))".to_owned()
                }
            };
            eval.eval_str(&source)
                .expect("mutate warm canonical inputs");
            // Do not reseed: these calls must refuse or truncate stale states.
            check_every_position(&mut eval, chunk, &format!("after {mutation:?}"));
            seed_canonical(&mut eval, chunk);
            check_every_position(&mut eval, chunk, &format!("reseeded after {mutation:?}"));
            assert_eq!(parse_cache_stats().mismatches, 0);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum LispResolver {
    Category,
    Alias,
    Default,
}

/// These resolver dependencies have no trustworthy mutation stamp. A warm
/// canonical run must be bypassed, and the legacy index must not take over.
#[test]
fn canonical_back_comments_bypass_unstamped_lisp_property_resolvers() {
    crate::test_utils::init_test_tracing();
    for resolver in [
        LispResolver::Category,
        LispResolver::Alias,
        LispResolver::Default,
    ] {
        let mut eval = Context::new();
        install_table(&mut eval, Dialect::C);
        eval.eval_str("(setq parse-sexp-lookup-properties t)")
            .expect("honor properties");
        set_text(&mut eval, &format!("a {}/* it's */", "a é ".repeat(40)));
        seed_canonical(&mut eval, 3);
        let (setup, mutate) = match resolver {
            LispResolver::Category => (
                "(progn (put 't7-back-comment-category 'syntax-table (cons 1 nil))
                   (put-text-property 2 3 'category 't7-back-comment-category))",
                "(setcar (get 't7-back-comment-category 'syntax-table) 7)",
            ),
            LispResolver::Alias => (
                "(progn (setq char-property-alias-alist '((syntax-table t7-back-comment-alias)))
                   (put-text-property 2 3 't7-back-comment-alias (cons 1 nil)))",
                "(setcar (get-text-property 2 't7-back-comment-alias) 7)",
            ),
            LispResolver::Default => (
                "(progn (setq default-text-properties (list 'syntax-table (list 1)))
                   (put-text-property 2 3 'face 'default))",
                "(setcar (plist-get default-text-properties 'syntax-table) 7)",
            ),
        };
        eval.eval_str(setup)
            .expect("install unstamped property resolver");
        let before = parse_cache_stats();
        check_every_position(&mut eval, 3, &format!("{resolver:?} before plist mutation"));
        eval.eval_str(mutate)
            .expect("mutate resolver descriptor without a property note");
        check_every_position(&mut eval, 3, &format!("{resolver:?} after plist mutation"));
        let after = parse_cache_stats();
        assert_eq!(
            after.canon_back_comments, before.canon_back_comments,
            "unstamped resolver reached canonical back-comment reuse: {resolver:?}, {before:?} -> {after:?}"
        );
        assert_eq!(after.mismatches, 0, "{after:?}");
    }
}
