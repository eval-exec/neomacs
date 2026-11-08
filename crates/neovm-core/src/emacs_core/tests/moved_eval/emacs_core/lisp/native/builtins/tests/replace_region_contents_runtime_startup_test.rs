//! Undo restoration asked of the loaded runtime image.
//!
//! `runtime_startup_eval_one` is minted in core. The Myers-diff and
//! minimal-runtime rows stay in eval. Assertions are unchanged.

use crate::test_utils::runtime_startup_eval_one;

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
        let restored = runtime_startup_eval_one(&replace_region_contents_undo_source(
            orig,
            new,
            "(progn (primitive-undo 1 buffer-undo-list) (buffer-string))",
        ));
        assert_eq!(restored, format!("OK {orig}"), "undoing {orig} -> {new}");
    }
}
