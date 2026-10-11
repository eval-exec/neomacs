//! `back_comment`'s lossage re-parse restarts from the buffer's safe-position
//! index (`buffer_text::SyntaxSafePositions`) instead of from BEGV. The answer
//! must never change: every `(forward-comment -1)` here runs twice, once with
//! the index bypassed (the parse from BEGV) and once through it, at chunk
//! sizes small enough to put a recorded position between the two characters
//! of every delimiter, after every escape, and inside nested comments — the
//! places a resume at an arbitrary position gets wrong. Between queries the
//! buffer text, the syntax table and `syntax-table` text properties change,
//! so a stale index would answer differently. (Every cached re-parse is also
//! cross-checked against the parse from BEGV by a debug assertion.)

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

const ALPHABET: &[char] = &[
    '/', '*', '\\', '\'', '"', '!', '#', '%', '(', ')', '{', '-', '}', '|', '\n', ' ', 'a', 'x',
    'é',
];

fn set_text(eval: &mut crate::emacs_core::eval::Context, text: &str) {
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.widen();
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert(text);
}

fn modify(eval: &mut crate::emacs_core::eval::Context, ch: char, descriptor: &str) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum(ch as i64), Value::string(descriptor)],
    )
    .expect("modify-syntax-entry");
}

/// Five comment dialects the scanner treats differently.
fn install_table(eval: &mut crate::emacs_core::eval::Context, kind: usize) {
    for ch in [
        '/', '*', '\\', '\'', '"', '!', '#', '%', '(', ')', '{', '-', '}', '|', '\n',
    ] {
        modify(eval, ch, ".");
    }
    modify(eval, '\\', "\\");
    modify(eval, '"', "\"");
    match kind {
        // C: `/* */` style a, `//` style b.
        0 => {
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
            modify(eval, '\n', "> b");
            modify(eval, '\'', "\"");
        }
        // elb-smie's table.
        1 => {
            modify(eval, '/', ". 124");
            modify(eval, '*', ". 23b");
            modify(eval, '\n', ">");
            modify(eval, '\'', "\"");
        }
        // Nested `(* *)` and `{- -}`.
        2 => {
            modify(eval, '(', "()1n");
            modify(eval, ')', ")(4n");
            modify(eval, '*', ". 23n");
            modify(eval, '{', "(}1n");
            modify(eval, '}', "){4n");
            modify(eval, '-', ". 23n");
            modify(eval, '\'', "\"");
        }
        // Comment and string fences.
        3 => {
            modify(eval, '!', "!");
            modify(eval, '|', "|");
            modify(eval, '/', ". 124b");
            modify(eval, '*', ". 23");
        }
        // A single-char opener that is also an ender's first char.
        _ => {
            modify(eval, '#', "< 3");
            modify(eval, '%', ". 4");
            modify(eval, '\n', ">");
            modify(eval, '\'', "\"");
        }
    }
}

/// `(forward-comment -1)` from Lisp position `pos`: (return value, point).
fn backward_comment(
    eval: &mut crate::emacs_core::eval::Context,
    pos: usize,
    bypass: bool,
) -> (Value, i64) {
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        let byte = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(pos - 1));
        buf.goto_emacs_byte_pos(byte);
    }
    BACK_COMMENT_SAFE_BYPASS.with(|b| b.set(bypass));
    let out = builtin_forward_comment(eval, vec![Value::fixnum(-1)]).expect("forward-comment");
    BACK_COMMENT_SAFE_BYPASS.with(|b| b.set(false));
    let point = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    (out, point)
}

/// Token soup biased toward what reaches `back_comment`'s lossage: comment
/// bodies holding string quotes, escapes and the other dialects' delimiters,
/// separated by filler long enough for restart positions to matter.
fn random_text(rng: &mut Rng, max_tokens: usize) -> String {
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
        "it's",
        "/* it's */",
        "// don't\n",
        "(* a ' b *)",
        "! x ' y !",
        "# a ' %",
    ];
    let mut out = String::new();
    for _ in 0..rng.below(max_tokens + 1) {
        if rng.below(3) == 0 {
            for _ in 0..rng.below(6) + 1 {
                out.push([' ', 'a', 'x', 'é'][rng.below(4)]);
            }
        } else {
            out.push_str(TOKENS[rng.below(TOKENS.len())]);
        }
    }
    out
}

