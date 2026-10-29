//! The wire-log reader must never parse a record the server is still writing.

use expect_test::expect;

use super::{ParityBatchCase, assert_oracle_batch_cases, lsp_pyright_oracle};

#[test]
fn wire_log_reader_ignores_an_unterminated_record() {
    let case = ParityBatchCase::value(
        "wire_log_reader_partial_tail",
        r##"(let* ((path (expand-file-name
                      "wire-log-partial.jsonl"
                      (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (append (lambda (text)
                          (with-temp-buffer
                            (insert text)
                            (append-to-file (point-min) (point-max) path))))
                (ids (lambda (records)
                       (mapcar (lambda (record)
                                 (neomacs-lsp-pyright-test-json record "id"))
                               records))))
         (funcall append "{\"id\":1}\n")
         ;; A record the server is still writing: no terminating newline yet.
         (funcall append "{\"id\":2,\"unfinished\":")
         (let ((complete (funcall ids
                                  (neomacs-lsp-pyright-test-read-json-lines path))))
           (funcall append "\"tail\"}\n")
           (list :complete complete
                 :finished (funcall ids
                                    (neomacs-lsp-pyright-test-read-json-lines path)))))"##,
        expect![[r#"OK (:complete (1) :finished (1 2))"#]],
    );
    assert_oracle_batch_cases(
        lsp_pyright_oracle("pyright"),
        "wire-log-reader-contract",
        "lsp_pyright_harness",
        &[case],
    );
}
