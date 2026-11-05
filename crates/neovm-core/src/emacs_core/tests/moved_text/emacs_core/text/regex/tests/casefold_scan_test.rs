//! Case-folded candidate scans in `re_search`.
//!
//! A case-folded search may skip a position only where the per-character loop
//! would have skipped it.  These tests pin down the facts that make the fast
//! scans exact, and compare them with the exhaustive scan.

use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

use neovm_text::emacs_core::regex_emacs::*;

/// Lisp buffer searches translate through the buffer's case-canon
/// char-table, the standard one included (its identity is not known after a
/// dump is loaded, and the hardwired translation would not fold σ/ς or µ/μ
/// as GNU's table does).  The fast scans must engage there: a case-folded
/// `re-search-forward` over 5000 `(` enters the matcher a handful of times,
/// and finds what the exhaustive scan finds.
#[test]
fn lisp_case_folded_search_engages_the_fast_scans() {
    crate::test_utils::init_test_tracing();
    let form = r#"(with-temp-buffer
                    (dotimes (_ 5000) (insert "( x "))
                    (insert "(DEFUN abc) (\x212Aey)")
                    (goto-char (point-min))
                    (let ((case-fold-search t))
                      (list (re-search-forward "(defun \\([a-z]+\\)" nil t)
                            (match-beginning 1)
                            (progn (goto-char (point-max))
                                   (re-search-backward "(defun" nil t))
                            (progn (goto-char (point-min))
                                   (re-search-forward "(key" nil t))
                            (string-match "σ" "ς")
                            (string-match "k" (string #x212A)))))"#;
    let eval = |form: &str| {
        let before = matcher_entry_count();
        let result = dfa::with_dfa_mode(dfa::DfaMode::Off, || {
            crate::test_utils::runtime_startup_eval_one(form)
        });
        (result, matcher_entry_count() - before)
    };
    // Each evaluation in the cached runtime makes searches of its own, and
    // the first one also builds the runtime.
    let _ = eval("nil");
    let (_, baseline) = eval("nil");
    let (fast, fast_entries) = eval(form);
    let (exhaustive, exhaustive_entries) = with_fastmap_disabled(|| eval(form));
    assert_eq!(exhaustive, "OK (20011 20008 20001 nil 0 nil)");
    assert_eq!(fast, exhaustive);
    assert!(
        exhaustive_entries > baseline + 40_000,
        "the exhaustive scan tries every position: {exhaustive_entries} (baseline {baseline})"
    );
    assert!(
        fast_entries < baseline + 10,
        "the fast scans should skip to the literals: {fast_entries} (baseline {baseline})"
    );
}
