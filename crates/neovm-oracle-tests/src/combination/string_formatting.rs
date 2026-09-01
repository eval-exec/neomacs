//! Complex oracle parity tests for string formatting combinations:
//! table formatting with aligned columns, box drawing, tree visualization,
//! progress bar rendering, diff output formatting, and log formatting
//! with timestamps and severity levels.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Table formatting with aligned columns (variable width)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_table_aligned_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute column widths from data, then format a table with proper
    // padding, header separator, and right-aligned numeric columns.
    let form = r#"(let ((headers '("Name" "Age" "Score" "City"))
      (rows '(("Alice" "30" "95.2" "New York")
              ("Bob" "25" "87.6" "Los Angeles")
              ("Carol" "35" "91.8" "Chicago")
              ("Dave" "28" "78.3" "San Francisco")
              ("Eve" "32" "99.1" "Boston")))
      (numeric-cols '(1 2)))  ;; Age and Score are numeric (right-aligned)
  ;; Compute column widths (max of header and all data widths)
  (let ((widths (mapcar #'length headers)))
    (dolist (row rows)
      (let ((i 0))
        (dolist (cell row)
          (when (> (length cell) (nth i widths))
            (setcar (nthcdr i widths) (length cell)))
          (setq i (1+ i)))))
    ;; Format a row with padding
    (let ((format-row
           (lambda (cells)
             (let ((parts nil) (i 0))
               (dolist (cell cells)
                 (let* ((w (nth i widths))
                        (padded
                         (if (memq i numeric-cols)
                             ;; Right-align numeric
                             (concat (make-string (- w (length cell)) ?\s)
                                     cell)
                           ;; Left-align text
                           (concat cell
                                   (make-string (- w (length cell)) ?\s)))))
                   (setq parts (cons padded parts))
                   (setq i (1+ i))))
               (mapconcat #'identity (nreverse parts) " | ")))))
      ;; Build table
      (let ((header-line (funcall format-row headers))
            (separator (mapconcat
                        (lambda (w) (make-string w ?-))
                        widths "-+-"))
            (data-lines (mapcar format-row rows)))
        (list header-line
              separator
              data-lines
              ;; Verify all lines have same length
              (let ((expected-len (length header-line))
                    (all-same t))
                (dolist (line data-lines)
                  (unless (= (length line) expected-len)
                    (setq all-same nil)))
                (list all-same expected-len)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Name  | Age | Score | City         \" \"------+-----+-------+--------------\" (\"Alice |  30 |  95.2 | New York     \" \"Bob   |  25 |  87.6 | Los Angeles  \" \"Carol |  35 |  91.8 | Chicago      \" \"Dave  |  28 |  78.3 | San Francisco\" \"Eve   |  32 |  99.1 | Boston       \") (t 35))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Box drawing around text (Unicode box-drawing characters)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_box_drawing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Draw a box around multi-line text using ASCII box characters.
    // Handles variable-width lines by padding to the longest.
    let form = r#"(let ((lines '("Hello, World!"
                   "This is a box."
                   "Multiple lines!"
                   "End.")))
  ;; Find max width
  (let ((max-width 0))
    (dolist (line lines)
      (when (> (length line) max-width)
        (setq max-width (length line))))
    ;; Add 2 for padding on each side
    (let* ((inner-width (+ max-width 2))
           (top (concat "+" (make-string inner-width ?-) "+"))
           (bottom (concat "+" (make-string inner-width ?-) "+"))
           (formatted-lines
            (mapcar (lambda (line)
                      (let ((padding (- max-width (length line))))
                        (concat "| " line
                                (make-string padding ?\s) " |")))
                    lines))
           ;; Build complete box
           (box-parts (append (list top)
                              formatted-lines
                              (list bottom)))
           (box-string (mapconcat #'identity box-parts "\n")))
      ;; Verify properties
      (list box-string
            ;; All lines should be the same length
            (let ((expected (length top)) (ok t))
              (dolist (part box-parts)
                (unless (= (length part) expected)
                  (setq ok nil)))
              (list ok expected))
            ;; Line count should be input lines + 2 (top + bottom)
            (length box-parts)
            ;; Top and bottom should match
            (string= top bottom)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"+-----------------+\\n| Hello, World!   |\\n| This is a box.  |\\n| Multiple lines! |\\n| End.            |\\n+-----------------+\" (t 19) 6 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree visualization (directory listing format)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_tree_visualization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Render a tree structure as indented lines with branch characters,
    // similar to the `tree` command output.
    let form = r#"(progn
  (fset 'neovm--test-render-tree
    (lambda (node prefix is-last)
      (let* ((name (car node))
             (children (cdr node))
             (connector (if is-last "`-- " "|-- "))
             (line (concat prefix connector name))
             (result (list line))
             (child-prefix (concat prefix
                                   (if is-last "    " "|   ")))
             (remaining children))
        (while remaining
          (let* ((child (car remaining))
                 (last-child (null (cdr remaining)))
                 (child-lines
                  (funcall 'neovm--test-render-tree
                           child child-prefix last-child)))
            (setq result (append result child-lines))
            (setq remaining (cdr remaining))))
        result)))

  (unwind-protect
      (let ((tree '("project"
                     ("src"
                      ("main.rs")
                      ("lib.rs")
                      ("utils"
                       ("helpers.rs")
                       ("math.rs")))
                     ("tests"
                      ("test_main.rs"))
                     ("Cargo.toml"))))
        ;; Render: root line + children
        (let* ((root-line (car tree))
               (children (cdr tree))
               (lines (list root-line))
               (remaining children))
          (while remaining
            (let* ((child (car remaining))
                   (last-child (null (cdr remaining)))
                   (child-lines
                    (funcall 'neovm--test-render-tree
                             child "" last-child)))
              (setq lines (append lines child-lines))
              (setq remaining (cdr remaining))))
          (list (mapconcat #'identity lines "\n")
                (length lines))))
    (fmakunbound 'neovm--test-render-tree)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"project\\n|-- src\\n|   |-- main.rs\\n|   |-- lib.rs\\n|   `-- utils\\n|       |-- helpers.rs\\n|       `-- math.rs\\n|-- tests\\n|   `-- test_main.rs\\n`-- Cargo.toml\" 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Progress bar rendering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_progress_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Render progress bars at various completion percentages with
    // configurable width and fill/empty characters.
    let form = r#"(let ((render-progress
         (lambda (percent width fill-char empty-char)
           (let* ((clamped (max 0 (min 100 percent)))
                  (filled (/ (* clamped width) 100))
                  (remaining (- width filled))
                  (bar (concat "["
                               (make-string filled fill-char)
                               (make-string remaining empty-char)
                               "]"))
                  (label (format "%3d%%" clamped)))
             (concat bar " " label)))))
  ;; Render bars at various percentages
  (let ((percentages '(0 10 25 33 50 67 75 90 100))
        (width 30))
    (let ((bars (mapcar (lambda (pct)
                          (funcall render-progress pct width ?# ?.))
                        percentages))
          ;; Also test with different style
          (fancy-bars (mapcar (lambda (pct)
                                (funcall render-progress pct 20 ?= ?-))
                              '(0 50 100))))
      ;; Verify all bars have same length (for given width)
      (let ((expected-len (+ width 2 1 4)) ;; [bar] pct%
            (all-same t))
        (dolist (bar bars)
          (unless (= (length bar) expected-len)
            (setq all-same nil)))
        (list bars
              fancy-bars
              all-same
              ;; Verify boundary cases
              (funcall render-progress -5 10 ?# ?.)
              (funcall render-progress 150 10 ?# ?.))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"[..............................]   0%\" \"[###...........................]  10%\" \"[#######.......................]  25%\" \"[#########.....................]  33%\" \"[###############...............]  50%\" \"[####################..........]  67%\" \"[######################........]  75%\" \"[###########################...]  90%\" \"[##############################] 100%\") (\"[--------------------]   0%\" \"[==========----------]  50%\" \"[====================] 100%\") t \"[..........]   0%\" \"[##########] 100%\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Diff output formatting (unified diff style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_unified_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate a simplified unified diff between two lists of lines,
    // using longest common subsequence to find matching lines.
    let form = r#"(progn
  ;; Simple LCS-based diff
  (fset 'neovm--test-diff-lcs
    (lambda (old-lines new-lines)
      (let* ((m (length old-lines))
             (n (length new-lines))
             ;; Build LCS table (using vectors for O(m*n) DP)
             (table (make-vector (1+ m) nil)))
        ;; Initialize table rows
        (dotimes (i (1+ m))
          (aset table i (make-vector (1+ n) 0)))
        ;; Fill DP table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (string= (nth (1- i) old-lines)
                              (nth (1- j) new-lines))
                    (aset (aref table i) j
                          (1+ (aref (aref table (1- i)) (1- j))))
                  (aset (aref table i) j
                        (max (aref (aref table (1- i)) j)
                             (aref (aref table i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to produce diff
        (let ((i m) (j n) (result nil))
          (while (or (> i 0) (> j 0))
            (cond
             ((and (> i 0) (> j 0)
                   (string= (nth (1- i) old-lines)
                             (nth (1- j) new-lines)))
              (setq result (cons (concat " " (nth (1- i) old-lines))
                                 result))
              (setq i (1- i) j (1- j)))
             ((and (> j 0)
                   (or (= i 0)
                       (>= (aref (aref table i) (1- j))
                            (aref (aref table (1- i)) j))))
              (setq result (cons (concat "+" (nth (1- j) new-lines))
                                 result))
              (setq j (1- j)))
             (t
              (setq result (cons (concat "-" (nth (1- i) old-lines))
                                 result))
              (setq i (1- i)))))
          result))))

  (unwind-protect
      (let* ((old '("alpha" "bravo" "charlie" "delta" "echo"))
             (new '("alpha" "BRAVO" "charlie" "foxtrot" "echo" "golf"))
             (diff (funcall 'neovm--test-diff-lcs old new))
             ;; Format as unified diff
             (header (list "--- old"
                           "+++ new"
                           (format "@@ -%d,%d +%d,%d @@"
                                   1 (length old)
                                   1 (length new))))
             (output (append header diff))
             ;; Count additions and deletions
             (additions 0)
             (deletions 0)
             (unchanged 0))
        (dolist (line diff)
          (cond
           ((string-prefix-p "+" line) (setq additions (1+ additions)))
           ((string-prefix-p "-" line) (setq deletions (1+ deletions)))
           (t (setq unchanged (1+ unchanged)))))
        (list output
              (list 'add additions 'del deletions 'same unchanged)
              ;; Verify that additions - deletions = new-len - old-len
              (= (- additions deletions)
                 (- (length new) (length old)))))
    (fmakunbound 'neovm--test-diff-lcs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"--- old\" \"+++ new\" \"@@ -1,5 +1,6 @@\" \" alpha\" \"-bravo\" \"+BRAVO\" \" charlie\" \"-delta\" \"+foxtrot\" \" echo\" \"+golf\") (add 3 del 2 same 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Log formatter with timestamps and severity levels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_log_formatter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Format log entries with timestamps, severity levels, component names,
    // and messages. Supports filtering by level and colorized output markers.
    let form = r#"(let ((levels '((debug . 0) (info . 1) (warn . 2) (error . 3) (fatal . 4)))
      (level-tags '((debug . "DEBUG") (info . "INFO ") (warn . "WARN ") (error . "ERROR") (fatal . "FATAL")))
      (entries
       '((10 debug "parser" "Tokenizing input")
         (11 info "server" "Listening on port 8080")
         (12 debug "parser" "Parsed 42 tokens")
         (15 warn "memory" "Usage above 80%")
         (16 info "server" "Request received")
         (18 error "db" "Connection timeout")
         (19 debug "cache" "Cache miss for key xyz")
         (20 fatal "server" "Out of memory")
         (21 info "server" "Restarting..."))))
  ;; Format a single log entry
  (let ((format-entry
         (lambda (entry)
           (let* ((ts (car entry))
                  (level (cadr entry))
                  (component (caddr entry))
                  (message (cadddr entry))
                  (tag (cdr (assq level level-tags)))
                  (ts-str (format "%04d" ts)))
             (format "[%s] %s [%-8s] %s" ts-str tag component message))))
        ;; Filter entries by minimum level
        (filter-by-level
         (lambda (entries min-level)
           (let ((min-val (cdr (assq min-level levels))))
             (delq nil
                   (mapcar (lambda (e)
                             (when (>= (cdr (assq (cadr e) levels))
                                       min-val)
                               e))
                           entries))))))
    ;; Format all entries
    (let* ((all-formatted (mapcar format-entry entries))
           ;; Filter to warn and above
           (warn-up (funcall filter-by-level entries 'warn))
           (warn-formatted (mapcar format-entry warn-up))
           ;; Count by level
           (level-counts
            (let ((counts nil))
              (dolist (e entries)
                (let ((existing (assq (cadr e) counts)))
                  (if existing
                      (setcdr existing (1+ (cdr existing)))
                    (setq counts (cons (cons (cadr e) 1) counts)))))
              (sort counts (lambda (a b)
                             (< (cdr (assq (car a) levels))
                                (cdr (assq (car b) levels)))))))
           ;; Group by component
           (by-component
            (let ((groups nil))
              (dolist (e entries)
                (let* ((comp (caddr e))
                       (existing (assoc comp groups)))
                  (if existing
                      (setcdr existing (1+ (cdr existing)))
                    (setq groups (cons (cons comp 1) groups)))))
              (sort groups (lambda (a b)
                             (string< (car a) (car b)))))))
      (list all-formatted
            warn-formatted
            level-counts
            by-component
            (length entries)
            (length warn-up)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"[0010] DEBUG [parser  ] Tokenizing input\" \"[0011] INFO  [server  ] Listening on port 8080\" \"[0012] DEBUG [parser  ] Parsed 42 tokens\" \"[0015] WARN  [memory  ] Usage above 80%\" \"[0016] INFO  [server  ] Request received\" \"[0018] ERROR [db      ] Connection timeout\" \"[0019] DEBUG [cache   ] Cache miss for key xyz\" \"[0020] FATAL [server  ] Out of memory\" \"[0021] INFO  [server  ] Restarting...\") (\"[0015] WARN  [memory  ] Usage above 80%\" \"[0018] ERROR [db      ] Connection timeout\" \"[0020] FATAL [server  ] Out of memory\") ((debug . 3) (info . 3) (warn . 1) (error . 1) (fatal . 1)) ((\"cache\" . 1) (\"db\" . 1) (\"memory\" . 1) (\"parser\" . 2) (\"server\" . 4)) 9 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Markdown table generator from data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strfmt_markdown_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate a Markdown-formatted table with column alignment specifiers
    // (left, center, right) from structured data.
    let form = r#"(let ((headers '("Language" "Year" "Typing" "Score"))
      (alignments '(left right center right))
      (rows '(("Rust" "2010" "Static" "95")
              ("Python" "1991" "Dynamic" "88")
              ("Haskell" "1990" "Static" "82")
              ("JavaScript" "1995" "Dynamic" "79")
              ("C" "1972" "Static" "91"))))
  ;; Compute column widths
  (let ((widths (mapcar #'length headers)))
    (dolist (row rows)
      (let ((i 0))
        (dolist (cell row)
          (when (> (length cell) (nth i widths))
            (setcar (nthcdr i widths) (length cell)))
          (setq i (1+ i)))))
    ;; Pad a cell according to alignment
    (let ((pad-cell
           (lambda (text width alignment)
             (let ((padding (- width (length text))))
               (cond
                ((eq alignment 'left)
                 (concat text (make-string padding ?\s)))
                ((eq alignment 'right)
                 (concat (make-string padding ?\s) text))
                ((eq alignment 'center)
                 (let* ((left-pad (/ padding 2))
                        (right-pad (- padding left-pad)))
                   (concat (make-string left-pad ?\s)
                           text
                           (make-string right-pad ?\s))))
                (t (concat text (make-string padding ?\s))))))))
      ;; Format a row
      (let ((format-row
             (lambda (cells)
               (concat "| "
                       (mapconcat
                        #'identity
                        (let ((result nil) (i 0))
                          (dolist (cell cells)
                            (setq result
                                  (cons (funcall pad-cell cell
                                                 (nth i widths)
                                                 (nth i alignments))
                                        result))
                            (setq i (1+ i)))
                          (nreverse result))
                        " | ")
                       " |")))
            ;; Separator line with alignment markers
            (make-separator
             (lambda ()
               (concat "| "
                       (mapconcat
                        #'identity
                        (let ((result nil) (i 0))
                          (dolist (w widths)
                            (let ((align (nth i alignments)))
                              (setq result
                                    (cons (cond
                                           ((eq align 'left)
                                            (concat ":" (make-string (1- w) ?-)))
                                           ((eq align 'right)
                                            (concat (make-string (1- w) ?-) ":"))
                                           ((eq align 'center)
                                            (concat ":" (make-string (- w 2) ?-) ":"))
                                           (t (make-string w ?-)))
                                          result)))
                            (setq i (1+ i)))
                          (nreverse result))
                        " | ")
                       " |"))))
        (let* ((header-line (funcall format-row headers))
               (sep-line (funcall make-separator))
               (data-lines (mapcar format-row rows))
               (table (mapconcat #'identity
                                 (append (list header-line sep-line)
                                         data-lines)
                                 "\n")))
          (list table
                (length data-lines)
                ;; Verify consistent line lengths
                (let ((lens (mapcar #'length
                                    (append (list header-line sep-line)
                                            data-lines))))
                  (apply #'= lens))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"| Language   | Year | Typing  | Score |\\n| :--------- | ---: | :-----: | ----: |\\n| Rust       | 2010 | Static  |    95 |\\n| Python     | 1991 | Dynamic |    88 |\\n| Haskell    | 1990 | Static  |    82 |\\n| JavaScript | 1995 | Dynamic |    79 |\\n| C          | 1972 | Static  |    91 |\" 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
