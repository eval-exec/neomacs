//! Complex oracle tests for text processing combinations:
//! ROT13 cipher, line-based sorting, buffer region transposition,
//! word-boundary text wrapping, search-and-replace with counting,
//! and multi-buffer content merge.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Buffer-based ROT13 cipher (char-by-char transform)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_rot13_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement ROT13 by iterating buffer chars, applying the transform,
    // and verify that double-ROT13 is identity
    let form = r#"(let ((rot13-buffer
                         (lambda ()
                           (goto-char (point-min))
                           (while (not (eobp))
                             (let* ((c (char-after))
                                    (new-c
                                     (cond
                                      ((and (>= c ?a) (<= c ?z))
                                       (+ ?a (% (+ (- c ?a) 13) 26)))
                                      ((and (>= c ?A) (<= c ?Z))
                                       (+ ?A (% (+ (- c ?A) 13) 26)))
                                      (t c))))
                               (delete-char 1)
                               (insert (char-to-string new-c)))))))
                    (with-temp-buffer
                      (insert "Hello, World! The Quick Brown Fox 123.")
                      (let ((original (buffer-string)))
                        ;; First ROT13
                        (funcall rot13-buffer)
                        (let ((encrypted (buffer-string)))
                          ;; Second ROT13 should restore original
                          (funcall rot13-buffer)
                          (let ((roundtrip (buffer-string)))
                            (list original
                                  encrypted
                                  roundtrip
                                  (string= original roundtrip)
                                  ;; Verify specific known ROT13 pairs
                                  (let ((check-pairs t))
                                    (dolist (pair '((?A . ?N) (?Z . ?M)
                                                   (?a . ?n) (?z . ?m)
                                                   (?0 . ?0) (?! . ?!)))
                                      (let* ((orig-char (car pair))
                                             (expected (cdr pair))
                                             (actual (cond
                                                      ((and (>= orig-char ?a)
                                                            (<= orig-char ?z))
                                                       (+ ?a (% (+ (- orig-char ?a) 13) 26)))
                                                      ((and (>= orig-char ?A)
                                                            (<= orig-char ?Z))
                                                       (+ ?A (% (+ (- orig-char ?A) 13) 26)))
                                                      (t orig-char))))
                                        (unless (= actual expected)
                                          (setq check-pairs nil))))
                                    check-pairs)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, World! The Quick Brown Fox 123.\" \"Uryyb, Jbeyq! Gur Dhvpx Oebja Sbk 123.\" \"Hello, World! The Quick Brown Fox 123.\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Line-based sorting with custom comparator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_line_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract lines from buffer, sort them by various criteria,
    // and reconstruct
    let form = r#"(with-temp-buffer
                    (insert "banana 3\n")
                    (insert "apple 10\n")
                    (insert "cherry 1\n")
                    (insert "date 7\n")
                    (insert "elderberry 2\n")
                    (insert "fig 5\n")
                    ;; Extract lines into a list
                    (let* ((text (buffer-substring (point-min) (point-max)))
                           (lines (split-string (string-trim-right text) "\n"))
                           ;; Sort alphabetically
                           (alpha-sorted
                            (sort (copy-sequence lines) #'string<))
                           ;; Sort by the numeric suffix (descending)
                           (num-sorted
                            (sort (copy-sequence lines)
                                  (lambda (a b)
                                    (> (string-to-number
                                        (car (last (split-string a " "))))
                                       (string-to-number
                                        (car (last (split-string b " "))))))))
                           ;; Sort by line length
                           (len-sorted
                            (sort (copy-sequence lines)
                                  (lambda (a b)
                                    (< (length a) (length b)))))
                           ;; Sort by fruit name length (part before space)
                           (name-len-sorted
                            (sort (copy-sequence lines)
                                  (lambda (a b)
                                    (< (length (car (split-string a " ")))
                                       (length (car (split-string b " "))))))))
                      (list alpha-sorted
                            num-sorted
                            len-sorted
                            name-len-sorted)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"apple 10\" \"banana 3\" \"cherry 1\" \"date 7\" \"elderberry 2\" \"fig 5\") (\"apple 10\" \"date 7\" \"fig 5\" \"banana 3\" \"elderberry 2\" \"cherry 1\") (\"fig 5\" \"date 7\" \"banana 3\" \"apple 10\" \"cherry 1\" \"elderberry 2\") (\"fig 5\" \"date 7\" \"apple 10\" \"banana 3\" \"cherry 1\" \"elderberry 2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer transposition (swap two regions)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_transpose_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Swap two non-overlapping regions in a buffer, preserving
    // everything else. This mimics transpose-regions logic.
    let form = r#"(with-temp-buffer
                    (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
                    ;; Swap "BBBB" (pos 6-10) with "DDDD" (pos 16-20)
                    ;; Strategy: extract both, delete right first (to preserve
                    ;; left positions), insert, then fix left.
                    (let* ((r1-start 6)
                           (r1-end 10)
                           (r2-start 16)
                           (r2-end 20)
                           (text1 (buffer-substring r1-start r1-end))
                           (text2 (buffer-substring r2-start r2-end)))
                      ;; Delete region 2 first (rightmost) and insert text1
                      (goto-char r2-start)
                      (delete-region r2-start r2-end)
                      (insert text1)
                      ;; Now delete region 1 and insert text2
                      (goto-char r1-start)
                      (delete-region r1-start r1-end)
                      (insert text2)
                      (let ((result (buffer-string)))
                        ;; Also do a second swap to verify roundtrip
                        (let* ((new-text1 (buffer-substring r1-start r1-end))
                               (new-text2 (buffer-substring r2-start r2-end)))
                          (goto-char r2-start)
                          (delete-region r2-start r2-end)
                          (insert new-text1)
                          (goto-char r1-start)
                          (delete-region r1-start r1-end)
                          (insert new-text2)
                          (list result
                                (buffer-string)
                                (string= (buffer-string)
                                         "AAAA-BBBB-CCCC-DDDD-EEEE")
                                text1 text2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"AAAA-DDDD-CCCC-BBBB-EEEE\" \"AAAA-BBBB-CCCC-DDDD-EEEE\" t \"BBBB\" \"DDDD\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Text wrapping: break lines at word boundaries to column width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_word_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement word-wrap: break a long string into lines of at most
    // `width` columns, breaking at word boundaries when possible
    let form = r#"(let ((word-wrap
                         (lambda (text width)
                           (let ((words (split-string text " "))
                                 (lines nil)
                                 (current-line ""))
                             (dolist (word words)
                               (cond
                                ;; Empty current line: just add the word
                                ((= (length current-line) 0)
                                 (setq current-line word))
                                ;; Adding word fits within width
                                ((<= (+ (length current-line) 1 (length word))
                                     width)
                                 (setq current-line
                                       (concat current-line " " word)))
                                ;; Doesn't fit: flush current, start new
                                (t
                                 (setq lines (cons current-line lines)
                                       current-line word))))
                             ;; Flush last line
                             (when (> (length current-line) 0)
                               (setq lines (cons current-line lines)))
                             (nreverse lines)))))
                    (let* ((text "The quick brown fox jumps over the lazy dog and then runs away very fast")
                           (w20 (funcall word-wrap text 20))
                           (w30 (funcall word-wrap text 30))
                           (w10 (funcall word-wrap text 10))
                           (w80 (funcall word-wrap text 80))
                           ;; Verify: no line exceeds width (except single
                           ;; words longer than width)
                           (all-ok-20
                            (let ((ok t))
                              (dolist (line w20)
                                (when (and (> (length line) 20)
                                           (string-match-p " " line))
                                  (setq ok nil)))
                              ok))
                           ;; Verify: reconstructed text matches original
                           (reconstructed (mapconcat #'identity w20 " "))
                           (matches (string= reconstructed text)))
                      (list w20 w30 w10 w80
                            all-ok-20 matches)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"The quick brown fox\" \"jumps over the lazy\" \"dog and then runs\" \"away very fast\") (\"The quick brown fox jumps over\" \"the lazy dog and then runs\" \"away very fast\") (\"The quick\" \"brown fox\" \"jumps over\" \"the lazy\" \"dog and\" \"then runs\" \"away very\" \"fast\") (\"The quick brown fox jumps over the lazy dog and then runs away very fast\") t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based search-and-replace with counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_search_replace_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform multiple search-and-replace operations in a buffer,
    // counting occurrences and tracking positions
    let form = r#"(with-temp-buffer
                    (insert "The cat sat on the mat. ")
                    (insert "The cat and the cat played. ")
                    (insert "A cat is a cat is a cat.")
                    (let* ((original (buffer-string))
                           ;; Count occurrences of "cat"
                           (cat-count
                            (let ((count 0))
                              (goto-char (point-min))
                              (while (search-forward "cat" nil t)
                                (setq count (1+ count)))
                              count))
                           ;; Collect positions of "cat"
                           (cat-positions
                            (let ((positions nil))
                              (goto-char (point-min))
                              (while (search-forward "cat" nil t)
                                (setq positions
                                      (cons (match-beginning 0) positions)))
                              (nreverse positions)))
                           ;; Replace "cat" with "dog" and count replacements
                           (replace-count
                            (let ((count 0))
                              (goto-char (point-min))
                              (while (search-forward "cat" nil t)
                                (replace-match "dog" t t)
                                (setq count (1+ count)))
                              count))
                           (after-first-replace (buffer-string))
                           ;; Replace "the" (case-insensitive) with "a"
                           (the-count
                            (let ((count 0)
                                  (case-fold-search t))
                              (goto-char (point-min))
                              (while (search-forward "the" nil t)
                                (replace-match "a" t t)
                                (setq count (1+ count)))
                              count))
                           (after-second-replace (buffer-string))
                           ;; Regexp replace: collapse multiple spaces
                           (_ (progn
                                (goto-char (point-min))
                                (while (re-search-forward "  +" nil t)
                                  (replace-match " " t t))))
                           (final (buffer-string)))
                      (list cat-count
                            cat-positions
                            replace-count
                            (= cat-count replace-count)
                            after-first-replace
                            the-count
                            final)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 (5 29 41 55 64 73) 6 t \"The dog sat on the mat. The dog and the dog played. A dog is a dog is a dog.\" 4 \"a dog sat on a mat. a dog and a dog played. A dog is a dog is a dog.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-buffer content merge pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_multi_buffer_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create multiple temp buffers with different content, then merge
    // their contents into a final buffer with section headers
    let form = r#"(let ((sections nil))
                    ;; Buffer 1: CSV data
                    (with-temp-buffer
                      (insert "name,age,city\n")
                      (insert "Alice,30,NYC\n")
                      (insert "Bob,25,LA\n")
                      (let ((lines nil))
                        (goto-char (point-min))
                        (while (not (eobp))
                          (let ((start (point)))
                            (end-of-line)
                            (setq lines (cons (buffer-substring start (point))
                                              lines))
                            (forward-line 1)))
                        (setq sections
                              (cons (cons "CSV-DATA"
                                          (nreverse lines))
                                    sections))))
                    ;; Buffer 2: Key-value config
                    (with-temp-buffer
                      (insert "host=localhost\n")
                      (insert "port=8080\n")
                      (insert "debug=true\n")
                      (let ((pairs nil))
                        (goto-char (point-min))
                        (while (re-search-forward
                                "^\\([^=]+\\)=\\(.*\\)$" nil t)
                          (setq pairs
                                (cons (cons (match-string 1)
                                            (match-string 2))
                                      pairs)))
                        (setq sections
                              (cons (cons "CONFIG"
                                          (nreverse pairs))
                                    sections))))
                    ;; Buffer 3: Numbered items
                    (with-temp-buffer
                      (insert "1. First item\n")
                      (insert "2. Second item\n")
                      (insert "3. Third item\n")
                      (let ((items nil))
                        (goto-char (point-min))
                        (while (re-search-forward
                                "^\\([0-9]+\\)\\. \\(.*\\)$" nil t)
                          (setq items
                                (cons (cons (string-to-number
                                             (match-string 1))
                                            (match-string 2))
                                      items)))
                        (setq sections
                              (cons (cons "ITEMS"
                                          (nreverse items))
                                    sections))))
                    ;; Merge into final buffer
                    (with-temp-buffer
                      (let ((all-sections (nreverse sections)))
                        (dolist (section all-sections)
                          (insert (format "=== %s ===\n" (car section)))
                          (dolist (entry (cdr section))
                            (insert (format "  %s\n"
                                            (if (consp entry)
                                                (format "%s: %s"
                                                        (car entry)
                                                        (cdr entry))
                                              entry))))
                          (insert "\n"))
                        (list (buffer-string)
                              (length all-sections)
                              (mapcar #'car all-sections)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"=== CSV-DATA ===\\n  name,age,city\\n  Alice,30,NYC\\n  Bob,25,LA\\n\\n=== CONFIG ===\\n  host: localhost\\n  port: 8080\\n  debug: true\\n\\n=== ITEMS ===\\n  1: First item\\n  2: Second item\\n  3: Third item\\n\\n\" 3 (\"CSV-DATA\" \"CONFIG\" \"ITEMS\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSV parser with quoting support and field extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textproc_csv_field_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse simple CSV (no quoted fields with commas), extract specific
    // columns, compute summary statistics on numeric columns
    let form = r#"(with-temp-buffer
                    (insert "name,score,grade\n")
                    (insert "Alice,95,A\n")
                    (insert "Bob,72,C\n")
                    (insert "Carol,88,B\n")
                    (insert "Dave,91,A\n")
                    (insert "Eve,65,D\n")
                    (let* ((text (buffer-substring (point-min) (point-max)))
                           (lines (split-string (string-trim-right text) "\n"))
                           (header (split-string (car lines) ","))
                           (data-lines (cdr lines))
                           ;; Parse each row into alist
                           (records
                            (mapcar
                             (lambda (line)
                               (let ((fields (split-string line ",")))
                                 (seq-mapn #'cons header fields)))
                             data-lines))
                           ;; Extract just the scores column
                           (scores
                            (mapcar
                             (lambda (rec)
                               (string-to-number
                                (cdr (assoc "score" rec))))
                             records))
                           ;; Statistics
                           (total (apply #'+ scores))
                           (count (length scores))
                           (avg (/ (float total) count))
                           (max-score (apply #'max scores))
                           (min-score (apply #'min scores))
                           ;; Filter: get names of A-grade students
                           (a-students
                            (mapcar
                             (lambda (rec)
                               (cdr (assoc "name" rec)))
                             (seq-filter
                              (lambda (rec)
                                (string= (cdr (assoc "grade" rec)) "A"))
                              records)))
                           ;; Sort records by score descending
                           (sorted-names
                            (mapcar
                             (lambda (rec)
                               (cdr (assoc "name" rec)))
                             (sort (copy-sequence records)
                                   (lambda (a b)
                                     (> (string-to-number
                                         (cdr (assoc "score" a)))
                                        (string-to-number
                                         (cdr (assoc "score" b)))))))))
                      (list header
                            (length records)
                            scores
                            total avg max-score min-score
                            a-students
                            sorted-names)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"score\" \"grade\") 5 (95 72 88 91 65) 411 82.2 95 65 (\"Alice\" \"Dave\") (\"Alice\" \"Dave\" \"Carol\" \"Bob\" \"Eve\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
