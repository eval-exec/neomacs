//! The byte-addressed scanners' per-character helpers: the `syntax-table`
//! property run cache (`SyntaxPropByteRun`) must answer exactly as a fresh
//! lookup in any query order — forward, backward, random — and the ASCII
//! fast path of `buffer_syntax_char_before` exactly as the general path.

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

const PIECES: &[&str] = &["a", "/", "*", " ", "\n", "é", "λ", "\\", "\"", "xy"];

fn eval_ok(eval: &mut crate::emacs_core::eval::Context, src: &str) -> Value {
    eval.eval_str(src)
        .unwrap_or_else(|e| panic!("{src}: {e:?}"))
}

/// Random text, then random writes: `syntax-table` and `category` puts
/// (overlapping each other), `face` puts that split intervals, removals,
/// whole-plist sets, and text inserts and deletes that shift everything.
/// After every write the table's cached syntax ranges must still cover
/// every interval carrying a syntax-relevant key.
fn random_buffer(eval: &mut crate::emacs_core::eval::Context, rng: &mut Rng, multibyte: bool) {
    let mut text = String::new();
    for _ in 0..(20 + rng.below(80)) {
        text.push_str(PIECES[rng.below(PIECES.len())]);
    }
    let quoted = crate::emacs_core::print::print_value(&Value::string(&text));
    eval_ok(
        eval,
        &format!(
            "(progn (put 'spbr-cat-a 'syntax-table '(2)) (put 'spbr-cat-b 'syntax-table '(3))
                    (set-buffer-multibyte t) (widen) (erase-buffer) (insert {quoted}))"
        ),
    );
    for _ in 0..rng.below(16) {
        let len = eval_ok(eval, "(point-max)").as_fixnum().expect("point-max") as usize;
        let a = 1 + rng.below(len);
        let b = 1 + rng.below(len);
        let (from, to) = (a.min(b), a.max(b));
        let form = match rng.below(9) {
            0 => format!("(put-text-property {from} {to} 'syntax-table '(1))"),
            1 => format!("(put-text-property {from} {to} 'syntax-table '(0))"),
            2 => format!("(put-text-property {from} {to} 'face 'bold)"),
            3 => format!("(remove-text-properties {from} {to} '(syntax-table nil))"),
            4 => format!("(put-text-property {from} {to} 'category 'spbr-cat-a)"),
            5 => format!("(put-text-property {from} {to} 'category 'spbr-cat-b)"),
            6 => format!("(set-text-properties {from} {to} '(face italic))"),
            7 => format!("(progn (goto-char {from}) (insert \"ab\"))"),
            _ => format!("(delete-region {from} {to})"),
        };
        eval_ok(eval, &form);
        // Exercise the lazy caches between writes, as scans do.
        let buf = eval.buffers.current_buffer().expect("buffer");
        let _ = buf.syntax_prop_free_run_end_at_char_pos(
            CharPos0::new(rng.below(len)),
            CharPos0::new(len + 4096),
        );
        if let Err(e) = buf.debug_syntax_caches_consistent() {
            panic!("after {form}: {e}");
        }
    }
    if !multibyte {
        eval_ok(eval, "(set-buffer-multibyte nil)");
    }
}

/// Byte positions of every character start in the accessible region.
fn char_starts(buf: &Buffer) -> Vec<EmacsBytePos> {
    let region = buf.accessible_char_region();
    (region.start().get()..region.end().get())
        .map(|c| buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(c)))
        .collect()
}

#[test]
fn the_byte_run_cache_answers_like_a_fresh_lookup_in_any_order() {
    let mut eval = crate::emacs_core::eval::Context::new();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0;
    for round in 0..300 {
        random_buffer(&mut eval, &mut rng, round % 3 != 0);
        let props = SyntaxProperties::for_scan(true, &eval.obarray, &eval.buffers);
        let buf = eval.buffers.current_buffer().expect("buffer");
        let starts = char_starts(buf);
        let mut orders: Vec<Vec<EmacsBytePos>> = vec![starts.clone()];
        orders.push(starts.iter().rev().copied().collect());
        let mut shuffled = starts.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, rng.below(i + 1));
        }
        orders.push(shuffled);
        for (o, order) in orders.iter().enumerate() {
            let cache = SyntaxPropByteRun::new(props);
            for &pos in order {
                let want = props.syntax_table_prop_at_emacs_byte(buf, pos);
                let got = cache.syntax_table_prop_at_emacs_byte(buf, pos);
                assert_eq!(
                    got.map(|v| v.bits()),
                    want.map(|v| v.bits()),
                    "round {round} order {o} byte {}: {:?}",
                    pos.get(),
                    buf.full_text_string()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 30000, "checked {checked}");
}

/// A backward walk over a property-free stretch refills once, not per
/// character.
#[test]
fn a_backward_walk_refills_once_per_run() {
    let mut eval = crate::emacs_core::eval::Context::new();
    eval_ok(
        &mut eval,
        "(progn (erase-buffer) (insert (make-string 200 ?a))
                (put-text-property 1 2 'syntax-table '(1)))",
    );
    let props = SyntaxProperties::for_scan(true, &eval.obarray, &eval.buffers);
    let buf = eval.buffers.current_buffer().expect("buffer");
    let cache = SyntaxPropByteRun::new(props);
    SYNTAX_BYTE_RUN_REFILLS.with(|c| c.set(0));
    for byte in (1..200).rev() {
        assert_eq!(
            cache.syntax_table_prop_at_emacs_byte(buf, EmacsBytePos::new(byte)),
            None
        );
    }
    let refills = SYNTAX_BYTE_RUN_REFILLS.with(|c| c.get());
    assert!(refills <= 2, "a backward walk refilled {refills} times");
}

