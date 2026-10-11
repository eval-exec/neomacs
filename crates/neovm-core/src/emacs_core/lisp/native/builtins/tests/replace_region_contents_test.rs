//! Tests for `replace-region-contents` / `replace-buffer-contents`, asserting
//! GNU Emacs behavior (oracle: `emacs --batch`).
//!
//! Two GNU-faithful properties are covered:
//!   (A) a FUNCTION SOURCE is called (with the region narrowed) and its return
//!       value used as the actual source, rather than throwing.
//!   (B) the replacement is the minimal (Myers-diff) non-destructive edit, so
//!       markers, point and untouched text outside the changed runs are
//!       preserved exactly as GNU does.
//!
//! These use the lightweight `subr.el` runtime (which defines `with-temp-buffer`
//! and the GNU `replace-buffer-contents` wrapper that delegates to the
//! `replace-region-contents` builtin) rather than the full loadup bootstrap.

use super::super::buffer::{ReplaceRegionChangeRun, replace_region_minimal_change_runs};
use crate::emacs_core::Context;
use crate::emacs_core::format_eval_result;
use crate::test_utils::load_minimal_gnu_backquote_runtime;

/// Convenience to turn an `&str` into a codepoint vector for the diff tests.
fn codes(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn run(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> ReplaceRegionChangeRun {
    ReplaceRegionChangeRun {
        a_start,
        a_end,
        b_start,
        b_end,
    }
}

#[test]
fn minimal_change_runs_pure_insertion() {
    // "abcdef" -> "abcXYZdef": insert "XYZ" between index 3 and 3.
    assert_eq!(
        replace_region_minimal_change_runs(&codes("abcdef"), &codes("abcXYZdef")),
        vec![run(3, 3, 3, 6)]
    );
}

#[test]
fn minimal_change_runs_identical_is_empty() {
    assert_eq!(
        replace_region_minimal_change_runs(&codes("abcdef"), &codes("abcdef")),
        Vec::<ReplaceRegionChangeRun>::new()
    );
}

#[test]
fn minimal_change_runs_two_insertions_keep_interior_match() {
    // "abc" -> "aXbYc": insert X before b, insert Y before c. The "b" between
    // them must remain a matched (untouched) character, i.e. two separate runs.
    assert_eq!(
        replace_region_minimal_change_runs(&codes("abc"), &codes("aXbYc")),
        vec![run(1, 1, 1, 2), run(2, 2, 3, 4)]
    );
}

#[test]
fn minimal_change_runs_two_deletions_keep_interior_match() {
    // "abcde" -> "ace": delete b and d, keeping c between them.
    assert_eq!(
        replace_region_minimal_change_runs(&codes("abcde"), &codes("ace")),
        vec![run(1, 2, 1, 1), run(3, 4, 2, 2)]
    );
}

#[test]
fn minimal_change_runs_substitution() {
    // "abZZef" region "ZZ" (indices 2..4) replaced by "CD": a single run.
    assert_eq!(
        replace_region_minimal_change_runs(&codes("ZZ"), &codes("CD")),
        vec![run(0, 2, 0, 2)]
    );
}

/// Evaluate FORMS in a fresh evaluator with the minimal GNU `subr.el` runtime
/// loaded, returning the formatted result of the final form
/// (`OK <printed>` / `ERR ...`), matching the other builtin tests' style.
fn eval_one(source: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);
    let results = eval.eval_str_each(source);
    let last = results.last().expect("at least one form");
    format_eval_result(last)
}

#[test]
fn replace_region_contents_calls_function_source() {
    // GNU: (with-temp-buffer (insert "abcdef")
    //        (replace-region-contents 2 5 (lambda () "ZZ")) (buffer-string))
    //   => "aZZef"
    let result = eval_one(
        r#"(with-temp-buffer
              (insert "abcdef")
              (replace-region-contents 2 5 (lambda () "ZZ"))
              (buffer-string))"#,
    );
    assert_eq!(result, r#"OK "aZZef""#);
}

#[test]
fn replace_region_contents_function_source_sees_narrowed_region() {
    // The function is called with the buffer narrowed to BEG..END, so
    // `buffer-string` inside it returns only the region's text.  GNU returns
    // "a<bcd>ef" where the replacement is the bracketed region.
    let result = eval_one(
        r#"(with-temp-buffer
              (insert "abcdef")
              (replace-region-contents 2 5
                (lambda () (concat "<" (buffer-string) ">")))
              (buffer-string))"#,
    );
    assert_eq!(result, r#"OK "a<bcd>ef""#);
}

#[test]
fn replace_buffer_contents_preserves_markers_minimally() {
    // GNU: inserting "XYZ" in the middle keeps the marker before the change at
    // its position and shifts the marker after it by the inserted length.
    //   => ("abcXYZdef" 2 8)
    let result = eval_one(
        r#"(let ((src (generate-new-buffer " s")) o)
              (with-current-buffer src (insert "abcXYZdef"))
              (with-temp-buffer
                (insert "abcdef")
                (let ((m1 (copy-marker 2)) (m2 (copy-marker 5)))
                  (replace-buffer-contents src)
                  (setq o (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)))))
              (kill-buffer src)
              o)"#,
    );
    assert_eq!(result, r#"OK ("abcXYZdef" 2 8)"#);
}

