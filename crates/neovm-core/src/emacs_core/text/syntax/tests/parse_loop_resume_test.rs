//! The scan loop resumes exactly (P3.4 S2): the lemma the parse cache and the
//! `syntax-propertize` pause rest on, proven on random input before any cache
//! exists.
//!
//! For random buffers, syntax tables (five comment dialects plus random flag
//! soup), `syntax-table` properties, starting states and options, every query
//! runs uninterrupted and then again from each of its loop tops:
//!
//! * a snapshot taken at loop top `p` of a scan to `TO`, resumed to any
//!   `TO' > p`, answers exactly as the uninterrupted scan to `TO'`;
//! * a scan paused at any loop top (the first included) and resumed answers
//!   exactly as the scan that never paused;
//! * a resume that supplies the syntax its first character was already read
//!   with (the mid-step `syntax-propertize` pause) answers the same.

use super::super::*;
use super::*;

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

/// Every loop top, as a resumable state.
struct RecordAll {
    tops: Vec<LoopState>,
}

impl ScanMode for RecordAll {
    const ACTIVE: bool = true;
    fn first_target(&self) -> usize {
        0
    }
    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        self.tops.push(top.to_state());
        TopAction::Continue(top.char_pos + 1)
    }
}

/// Pause at the loop top at or after `at`.
struct PauseAt {
    at: usize,
}

impl ScanMode for PauseAt {
    const ACTIVE: bool = true;
    fn first_target(&self) -> usize {
        self.at
    }
    fn at_loop_top(&mut self, _top: LoopTop<'_>) -> TopAction {
        TopAction::Pause
    }
}

/// A plain scan that accepts a first-character syntax override.
struct Overriding;

