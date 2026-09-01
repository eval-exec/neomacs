//! Complex oracle tests combining buffer operations with algorithms:
//! in-buffer sorting, buffer-based diff, template expansion in buffer,
//! buffer-based state machine, and streaming buffer processing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// In-buffer line sorting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_sort_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "cherry\napple\nbanana\ndate\nelderberry\n")
                    ;; Collect lines
                    (goto-char (point-min))
                    (let ((lines nil))
                      (while (not (eobp))
                        (setq lines
                              (cons (buffer-substring
                                     (line-beginning-position)
                                     (line-end-position))
                                    lines))
                        (forward-line 1))
                      ;; Sort and rebuild
                      (setq lines (sort (nreverse lines) #'string<))
                      (erase-buffer)
                      (dolist (l lines)
                        (insert l "\n"))
                      (buffer-string)))"####;
    let expect =
        expect_test::expect![[r#""OK \"apple\\nbanana\\ncherry\\ndate\\nelderberry\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based key=value config parser/writer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_config_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "name=Alice\nage=30\ncity=Boston\nrole=dev\n")
                    ;; Parse
                    (goto-char (point-min))
                    (let ((config nil))
                      (while (re-search-forward
                              "^\\([^=]+\\)=\\(.*\\)$" nil t)
                        (setq config
                              (cons (cons (match-string 1)
                                          (match-string 2))
                                    config)))
                      (setq config (nreverse config))
                      ;; Modify
                      (setcdr (assoc "age" config) "31")
                      (setq config
                            (append config
                                    (list (cons "team" "core"))))
                      ;; Write back
                      (erase-buffer)
                      (dolist (pair config)
                        (insert (car pair) "=" (cdr pair) "\n"))
                      (list config (buffer-string))))"####;
    let expect = expect_test::expect![[
        r#""OK (((\"name\" . \"Alice\") (\"age\" . \"31\") (\"city\" . \"Boston\") (\"role\" . \"dev\") (\"team\" . \"core\")) \"name=Alice\\nage=31\\ncity=Boston\\nrole=dev\\nteam=core\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer template expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_template_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "Dear {{name}},\n\n")
                    (insert "Thank you for your order #{{order_id}}.\n")
                    (insert "We will ship {{item_count}} items to {{city}}.\n")
                    (insert "\nBest regards,\n{{company}}")
                    (let ((vars '(("name" . "Alice")
                                  ("order_id" . "12345")
                                  ("item_count" . "3")
                                  ("city" . "Boston")
                                  ("company" . "ACME Corp"))))
                      (dolist (v vars)
                        (goto-char (point-min))
                        (while (search-forward
                                (concat "{{" (car v) "}}") nil t)
                          (replace-match (cdr v) t t)))
                      (buffer-string)))"####;
    let expect = expect_test::expect![[
        r#""OK \"Dear Alice,\\n\\nThank you for your order #12345.\\nWe will ship 3 items to Boston.\\n\\nBest regards,\\nACME Corp\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based word counting with position tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_word_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "The quick brown fox jumps over the lazy dog.\n")
                    (insert "The fox was very quick and very lazy.\n")
                    (goto-char (point-min))
                    (let ((word-freq (make-hash-table :test 'equal))
                          (word-positions (make-hash-table :test 'equal)))
                      (while (re-search-forward "\\b\\([a-zA-Z]+\\)\\b" nil t)
                        (let* ((word (downcase (match-string 1)))
                               (pos (match-beginning 0)))
                          (puthash word (1+ (gethash word word-freq 0))
                                   word-freq)
                          (puthash word
                                   (append (gethash word word-positions nil)
                                           (list pos))
                                   word-positions)))
                      ;; Find words appearing more than once
                      (let ((repeated nil))
                        (maphash (lambda (w count)
                                   (when (> count 1)
                                     (setq repeated
                                           (cons (list w count
                                                       (gethash w word-positions))
                                                 repeated))))
                                 word-freq)
                        (sort repeated
                              (lambda (a b)
                                (or (> (nth 1 a) (nth 1 b))
                                    (and (= (nth 1 a) (nth 1 b))
                                         (string< (car a) (car b)))))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"the\" 3 (1 32 46)) (\"fox\" 2 (17 50)) (\"lazy\" 2 (36 78)) (\"quick\" 2 (5 63)) (\"very\" 2 (58 73)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based markdown heading extractor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_markdown_toc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "# Introduction\n\nSome text.\n\n")
                    (insert "## Background\n\nMore text.\n\n")
                    (insert "## Methods\n\n### Data Collection\n\n")
                    (insert "### Analysis\n\n## Results\n\n")
                    (insert "# Conclusion\n")
                    (goto-char (point-min))
                    (let ((toc nil))
                      (while (re-search-forward
                              "^\\(#+\\) \\(.+\\)$" nil t)
                        (let ((level (length (match-string 1)))
                              (title (match-string 2))
                              (line (line-number-at-pos
                                     (match-beginning 0))))
                          (setq toc
                                (cons (list level title line) toc))))
                      ;; Format TOC with indentation
                      (mapconcat
                       (lambda (entry)
                         (let ((indent (make-string
                                        (* 2 (1- (nth 0 entry))) ?\ )))
                           (format "%s- %s (L%d)"
                                   indent
                                   (nth 1 entry)
                                   (nth 2 entry))))
                       (nreverse toc)
                       "\n")))"####;
    let expect = expect_test::expect![[
        r#""OK \"- Introduction (L1)\\n  - Background (L5)\\n  - Methods (L9)\\n    - Data Collection (L11)\\n    - Analysis (L13)\\n  - Results (L15)\\n- Conclusion (L17)\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based CSV transform
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufalgo_csv_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read CSV, filter and transform, write back
    let form = r####"(with-temp-buffer
                    (insert "name,age,dept\n")
                    (insert "Alice,30,eng\n")
                    (insert "Bob,25,qa\n")
                    (insert "Carol,35,eng\n")
                    (insert "Dave,28,ops\n")
                    (insert "Eve,32,eng\n")
                    ;; Parse
                    (goto-char (point-min))
                    (forward-line 1) ;; skip header
                    (let ((records nil))
                      (while (not (eobp))
                        (let ((line (buffer-substring
                                     (line-beginning-position)
                                     (line-end-position))))
                          (when (> (length line) 0)
                            (let ((fields (split-string line ",")))
                              (setq records
                                    (cons (list (nth 0 fields)
                                                (string-to-number (nth 1 fields))
                                                (nth 2 fields))
                                          records)))))
                        (forward-line 1))
                      ;; Filter: eng dept, sort by age
                      (setq records
                            (sort
                             (seq-filter
                              (lambda (r) (string= (nth 2 r) "eng"))
                              (nreverse records))
                             (lambda (a b) (< (nth 1 a) (nth 1 b)))))
                      ;; Write back as TSV
                      (erase-buffer)
                      (insert "name\tage\n")
                      (dolist (r records)
                        (insert (nth 0 r) "\t"
                                (number-to-string (nth 1 r)) "\n"))
                      (buffer-string)))"####;
    let expect =
        expect_test::expect![[r#""OK \"name\tage\\nAlice\t30\\nEve\t32\\nCarol\t35\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
