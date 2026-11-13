//! GNU Emacs 31.1 snapshots for the width-only format scan optimization.
//! Captured from GNU Emacs 31.1 with its standard character width table.
//! Precision 0 with leading zero-width characters remains outside this patch.

#[test]
fn gdh_format_width_scan_observes_mutable_widths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(let ((char-width-table (copy-sequence char-width-table)))
  (aset char-width-table ?é 7)
  (let ((wide (format "%3s" "éabcdef")))
    (aset char-width-table ?é 0)
    (let ((zero (format "%3s" "éé")))
      (aset char-width-table ?é 2)
      (list wide zero (format "%3s" "éé")
            (format "%-3s" "éé") (format "%0s" "éabcdef")))))"#,
        ),
        r#"OK ("éabcdef" "   éé" "éé" "éé" "éabcdef")"#,
    );
}

#[test]
fn gdh_format_width_scan_zero_width_content_and_alignment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(let ((char-width-table (copy-sequence char-width-table)))
  (aset char-width-table ?é 0)
  (list (format "%4s" "éé") (format "%-4s" "éé")
        (format "%4s" "ééabc") (format "%-4s" "ééabc")))"#,
        ),
        r#"OK ("    éé" "éé    " " ééabc" "ééabc ")"#,
    );
}

#[test]
fn gdh_format_width_scan_preserves_full_property_bounds() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(let* ((s (propertize "abcdef" 'face 'bold))
       (short-left (format "%-2s" s))
       (short-right (format "%2s" s))
       (pad-left (format "%-9s" s))
       (pad-right (format "%9s" s)))
  (list (substring-no-properties short-left)
        (substring-no-properties short-right)
        (get-text-property 5 'face short-left)
        (get-text-property 5 'face short-right)
        (substring-no-properties pad-left)
        (get-text-property 5 'face pad-left)
        (substring-no-properties pad-right)
        (get-text-property 0 'face pad-right)
        (get-text-property 3 'face pad-right)
        (get-text-property 8 'face pad-right)))"#,
        ),
        r#"OK ("abcdef" "abcdef" bold bold "abcdef   " bold "   abcdef" nil bold bold)"#,
    );
}

#[test]
fn gdh_format_width_scan_keeps_precision_truncation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(let ((char-width-table (copy-sequence char-width-table)))
  (aset char-width-table ?é 2)
  (list (format "%2.4s" "abcdef")
        (format "%1.4s" "éééabc")
        (format "%-6.4s" "éééabc")
        (format "%6.0s" "abcdef")))"#,
        ),
        r#"OK ("abcd" "éé" "éé  " "      ")"#,
    );
}
