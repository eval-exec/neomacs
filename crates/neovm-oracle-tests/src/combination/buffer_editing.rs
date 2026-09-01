//! Oracle parity tests for buffer editing algorithm patterns.
//!
//! Covers: in-buffer line sorting, deduplication of adjacent lines,
//! column extraction, region transposition, Caesar cipher on buffer
//! content, and comment toggling with prefix.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// In-buffer sort: sort lines alphabetically, then sort paragraphs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_sort_lines_and_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect lines, sort them, rewrite buffer.
    // Then treat blank-line-separated blocks as paragraphs and sort those.
    let form = r####"(progn
                    (fset 'neovm--test-sort-lines
                          (lambda ()
                            "Sort all non-empty lines in buffer alphabetically."
                            (goto-char (point-min))
                            (let ((lines nil))
                              (while (not (eobp))
                                (let ((line (buffer-substring
                                             (line-beginning-position)
                                             (line-end-position))))
                                  (when (> (length line) 0)
                                    (setq lines (cons line lines))))
                                (forward-line 1))
                              (setq lines (sort (nreverse lines) #'string<))
                              (erase-buffer)
                              (dolist (l lines) (insert l "\n")))))
                    (fset 'neovm--test-sort-paragraphs
                          (lambda ()
                            "Sort paragraphs (blank-line separated blocks) by first line."
                            (goto-char (point-min))
                            (let ((paragraphs nil)
                                  (current-para nil))
                              (while (not (eobp))
                                (let ((line (buffer-substring
                                             (line-beginning-position)
                                             (line-end-position))))
                                  (if (string= line "")
                                      (when current-para
                                        (setq paragraphs
                                              (cons (nreverse current-para) paragraphs))
                                        (setq current-para nil))
                                    (setq current-para (cons line current-para))))
                                (forward-line 1))
                              (when current-para
                                (setq paragraphs
                                      (cons (nreverse current-para) paragraphs)))
                              ;; Sort paragraphs by first line
                              (setq paragraphs
                                    (sort (nreverse paragraphs)
                                          (lambda (a b) (string< (car a) (car b)))))
                              ;; Rewrite
                              (erase-buffer)
                              (let ((first t))
                                (dolist (para paragraphs)
                                  (unless first (insert "\n"))
                                  (setq first nil)
                                  (dolist (line para)
                                    (insert line "\n")))))))
                    (unwind-protect
                        (list
                         ;; Test 1: sort lines
                         (with-temp-buffer
                           (insert "cherry\napple\nbanana\ndate\n")
                           (neovm--test-sort-lines)
                           (buffer-string))
                         ;; Test 2: sort paragraphs
                         (with-temp-buffer
                           (insert "Zebra info\nMore zebra\n\n")
                           (insert "Apple data\nApple detail\n\n")
                           (insert "Mango note\n")
                           (neovm--test-sort-paragraphs)
                           (buffer-string)))
                      (fmakunbound 'neovm--test-sort-lines)
                      (fmakunbound 'neovm--test-sort-paragraphs)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"apple\\nbanana\\ncherry\\ndate\\n\" \"Apple data\\nApple detail\\n\\nMango note\\n\\nZebra info\\nMore zebra\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-buffer deduplicate adjacent lines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_deduplicate_adjacent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove consecutive duplicate lines (like Unix `uniq`),
    // optionally case-insensitive, and report how many were removed
    let form = r####"(progn
                    (fset 'neovm--test-dedup-adjacent
                          (lambda (case-fold)
                            "Remove consecutive duplicate lines. Return count removed."
                            (goto-char (point-min))
                            (let ((removed 0)
                                  (prev-line nil))
                              (while (not (eobp))
                                (let* ((line (buffer-substring
                                              (line-beginning-position)
                                              (line-end-position)))
                                       (cmp-line (if case-fold (downcase line) line))
                                       (cmp-prev (if (and case-fold prev-line)
                                                     (downcase prev-line)
                                                   prev-line)))
                                  (if (and cmp-prev (string= cmp-line cmp-prev))
                                      ;; Delete this duplicate line
                                      (progn
                                        (let ((start (line-beginning-position))
                                              (end (min (1+ (line-end-position)) (point-max))))
                                          (delete-region start end))
                                        (setq removed (1+ removed)))
                                    ;; Keep this line, advance
                                    (setq prev-line line)
                                    (forward-line 1))))
                              removed)))
                    (unwind-protect
                        (list
                         ;; Case-sensitive dedup
                         (with-temp-buffer
                           (insert "aaa\naaa\nbbb\nbbb\nbbb\nccc\naaa\naaa\n")
                           (let ((count (neovm--test-dedup-adjacent nil)))
                             (list count (buffer-string))))
                         ;; Case-insensitive dedup
                         (with-temp-buffer
                           (insert "Hello\nhello\nHELLO\nWorld\nworld\nfoo\n")
                           (let ((count (neovm--test-dedup-adjacent t)))
                             (list count (buffer-string))))
                         ;; No duplicates
                         (with-temp-buffer
                           (insert "alpha\nbeta\ngamma\n")
                           (let ((count (neovm--test-dedup-adjacent nil)))
                             (list count (buffer-string)))))
                      (fmakunbound 'neovm--test-dedup-adjacent)))"####;
    let expect = expect_test::expect![[
        r#""OK ((4 \"aaa\\nbbb\\nccc\\naaa\\n\") (3 \"Hello\\nWorld\\nfoo\\n\") (0 \"alpha\\nbeta\\ngamma\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-buffer column extraction (cut specific columns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_column_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract specific columns from a fixed-width table, rewrite buffer
    // with only selected columns
    let form = r####"(progn
                    (fset 'neovm--test-extract-columns
                          (lambda (col-specs)
                            "Extract columns defined by COL-SPECS ((start . end) ...) from each line.
                             START and END are 0-based character positions."
                            (goto-char (point-min))
                            (let ((output-lines nil))
                              (while (not (eobp))
                                (let* ((line (buffer-substring
                                              (line-beginning-position)
                                              (line-end-position)))
                                       (parts nil))
                                  (dolist (spec (reverse col-specs))
                                    (let* ((start (car spec))
                                           (end (min (cdr spec) (length line)))
                                           (col (if (< start (length line))
                                                    (substring line start end)
                                                  "")))
                                      (setq parts (cons col parts))))
                                  (setq output-lines
                                        (cons (mapconcat #'identity parts "|") output-lines)))
                                (forward-line 1))
                              ;; Rewrite buffer
                              (erase-buffer)
                              (dolist (line (nreverse output-lines))
                                (insert line "\n")))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "Alice     30  Engineer  Boston\n")
                          (insert "Bob       25  Designer  Seattle\n")
                          (insert "Carol     35  Manager   Denver\n")
                          (insert "Dave      28  Analyst   Austin\n")
                          ;; Extract name (0-10) and city (24-30)
                          (neovm--test-extract-columns '((0 . 10) (24 . 30)))
                          (let ((result1 (buffer-string)))
                            ;; Now extract age (10-14) only
                            (erase-buffer)
                            (insert "Alice     30  Engineer  Boston\n")
                            (insert "Bob       25  Designer  Seattle\n")
                            (insert "Carol     35  Manager   Denver\n")
                            (neovm--test-extract-columns '((10 . 14)))
                            (list result1 (buffer-string))))
                      (fmakunbound 'neovm--test-extract-columns)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"Alice     |Boston\\nBob       |Seattl\\nCarol     |Denver\\nDave      |Austin\\n\" \"30  \\n25  \\n35  \\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer transposition (swap two regions)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_transpose_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Swap two non-overlapping regions in a buffer,
    // handling the index shifting from the first replacement
    let form = r####"(progn
                    (fset 'neovm--test-transpose-regions
                          (lambda (start1 end1 start2 end2)
                            "Swap region [START1,END1) with [START2,END2). Assumes START1 < START2."
                            ;; Extract both regions first
                            (let ((text1 (buffer-substring start1 end1))
                                  (text2 (buffer-substring start2 end2)))
                              ;; Delete second region first (higher positions) to avoid shifting
                              (delete-region start2 end2)
                              (goto-char start2)
                              (insert text1)
                              ;; Now handle the first region
                              ;; The shift from replacing region2 is: (length text1) - (length text2)
                              (delete-region start1 end1)
                              (goto-char start1)
                              (insert text2))))
                    (unwind-protect
                        (list
                         ;; Swap two words
                         (with-temp-buffer
                           (insert "The quick brown fox jumps")
                           ;; Swap "quick" (5-10) with "fox" (17-20)
                           (neovm--test-transpose-regions 5 10 17 20)
                           (buffer-string))
                         ;; Swap first and last lines
                         (with-temp-buffer
                           (insert "first line\nmiddle stuff\nlast line")
                           ;; "first line" = 1-11, "last line" = 27-36
                           (neovm--test-transpose-regions 1 11 27 36)
                           (buffer-string))
                         ;; Swap equal-length regions
                         (with-temp-buffer
                           (insert "AABB--CCDD--EEFF")
                           ;; "AABB" (1-5) with "EEFF" (13-17)
                           (neovm--test-transpose-regions 1 5 13 17)
                           (buffer-string)))
                      (fmakunbound 'neovm--test-transpose-regions)))"####;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 27 36)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-buffer Caesar cipher encryption/decryption
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_caesar_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Apply Caesar cipher (rotate letters by N) to buffer content in-place,
    // preserving non-letter characters. Verify encrypt then decrypt = original.
    let form = r####"(progn
                    (fset 'neovm--test-caesar-apply
                          (lambda (shift)
                            "Apply Caesar cipher with SHIFT to buffer. Modifies in-place."
                            (goto-char (point-min))
                            (while (not (eobp))
                              (let ((ch (char-after (point))))
                                (cond
                                 ;; Uppercase
                                 ((and (>= ch ?A) (<= ch ?Z))
                                  (let ((new-ch (+ ?A (% (+ (- ch ?A) shift) 26))))
                                    (delete-char 1)
                                    (insert-char new-ch)))
                                 ;; Lowercase
                                 ((and (>= ch ?a) (<= ch ?z))
                                  (let ((new-ch (+ ?a (% (+ (- ch ?a) shift) 26))))
                                    (delete-char 1)
                                    (insert-char new-ch)))
                                 ;; Non-letter: skip
                                 (t (forward-char 1)))))))
                    (unwind-protect
                        (let ((original "Hello, World! The Quick Brown Fox 123."))
                          ;; Encrypt with shift 13 (ROT13)
                          (let ((encrypted
                                 (with-temp-buffer
                                   (insert original)
                                   (neovm--test-caesar-apply 13)
                                   (buffer-string))))
                            ;; Decrypt by applying shift 13 again (ROT13 is self-inverse)
                            (let ((decrypted
                                   (with-temp-buffer
                                     (insert encrypted)
                                     (neovm--test-caesar-apply 13)
                                     (buffer-string))))
                              ;; Also test with shift 7 and reverse shift 19
                              (let ((enc7
                                     (with-temp-buffer
                                       (insert original)
                                       (neovm--test-caesar-apply 7)
                                       (buffer-string))))
                                (let ((dec7
                                       (with-temp-buffer
                                         (insert enc7)
                                         (neovm--test-caesar-apply 19)
                                         (buffer-string))))
                                  (list encrypted decrypted
                                        (string= decrypted original)
                                        enc7 dec7
                                        (string= dec7 original)
                                        ;; Encrypted differs from original
                                        (not (string= encrypted original))))))))
                      (fmakunbound 'neovm--test-caesar-apply)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"Uryyb, Jbeyq! Gur Dhvpx Oebja Sbk 123.\" \"Hello, World! The Quick Brown Fox 123.\" t \"Olssv, Dvysk! Aol Xbpjr Iyvdu Mve 123.\" \"Hello, World! The Quick Brown Fox 123.\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comment toggling: add/remove line comment prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_comment_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Toggle line comments: if line starts with prefix, remove it;
    // otherwise add it. Handle indentation-preserving comments.
    let form = r##"(progn
                    (fset 'neovm--test-toggle-comments
                          (lambda (prefix start-line end-line)
                            "Toggle comment PREFIX on lines START-LINE to END-LINE (1-based)."
                            (let ((prefix-re (concat "^\\([ \t]*\\)"
                                                     (regexp-quote prefix)
                                                     " ?"))
                                  (line-num 1))
                              (goto-char (point-min))
                              (while (not (eobp))
                                (when (and (>= line-num start-line)
                                           (<= line-num end-line))
                                  (let ((line-start (line-beginning-position))
                                        (line-end (line-end-position)))
                                    (let ((line-text (buffer-substring line-start line-end)))
                                      (if (string-match prefix-re line-text)
                                          ;; Remove comment prefix
                                          (progn
                                            (delete-region line-start line-end)
                                            (goto-char line-start)
                                            (insert (concat (match-string 1 line-text)
                                                            (substring line-text (match-end 0)))))
                                        ;; Add comment prefix after leading whitespace
                                        (let ((indent ""))
                                          (when (string-match "^\\([ \t]*\\)" line-text)
                                            (setq indent (match-string 1 line-text)))
                                          (delete-region line-start line-end)
                                          (goto-char line-start)
                                          (insert (concat indent prefix " "
                                                          (substring line-text (length indent)))))))))
                                (forward-line 1)
                                (setq line-num (1+ line-num))))))
                    (unwind-protect
                        (list
                         ;; Add comments to all lines
                         (with-temp-buffer
                           (insert "def hello():\n    print('hi')\n    return True\n")
                           (neovm--test-toggle-comments "#" 1 3)
                           (buffer-string))
                         ;; Remove comments (toggle back)
                         (with-temp-buffer
                           (insert "# def hello():\n#     print('hi')\n#     return True\n")
                           (neovm--test-toggle-comments "#" 1 3)
                           (buffer-string))
                         ;; Toggle specific line range (lines 2-3 of 4)
                         (with-temp-buffer
                           (insert "line one\nline two\nline three\nline four\n")
                           (neovm--test-toggle-comments "//" 2 3)
                           (buffer-string))
                         ;; Mixed: some commented, some not -> all should toggle
                         (with-temp-buffer
                           (insert "  normal\n  ;; commented\n  also normal\n")
                           (neovm--test-toggle-comments ";;" 1 3)
                           (buffer-string)))
                      (fmakunbound 'neovm--test-toggle-comments)))"##;
    let expect = expect_test::expect![[
        r##""OK (\"# def hello():\\n    # print('hi')\\n    # return True\\n\" \"def hello():\\n    print('hi')\\n    return True\\n\" \"line one\\n// line two\\n// line three\\nline four\\n\" \"  ;; normal\\n  commented\\n  ;; also normal\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-buffer line reversal with region awareness
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufedit_reverse_lines_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reverse the order of lines within a specified region, leaving
    // content before and after the region untouched
    let form = r####"(progn
                    (fset 'neovm--test-reverse-lines-region
                          (lambda (beg end)
                            "Reverse order of lines in region [BEG, END)."
                            (let ((lines nil))
                              (goto-char beg)
                              (while (< (point) end)
                                (let ((lbeg (line-beginning-position))
                                      (lend (min (1+ (line-end-position)) end)))
                                  (setq lines (cons (buffer-substring lbeg lend) lines))
                                  (goto-char lend)))
                              ;; lines is already reversed from cons-ing
                              (delete-region beg end)
                              (goto-char beg)
                              (dolist (line lines)
                                (insert line)))))
                    (unwind-protect
                        (list
                         ;; Reverse all lines
                         (with-temp-buffer
                           (insert "alpha\nbeta\ngamma\ndelta\nepsilon\n")
                           (neovm--test-reverse-lines-region (point-min) (point-max))
                           (buffer-string))
                         ;; Reverse middle lines only (lines 2-4)
                         (with-temp-buffer
                           (insert "first\nsecond\nthird\nfourth\nfifth\n")
                           ;; second starts at pos 7, fifth starts at pos 27
                           (goto-char (point-min))
                           (forward-line 1)
                           (let ((beg (point)))
                             (forward-line 3)
                             (neovm--test-reverse-lines-region beg (point)))
                           (buffer-string))
                         ;; Single line region (no-op)
                         (with-temp-buffer
                           (insert "only\n")
                           (neovm--test-reverse-lines-region (point-min) (point-max))
                           (buffer-string)))
                      (fmakunbound 'neovm--test-reverse-lines-region)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"epsilon\\ndelta\\ngamma\\nbeta\\nalpha\\n\" \"first\\nfourth\\nthird\\nsecond\\nfifth\\n\" \"only\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
