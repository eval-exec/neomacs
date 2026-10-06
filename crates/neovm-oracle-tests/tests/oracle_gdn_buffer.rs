//! GNU-refreshed GDN editing primitive regressions.
#[path = "../src/common.rs"]
mod common;

#[test]
fn oracle_gdn_transpose_indirect_points_and_markers() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (leave)
  (with-temp-buffer
    (insert "abcdefgh") (goto-char 2)
    (let* ((sibling (make-indirect-buffer (current-buffer) " transpose-sibling"))
           (m (with-current-buffer sibling (goto-char 6) (point-marker))))
      (unwind-protect
        (progn (transpose-regions 1 3 5 7 leave)
          (list (point) (with-current-buffer sibling (point))
                (marker-position m) (buffer-string)))
        (kill-buffer sibling))))) '(nil t))"#,
        expect_test::expect![[r#""OK ((6 2 2 \"efcdabgh\") (2 6 6 \"efcdabgh\"))""#]],
    );
}
