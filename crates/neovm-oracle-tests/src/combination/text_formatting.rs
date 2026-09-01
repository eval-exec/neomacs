//! Oracle parity tests for text formatting algorithms:
//! word wrapping, center alignment, left/right justification,
//! ASCII table building, thousands separators, and paragraph reflow.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Word wrapping with greedy algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_word_wrap_greedy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-word-wrap
    (lambda (text width)
      "Wrap TEXT to WIDTH columns using greedy algorithm.
       Splits on spaces, never breaks words. Returns list of lines."
      (let ((words (let ((result nil) (start 0) (len (length text)))
                     (while (< start len)
                       ;; Skip leading spaces
                       (while (and (< start len) (= (aref text start) ?\s))
                         (setq start (1+ start)))
                       (when (< start len)
                         (let ((end start))
                           (while (and (< end len) (/= (aref text end) ?\s))
                             (setq end (1+ end)))
                           (setq result (cons (substring text start end) result)
                                 start end))))
                     (nreverse result)))
            (lines nil)
            (current-line ""))
        (dolist (word words)
          (cond
           ;; Empty line: start fresh
           ((string= current-line "")
            (setq current-line word))
           ;; Fits on current line
           ((<= (+ (length current-line) 1 (length word)) width)
            (setq current-line (concat current-line " " word)))
           ;; Doesn't fit: flush and start new line
           (t
            (setq lines (cons current-line lines)
                  current-line word))))
        ;; Flush last line
        (unless (string= current-line "")
          (setq lines (cons current-line lines)))
        (nreverse lines))))

  (unwind-protect
      (let ((text "The quick brown fox jumped over the lazy sleeping dog while the cat watched from the warm sunny windowsill and the birds sang their morning songs in the tall oak tree"))
        (list
         ;; Wrap to 40 columns
         (let ((lines (funcall 'neovm--test-word-wrap text 40)))
           (list lines
                 (length lines)
                 ;; Verify no line exceeds width
                 (let ((ok t))
                   (dolist (l lines)
                     (when (> (length l) 40) (setq ok nil)))
                   ok)))
         ;; Wrap to 20 columns
         (let ((lines (funcall 'neovm--test-word-wrap text 20)))
           (list lines
                 (length lines)
                 (let ((ok t))
                   (dolist (l lines)
                     (when (> (length l) 20) (setq ok nil)))
                   ok)))
         ;; Wrap to 80 columns (few line breaks)
         (length (funcall 'neovm--test-word-wrap text 80))
         ;; Edge: single long word
         (funcall 'neovm--test-word-wrap "supercalifragilistic" 10)))
    (fmakunbound 'neovm--test-word-wrap)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"The quick brown fox jumped over the lazy\" \"sleeping dog while the cat watched from\" \"the warm sunny windowsill and the birds\" \"sang their morning songs in the tall oak\" \"tree\") 5 t) ((\"The quick brown fox\" \"jumped over the lazy\" \"sleeping dog while\" \"the cat watched from\" \"the warm sunny\" \"windowsill and the\" \"birds sang their\" \"morning songs in the\" \"tall oak tree\") 9 t) 3 (\"supercalifragilistic\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Center-align text within fixed width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_center_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-center
    (lambda (text width)
      "Center TEXT within WIDTH chars. If text is longer than width, return as-is.
       Odd padding goes to the right."
      (if (>= (length text) width)
          text
        (let* ((total-pad (- width (length text)))
               (left-pad (/ total-pad 2))
               (right-pad (- total-pad left-pad)))
          (concat (make-string left-pad ?\s)
                  text
                  (make-string right-pad ?\s))))))

  (unwind-protect
      (let ((width 30)
            (lines '("Title" "Subtitle Goes Here" "By Author Name" "" "Chapter One" "A very long line that exceeds the width limit")))
        (let ((centered (mapcar (lambda (l) (funcall 'neovm--test-center l width)) lines)))
          (list centered
                ;; Verify all lines that were shorter are now exactly `width`
                (mapcar #'length centered)
                ;; Verify the text content is preserved (trimmed)
                (mapcar (lambda (l) (string-trim l)) centered)
                ;; Verify padding is balanced (left <= right, diff <= 1)
                (mapcar (lambda (l)
                          (if (= (length l) 0) t
                            (let ((left-spaces 0) (i 0))
                              (while (and (< i (length l)) (= (aref l i) ?\s))
                                (setq left-spaces (1+ left-spaces) i (1+ i)))
                              (let ((right-spaces 0) (j (1- (length l))))
                                (while (and (>= j 0) (= (aref l j) ?\s))
                                  (setq right-spaces (1+ right-spaces) j (1- j)))
                                (<= (abs (- left-spaces right-spaces)) 1)))))
                        centered))))
    (fmakunbound 'neovm--test-center)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"            Title             \" \"      Subtitle Goes Here      \" \"        By Author Name        \" \"                              \" \"         Chapter One          \" \"A very long line that exceeds the width limit\") (30 30 30 30 30 45) (\"Title\" \"Subtitle Goes Here\" \"By Author Name\" \"\" \"Chapter One\" \"A very long line that exceeds the width limit\") (t t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Left/right justify text (pad between words)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_justify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-justify
    (lambda (text width)
      "Fully justify TEXT to WIDTH by distributing extra spaces between words.
       Extra spaces distributed left-to-right. Single-word lines are left-aligned."
      ;; Split into words
      (let ((words nil) (start 0) (len (length text)))
        (while (< start len)
          (while (and (< start len) (= (aref text start) ?\s))
            (setq start (1+ start)))
          (when (< start len)
            (let ((end start))
              (while (and (< end len) (/= (aref text end) ?\s))
                (setq end (1+ end)))
              (setq words (cons (substring text start end) words)
                    start end))))
        (setq words (nreverse words))
        (if (<= (length words) 1)
            ;; Single word or empty: left-align
            (let ((w (or (car words) "")))
              (concat w (make-string (max 0 (- width (length w))) ?\s)))
          ;; Multiple words: distribute spaces
          (let* ((total-word-len (apply #'+ (mapcar #'length words)))
                 (total-spaces (- width total-word-len))
                 (gaps (1- (length words)))
                 (base-spaces (/ total-spaces gaps))
                 (extra (% total-spaces gaps))
                 (result "")
                 (i 0))
            (dolist (word words)
              (setq result (concat result word))
              (when (< i (1- (length words)))
                (let ((sp (+ base-spaces (if (< i extra) 1 0))))
                  (setq result (concat result (make-string sp ?\s)))))
              (setq i (1+ i)))
            result)))))

  (unwind-protect
      (let ((width 50))
        (let ((test-lines '("This is a line of text to justify"
                             "Short"
                             "Two words"
                             "The quick brown fox jumps over the dog"
                             "a b c d e f g h")))
          (let ((justified (mapcar (lambda (l) (funcall 'neovm--test-justify l width)) test-lines)))
            (list justified
                  ;; Verify all lines are exactly width
                  (mapcar #'length justified)
                  (let ((all-correct t))
                    (dolist (j justified)
                      (unless (= (length j) width) (setq all-correct nil)))
                    all-correct)
                  ;; Verify word content preserved
                  (mapcar (lambda (j) (length (split-string j " " t))) justified)))))
    (fmakunbound 'neovm--test-justify)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"This    is    a    line   of   text   to   justify\" \"Short                                             \" \"Two                                          words\" \"The   quick   brown   fox   jumps   over  the  dog\" \"a      b      c      d      e      f      g      h\") (50 50 50 50 50) t (8 1 2 8 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Build ASCII table with column alignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_ascii_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-build-table
    (lambda (headers rows alignments)
      "Build an ASCII table with borders.
       ALIGNMENTS is a list of 'left, 'right, or 'center per column."
      ;; Compute column widths
      (let ((widths (mapcar #'length headers)))
        (dolist (row rows)
          (let ((i 0))
            (dolist (cell row)
              (when (> (length cell) (nth i widths))
                (setcar (nthcdr i widths) (length cell)))
              (setq i (1+ i)))))
        ;; Pad cell according to alignment
        (let ((pad (lambda (text w align)
                     (let ((gap (- w (length text))))
                       (cond
                        ((eq align 'right)
                         (concat (make-string gap ?\s) text))
                        ((eq align 'center)
                         (let* ((lp (/ gap 2))
                                (rp (- gap lp)))
                           (concat (make-string lp ?\s) text (make-string rp ?\s))))
                        (t (concat text (make-string gap ?\s))))))))
          ;; Build separator
          (let ((sep (concat "+-"
                             (mapconcat (lambda (w) (make-string w ?-))
                                        widths "-+-")
                             "-+")))
            ;; Build header row
            (let ((hdr (concat "| "
                               (mapconcat #'identity
                                          (let ((result nil) (i 0))
                                            (dolist (h headers)
                                              (setq result (cons (funcall pad h (nth i widths) (nth i alignments))
                                                                 result)
                                                    i (1+ i)))
                                            (nreverse result))
                                          " | ")
                               " |")))
              ;; Build data rows
              (let ((data-rows
                     (mapcar (lambda (row)
                               (concat "| "
                                       (mapconcat #'identity
                                                  (let ((result nil) (i 0))
                                                    (dolist (cell row)
                                                      (setq result (cons (funcall pad cell (nth i widths) (nth i alignments))
                                                                         result)
                                                            i (1+ i)))
                                                    (nreverse result))
                                                  " | ")
                                       " |"))
                             rows)))
                (list sep hdr sep data-rows sep))))))))

  (unwind-protect
      (let ((result (funcall 'neovm--test-build-table
                             '("Name" "Age" "Score" "Grade")
                             '(("Alice" "30" "95.5" "A")
                               ("Bob" "25" "87.3" "B+")
                               ("Caroline" "35" "91.0" "A-")
                               ("Dave" "28" "78.9" "C+"))
                             '(left right center center))))
        (let ((sep (nth 0 result))
              (hdr (nth 1 result))
              (data-rows (nth 3 result)))
          (list
           ;; The table structure
           result
           ;; All lines have same length
           (let ((expected (length sep)) (ok t))
             (unless (= (length hdr) expected) (setq ok nil))
             (dolist (r data-rows)
               (unless (= (length r) expected) (setq ok nil)))
             (list ok expected))
           ;; Row count
           (length data-rows))))
    (fmakunbound 'neovm--test-build-table)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"+----------+-----+-------+-------+\" \"| Name     | Age | Score | Grade |\" \"+----------+-----+-------+-------+\" (\"| Alice    |  30 | 95.5  |   A   |\" \"| Bob      |  25 | 87.3  |  B+   |\" \"| Caroline |  35 | 91.0  |  A-   |\" \"| Dave     |  28 | 78.9  |  C+   |\") \"+----------+-----+-------+-------+\") (t 34) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format numbers with thousands separators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_thousands_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-format-number
    (lambda (n separator group-size)
      "Format integer N with SEPARATOR every GROUP-SIZE digits from the right.
       Handles negative numbers."
      (let* ((neg (< n 0))
             (s (number-to-string (abs n)))
             (len (length s))
             (result nil)
             (count 0)
             (i (1- len)))
        (while (>= i 0)
          (setq result (cons (aref s i) result)
                count (1+ count))
          (when (and (= (% count group-size) 0) (> i 0))
            (setq result (cons (aref separator 0) result)))
          (setq i (1- i)))
        (let ((formatted (concat result)))
          (if neg (concat "-" formatted) formatted)))))

  (unwind-protect
      (let ((test-cases '((0 "," 3)
                           (42 "," 3)
                           (1000 "," 3)
                           (1234567 "," 3)
                           (-9876543 "," 3)
                           (1000000000 "," 3)
                           (1234567890 "." 3)      ;; European style with dot
                           (123456789 "_" 3)       ;; Rust-style underscores
                           (12345678 " " 4)        ;; Group by 4
                           (-1 "," 3))))
        (mapcar (lambda (tc)
                  (let ((n (nth 0 tc))
                        (sep (nth 1 tc))
                        (gs (nth 2 tc)))
                    (list n (funcall 'neovm--test-format-number n sep gs))))
                test-cases))
    (fmakunbound 'neovm--test-format-number)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 \"0\") (42 \"42\") (1000 \"1,000\") (1234567 \"1,234,567\") (-9876543 \"-9,876,543\") (1000000000 \"1,000,000,000\") (1234567890 \"1.234.567.890\") (123456789 \"123_456_789\") (12345678 \"1234 5678\") (-1 \"-1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Paragraph reflow (unwrap then rewrap)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_paragraph_reflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-reflow
    (lambda (text new-width)
      "Reflow TEXT to NEW-WIDTH. Unwraps paragraphs (separated by blank lines),
       then re-wraps each to the new width. Preserves paragraph boundaries."
      ;; Split into paragraphs (separated by empty lines)
      (let ((paragraphs nil)
            (current-para nil)
            (lines (let ((result nil) (start 0) (len (length text)))
                     (while (<= start len)
                       (let ((end (or (let ((p start) (found nil))
                                        (while (and (< p len) (not found))
                                          (when (= (aref text p) ?\n)
                                            (setq found p))
                                          (setq p (1+ p)))
                                        found)
                                      len)))
                         (setq result (cons (substring text start end) result)
                               start (1+ end))))
                     (nreverse result))))
        ;; Group lines into paragraphs
        (dolist (line lines)
          (if (string= (string-trim line) "")
              (progn
                (when current-para
                  (setq paragraphs (cons (nreverse current-para) paragraphs)
                        current-para nil)))
            (setq current-para (cons line current-para))))
        (when current-para
          (setq paragraphs (cons (nreverse current-para) paragraphs)))
        (setq paragraphs (nreverse paragraphs))
        ;; Unwrap each paragraph into a single string, then rewrap
        (let ((reflowed
               (mapcar
                (lambda (para-lines)
                  ;; Join all lines with spaces, normalize whitespace
                  (let ((joined (mapconcat #'identity para-lines " "))
                        (words nil) (start 0))
                    (let ((len (length joined)))
                      (while (< start len)
                        (while (and (< start len) (= (aref joined start) ?\s))
                          (setq start (1+ start)))
                        (when (< start len)
                          (let ((end start))
                            (while (and (< end len) (/= (aref joined end) ?\s))
                              (setq end (1+ end)))
                            (setq words (cons (substring joined start end) words)
                                  start end)))))
                    (setq words (nreverse words))
                    ;; Wrap to new width
                    (let ((result-lines nil) (cur ""))
                      (dolist (w words)
                        (cond
                         ((string= cur "")
                          (setq cur w))
                         ((<= (+ (length cur) 1 (length w)) new-width)
                          (setq cur (concat cur " " w)))
                         (t
                          (setq result-lines (cons cur result-lines)
                                cur w))))
                      (unless (string= cur "")
                        (setq result-lines (cons cur result-lines)))
                      (nreverse result-lines))))
                paragraphs)))
          ;; Join paragraphs with blank line between them
          (let ((all-lines nil) (first t))
            (dolist (para-lines reflowed)
              (unless first
                (setq all-lines (cons "" all-lines)))
              (dolist (l para-lines)
                (setq all-lines (cons l all-lines)))
              (setq first nil))
            (nreverse all-lines))))))

  (unwind-protect
      (let ((text "This is the first paragraph that has been
wrapped at a narrow width. It should be
unwrapped and then re-wrapped to fit the
new column width properly.

Here is a second paragraph. This one is
also wrapped at the same narrow width
and needs to be reflowed independently
from the first paragraph.

Short paragraph."))
        (list
         ;; Reflow to 60 columns
         (let ((lines (funcall 'neovm--test-reflow text 60)))
           (list lines
                 (length lines)
                 ;; Verify no content line exceeds 60 chars
                 (let ((ok t))
                   (dolist (l lines)
                     (when (and (not (string= l ""))
                                (> (length l) 60))
                       (setq ok nil)))
                   ok)))
         ;; Reflow to 40 columns (narrower)
         (length (funcall 'neovm--test-reflow text 40))
         ;; Reflow to 100 columns (wider -- should produce fewer lines)
         (length (funcall 'neovm--test-reflow text 100))))
    (fmakunbound 'neovm--test-reflow)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"This is the first paragraph that has been wrapped at a\" \"narrow width. It should be unwrapped and then re-wrapped to\" \"fit the new column width properly.\" \"\" \"Here is a second paragraph. This one is also wrapped at the\" \"same narrow width and needs to be reflowed independently\" \"from the first paragraph.\" \"\" \"Short paragraph.\") 9 t) 11 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Column-based data formatting with truncation and ellipsis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_textfmt_column_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-format-columns
    (lambda (data col-specs)
      "Format DATA (list of lists of strings) into fixed-width columns.
       COL-SPECS is list of (width alignment truncate-with-ellipsis).
       Returns list of formatted lines."
      (mapcar
       (lambda (row)
         (let ((parts nil) (i 0))
           (dolist (cell row)
             (let* ((spec (nth i col-specs))
                    (width (nth 0 spec))
                    (align (nth 1 spec))
                    (truncate-p (nth 2 spec))
                    ;; Truncate if needed
                    (truncated
                     (if (and truncate-p (> (length cell) width))
                         (concat (substring cell 0 (- width 3)) "...")
                       cell))
                    ;; Pad to width
                    (gap (max 0 (- width (length truncated))))
                    (padded
                     (cond
                      ((eq align 'right)
                       (concat (make-string gap ?\s) truncated))
                      ((eq align 'center)
                       (let* ((lp (/ gap 2)) (rp (- gap lp)))
                         (concat (make-string lp ?\s) truncated (make-string rp ?\s))))
                      (t
                       (concat truncated (make-string gap ?\s))))))
               (setq parts (cons padded parts)
                     i (1+ i))))
           (mapconcat #'identity (nreverse parts) " | ")))
       data)))

  (unwind-protect
      (let ((specs '((15 left t)    ;; Name: 15 wide, left, truncate
                     (8 right nil)   ;; ID: 8 wide, right, no truncate
                     (20 left t)     ;; Description: 20 wide, left, truncate
                     (10 center nil) ;; Status: 10 wide, center, no truncate
                     ))
            (data '(("Alice Johnson" "1234" "Senior Developer" "Active")
                    ("Bob" "5678" "Junior Developer Intern" "Active")
                    ("Caroline Beauregard-Smith" "91011" "Team Lead" "On Leave")
                    ("D" "0" "A" "X"))))
        (let ((formatted (funcall 'neovm--test-format-columns data specs)))
          (list
           formatted
           ;; All lines should have same length
           (let ((lens (mapcar #'length formatted)))
             (list lens (apply #'= lens)))
           ;; Verify truncation happened where expected
           (length formatted))))
    (fmakunbound 'neovm--test-format-columns)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice Johnson   |     1234 | Senior Developer     |   Active  \" \"Bob             |     5678 | Junior Developer ... |   Active  \" \"Caroline Bea... |    91011 | Team Lead            |  On Leave \" \"D               |        0 | A                    |     X     \") ((62 62 62 62) t) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