#[test]
fn replace_buffer_contents_identical_does_not_disturb_markers() {
    // Replacing a buffer with identical contents must not move any marker;
    // GNU leaves the marker at position 5.
    let result = eval_one(
        r#"(let ((src (generate-new-buffer " s")) o)
              (with-current-buffer src (insert "abcdef"))
              (with-temp-buffer
                (insert "abcdef")
                (let ((m (copy-marker 5)))
                  (replace-buffer-contents src)
                  (setq o (list (buffer-string) (marker-position m)))))
              (kill-buffer src)
              o)"#,
    );
    assert_eq!(result, r#"OK ("abcdef" 5)"#);
}

#[test]
fn replace_buffer_contents_preserves_point_minimally() {
    // Point at 4 (between "abc" and "def") is treated like an insert-before
    // marker; after inserting "XYZ" before "def" GNU leaves point at 7.
    let result = eval_one(
        r#"(let ((src (generate-new-buffer " s")) o)
              (with-current-buffer src (insert "abcXYZdef"))
              (with-temp-buffer
                (insert "abcdef")
                (goto-char 4)
                (replace-buffer-contents src)
                (setq o (list (buffer-string) (point))))
              (kill-buffer src)
              o)"#,
    );
    assert_eq!(result, r#"OK ("abcXYZdef" 7)"#);
}

#[test]
fn replace_region_contents_relocates_point_across_multiple_change_runs() {
    // GNU `replace_range` relocates point as an insert-before marker for each
    // change run.  This compact-to-pretty JSON replacement contains several
    // separate insertions.  Every possible starting point exercises the
    // cumulative mapping; in particular, point at the original END must finish
    // at the replacement END so callers can continue parsing from there.
    let result = eval_one(
        r#"(mapcar
             (lambda (position)
               (with-temp-buffer
                 (insert "{\"x\":1}")
                 (goto-char position)
                 (replace-region-contents
                  (point-min) (point-max) "{\n  \"x\": 1\n}")
                 (point)))
             '(1 2 3 4 5 6 7 8))"#,
    );
    assert_eq!(result, "OK (1 5 6 7 8 10 12 13)");
}

#[test]
fn replace_buffer_contents_multi_run_marker_placement_matches_gnu() {
    // Two separate insertions (X before b, Y before c) — a case with multiple
    // change runs.  GNU markers 1..4 => 1 2 4 6.
    let result = eval_one(
        r#"(let ((src (generate-new-buffer " s")) o)
              (with-current-buffer src (insert "aXbYc"))
              (with-temp-buffer
                (insert "abc")
                (let ((m1 (copy-marker 1)) (m2 (copy-marker 2))
                      (m3 (copy-marker 3)) (m4 (copy-marker 4)))
                  (replace-buffer-contents src)
                  (setq o (list (buffer-string)
                                (marker-position m1) (marker-position m2)
                                (marker-position m3) (marker-position m4)))))
              (kill-buffer src)
              o)"#,
    );
    assert_eq!(result, r#"OK ("aXbYc" 1 2 4 6)"#);
}