#[test]
fn safe_position_restarts_answer_exactly_as_the_parse_from_begv() {
    crate::test_utils::init_test_tracing();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let starts_before = BACK_COMMENT_SAFE_STARTS.with(|c| c.get());
    let mut queries = 0usize;
    for kind in 0..5 {
        for escapable in [false, true] {
            let mut eval = crate::emacs_core::eval::Context::new();
            install_table(&mut eval, kind);
            let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
            eval.set_buffer_local_binding_by_id(
                buffer_id,
                crate::emacs_core::intern::intern("comment-end-can-be-escaped"),
                Value::bool_val(escapable),
            )
            .expect("comment-end-can-be-escaped");
            for round in 0..40 {
                let text = random_text(&mut rng, 60);
                set_text(&mut eval, &text);
                let chunk = [1usize, 2, 3, 5, 64][round % 5];
                BACK_COMMENT_SAFE_CHUNK_OVERRIDE.with(|c| c.set(chunk));
                let len = text.chars().count();
                for step in 0..3 {
                    for pos in 1..=len + 1 {
                        let want = backward_comment(&mut eval, pos, true);
                        let got = backward_comment(&mut eval, pos, false);
                        assert_eq!(
                            got, want,
                            "table {kind} escapable {escapable} chunk {chunk} step {step} \
                             pos {pos} text {text:?}"
                        );
                        queries += 1;
                    }
                    // Change something the parse reads, then query again.
                    match rng.below(3) {
                        0 if len > 0 => {
                            // Replace one character.
                            let at = rng.below(len);
                            let ch = ALPHABET[rng.below(ALPHABET.len())];
                            let buf = eval.buffers.current_buffer_mut().expect("buffer");
                            let b0 = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(at));
                            let b1 = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(at + 1));
                            buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
                                b0.get(),
                                b1.get(),
                            ));
                            buf.goto_emacs_byte_pos(b0);
                            buf.insert(&ch.to_string());
                        }
                        1 => {
                            // Flip the quote's syntax.
                            let d = if rng.below(2) == 0 { "\"" } else { "." };
                            modify(&mut eval, '\'', d);
                        }
                        _ if len > 1 => {
                            // A `syntax-table` property, honoured.
                            let at = rng.below(len) + 1;
                            let class = [1, 7, 11, 12, 14][rng.below(5)];
                            eval.obarray
                                .set_symbol_value("parse-sexp-lookup-properties", Value::T);
                            eval.eval_str(&format!(
                                "(put-text-property {at} {} 'syntax-table '({class}))",
                                at + 1
                            ))
                            .expect("put-text-property");
                        }
                        _ => {}
                    }
                    // The text may have changed length.
                    let _ = step;
                }
                eval.obarray
                    .set_symbol_value("parse-sexp-lookup-properties", Value::NIL);
            }
        }
    }
    BACK_COMMENT_SAFE_CHUNK_OVERRIDE.with(|c| c.set(0));
    let restarts = BACK_COMMENT_SAFE_STARTS.with(|c| c.get()) - starts_before;
    let reparses = BACK_COMMENT_SAFE_REPARSES.with(|c| c.get());
    assert!(reparses > 1_000, "lossage re-parses exercised: {reparses}");
    assert!(queries > 5_000, "queries {queries}");
    assert!(
        restarts > 300,
        "the index must actually be used: {restarts} restarts after BEGV in {queries} queries"
    );
}

/// Build the index over `text` (C table, chunk 1) with a query at the end,
/// apply `change`, and return the cached and bypassed answers at the end —
/// plus the answer before the change, to prove the change mattered.
fn answers_around_a_change(
    text: &str,
    honor: bool,
    change: impl FnOnce(&mut crate::emacs_core::eval::Context),
) -> ((Value, i64), (Value, i64), (Value, i64)) {
    let mut eval = crate::emacs_core::eval::Context::new();
    install_table(&mut eval, 0);
    if honor {
        eval.obarray
            .set_symbol_value("parse-sexp-lookup-properties", Value::T);
    }
    set_text(&mut eval, text);
    BACK_COMMENT_SAFE_CHUNK_OVERRIDE.with(|c| c.set(1));
    let end = || text.chars().count() + 1;
    let before = backward_comment(&mut eval, end(), false);
    change(&mut eval);
    let len = eval
        .buffers
        .current_buffer()
        .expect("buffer")
        .point_max_char_pos()
        .get()
        + 1;
    let cached = backward_comment(&mut eval, len, false);
    let bypassed = backward_comment(&mut eval, len, true);
    BACK_COMMENT_SAFE_CHUNK_OVERRIDE.with(|c| c.set(0));
    (before, cached, bypassed)
}

/// A `syntax-table` property put before recorded restart positions changes
/// the parse (the text itself does not change).
#[test]
fn a_syntax_table_property_before_restart_positions_is_seen() {
    crate::test_utils::init_test_tracing();
    let (before, cached, bypassed) = answers_around_a_change("a a a a /* it's */", true, |eval| {
        eval.eval_str("(put-text-property 2 3 'syntax-table '(7))")
            .expect("put-text-property");
    });
    assert_eq!(cached, bypassed);
    assert_ne!(before, bypassed, "the property must change the answer");
}

/// `replace-match` edits through the replace path, not insert/delete.
#[test]
fn a_replace_match_before_restart_positions_is_seen() {
    crate::test_utils::init_test_tracing();
    let (before, cached, bypassed) = answers_around_a_change("a a a a /* it's */", false, |eval| {
        eval.eval_str("(progn (goto-char 1) (looking-at \"a\") (replace-match \"\\\"\"))")
            .expect("replace-match");
    });
    assert_eq!(cached, bypassed);
    assert_ne!(before, bypassed, "the replacement must change the answer");
}

/// `set-buffer-multibyte` rewrites the storage and shifts char positions.
#[test]
fn set_buffer_multibyte_round_trip_is_seen() {
    crate::test_utils::init_test_tracing();
    let (_, cached, bypassed) =
        answers_around_a_change("\u{e9}\u{e9} a a /* it's */", false, |eval| {
            eval.eval_str("(progn (set-buffer-multibyte nil) (set-buffer-multibyte t))")
                .expect("set-buffer-multibyte");
        });
    assert_eq!(cached, bypassed);
}
