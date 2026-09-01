//! Oracle parity tests for advanced `with-temp-buffer` patterns:
//! multiple insert/delete/search cycles, nested with-temp-buffer,
//! interaction with save-excursion/save-restriction, buffer-local variables
//! in temp buffers, returning complex values, temp buffer with text properties,
//! temp buffer for parsing (splitting lines, extracting fields), temp buffer
//! as string builder, and error handling inside temp buffer.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Multiple insert/delete/search cycles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_multi_cycle_insert_delete_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform multiple rounds of insert, search, delete, and verify buffer
    // state after each cycle.
    let form = r####"(with-temp-buffer
  ;; Cycle 1: insert and delete a middle region
  (insert "alpha beta gamma delta epsilon")
  (goto-char (point-min))
  (search-forward "beta ")
  (let ((start1 (match-beginning 0)))
    (search-forward "delta ")
    (let ((end1 (point)))
      (delete-region start1 end1)
      (let ((after-cycle1 (buffer-string)))
        ;; Cycle 2: insert at point (which is where deletion happened)
        (goto-char start1)
        (insert "REPLACED ")
        (let ((after-cycle2 (buffer-string)))
          ;; Cycle 3: search-and-replace loop
          (goto-char (point-min))
          (let ((count 0))
            (while (search-forward "a" nil t)
              (replace-match "A" t t)
              (setq count (1+ count)))
            (let ((after-cycle3 (buffer-string)))
              ;; Cycle 4: delete from point-min to first space
              (goto-char (point-min))
              (search-forward " ")
              (delete-region (point-min) (point))
              (list after-cycle1 after-cycle2 after-cycle3
                    (buffer-string) count))))))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"alpha epsilon\" \"alpha REPLACED epsilon\" \"AlphA REPLACED epsilon\" \"REPLACED epsilon\" 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_interleaved_insert_search_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave insertions, forward/backward searches, and deletions
    // tracking positions precisely.
    let form = r####"(with-temp-buffer
  (insert "line1: foo\nline2: bar\nline3: baz\nline4: qux\n")
  ;; Delete line2 entirely
  (goto-char (point-min))
  (forward-line 1)
  (let ((line2-start (point)))
    (forward-line 1)
    (delete-region line2-start (point)))
  ;; Insert a new line after line1
  (goto-char (point-min))
  (end-of-line)
  (insert "\ninserted: NEW")
  ;; Search backward for "foo" from end
  (goto-char (point-max))
  (search-backward "foo")
  (let ((foo-pos (point)))
    ;; Replace "baz" with "BAZ!!!"
    (goto-char (point-min))
    (search-forward "baz")
    (replace-match "BAZ!!!" t t)
    (list (buffer-string)
          (count-lines (point-min) (point-max))
          foo-pos)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"line1: foo\\ninserted: NEW\\nline3: BAZ!!!\\nline4: qux\\n\" 4 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested with-temp-buffer with data flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_nested_data_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer temp buffer prepares data, inner temp buffer transforms it,
    // result flows back to outer.
    let form = r####"(with-temp-buffer
  (insert "hello world foo bar baz")
  (let ((original (buffer-string))
        (words nil))
    ;; Extract words using inner temp buffer
    (let ((word-list
           (with-temp-buffer
             (insert original)
             (goto-char (point-min))
             (let ((result nil))
               (while (not (eobp))
                 (let ((word-start (point)))
                   (skip-chars-forward "^ \n")
                   (when (> (point) word-start)
                     (push (buffer-substring word-start (point)) result))
                   (skip-chars-forward " \n")))
               (nreverse result)))))
      ;; Back in outer buffer, build a CSV from the words
      (erase-buffer)
      (insert (mapconcat 'identity word-list ","))
      (list word-list
            (buffer-string)
            (length word-list)))))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\" \"foo\" \"bar\" \"baz\") \"hello,world,foo,bar,baz\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_deeply_nested_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Four levels of nesting, each producing a value that the next level uses.
    let form = r####"(with-temp-buffer
  (insert "seed")
  (let ((v1 (buffer-string)))
    (let ((v2 (with-temp-buffer
                (insert (concat v1 "-L2"))
                (upcase-region (point-min) (point-max))
                (buffer-string))))
      (let ((v3 (with-temp-buffer
                  (insert (concat v2 "-L3"))
                  (goto-char (point-min))
                  (while (search-forward "-" nil t)
                    (replace-match "_" t t))
                  (buffer-string))))
        (let ((v4 (with-temp-buffer
                    (insert (concat v3 "-L4"))
                    (buffer-string))))
          ;; Outer buffer still has "seed"
          (list v1 v2 v3 v4
                (buffer-string)
                (string= (buffer-string) "seed")))))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"seed\" \"SEED-L2\" \"SEED_L2_L3\" \"SEED_L2_L3-L4\" \"seed\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_save_excursion_inside() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion inside with-temp-buffer: point is restored after
    // excursion even within the temp buffer.
    let form = r####"(with-temp-buffer
  (insert "AAABBBCCC")
  (goto-char 4)
  (let ((before-point (point)))
    (save-excursion
      (goto-char (point-min))
      (insert "PREFIX-")
      (goto-char (point-max))
      (insert "-SUFFIX"))
    ;; Point should be restored (adjusted for PREFIX- insertion)
    (let ((after-point (point))
          (content (buffer-string)))
      ;; save-excursion should have moved point back
      ;; but the insertion before point shifts it
      (list before-point after-point content
            (buffer-substring after-point (+ after-point 3))))))"####;
    let expect = expect_test::expect![[r#""OK (4 11 \"PREFIX-AAABBBCCC-SUFFIX\" \"BBB\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_save_excursion_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple nested save-excursions inside with-temp-buffer.
    let form = r####"(with-temp-buffer
  (insert "0123456789")
  (goto-char 5)
  (let ((p1 (point)))
    (save-excursion
      (goto-char 1)
      (let ((p2 (point)))
        (save-excursion
          (goto-char 8)
          (let ((p3 (point)))
            ;; Innermost: insert at position 8
            (insert "X")
            (list p1 p2 p3 (point) (buffer-string))))))
    ;; After all save-excursions, point restored
    (list (point) (buffer-string))))"####;
    let expect = expect_test::expect![[r#""OK (5 \"0123456X789\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with save-restriction (narrowing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_save_restriction_inside() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-restriction + narrow-to-region inside with-temp-buffer,
    // then widen to verify full contents.
    let form = r####"(with-temp-buffer
  (insert "HEADER\ndata line 1\ndata line 2\ndata line 3\nFOOTER")
  ;; Narrow to data lines only
  (goto-char (point-min))
  (forward-line 1)
  (let ((data-start (point)))
    (goto-char (point-max))
    (beginning-of-line)
    (let ((data-end (point)))
      (save-restriction
        (narrow-to-region data-start data-end)
        (let ((narrowed (buffer-string))
              (nmin (point-min))
              (nmax (point-max)))
          ;; Upcase within narrowed region
          (upcase-region (point-min) (point-max))
          (let ((upcased-narrow (buffer-string)))
            ;; After save-restriction, widened automatically
            (list narrowed nmin nmax upcased-narrow))))
      ;; After save-restriction exits, restriction is removed
      (list (point-min) (point-max) (buffer-string)))))"####;
    let expect = expect_test::expect![[
        r#""OK (1 50 \"HEADER\\nDATA LINE 1\\nDATA LINE 2\\nDATA LINE 3\\nFOOTER\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local variables in temp buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_buffer_local_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set buffer-local variables inside with-temp-buffer; they should
    // not leak outside.
    let form = r####"(progn
  (defvar neovm--wtba-testvar 'global-value)
  (unwind-protect
      (let ((before neovm--wtba-testvar))
        (let ((result
               (with-temp-buffer
                 (make-local-variable 'neovm--wtba-testvar)
                 (setq neovm--wtba-testvar 'local-in-temp)
                 (list neovm--wtba-testvar
                       (local-variable-p 'neovm--wtba-testvar)
                       (default-value 'neovm--wtba-testvar)))))
          ;; After with-temp-buffer, the temp buffer is killed,
          ;; so the buffer-local binding is gone.
          (list before result neovm--wtba-testvar)))
    (makunbound 'neovm--wtba-testvar)))"####;
    let expect = expect_test::expect![[
        r#""OK (global-value (local-in-temp t global-value) global-value)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Returning complex values from temp buffer body
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_return_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a hash table inside with-temp-buffer by parsing structured text,
    // return it and verify outside.
    let form = r####"(let ((ht (with-temp-buffer
                (insert "name:Alice\nage:30\ncity:NYC\nlang:Lisp\n")
                (let ((table (make-hash-table :test 'equal)))
                  (goto-char (point-min))
                  (while (not (eobp))
                    (let ((line-start (point)))
                      (end-of-line)
                      (let* ((line (buffer-substring line-start (point)))
                             (colon (string-match ":" line)))
                        (when colon
                          (puthash (substring line 0 colon)
                                   (substring line (1+ colon))
                                   table)))
                      (forward-line 1)))
                  table))))
  (list (hash-table-count ht)
        (gethash "name" ht)
        (gethash "age" ht)
        (gethash "city" ht)
        (gethash "lang" ht)
        (gethash "missing" ht 'default)))"####;
    let expect = expect_test::expect![[r#""OK (4 \"Alice\" \"30\" \"NYC\" \"Lisp\" default)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_return_nested_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a simple CSV-like format into nested lists.
    let form = r####"(let ((parsed
         (with-temp-buffer
           (insert "a,b,c\n1,2,3\nx,y,z\n")
           (goto-char (point-min))
           (let ((rows nil))
             (while (not (eobp))
               (let ((line-start (point)))
                 (end-of-line)
                 (let ((line (buffer-substring line-start (point))))
                   (when (> (length line) 0)
                     (let ((fields nil)
                           (pos 0))
                       (while (string-match "\\([^,\n]+\\)\\(?:,\\|\\'\\)" line pos)
                         (push (match-string 1 line) fields)
                         (setq pos (match-end 0)))
                       (push (nreverse fields) rows))))
                 (forward-line 1)))
             (nreverse rows)))))
  (list parsed
        (length parsed)
        (nth 0 parsed)
        (nth 1 parsed)
        (nth 2 parsed)))"####;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\") (\"x\" \"y\" \"z\")) 3 (\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\") (\"x\" \"y\" \"z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Temp buffer with text properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_text_properties_multiple_faces() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert text with multiple overlapping text properties, verify them.
    let form = r####"(with-temp-buffer
  (insert "abcdefghij")
  ;; Apply different properties to different ranges
  (put-text-property 1 4 'face 'bold)         ;; abc
  (put-text-property 3 7 'custom-id 42)       ;; cdef
  (put-text-property 5 9 'category 'special)   ;; efgh
  (put-text-property 1 10 'source "test")      ;; all
  ;; Read back
  (list
    ;; Position 1 (a): face=bold, source=test
    (get-text-property 1 'face)
    (get-text-property 1 'source)
    (get-text-property 1 'custom-id)
    ;; Position 3 (c): face=bold, custom-id=42, source=test
    (get-text-property 3 'face)
    (get-text-property 3 'custom-id)
    ;; Position 5 (e): custom-id=42, category=special, source=test
    (get-text-property 5 'custom-id)
    (get-text-property 5 'category)
    ;; Position 8 (h): category=special, source=test
    (get-text-property 8 'category)
    (get-text-property 8 'face)
    ;; Position 10 (j): source=test only -- note: property at pos 10 is the last char
    (get-text-property 10 'source)
    (get-text-property 10 'category)))"####;
    let expect = expect_test::expect![[
        r#""OK (bold \"test\" nil bold 42 42 special special nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Temp buffer for line splitting and field extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_split_lines_extract_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use temp buffer to split a multi-line string into lines, extract
    // specific fields, and aggregate.
    let form = r####"(let ((input "Alice 30 Engineer\nBob 25 Designer\nCharlie 35 Manager\nDiana 28 Developer"))
  (with-temp-buffer
    (insert input)
    (goto-char (point-min))
    (let ((names nil)
          (total-age 0)
          (jobs nil)
          (line-count 0))
      (while (not (eobp))
        (let ((line-start (point)))
          (end-of-line)
          (let* ((line (buffer-substring line-start (point)))
                 (parts nil)
                 (pos 0))
            ;; Split line by spaces
            (while (string-match "\\([^ ]+\\)" line pos)
              (push (match-string 1 line) parts)
              (setq pos (match-end 0)))
            (setq parts (nreverse parts))
            (when (>= (length parts) 3)
              (push (nth 0 parts) names)
              (setq total-age (+ total-age (string-to-number (nth 1 parts))))
              (push (nth 2 parts) jobs)
              (setq line-count (1+ line-count))))
          (forward-line 1)))
      (list (nreverse names)
            total-age
            (nreverse jobs)
            line-count
            (/ total-age line-count)))))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Charlie\" \"Diana\") 118 (\"Engineer\" \"Designer\" \"Manager\" \"Developer\") 4 29)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Temp buffer as string builder
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_string_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complex string incrementally using a temp buffer as a
    // string builder, with conditional insertions and formatting.
    let form = r####"(let ((items '(("apple" . 3) ("banana" . 0) ("cherry" . 7) ("date" . 1) ("elderberry" . 0)))
       (header "=== INVENTORY ==="))
  (with-temp-buffer
    (insert header "\n")
    (insert (make-string (length header) ?-) "\n")
    (let ((total 0)
          (non-zero 0))
      (dolist (item items)
        (let ((name (car item))
              (qty (cdr item)))
          (when (> qty 0)
            (insert (format "  %-12s: %3d\n" name qty))
            (setq total (+ total qty))
            (setq non-zero (1+ non-zero)))))
      (insert (make-string 20 ?-) "\n")
      (insert (format "  Total items: %d (%d types)\n" total non-zero))
      (list (buffer-string) total non-zero))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"=== INVENTORY ===\\n-----------------\\n  apple       :   3\\n  cherry      :   7\\n  date        :   1\\n--------------------\\n  Total items: 11 (3 types)\\n\" 11 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_string_builder_with_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Conditionally build different output formats using temp buffer.
    let form = r####"(let ((data '((1 "one" t) (2 "two" nil) (3 "three" t) (4 "four" nil) (5 "five" t))))
  (let ((result-plain
         (with-temp-buffer
           (dolist (entry data)
             (let ((num (nth 0 entry))
                   (word (nth 1 entry))
                   (active (nth 2 entry)))
               (insert (format "%d=%s" num word))
               (when active (insert "*"))
               (insert " ")))
           ;; Remove trailing space
           (when (> (buffer-size) 0)
             (delete-char -1))
           (buffer-string)))
        (result-filtered
         (with-temp-buffer
           (let ((first t))
             (dolist (entry data)
               (when (nth 2 entry)
                 (unless first (insert ", "))
                 (insert (nth 1 entry))
                 (setq first nil))))
           (buffer-string))))
    (list result-plain result-filtered)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"1=one* 2=two 3=three* 4=four 5=five*\" \"one, three, five\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling inside temp buffer: unwind-protect interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_unwind_protect_inside() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect inside with-temp-buffer ensures cleanup even on error.
    let form = r####"(let ((cleanup-ran nil))
  (condition-case err
      (with-temp-buffer
        (insert "important data")
        (unwind-protect
            (progn
              (insert " more data")
              (error "planned failure"))
          (setq cleanup-ran t)))
    (error
     (list 'caught (cadr err) cleanup-ran))))"####;
    let expect = expect_test::expect![[r#""OK (caught \"planned failure\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_adv_error_in_nested_does_not_corrupt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Error in deeply nested with-temp-buffer does not corrupt any
    // outer buffer's state.
    let form = r####"(with-temp-buffer
  (insert "outer-level-1")
  (let ((outer1 (buffer-string)))
    (let ((mid-result
           (condition-case _
               (with-temp-buffer
                 (insert "middle-level")
                 (let ((mid (buffer-string)))
                   (condition-case _
                       (with-temp-buffer
                         (insert "inner-level")
                         (error "inner boom")
                         (buffer-string))
                     (error 'inner-caught))
                   ;; Middle buffer intact after inner error
                   (list mid (buffer-string)
                         (string= mid (buffer-string)))))
             (error 'mid-caught))))
      ;; Outer buffer intact
      (list outer1
            (buffer-string)
            (string= outer1 (buffer-string))
            mid-result))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"outer-level-1\" \"outer-level-1\" t (\"middle-level\" \"middle-level\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Temp buffer for regex-based parsing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_regex_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use temp buffer with regex searching to parse structured text.
    let form = r####"(with-temp-buffer
  (insert "ERROR [2024-01-15] disk full\nWARN [2024-01-16] low memory\nINFO [2024-01-17] started\nERROR [2024-01-18] timeout\n")
  (goto-char (point-min))
  (let ((errors nil)
        (warnings nil)
        (infos nil))
    (while (re-search-forward "^\\(ERROR\\|WARN\\|INFO\\) \\[\\([^]]+\\)\\] \\(.*\\)$" nil t)
      (let ((level (match-string 1))
            (date (match-string 2))
            (msg (match-string 3)))
        (cond
         ((string= level "ERROR") (push (cons date msg) errors))
         ((string= level "WARN") (push (cons date msg) warnings))
         ((string= level "INFO") (push (cons date msg) infos)))))
    (list (nreverse errors)
          (nreverse warnings)
          (nreverse infos)
          (length errors)
          (length warnings)
          (length infos))))"####;
    let expect = expect_test::expect![[
        r#""OK (((\"2024-01-15\" . \"disk full\") (\"2024-01-18\" . \"timeout\")) ((\"2024-01-16\" . \"low memory\")) ((\"2024-01-17\" . \"started\")) 1 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Temp buffer: erase-buffer and reuse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_adv_erase_and_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Erase the temp buffer and reuse it multiple times within the same
    // with-temp-buffer form.
    let form = r####"(with-temp-buffer
  (let ((results nil))
    ;; Round 1
    (insert "round one")
    (push (buffer-string) results)
    (push (buffer-size) results)
    (erase-buffer)
    ;; Round 2
    (insert "round two content here")
    (push (buffer-string) results)
    (push (buffer-size) results)
    (erase-buffer)
    ;; Round 3: build from numbers
    (dotimes (i 5)
      (insert (number-to-string (* i i)) " "))
    (push (buffer-string) results)
    (push (point) results)
    (erase-buffer)
    ;; Round 4: empty
    (push (buffer-size) results)
    (push (buffer-string) results)
    (nreverse results)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"round one\" 9 \"round two content here\" 22 \"0 1 4 9 16 \" 12 0 \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