#[test]
fn replace_buffer_contents_multibyte_marker_placement_matches_gnu() {
    // Multibyte content (varying byte widths) must map char positions to byte
    // offsets correctly.  A="αβ" -> B="α世βX"; GNU markers 1..3 => 1 2 4.
    let result = eval_one(
        "(let ((src (generate-new-buffer \" s\")) o)
              (with-current-buffer src (insert \"\u{3b1}\u{4e16}\u{3b2}X\"))
              (with-temp-buffer
                (insert \"\u{3b1}\u{3b2}\")
                (let ((m1 (copy-marker 1)) (m2 (copy-marker 2)) (m3 (copy-marker 3)))
                  (replace-buffer-contents src)
                  (setq o (list (buffer-string)
                                (marker-position m1) (marker-position m2) (marker-position m3)))))
              (kill-buffer src)
              o)",
    );
    assert_eq!(result, "OK (\"\u{3b1}\u{4e16}\u{3b2}X\" 1 2 4)");
}

#[test]
fn replace_region_contents_string_source_preserves_text_properties() {
    // A string SOURCE with text properties is spliced in with its properties
    // intact.  GNU: ("abCDef" bold).
    let result = eval_one(
        r#"(with-temp-buffer
              (insert "abZZef")
              (replace-region-contents 3 5 (propertize "CD" 'face 'bold))
              (list (buffer-substring-no-properties 1 (point-max))
                    (get-text-property 3 'face)))"#,
    );
    assert_eq!(result, r#"OK ("abCDef" bold)"#);
}

#[test]
fn replace_region_contents_vector_source_preserves_text_properties() {
    // A [SBUF SBEG SEND] vector SOURCE copies the substring with its
    // properties.  GNU: ("ab23ef" italic italic).
    let result = eval_one(
        r#"(let ((src (get-buffer-create "*rrc-src*")))
              (with-current-buffer src
                (erase-buffer)
                (insert "1234")
                (put-text-property 2 4 'face 'italic))
              (prog1
                  (with-temp-buffer
                    (insert "abZZef")
                    (replace-region-contents 3 5 (vector src 2 4))
                    (list (buffer-substring-no-properties 1 (point-max))
                          (get-text-property 3 'face)
                          (get-text-property 4 'face)))
                (kill-buffer src)))"#,
    );
    assert_eq!(result, r#"OK ("ab23ef" italic italic)"#);
}

#[test]
fn replace_region_contents_keeps_source_properties_without_inheriting_destination_properties() {
    // GNU 31 passes false as replace_range's property-inheritance flag even
    // when replace-region-contents receives a non-nil INHERIT argument.  The
    // optional argument occupies replace_range's independent match-data slot.
    // Thus the unchanged destination prefix keeps its property, the inserted
    // suffix keeps its source property, and the suffix does not acquire the
    // adjoining destination property.
    let result = eval_one(
        r#"(with-temp-buffer
              (insert "x")
              (put-text-property 1 2 'destination 'kept)
              (replace-region-contents
               1 2 (propertize "xy" 'source 'inserted)
               0.1 nil 'inherit)
              (buffer-string))"#,
    );
    assert_eq!(
        result,
        r#"OK #("xy" 0 1 (destination kept) 1 2 (source inserted))"#
    );
}

#[test]
fn replace_region_contents_rejects_self_buffer() {
    // GNU signals "Cannot replace a buffer with itself".
    let result = eval_one(
        r#"(with-temp-buffer
              (insert "abcdef")
              (condition-case err
                  (replace-region-contents 1 4 (current-buffer))
                (error (list (car err) (car (cdr err))))))"#,
    );
    assert_eq!(
        result,
        r#"OK (error "Cannot replace a buffer with itself")"#
    );
}

#[test]
fn replace_buffer_contents_deletion_run_marker_placement_matches_gnu() {
    // Deletions of "b" and "d".  GNU markers 1..6 => 1 2 2 3 3 4.
    let result = eval_one(
        r#"(let ((src (generate-new-buffer " s")) o)
              (with-current-buffer src (insert "ace"))
              (with-temp-buffer
                (insert "abcde")
                (let ((m1 (copy-marker 1)) (m2 (copy-marker 2)) (m3 (copy-marker 3))
                      (m4 (copy-marker 4)) (m5 (copy-marker 5)) (m6 (copy-marker 6)))
                  (replace-buffer-contents src)
                  (setq o (list (buffer-string)
                                (marker-position m1) (marker-position m2) (marker-position m3)
                                (marker-position m4) (marker-position m5) (marker-position m6)))))
              (kill-buffer src)
              o)"#,
    );
    assert_eq!(result, r#"OK ("ace" 1 2 2 3 3 4)"#);
}

/// Run `replace-region-contents` over the whole of a temp buffer holding
/// ORIG, replacing it with NEW supplied through a buffer SOURCE, and return
/// FORM's value with `buffer-undo-list` freshly enabled for the replacement
/// only.
fn replace_region_contents_undo_probe(orig: &str, new: &str, form: &str) -> String {
    eval_one(&replace_region_contents_undo_source(orig, new, form))
}

fn replace_region_contents_undo_source(orig: &str, new: &str, form: &str) -> String {
    format!(
        r#"(with-temp-buffer
              (insert {orig})
              (setq buffer-undo-list nil)
              (let ((temp (with-current-buffer (generate-new-buffer " *r*")
                            (insert {new}) (current-buffer))))
                (replace-region-contents (point-min) (point-max)
                                         (lambda (&rest _) temp)))
              {form})"#
    )
}

#[test]
fn replace_region_contents_undo_restores_the_original_text() {
    // DIVERGENCES.md 47 (data-loss class): undoing a `replace-region-contents`
    // whose Myers diff produces two adjacent insertion runs used to give back
    // "grmet" -- neither the original nor the replacement.  GNU restores
    // "greet" exactly.  The other rows are the single-hunk shapes that already
    // worked and must stay working.
    for (orig, new) in [
        (r#""greet""#, r#""welcome""#),
        (r#""abc""#, r#""xyz""#),
        (r#""abcde""#, r#""aZcYe""#),
        (r#""abcde""#, r#""aZZcYYe""#),
        (r#""abc""#, r#""aXbYc""#),
    ] {
        // Replaying the records is lisp/simple.el's `primitive-undo', which
        // has no Rust implementation (GNU has no C one either), so this row
        // is asked of the loaded runtime rather than of a bare evaluator.
        let restored =
            crate::test_utils::runtime_startup_eval_one(&replace_region_contents_undo_source(
                orig,
                new,
                "(progn (primitive-undo 1 buffer-undo-list) (buffer-string))",
            ));
        assert_eq!(restored, format!("OK {orig}"), "undoing {orig} -> {new}");
    }
}

#[test]
fn replace_region_contents_undo_list_shape_matches_gnu() {
    // The undo-list shape is the stronger pin: text can come back right by
    // accident, but only GNU's exact record sequence proves the recording
    // shape.  GNU `replace_range` records `record_insert` then
    // `record_delete` for EVERY change run whatever the run's lengths, so a
    // pure-insertion run still conses a zero-length deletion ("" . POS) and a
    // pure-deletion run still conses a zero-length insertion (POS . POS).
    // Those zero-length records are load-bearing: `record_insert` coalesces
    // into the newest record only when that record is an insertion ending
    // where the new one begins, so they keep two adjacent runs from merging
    // into one wide record that undo would then delete as a single span.
    //
    // Oracle: emacs 31.0.90 --batch, `buffer-undo-list` after the replacement.
    for (orig, new, undo_list) in [
        (
            r#""greet""#,
            r#""welcome""#,
            r#"(("gr" . 1) (3 . 4) ("" . 4) (4 . 8) ("t" . -5) (6 . 6))"#,
        ),
        (r#""abc""#, r#""xyz""#, r#"(("abc" . -1) (4 . 7))"#),
        (
            r#""abcde""#,
            r#""aZcYe""#,
            r#"(("b" . 2) (3 . 4) ("d" . 4) (5 . 6) 6)"#,
        ),
        (
            r#""abcde""#,
            r#""aZZcYYe""#,
            r#"(("b" . 2) (3 . 5) ("d" . 4) (5 . 7) 6)"#,
        ),
        (
            r#""abc""#,
            r#""aXbYc""#,
            r#"(("" . 2) (2 . 3) ("" . 3) (3 . 4) 4)"#,
        ),
    ] {
        let recorded = replace_region_contents_undo_probe(orig, new, "buffer-undo-list");
        assert_eq!(
            recorded,
            format!("OK {undo_list}"),
            "undo list for {orig} -> {new}"
        );
    }
}

#[test]
fn replace_region_contents_records_gnu_undo_boundary_before_the_change_runs() {
    // GNU `Freplace_region_contents` calls `Fundo_boundary` once compareseq
    // has succeeded and before it walks the change runs, so a pre-existing
    // undo list gets a boundary consed onto it -- even when the diff turns out
    // to be empty.  The boundary also sets `point_before_last_command_or_undo`,
    // which is why the first change run conses the point entry that follows
    // it.  The trivial "one side is empty" path returns before reaching
    // `Fundo_boundary`, so it records no boundary at all (and its
    // pure-deletion run still conses the zero-length insertion `(4 . 4)`).
    //
    // Oracle: emacs 31.0.90 --batch.
    let seeded = |orig: &str, new: &str| {
        eval_one(&format!(
            r#"(with-temp-buffer
                  (insert "{orig}")
                  (setq buffer-undo-list '((1 . 2)))
                  (let ((temp (with-current-buffer (generate-new-buffer " *r*")
                                (insert "{new}") (current-buffer))))
                    (replace-region-contents (point-min) (point-max)
                                             (lambda (&rest _) temp)))
                  buffer-undo-list)"#
        ))
    };
    assert_eq!(seeded("abc", "abc"), "OK (nil (1 . 2))");
    assert_eq!(
        seeded("abcde", "aZcYe"),
        r#"OK (("b" . 2) (3 . 4) ("d" . 4) (5 . 6) 6 nil (1 . 2))"#
    );
    assert_eq!(seeded("abc", ""), r#"OK (("abc" . -1) (4 . 4) (1 . 2))"#);

    // The point entry is the point at the boundary, not the buffer end.
    let point_not_at_eob = eval_one(
        r#"(with-temp-buffer
              (insert "abcde")
              (setq buffer-undo-list nil)
              (goto-char 2)
              (let ((temp (with-current-buffer (generate-new-buffer " *r*")
                            (insert "aZcYe") (current-buffer))))
                (replace-region-contents (point-min) (point-max)
                                         (lambda (&rest _) temp)))
              (list (point) buffer-undo-list))"#,
    );
    assert_eq!(
        point_not_at_eob,
        r#"OK (2 (("b" . 2) (3 . 4) ("d" . 4) (5 . 6) 2))"#
    );
}

#[test]
fn replace_region_contents_insertion_run_reaches_indirect_buffer_siblings() {
    // A pure-insertion change run is applied in the base buffer while an
    // indirect buffer holds the markers and the overlay.  GNU
    // `replace_range` delegates `old_chars == 0` to
    // `adjust_markers_for_insert` (insdel.c:351), so the plain marker at the
    // insertion point stays put, the insertion-type marker advances, and the
    // overlay grows -- and all of that has to reach the sibling buffer.
    //
    // This pins the sibling side of the empty-old-range replacement, which is
    // the seam the recording unification moves from insert-shaped metadata to
    // replace-shaped metadata.
    //
    // Oracle: emacs 31.0.90 --batch => ("aXbYc" 2 3 2 4 1).
    let result = eval_one(
        r#"(let* ((base (generate-new-buffer " base"))
                  (ind (make-indirect-buffer base " ind"))
                  out)
             (with-current-buffer base (insert "abc"))
             (with-current-buffer ind
               (let ((m-plain (copy-marker 2))
                     (m-advance (copy-marker 2 t))
                     (ov (make-overlay 2 3)))
                 (with-current-buffer base
                   (let ((temp (with-current-buffer (generate-new-buffer " *r*")
                                 (insert "aXbYc") (current-buffer))))
                     (replace-region-contents (point-min) (point-max)
                                              (lambda (&rest _) temp))))
                 (setq out (list (buffer-string)
                                 (marker-position m-plain)
                                 (marker-position m-advance)
                                 (overlay-start ov) (overlay-end ov)
                                 (point)))))
             (kill-buffer ind) (kill-buffer base)
             out)"#,
    );
    assert_eq!(result, r#"OK ("aXbYc" 2 3 2 4 1)"#);
}
