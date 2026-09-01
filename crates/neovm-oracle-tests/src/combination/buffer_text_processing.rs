//! Oracle parity tests for buffer text processing algorithms.
//!
//! Covers: line-by-line transformation, in-buffer search-and-replace with counting,
//! XML/HTML tag matching, table extraction and reformatting, undo simulation,
//! and comment stripping from pseudo-code.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Line-by-line buffer transformation (map over lines)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_line_by_line_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Apply a transformation function to each line: number the lines,
    // uppercase the first word, and append line length
    let form = r#"(progn
                    (fset 'neovm--test-transform-line
                          (lambda (line-num line-text)
                            (let* ((trimmed (string-trim line-text))
                                   (first-space (string-match " " trimmed))
                                   (first-word (if first-space
                                                   (substring trimmed 0 first-space)
                                                 trimmed))
                                   (rest (if first-space
                                             (substring trimmed first-space)
                                           "")))
                              (format "%03d: %s%s [%d]"
                                      line-num (upcase first-word) rest (length trimmed)))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "the quick brown fox\n")
                          (insert "jumps over\n")
                          (insert "the lazy dog\n")
                          (insert "and runs away\n")
                          (goto-char (point-min))
                          (let ((line-num 1)
                                (result-lines nil))
                            (while (not (eobp))
                              (let ((line-start (point)))
                                (end-of-line)
                                (let ((line-text (buffer-substring line-start (point))))
                                  (when (> (length line-text) 0)
                                    (setq result-lines
                                          (cons (neovm--test-transform-line line-num line-text)
                                                result-lines))
                                    (setq line-num (1+ line-num)))))
                              (forward-line 1))
                            ;; Build output buffer
                            (erase-buffer)
                            (dolist (line (nreverse result-lines))
                              (insert line "\n"))
                            (buffer-string)))
                      (fmakunbound 'neovm--test-transform-line)))"#;
    let expect = expect_test::expect![[
        r#""OK \"001: THE quick brown fox [19]\\n002: JUMPS over [10]\\n003: THE lazy dog [12]\\n004: AND runs away [13]\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-buffer search and replace with counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_search_replace_with_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-pattern search and replace: track count per pattern,
    // positions of replacements, and final buffer state
    let form = r#"(with-temp-buffer
                    (insert "The cat sat on the mat. The cat chased the rat on the mat.")
                    (let ((replacements '(("cat" . "dog")
                                         ("mat" . "rug")
                                         ("rat" . "mouse")))
                          (stats nil))
                      (dolist (repl replacements)
                        (goto-char (point-min))
                        (let ((count 0)
                              (positions nil))
                          (while (search-forward (car repl) nil t)
                            (let ((pos (match-beginning 0)))
                              (replace-match (cdr repl) t t)
                              (setq count (1+ count))
                              (setq positions (cons pos positions))))
                          (setq stats
                                (cons (list (car repl) (cdr repl) count (nreverse positions))
                                      stats))))
                      (list (buffer-string) (nreverse stats))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"The dog sat on the rug. The dog chased the mouse on the rug.\" ((\"cat\" \"dog\" 2 (5 29)) (\"mat\" \"rug\" 2 (20 55)) (\"rat\" \"mouse\" 1 (44))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based XML/HTML tag matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_html_tag_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse pseudo-HTML, extract tag tree structure, detect mismatches
    let form = r#"(progn
                    (defvar neovm--test-tag-stack nil)
                    (defvar neovm--test-tag-tree nil)
                    (unwind-protect
                        (with-temp-buffer
                          (insert "<div><p>hello</p><span>world</span></div>")
                          (goto-char (point-min))
                          (setq neovm--test-tag-stack nil)
                          (setq neovm--test-tag-tree nil)
                          (while (re-search-forward "<\\(/\\)?\\([a-zA-Z]+\\)>" nil t)
                            (let ((is-close (match-string 1))
                                  (tag-name (match-string 2))
                                  (tag-pos (match-beginning 0)))
                              (if is-close
                                  ;; Closing tag: pop from stack, record pair
                                  (let ((open-info (car neovm--test-tag-stack)))
                                    (setq neovm--test-tag-stack (cdr neovm--test-tag-stack))
                                    (when open-info
                                      (let ((open-tag (car open-info))
                                            (open-pos (cdr open-info)))
                                        (setq neovm--test-tag-tree
                                              (cons (list 'pair open-tag open-pos
                                                          tag-name tag-pos
                                                          (string= open-tag tag-name))
                                                    neovm--test-tag-tree)))))
                                ;; Opening tag: push to stack
                                (setq neovm--test-tag-stack
                                      (cons (cons tag-name tag-pos) neovm--test-tag-stack)))))
                          ;; Any unclosed tags?
                          (let ((unclosed (mapcar (lambda (s) (list 'unclosed (car s) (cdr s)))
                                                 neovm--test-tag-stack)))
                            (list (nreverse neovm--test-tag-tree)
                                  unclosed)))
                      (makunbound 'neovm--test-tag-stack)
                      (makunbound 'neovm--test-tag-tree)))"#;
    let expect = expect_test::expect![[
        r#""OK (((pair \"p\" 6 \"p\" 14 t) (pair \"span\" 18 \"span\" 29 t) (pair \"div\" 1 \"div\" 36 t)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract and reformat a table from buffer text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_table_extract_reformat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a pipe-separated table, compute column widths, and reformat
    // with proper alignment
    let form = r#"(progn
                    (fset 'neovm--test-pad-right
                          (lambda (s width)
                            (let ((pad (- width (length s))))
                              (if (> pad 0)
                                  (concat s (make-string pad ?\ ))
                                s))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "Name|Age|City\n")
                          (insert "Alice|30|Boston\n")
                          (insert "Bob|25|San Francisco\n")
                          (insert "Charlie|35|NY\n")
                          (goto-char (point-min))
                          ;; Parse rows
                          (let ((rows nil))
                            (while (not (eobp))
                              (let ((line-start (point)))
                                (end-of-line)
                                (let ((line (buffer-substring line-start (point))))
                                  (when (> (length line) 0)
                                    (setq rows (cons (split-string line "|") rows))))
                                (forward-line 1)))
                            (setq rows (nreverse rows))
                            ;; Compute max width per column
                            (let* ((num-cols (length (car rows)))
                                   (widths (make-list num-cols 0)))
                              (dolist (row rows)
                                (let ((i 0))
                                  (dolist (cell row)
                                    (let ((w (nthcdr i widths)))
                                      (when (> (length cell) (car w))
                                        (setcar w (length cell))))
                                    (setq i (1+ i)))))
                              ;; Build reformatted table
                              (erase-buffer)
                              (let ((row-num 0))
                                (dolist (row rows)
                                  (let ((i 0)
                                        (formatted-cells nil))
                                    (dolist (cell row)
                                      (setq formatted-cells
                                            (cons (neovm--test-pad-right cell (nth i widths))
                                                  formatted-cells))
                                      (setq i (1+ i)))
                                    (insert (mapconcat #'identity (nreverse formatted-cells) " | ") "\n"))
                                  ;; Add separator after header
                                  (when (= row-num 0)
                                    (let ((sep-parts nil)
                                          (j 0))
                                      (dolist (w widths)
                                        (setq sep-parts (cons (make-string w ?-) sep-parts))
                                        (setq j (1+ j)))
                                      (insert (mapconcat #'identity (nreverse sep-parts) "-+-") "\n")))
                                  (setq row-num (1+ row-num))))
                              (buffer-string))))
                      (fmakunbound 'neovm--test-pad-right)))"#;
    let expect = expect_test::expect![[
        r#""OK \"Name    | Age | City         \\n--------+-----+--------------\\nAlice   | 30  | Boston       \\nBob     | 25  | San Francisco\\nCharlie | 35  | NY           \\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer undo simulation (record + replay changes)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_undo_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple undo log: record each buffer modification,
    // then replay them in reverse to restore original state
    let form = r#"(progn
                    (defvar neovm--test-undo-log nil)
                    (fset 'neovm--test-undo-insert
                          (lambda (pos text)
                            "Insert TEXT at POS, recording an undo entry."
                            (save-excursion
                              (goto-char pos)
                              (insert text)
                              ;; Record: to undo an insert, we delete from pos to pos+len
                              (setq neovm--test-undo-log
                                    (cons (list 'delete pos (+ pos (length text)))
                                          neovm--test-undo-log)))))
                    (fset 'neovm--test-undo-delete
                          (lambda (beg end)
                            "Delete region BEG..END, recording an undo entry."
                            (let ((deleted-text (buffer-substring beg end)))
                              (delete-region beg end)
                              ;; Record: to undo a delete, we re-insert at beg
                              (setq neovm--test-undo-log
                                    (cons (list 'insert beg deleted-text)
                                          neovm--test-undo-log)))))
                    (fset 'neovm--test-undo-replay
                          (lambda ()
                            "Replay undo log to restore original buffer state."
                            (dolist (entry neovm--test-undo-log)
                              (let ((op (car entry)))
                                (cond
                                 ((eq op 'insert)
                                  (save-excursion
                                    (goto-char (cadr entry))
                                    (insert (caddr entry))))
                                 ((eq op 'delete)
                                  (delete-region (cadr entry) (caddr entry))))))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "Hello World")
                          (let ((original (buffer-string)))
                            (setq neovm--test-undo-log nil)
                            ;; Make several modifications
                            (neovm--test-undo-insert 6 "Beautiful ")
                            (let ((after-insert (buffer-string)))
                              (neovm--test-undo-delete 1 6)
                              (let ((after-delete (buffer-string)))
                                (neovm--test-undo-insert 1 "Goodbye ")
                                (let ((after-all (buffer-string))
                                      (log-len (length neovm--test-undo-log)))
                                  ;; Now undo everything
                                  (neovm--test-undo-replay)
                                  (let ((restored (buffer-string)))
                                    (list original
                                          after-insert
                                          after-delete
                                          after-all
                                          log-len
                                          restored
                                          (string= original restored))))))))
                      (fmakunbound 'neovm--test-undo-insert)
                      (fmakunbound 'neovm--test-undo-delete)
                      (fmakunbound 'neovm--test-undo-replay)
                      (makunbound 'neovm--test-undo-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"HelloBeautiful  World\" \"Beautiful  World\" \"Goodbye Beautiful  World\" 3 \"Hello World\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comment stripping from pseudo-code
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_comment_stripping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Strip single-line (//) and multi-line (/* ... */) comments from
    // C-like pseudo-code, preserving strings that contain comment-like sequences
    let form = r#"(progn
                    (fset 'neovm--test-strip-comments
                          (lambda ()
                            "Strip comments from current buffer in-place."
                            (goto-char (point-min))
                            ;; First pass: remove // line comments
                            (while (re-search-forward "//.*$" nil t)
                              (replace-match ""))
                            ;; Second pass: remove /* ... */ block comments (non-greedy)
                            (goto-char (point-min))
                            (while (search-forward "/*" nil t)
                              (let ((comment-start (- (point) 2)))
                                (if (search-forward "*/" nil t)
                                    (delete-region comment-start (point))
                                  ;; Unterminated block comment: delete to end
                                  (delete-region comment-start (point-max)))))
                            ;; Third pass: remove blank lines
                            (goto-char (point-min))
                            (while (re-search-forward "^[ \t]*\n" nil t)
                              (replace-match ""))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "int x = 5; // initialize x\n")
                          (insert "/* This is a\n")
                          (insert "   multi-line comment */\n")
                          (insert "int y = 10; // another comment\n")
                          (insert "int z = x + y; /* inline */ int w = 0;\n")
                          (neovm--test-strip-comments)
                          (let ((stripped (buffer-string)))
                            ;; Also verify by splitting into lines
                            (let ((lines (split-string stripped "\n" t)))
                              (list stripped lines (length lines)))))
                      (fmakunbound 'neovm--test-strip-comments)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"int x = 5; \\nint y = 10; \\nint z = x + y;  int w = 0;\\n\" (\"int x = 5; \" \"int y = 10; \" \"int z = x + y;  int w = 0;\") 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer text deduplication with ordering preservation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btp_line_dedup_preserve_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove duplicate lines while preserving first-occurrence order,
    // and track duplicate count per line
    let form = r#"(with-temp-buffer
                    (insert "apple\nbanana\napple\ncherry\nbanana\napple\ndate\ncherry\n")
                    (goto-char (point-min))
                    (let ((seen (make-hash-table :test 'equal))
                          (unique-lines nil)
                          (dup-counts (make-hash-table :test 'equal)))
                      ;; First pass: count occurrences and collect unique lines in order
                      (while (not (eobp))
                        (let ((line-start (point)))
                          (end-of-line)
                          (let ((line (buffer-substring line-start (point))))
                            (when (> (length line) 0)
                              (puthash line (1+ (gethash line dup-counts 0)) dup-counts)
                              (unless (gethash line seen)
                                (puthash line t seen)
                                (setq unique-lines (cons line unique-lines)))))
                          (forward-line 1)))
                      (setq unique-lines (nreverse unique-lines))
                      ;; Rebuild buffer with unique lines only
                      (erase-buffer)
                      (dolist (line unique-lines)
                        (insert line "\n"))
                      (let ((deduped (buffer-string)))
                        ;; Collect stats
                        (let ((stats nil))
                          (dolist (line unique-lines)
                            (setq stats (cons (cons line (gethash line dup-counts))
                                              stats)))
                          (list deduped (nreverse stats))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"apple\\nbanana\\ncherry\\ndate\\n\" ((\"apple\" . 3) (\"banana\" . 2) (\"cherry\" . 2) (\"date\" . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