impl ScanMode for Overriding {
    const ACTIVE: bool = false;
    const OVERRIDE: bool = true;
    fn at_loop_top(&mut self, _top: LoopTop<'_>) -> TopAction {
        TopAction::Continue(usize::MAX)
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
    "?\\(",
    "\"a\\\"b\"",
];

fn random_text(rng: &mut Rng, max_tokens: usize) -> String {
    let mut out = String::new();
    for _ in 0..rng.below(max_tokens + 1) {
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

/// Five fixed dialects, then random descriptors with random comment flags.
fn install_table(eval: &mut crate::emacs_core::eval::Context, kind: usize, rng: &mut Rng) {
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
        4 => {
            modify(eval, '#', "< 3");
            modify(eval, '%', ". 4");
            modify(eval, '\n', ">");
        }
        _ => {
            const CLASSES: &[&str] = &[
                ".", "w", "_", "(", ")", "\"", "\\", "/", "'", "<", ">", "!", "|", " ",
            ];
            const FLAGS: &[&str] = &[
                "", "1", "2", "3", "4", "b", "c", "n", "p", "14", "23", "124b", "23n",
            ];
            for ch in ['/', '*', '#', '%', '-', '!', '|', ';', '\'', '\n', '?'] {
                let class = CLASSES[rng.below(CLASSES.len())];
                let flags = FLAGS[rng.below(FLAGS.len())];
                let descriptor = match class {
                    "(" => format!("(){flags}"),
                    ")" => format!(")({flags}"),
                    _ => format!("{class} {flags}"),
                };
                modify(eval, ch, &descriptor);
            }
        }
    }
}

fn set_text(eval: &mut crate::emacs_core::eval::Context, text: &str) {
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.widen();
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert(text);
}

#[derive(Clone, Copy, Debug)]
struct Opts {
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: CommentStopMode,
}

fn random_opts(rng: &mut Rng) -> Opts {
    Opts {
        target_depth: [None, None, None, Some(-1), Some(0), Some(1), Some(2)][rng.below(7)],
        stop_before: rng.below(5) == 0,
        commentstop: [
            CommentStopMode::None,
            CommentStopMode::None,
            CommentStopMode::Comment,
            CommentStopMode::SyntaxTable,
        ][rng.below(4)],
    }
}

/// One scan of the current buffer under the context's own environment.
fn scan<M: ScanMode>(
    eval: &crate::emacs_core::eval::Context,
    entry: Entry,
    to_char: usize,
    opts: Opts,
    mode: &mut M,
) -> ScanEnd {
    let buf = eval.buffers.current_buffer().expect("current buffer");
    let table = SyntaxTable::for_buffer(buf);
    let honor = parse_sexp_lookup_properties_enabled(eval);
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let escape_policy = CommentEndEscapePolicy::for_context(eval);
    run_parse_loop(
        buf,
        &table,
        entry,
        to_char,
        opts.target_depth,
        opts.stop_before,
        opts.commentstop,
        props,
        escape_policy,
        mode,
    )
}

fn finished(end: ScanEnd) -> (PartialParseState, i64) {
    match end {
        ScanEnd::Finished(finish) => (finish.state, finish.stop),
        ScanEnd::Paused(at) => panic!("unexpected pause at {}", at.char_pos),
    }
}

/// The syntax the loop reads the character at `at` with, computed the slow
/// way (no flat table, fresh property cache).
fn syntax_at(
    eval: &crate::emacs_core::eval::Context,
    at: &LoopState,
) -> (SyntaxClass, SyntaxFlags) {
    let buf = eval.buffers.current_buffer().expect("current buffer");
    let table = SyntaxTable::for_buffer(buf);
    let honor = parse_sexp_lookup_properties_enabled(eval);
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let prop_cache = SyntaxPropRange::new(props);
    let ch = ParseBufferChars::at_emacs_byte(buf, at.byte_pos).peek();
    let entry = effective_syntax_entry_for_abs_char(buf, &table, ch, at.char_pos, &prop_cache);
    (entry.class, entry.flags)
}

#[test]
fn a_scan_resumed_at_any_loop_top_answers_as_the_uninterrupted_scan() {
    crate::test_utils::init_test_tracing();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut resumes = 0usize;
    let mut other_to = 0usize;
    let mut pauses = 0usize;
    let mut overrides = 0usize;
    let mut first_top_pauses_in_comment = 0usize;
    for kind in 0..8 {
        for escapable in [false, true] {
            for honor in [false, true] {
                let mut eval = crate::emacs_core::eval::Context::new();
                install_table(&mut eval, kind, &mut rng);
                let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
                eval.set_buffer_local_binding_by_id(
                    buffer_id,
                    crate::emacs_core::intern::intern("comment-end-can-be-escaped"),
                    Value::bool_val(escapable),
                )
                .expect("comment-end-can-be-escaped");
                eval.obarray
                    .set_symbol_value("parse-sexp-lookup-properties", Value::bool_val(honor));
                for _round in 0..12 {
                    let text = random_text(&mut rng, 40);
                    set_text(&mut eval, &text);
                    let len = text.chars().count();
                    if honor && len > 1 {
                        for _ in 0..rng.below(4) {
                            let at = rng.below(len) + 1;
                            let end = (at + 1 + rng.below(3)).min(len + 1);
                            let class = [0, 1, 3, 4, 5, 7, 11, 12, 14, 15][rng.below(10)];
                            eval.eval_str(&format!(
                                "(put-text-property {at} {end} 'syntax-table '({class}))"
                            ))
                            .expect("put-text-property");
                        }
                    }
                    for _query in 0..12 {
                        let from = rng.below(len + 1) + 1;
                        let to = from + rng.below(len + 2 - from);
                        let opts = random_opts(&mut rng);
                        // OLDSTATE: nil, or the real state at FROM (which may
                        // sit inside a string or comment, or straddle a pair).
                        let oldstate = if rng.below(3) == 0 || from == 1 {
                            None
                        } else {
                            let buf = eval.buffers.current_buffer().expect("buffer");
                            let table = SyntaxTable::for_buffer(buf);
                            let props =
                                SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
                            let policy = CommentEndEscapePolicy::for_context(&eval);
                            let (state, _) = parse_state_from_range_with_options(
                                buf,
                                &table,
                                1,
                                from as i64,
                                None,
                                false,
                                None,
                                CommentStopMode::None,
                                props,
                                policy,
                            );
                            Some(state)
                        };
                        let from_char = from - 1;
                        let to_char = to - 1;
                        let fresh = || Entry::Fresh {
                            from_char,
                            state: PartialParseState::from_oldstate(oldstate.as_ref()),
                            from_oldstate: oldstate.is_some(),
                        };
                        let want = finished(scan(&eval, fresh(), to_char, opts, &mut Plain));
                        let mut rec = RecordAll { tops: Vec::new() };
                        let got = finished(scan(&eval, fresh(), to_char, opts, &mut rec));
                        assert_eq!(got, want, "recording changed the answer: {text:?}");
                        let ctx = || {
                            format!(
                                "kind {kind} esc {escapable} honor {honor} text {text:?} from {from} to {to} {opts:?} oldstate {oldstate:?}"
                            )
                        };

                        for (i, top) in rec.tops.iter().enumerate() {
                            // Snapshots after FROM (the first loop top of a
                            // fresh scan still owes its comment-resume
                            // syntax, which `to_state` does not carry).
                            if i == 0 {
                                continue;
                            }
                            let resumed = finished(scan(
                                &eval,
                                Entry::Resume {
                                    at: top.clone(),
                                    first_syntax: None,
                                },
                                to_char,
                                opts,
                                &mut Plain,
                            ));
                            assert_eq!(resumed, want, "resume at {} {}", top.char_pos, ctx());
                            resumes += 1;

                            // The same snapshot answers for any later TO.
                            let other_to_char =
                                top.char_pos + 1 + rng.below(len + 1 - top.char_pos);
                            let direct =
                                finished(scan(&eval, fresh(), other_to_char, opts, &mut Plain));
                            let via = finished(scan(
                                &eval,
                                Entry::Resume {
                                    at: top.clone(),
                                    first_syntax: None,
                                },
                                other_to_char,
                                opts,
                                &mut Plain,
                            ));
                            assert_eq!(
                                via,
                                direct,
                                "snapshot at {} to other TO {} {}",
                                top.char_pos,
                                other_to_char + 1,
                                ctx()
                            );
                            other_to += 1;

                            // Resumed with the syntax its character was read with.
                            let first = syntax_at(&eval, top);
                            let overridden = finished(scan(
                                &eval,
                                Entry::Resume {
                                    at: top.clone(),
                                    first_syntax: Some(first),
                                },
                                to_char,
                                opts,
                                &mut Overriding,
                            ));
                            assert_eq!(overridden, want, "override at {} {}", top.char_pos, ctx());
                            overrides += 1;
                        }

                        // Pause at every loop top, the first included.
                        for top in &rec.tops {
                            let mut pause = PauseAt { at: top.char_pos };
                            let paused = match scan(&eval, fresh(), to_char, opts, &mut pause) {
                                ScanEnd::Paused(at) => at,
                                ScanEnd::Finished(_) => {
                                    panic!("no pause at {} {}", top.char_pos, ctx())
                                }
                            };
                            assert_eq!(paused.char_pos, top.char_pos);
                            if paused.comment_resume.is_some() {
                                first_top_pauses_in_comment += 1;
                            }
                            let resumed = finished(scan(
                                &eval,
                                Entry::Resume {
                                    at: paused,
                                    first_syntax: None,
                                },
                                to_char,
                                opts,
                                &mut Plain,
                            ));
                            assert_eq!(resumed, want, "pause at {} {}", top.char_pos, ctx());
                            pauses += 1;
                        }
                    }
                }
            }
        }
    }
    tracing::info!(
        resumes,
        other_to,
        pauses,
        overrides,
        first_top_pauses_in_comment,
        "resume self-test coverage"
    );
    assert!(resumes > 20_000, "resumes {resumes}");
    assert!(other_to > 20_000, "other-TO resumes {other_to}");
    assert!(pauses > 20_000, "pauses {pauses}");
    assert!(overrides > 20_000, "overrides {overrides}");
    assert!(
        first_top_pauses_in_comment > 10,
        "pauses that carried a comment-resume syntax: {first_top_pauses_in_comment}"
    );
}
