//! `forward-line`, `line-number-at-pos`, `count-lines`, `pos-bol`/`pos-eol`
//! and `line-beginning-position`/`line-end-position` answer the same with
//! the text line index on (verify mode: every index answer is also checked
//! against a scan) as with it off, over random text, random edits, narrowing
//! and `selective-display`.

use super::*;
use crate::buffer::text_index::tests::Rng;
use crate::buffer::text_index::{
    TextLineIndexConfig, TextLineIndexMode, with_text_line_index_config,
};

fn eager(mode: TextLineIndexMode) -> TextLineIndexConfig {
    TextLineIndexConfig::eager(mode, 32)
}

/// A random line of text: ASCII, a multibyte character, sometimes a `\r`.
fn random_line(rng: &mut Rng) -> String {
    let mut line = String::new();
    for _ in 0..rng.below(12) {
        line.push(match rng.below(10) {
            0 => '\u{e9}',
            1 => '\u{4e2d}',
            2 => '\r',
            3 => ' ',
            _ => (b'a' + rng.below(26) as u8) as char,
        });
    }
    line.push('\n');
    line
}

fn lisp_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// FORM's printed value with the index off and with it on (verify).
fn both_ways(ev: &mut Context, form: &str) -> (String, String) {
    let run = |ev: &mut Context, mode| {
        with_text_line_index_config(eager(mode), || {
            let value = ev
                .eval_str(form)
                .unwrap_or_else(|err| panic!("{form}: {err:?}"));
            crate::emacs_core::print_value(&value)
        })
    };
    let off = run(ev, TextLineIndexMode::Off);
    let on = run(ev, TextLineIndexMode::Verify);
    (off, on)
}

#[test]
fn line_motion_and_counting_agree_with_the_index_on_and_off() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let mut rng = Rng::new(42);
    let text: String = (0..400).map(|_| random_line(&mut rng)).collect();
    eval_str(
        &mut ev,
        &format!("(progn (erase-buffer) (insert {}))", lisp_string(&text)),
    );
    for round in 0..40 {
        let size = eval_int(&mut ev, "(point-max)");
        let pos = |rng: &mut Rng| 1 + rng.below(size as usize) as i64;
        // A random narrowing (or none), and sometimes selective display.
        let (a, b) = (pos(&mut rng), pos(&mut rng));
        let narrow = if rng.below(3) == 0 {
            format!("(narrow-to-region {} {})", a.min(b), a.max(b))
        } else {
            "(widen)".to_owned()
        };
        let selective = if rng.below(4) == 0 { "t" } else { "nil" };
        eval_str(
            &mut ev,
            &format!("(progn {narrow} (setq selective-display {selective}))"),
        );
        for _ in 0..8 {
            let p = pos(&mut rng);
            let q = pos(&mut rng);
            let n = match rng.below(4) {
                0 => rng.below(5) as i64 - 2,
                1 => rng.below(300) as i64 - 150,
                2 => 100_000,
                _ => -100_000,
            };
            let form = format!(
                "(save-restriction
                   (let ((p (max (point-min) (min (point-max) {p})))
                         (q (max (point-min) (min (point-max) {q}))))
                     (list (progn (goto-char p) (list (forward-line {n}) (point)))
                           (progn (goto-char p) (list (pos-bol {n}) (pos-eol {n})))
                           (progn (goto-char p)
                                  (list (line-beginning-position {n}) (line-end-position {n})))
                           (line-number-at-pos p)
                           (line-number-at-pos p t)
                           (count-lines p q)
                           (progn (goto-char q) (forward-line (- {n})) (point)))))"
            );
            let (off, on) = both_ways(&mut ev, &form);
            assert_eq!(
                off, on,
                "round {round}: {narrow} selective {selective}: {form}"
            );
        }
        // A random edit through the Lisp primitives.
        eval_str(&mut ev, "(widen)");
        let size = eval_int(&mut ev, "(point-max)");
        let p = 1 + rng.below(size as usize) as i64;
        let q = (p + rng.below(200) as i64).min(size);
        let edit = match rng.below(3) {
            0 => format!(
                "(progn (goto-char {p}) (insert {}))",
                lisp_string(&random_line(&mut rng))
            ),
            1 => format!("(delete-region {p} {q})"),
            _ => format!(
                "(progn (goto-char {p}) (delete-region {p} {q}) (insert {}))",
                lisp_string(&random_line(&mut rng).repeat(3))
            ),
        };
        let run_edit = |ev: &mut Context| {
            with_text_line_index_config(eager(TextLineIndexMode::Verify), || {
                eval_str(ev, &edit);
            })
        };
        run_edit(&mut ev);
    }
    let whole = "(progn (widen) (list (count-lines (point-min) (point-max))
                                      (progn (goto-char (point-max)) (forward-line -100000))
                                      (point)))";
    let (off, on) = both_ways(&mut ev, whole);
    assert_eq!(off, on);
    assert!(
        ev.buffers
            .current_buffer()
            .unwrap()
            .has_line_index_for_test(),
        "the queries built the buffer's index"
    );
    // A snapshot shares it.
    let served = with_text_line_index_config(eager(TextLineIndexMode::Verify), || {
        let buffer = ev.buffers.current_buffer().unwrap();
        let end = buffer.total_emacs_byte_end_pos();
        buffer
            .text_snapshot()
            .indexed_newline_count(crate::buffer::EmacsByteRange::new(
                crate::buffer::EmacsBytePos::ZERO,
                end,
            ))
    });
    assert!(
        served.is_some(),
        "the index was built and the snapshot shares it"
    );
    assert_eq!(crate::buffer::text_index::text_line_index_mismatches(), 0);
}

