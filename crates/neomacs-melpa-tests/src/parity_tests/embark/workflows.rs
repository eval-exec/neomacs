use expect_test::expect;

use super::ParityBatchCase;

/// Drive `embark-target-url-at-point' on a buffer with a URL at point and
/// assert it returns the URL target (type + value).
fn url_finder_identifies_url_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "url_finder_identifies_url_at_point",
        r####"
(with-temp-buffer
  (insert "see https://example.com/path now")
  (goto-char (point-min))
  (re-search-forward "https://example\\.com/path")
  (goto-char (match-beginning 0))
  (forward-char 2)
  (let ((target (embark-target-url-at-point)))
    (list :found (and target t)
          :type (car target)
          :value (cadr target))))
"####,
        expect![[r#"OK (:found t :type url :value "https://example.com/path")"#]],
    )
}

/// Point on an inner open paren: `embark-target-expression-at-point' reads
/// that nested sexp (the top-level defun guard filters whole-buffer sexps).
fn expression_finder_reads_inner_sexp() -> ParityBatchCase {
    ParityBatchCase::value(
        "expression_finder_reads_inner_sexp",
        r####"
(with-temp-buffer
  (insert "(foo (bar baz))")
  (goto-char (point-min))
  (re-search-forward "(bar baz)")
  (goto-char (match-beginning 0))
  (let ((target (embark-target-expression-at-point)))
    (list :type (car target)
          :value (cadr target))))
"####,
        expect![[r#"OK (:type expression :value "(bar baz)")"#]],
    )
}

/// Drive `embark-target-email-at-point' on an email address at point.
fn email_finder_identifies_email_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "email_finder_identifies_email_at_point",
        r####"
(with-temp-buffer
  (insert "contact a@b.com please")
  (goto-char (point-min))
  (re-search-forward "a@b\\.com")
  (goto-char (match-beginning 0))
  (forward-char 1)
  (let ((target (embark-target-email-at-point)))
    (list :found (and target t)
          :type (car target)
          :value (cadr target))))
"####,
        expect![[r#"OK (:found t :type email :value "a@b.com")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        url_finder_identifies_url_at_point(),
        expression_finder_reads_inner_sexp(),
        email_finder_identifies_email_at_point(),
    ]
}