#[test]
fn the_ascii_char_before_is_the_general_answer() {
    let mut eval = crate::emacs_core::eval::Context::new();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut checked = 0;
    for round in 0..40 {
        random_buffer(&mut eval, &mut rng, round % 2 == 0);
        if round % 4 == 1 {
            // Raw bytes in a multibyte buffer.
            eval_ok(
                &mut eval,
                "(progn (set-buffer-multibyte t) (goto-char (point-max))
                        (insert (string-to-multibyte \"a\\200\\377b\")))",
            );
        }
        let buf = eval.buffers.current_buffer().expect("buffer");
        let end = buf.total_emacs_byte_end_pos().get();
        for byte in 0..=end + 1 {
            let pos = EmacsBytePos::new(byte);
            let got =
                buffer_syntax_char_before(buf, pos).map(|u| (u.ch, u.start.get(), u.end.get()));
            let want = buf.char_before_emacs_byte_pos(pos).map(|ch| {
                let len = buf
                    .char_before_emacs_byte_len(pos)
                    .map(|len| len.max(EmacsByteLen::new(1)))
                    .unwrap_or_else(|| EmacsByteLen::new(ch.len_utf8().max(1)));
                (ch, pos.saturating_sub_len(len).get(), pos.get())
            });
            assert_eq!(got, want, "round {round} byte {byte}");
            checked += 1;
        }
    }
    assert!(checked > 2000, "checked {checked}");
}

/// A `syntax-table` put that overlaps an earlier one must not hide the
/// earlier property's surviving part from later scans. GNU answers 4/5 for
/// each probe; the range cache dropped the overlapped entry whole, so every
/// scan below ran through the punctuation at char 5 to the buffer end.
#[test]
fn an_overlapping_syntax_table_put_keeps_the_earlier_property_visible() {
    let mut eval = crate::emacs_core::eval::Context::new();
    let answer = |eval: &mut crate::emacs_core::eval::Context, src: &str| {
        crate::emacs_core::print::print_value(&eval_ok(eval, src))
    };
    eval_ok(
        &mut eval,
        "(progn (erase-buffer) (insert \"aaaaaaaaaaaaaaaaaaaa\")
                (set (make-local-variable 'parse-sexp-lookup-properties) t)
                (put-text-property 5 8 'syntax-table '(1)))",
    );
    // A first scan builds the cached ranges the put below must maintain.
    assert_eq!(
        answer(
            &mut eval,
            "(progn (goto-char 1) (list (skip-syntax-forward \"w\") (point)))"
        ),
        "(4 5)"
    );
    eval_ok(&mut eval, "(put-text-property 6 12 'syntax-table '(2))");
    assert_eq!(
        answer(
            &mut eval,
            "(progn (goto-char 1) (list (skip-syntax-forward \"w\") (point)))"
        ),
        "(4 5)",
        "skip-syntax-forward"
    );
    assert_eq!(
        answer(
            &mut eval,
            "(progn (goto-char 1) (re-search-forward \"\\\\sw+\") (point))"
        ),
        "5",
        "re-search-forward"
    );
    assert_eq!(
        answer(&mut eval, "(progn (goto-char 1) (forward-word 1) (point))"),
        "5",
        "forward-word"
    );
}

/// A `syntax-table` removal inside an earlier put must not leave the scan
/// believing the whole put is one run. GNU stops at 10; the range cache
/// served the stale entry's end as the run end, so the scan from 7 treated
/// the punctuation at 10..15 as word syntax and ran to the buffer end.
#[test]
fn a_syntax_table_removal_inside_a_put_splits_the_run() {
    let mut eval = crate::emacs_core::eval::Context::new();
    let answer = |eval: &mut crate::emacs_core::eval::Context, src: &str| {
        crate::emacs_core::print::print_value(&eval_ok(eval, src))
    };
    eval_ok(
        &mut eval,
        "(progn (erase-buffer) (insert \"aaaaaaaaaaaaaaaaaaaa\")
                (set (make-local-variable 'parse-sexp-lookup-properties) t)
                (put-text-property 3 16 'syntax-table '(1)))",
    );
    assert_eq!(
        answer(
            &mut eval,
            "(progn (goto-char 1) (list (skip-syntax-forward \"w\") (point)))"
        ),
        "(2 3)"
    );
    eval_ok(
        &mut eval,
        "(remove-text-properties 7 10 '(syntax-table nil))",
    );
    assert_eq!(
        answer(
            &mut eval,
            "(progn (goto-char 7) (list (skip-syntax-forward \"w\") (point)))"
        ),
        "(3 10)"
    );
}
