//! Advanced oracle parity tests for regexp search patterns in buffers:
//! `re-search-forward` with COUNT, NOERROR, BOUND args, `re-search-backward`
//! combined with match data, capturing groups in buffer searches, search+replace
//! loops, multi-line regexp patterns, and search with narrowing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// re-search-forward with COUNT parameter — find Nth occurrence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_forward_count_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "aa-11 bb-22 cc-33 dd-44 ee-55 ff-66")
  (goto-char (point-min))
  ;; COUNT=1 (default): find first match
  (let ((r1 (progn (goto-char (point-min))
                   (re-search-forward "[a-z]+-[0-9]+" nil t 1)
                   (list (match-string 0) (point)))))
    ;; COUNT=3: find 3rd occurrence from beginning
    (let ((r2 (progn (goto-char (point-min))
                     (re-search-forward "[a-z]+-[0-9]+" nil t 3)
                     (list (match-string 0) (point)))))
      ;; COUNT=6: find last (6th) occurrence
      (let ((r3 (progn (goto-char (point-min))
                       (re-search-forward "[a-z]+-[0-9]+" nil t 6)
                       (list (match-string 0) (point)))))
        ;; COUNT=7: one past the last, should return nil with NOERROR=t
        (let ((r4 (progn (goto-char (point-min))
                         (re-search-forward "[a-z]+-[0-9]+" nil t 7))))
          (list r1 r2 r3 r4))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"aa-11\" 6) (\"cc-33\" 18) (\"ff-66\" 36) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward with BOUND parameter — limit search range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_forward_bound_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "apple:100 banana:200 cherry:300 date:400 elderberry:500")
  ;; Search with BOUND that stops before "cherry"
  (goto-char (point-min))
  (let* ((bound-pos 22)
         (r1 (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" bound-pos t))
         (r1-match (when r1 (list (match-string 1) (match-string 2)))))
    ;; Continue search with same bound — should find banana but not cherry
    (let* ((r2 (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" bound-pos t))
           (r2-match (when r2 (list (match-string 1) (match-string 2)))))
      ;; Third search should fail (bound reached)
      (let ((r3 (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" bound-pos t)))
        ;; Now search without bound — should find remaining
        (let ((remaining nil))
          (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
            (setq remaining (cons (list (match-string 1) (match-string 2)) remaining)))
          (list r1-match r2-match r3 (nreverse remaining)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"apple\" \"100\") (\"banana\" \"200\") nil ((\"cherry\" \"300\") (\"date\" \"400\") (\"elderberry\" \"500\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward/backward with capturing groups and match data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_capturing_groups_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "func add(x int, y int) int { return x + y }")
  ;; Parse function signature with multiple capture groups
  (goto-char (point-min))
  (re-search-forward "func \\([a-z]+\\)(\\([^)]*\\)) \\([a-z]+\\)" nil t)
  (let ((full-match (match-string 0))
        (func-name (match-string 1))
        (params (match-string 2))
        (ret-type (match-string 3))
        ;; Match position data
        (md (match-data)))
    ;; Verify match-beginning/match-end consistency
    (let ((beg0 (match-beginning 0))
          (end0 (match-end 0))
          (beg1 (match-beginning 1))
          (end1 (match-end 1))
          (beg2 (match-beginning 2))
          (end2 (match-end 2))
          (beg3 (match-beginning 3))
          (end3 (match-end 3)))
      ;; Verify buffer-substring matches match-string
      (let ((verify1 (string= func-name (buffer-substring beg1 end1)))
            (verify2 (string= params (buffer-substring beg2 end2)))
            (verify3 (string= ret-type (buffer-substring beg3 end3))))
        ;; Now search backward from end
        (goto-char (point-max))
        (re-search-backward "\\(return\\) \\(.+\\)" nil t)
        (let ((kw (match-string 1))
              (expr (match-string 2)))
          (list full-match func-name params ret-type
                verify1 verify2 verify3
                (length md)
                kw expr))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"func add(x int, y int) int\" \"add\" \"x int, y int\" \"int\" t t t 8 \"return\" \"x + y }\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Search and replace loop: replace all occurrences with transformed text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_replace_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "price: $10, discount: $5, total: $15, tax: $2, fee: $3")
  ;; Replace every $N with N*100 cents representation
  (goto-char (point-min))
  (let ((count 0))
    (while (re-search-forward "\\$\\([0-9]+\\)" nil t)
      (let* ((dollars (string-to-number (match-string 1)))
             (cents (* dollars 100))
             (replacement (format "%d cents" cents)))
        (replace-match replacement t t)
        (setq count (1+ count))))
    (list (buffer-string) count)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"price: 1000 cents, discount: 500 cents, total: 1500 cents, tax: 200 cents, fee: 300 cents\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-line regexp patterns in buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_multiline_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "BEGIN\nline one\nline two\nEND\nBEGIN\nalpha\nbeta\ngamma\nEND\ntrailing")
  ;; Collect all content between BEGIN and END blocks
  (goto-char (point-min))
  (let ((blocks nil))
    (while (re-search-forward "^BEGIN$" nil t)
      (let ((start (point)))
        (when (re-search-forward "^END$" nil t)
          (let ((end (match-beginning 0)))
            ;; Collect lines between BEGIN and END
            (let ((content (string-trim (buffer-substring start end))))
              (setq blocks (cons content blocks)))))))
    ;; Also count total lines
    (goto-char (point-min))
    (let ((line-count 0))
      (while (re-search-forward "\n" nil t)
        (setq line-count (1+ line-count)))
      (list (nreverse blocks)
            (length blocks)
            line-count))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"line one\\nline two\" \"alpha\\nbeta\\ngamma\") 1 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Search with narrowing: restrict search to narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "header: SKIP\ndata: ALPHA=1\ndata: BETA=2\ndata: GAMMA=3\nfooter: SKIP")
  ;; Find all data entries using narrowing
  (save-restriction
    ;; Narrow to just the data lines (skip header and footer)
    (goto-char (point-min))
    (re-search-forward "^data:" nil t)
    (beginning-of-line)
    (let ((data-start (point)))
      (goto-char (point-max))
      (re-search-backward "^data:" nil t)
      (end-of-line)
      (let ((data-end (point)))
        (narrow-to-region data-start data-end)
        ;; Now search within narrowed region
        (goto-char (point-min))
        (let ((entries nil))
          (while (re-search-forward "data: \\([A-Z]+\\)=\\([0-9]+\\)" nil t)
            (setq entries (cons (cons (match-string 1) (string-to-number (match-string 2)))
                                entries)))
          ;; point-min and point-max reflect narrowed region
          (let ((narrow-min (point-min))
                (narrow-max (point-max))
                (entry-list (nreverse entries)))
            ;; Widen and verify full buffer still accessible
            (widen)
            (list entry-list
                  (length entry-list)
                  narrow-min
                  narrow-max
                  (point-min)
                  (point-max))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"ALPHA\" . 1) (\"BETA\" . 2) (\"GAMMA\" . 3)) 3 14 54 1 67)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV parser using re-search with field extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-parse-csv-line
    (lambda (line)
      "Parse a single CSV line into a list of fields."
      (with-temp-buffer
        (insert line)
        (goto-char (point-min))
        (let ((fields nil)
              (field-start (point)))
          ;; Find commas, collecting fields
          (while (re-search-forward "," nil t)
            (let ((field (buffer-substring field-start (1- (point)))))
              (setq fields (cons (string-trim field) fields))
              (setq field-start (point))))
          ;; Last field (after final comma or the whole line)
          (setq fields (cons (string-trim (buffer-substring field-start (point-max))) fields))
          (nreverse fields)))))

  (fset 'neovm--test-parse-csv
    (lambda (text)
      "Parse CSV text into header + list of alists."
      (with-temp-buffer
        (insert text)
        (goto-char (point-min))
        ;; Parse header
        (let ((header-end (progn (re-search-forward "$" nil t) (point))))
          (let ((headers (funcall 'neovm--test-parse-csv-line
                                  (buffer-substring (point-min) header-end)))
                (rows nil))
            ;; Parse remaining lines
            (while (re-search-forward "^.+$" nil t)
              (let* ((line (match-string 0))
                     (fields (funcall 'neovm--test-parse-csv-line line))
                     (row nil)
                     (h headers)
                     (f fields))
                (while (and h f)
                  (setq row (cons (cons (car h) (car f)) row))
                  (setq h (cdr h))
                  (setq f (cdr f)))
                (setq rows (cons (nreverse row) rows))))
            (list headers (nreverse rows)))))))

  (unwind-protect
      (let ((csv "name,age,city
Alice,30,NYC
Bob,25,LA
Charlie,35,Chicago
Diana,28,Boston"))
        (let* ((parsed (funcall 'neovm--test-parse-csv csv))
               (headers (car parsed))
               (rows (cadr parsed)))
          (list headers
                (length rows)
                ;; First row
                (cdr (assoc "name" (car rows)))
                (cdr (assoc "age" (car rows)))
                ;; Last row
                (cdr (assoc "name" (car (last rows))))
                (cdr (assoc "city" (car (last rows)))))))
    (fmakunbound 'neovm--test-parse-csv-line)
    (fmakunbound 'neovm--test-parse-csv)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"age\" \"city\") 4 \"Alice\" \"30\" \"Diana\" \"Boston\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: re-search-forward and backward interleaved with save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_interleaved_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "[section-A]\nkey1=val1\nkey2=val2\n[section-B]\nkey3=val3\nkey4=val4\n[section-C]\nkey5=val5")
  ;; Parse INI-like sections: collect section headers and their key=value pairs
  (goto-char (point-min))
  (let ((sections nil))
    (while (re-search-forward "^\\[\\([^]]+\\)\\]$" nil t)
      (let ((section-name (match-string 1))
            (section-start (1+ (match-end 0)))
            (pairs nil))
        ;; Use save-excursion to peek ahead for the next section boundary
        (let ((section-end
               (save-excursion
                 (if (re-search-forward "^\\[" nil t)
                     (1- (match-beginning 0))
                   (point-max)))))
          ;; Now scan key=value pairs within this section's bounds
          (save-excursion
            (goto-char section-start)
            (while (re-search-forward "^\\([^=\n]+\\)=\\(.*\\)$" section-end t)
              (setq pairs (cons (cons (match-string 1) (match-string 2)) pairs))))
          (setq sections (cons (cons section-name (nreverse pairs)) sections)))))
    ;; Verify structure
    (let ((result (nreverse sections)))
      (list (length result)
            ;; Section names
            (mapcar #'car result)
            ;; Key-value pairs per section
            (mapcar (lambda (s) (length (cdr s))) result)
            ;; Full data
            result))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 (\"section-A\" \"section-B\" \"section-C\") (2 2 1) ((\"section-A\" (\"key1\" . \"val1\") (\"key2\" . \"val2\")) (\"section-B\" (\"key3\" . \"val3\") (\"key4\" . \"val4\")) (\"section-C\" (\"key5\" . \"val5\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