/// The forms of `selective_display_gnu.rs` (pinned to GNU there) give the
/// same answers with the index on.
#[test]
fn selective_display_line_numbers_are_unchanged_with_the_index() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(progn
  (erase-buffer)
  (insert "a\rb\nc\rd\ne\r")
  (let ((at (lambda () (mapcar #'line-number-at-pos '(1 2 3 4 5 6 7 8 9 10 11)))))
    (list (progn (setq selective-display nil) (funcall at))
          (progn (setq selective-display t) (funcall at))
          (progn (setq selective-display 2) (funcall at))
          (progn (setq selective-display t)
                 (narrow-to-region 3 9)
                 (prog1 (list (line-number-at-pos 9)
                              (line-number-at-pos 9 t)
                              (line-number-at-pos 5))
                   (widen))))))
"#;
    let mut ev = runtime_startup_context();
    let (off, on) = both_ways(&mut ev, form);
    assert_eq!(off, on);
}

/// Every way Lisp changes buffer text, each followed by line queries with
/// the index in verify mode: every edit is recounted around its position
/// and every answer checked against a scan, and the answers equal the ones
/// with the index off.
#[test]
fn every_lisp_edit_path_keeps_the_index_exact() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let mut rng = Rng::new(7);
    let text: String = (0..300).map(|_| random_line(&mut rng)).collect();
    let probe = "(list (line-number-at-pos (point-max))
                       (line-number-at-pos (/ (point-max) 2))
                       (count-lines (point-min) (point-max))
                       (progn (goto-char (point-min)) (forward-line 150) (point))
                       (progn (goto-char (point-max)) (forward-line -150) (point))
                       (progn (goto-char (point-min)) (forward-line 100000)))";
    let edits = [
        // Undo of an insertion and a deletion.
        "(progn (buffer-enable-undo) (undo-boundary)
                (goto-char 500) (insert \"u1\\nu2\\n\") (undo-boundary)
                (delete-region 100 400) (undo-boundary)
                (primitive-undo 1 buffer-undo-list)
                (setq buffer-undo-list (cdr buffer-undo-list)))",
        "(let ((f (make-temp-file \"neovm-line-index\")))
           (unwind-protect
               (progn (with-temp-file f (insert \"one\\ntwo\\r\\nthree\\n\"))
                      (goto-char 300) (insert-file-contents f)
                      (goto-char (point-max)) (insert-file-contents f nil 2 9))
             (delete-file f)))",
        "(progn (encode-coding-region 1 800 'utf-8) (decode-coding-region 1 800 'utf-8))",
        "(progn (encode-coding-region 200 600 'utf-16le) (decode-coding-region 200 400 'latin-1))",
        "(progn (set-buffer-multibyte nil) (goto-char 50) (insert \"\\n\\n\") (set-buffer-multibyte t))",
        "(subst-char-in-region 1 (point-max) ?a ?\\n)",
        "(subst-char-in-region 1 (point-max) ?\\n ?Z)",
        "(transpose-regions 10 200 400 1000)",
        "(progn (goto-char 1) (while (re-search-forward \"b+\" nil t) (replace-match \"\\n\")))",
        "(progn (upcase-region 1 (point-max)) (capitalize-region 100 900))",
        "(translate-region 1 (point-max) (let ((s (make-string 256 ?x))) (dotimes (i 256) (aset s i i)) (aset s ?c ?\\n) s))",
        "(progn (goto-char 700) (insert-char ?\\n 40) (delete-char -10))",
        "(progn (narrow-to-region 100 900) (goto-char (point-min)) (insert \"n\\n\") (delete-region (point-min) 300) (widen))",
        "(let ((other (generate-new-buffer \"line-index-swap\")))
           (with-current-buffer other (insert \"a\\nb\\nc\\n\"))
           (buffer-swap-text other)
           (goto-char 2) (insert \"\\n\")
           (buffer-swap-text other)
           (kill-buffer other))",
        "(progn (erase-buffer) (insert (make-string 50 ?\\n) \"tail\"))",
    ];
    eval_str(
        &mut ev,
        &format!("(progn (erase-buffer) (insert {}))", lisp_string(&text)),
    );
    for edit in edits {
        with_text_line_index_config(eager(TextLineIndexMode::Verify), || {
            // Make sure an index exists before the edit, so the edit
            // maintains it rather than a later query building it.
            eval_str(&mut ev, "(line-number-at-pos (point-max))");
            eval_str(&mut ev, edit);
        });
        let (off, on) = both_ways(&mut ev, probe);
        assert_eq!(off, on, "after {edit}");
        // Refill for the next edit.
        eval_str(
            &mut ev,
            &format!(
                "(progn (widen) (goto-char (point-max)) (insert {}))",
                lisp_string(&text[..text.len() / 3])
            ),
        );
    }
}
