//! Oracle parity tests for `put-text-property` with complex patterns:
//! START/END/PROPERTY/VALUE arguments, single and multiple properties,
//! overlapping ranges, overwriting, interaction with get-text-property
//! and text-properties-at, building syntax highlighting, and text
//! annotation systems using text properties.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Single property on full string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_full_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "hello world")))
  (put-text-property 0 (length s) 'face 'bold s)
  (list
    ;; Every position should have the property
    (get-text-property 0 'face s)
    (get-text-property 5 'face s)
    (get-text-property 10 'face s)
    ;; text-properties-at for first char
    (text-properties-at 0 s)
    ;; Verify the string content is unchanged
    (substring-no-properties s)
    ;; Property at boundary (length - 1)
    (get-text-property (1- (length s)) 'face s)))"#;
    let expect =
        expect_test::expect![[r#""OK (bold bold bold (face bold) \"hello world\" bold)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial range: START and END within string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_partial_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "abcdefghij")))
  ;; Put property only on positions 2-5
  (put-text-property 2 5 'face 'italic s)
  (list
    ;; Before range: no property
    (get-text-property 0 'face s)
    (get-text-property 1 'face s)
    ;; Inside range: has property
    (get-text-property 2 'face s)
    (get-text-property 3 'face s)
    (get-text-property 4 'face s)
    ;; At END (exclusive): no property
    (get-text-property 5 'face s)
    ;; After range: no property
    (get-text-property 8 'face s)
    ;; next-property-change from 0 should find boundary at 2
    (next-property-change 0 s)
    ;; next-property-change from 2 should find boundary at 5
    (next-property-change 2 s)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil italic italic italic nil nil 2 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple properties on overlapping ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_overlapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "abcdefghij")))
  ;; First property: face on 0-6
  (put-text-property 0 6 'face 'bold s)
  ;; Second property: color on 3-8 (overlaps with face)
  (put-text-property 3 8 'color 'red s)
  (list
    ;; Position 0-2: only face
    (text-properties-at 0 s)
    (text-properties-at 2 s)
    ;; Position 3-5: both face and color
    (text-properties-at 3 s)
    (text-properties-at 5 s)
    ;; Position 6-7: only color
    (text-properties-at 6 s)
    (text-properties-at 7 s)
    ;; Position 8-9: nothing
    (text-properties-at 8 s)
    ;; Property boundaries
    (next-property-change 0 s)
    (next-property-change 3 s)
    (next-property-change 6 s)))"#;
    let expect = expect_test::expect![[
        r#""OK ((face bold) (face bold) (color red face bold) (color red face bold) (color red) (color red) nil 3 6 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overwriting existing properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "hello world")))
  ;; Set face to bold on entire string
  (put-text-property 0 (length s) 'face 'bold s)
  ;; Verify initial state
  (let ((initial-face (get-text-property 0 'face s)))
    ;; Overwrite face to italic on positions 0-5
    (put-text-property 0 5 'face 'italic s)
    ;; Overwrite face to underline on positions 3-8
    (put-text-property 3 8 'face 'underline s)
    (list
      ;; 0-2: italic (overwritten from bold)
      (get-text-property 0 'face s)
      (get-text-property 2 'face s)
      ;; 3-4: underline (overwritten from italic)
      (get-text-property 3 'face s)
      (get-text-property 4 'face s)
      ;; 5-7: underline (overwritten from bold)
      (get-text-property 5 'face s)
      (get-text-property 7 'face s)
      ;; 8-10: bold (original, never overwritten)
      (get-text-property 8 'face s)
      (get-text-property 10 'face s)
      ;; Initial was bold
      initial-face)))"#;
    let expect = expect_test::expect![[
        r#""OK (italic italic underline underline underline underline bold bold bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with get-text-property and text-properties-at
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "test string")))
  ;; Set multiple different properties
  (put-text-property 0 4 'face 'bold s)
  (put-text-property 0 4 'help-echo "this is test" s)
  (put-text-property 0 4 'mouse-face 'highlight s)
  (put-text-property 5 11 'face 'italic s)
  (put-text-property 5 11 'invisible t s)
  (list
    ;; get-text-property returns the value for a specific property
    (get-text-property 0 'face s)
    (get-text-property 0 'help-echo s)
    (get-text-property 0 'mouse-face s)
    ;; text-properties-at returns all properties at position
    (text-properties-at 0 s)
    (text-properties-at 5 s)
    ;; get-text-property for missing property returns nil
    (get-text-property 0 'nonexistent s)
    ;; Setting property value to nil
    (progn
      (put-text-property 0 4 'help-echo nil s)
      (get-text-property 0 'help-echo s))
    ;; Verify face is still there after removing help-echo
    (get-text-property 0 'face s)
    ;; next-single-property-change for specific property
    (next-single-property-change 0 'face s)
    (next-single-property-change 0 'invisible s)))"#;
    let expect = expect_test::expect![[
        r#""OK (bold \"this is test\" highlight (mouse-face highlight help-echo nil face bold) (invisible t face italic) nil nil bold 4 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building syntax highlighting with put-text-property
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_syntax_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a simple syntax highlighter that colorizes keywords,
    // strings, and comments in a code snippet.
    let form = r#"(let ((code (copy-sequence "(defun hello (name) \"greeting\" ;; comment\n  (message name))"))
      (keywords '("defun" "message"))
      (result nil))
  ;; Highlight keywords by searching and applying face
  (dolist (kw keywords)
    (let ((pos 0))
      (while (let ((found (string-search kw code pos)))
               (when found
                 (put-text-property found (+ found (length kw)) 'face 'font-lock-keyword-face code)
                 (setq pos (+ found (length kw)))
                 t)))))
  ;; Highlight string literals (find matching quotes)
  (let ((pos 0))
    (while (let ((start (string-search "\"" code pos)))
             (when start
               (let ((end (string-search "\"" code (1+ start))))
                 (when end
                   (put-text-property start (1+ end) 'face 'font-lock-string-face code)
                   (setq pos (+ end 1))
                   t))))))
  ;; Highlight comments (;; to end of line)
  (let ((comment-start (string-search ";;" code)))
    (when comment-start
      (let ((eol (or (string-search "\n" code comment-start) (length code))))
        (put-text-property comment-start eol 'face 'font-lock-comment-face code))))
  ;; Collect face info at various positions
  (list
    ;; '(' has no face
    (get-text-property 0 'face code)
    ;; 'defun' has keyword face (position 1)
    (get-text-property 1 'face code)
    ;; 'hello' has no face
    (get-text-property 7 'face code)
    ;; string literal has string face
    (let ((str-start (string-search "\"" code)))
      (get-text-property str-start 'face code))
    ;; comment has comment face
    (let ((cmt-start (string-search ";;" code)))
      (get-text-property cmt-start 'face code))
    ;; 'message' keyword
    (let ((msg-pos (string-search "message" code)))
      (get-text-property msg-pos 'face code))
    ;; Count distinct property regions using next-property-change
    (let ((count 0) (pos 0))
      (while (let ((next (next-property-change pos code)))
               (when next
                 (setq count (1+ count))
                 (setq pos next)
                 t)))
      count)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil font-lock-keyword-face nil font-lock-string-face font-lock-comment-face font-lock-keyword-face 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: text annotation system using text properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_annotation_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a text annotation system: annotate regions of text with
    // metadata (author, timestamp, category), then query annotations.
    let form = r#"(let ((doc (copy-sequence "The quick brown fox jumps over the lazy dog")))
  ;; Annotate "quick brown" with author and category
  (let ((start (string-search "quick" doc))
        (end (+ (string-search "brown" doc) (length "brown"))))
    (put-text-property start end 'author "alice" doc)
    (put-text-property start end 'category 'adjective doc)
    (put-text-property start end 'timestamp 1000 doc))
  ;; Annotate "jumps over" with different author
  (let ((start (string-search "jumps" doc))
        (end (+ (string-search "over" doc) (length "over"))))
    (put-text-property start end 'author "bob" doc)
    (put-text-property start end 'category 'verb doc)
    (put-text-property start end 'timestamp 2000 doc))
  ;; Annotate "lazy" with yet another author
  (let ((start (string-search "lazy" doc))
        (end (+ start (length "lazy"))))
    (put-text-property start end 'author "charlie" doc)
    (put-text-property start end 'category 'adjective doc)
    (put-text-property start end 'timestamp 3000 doc))
  ;; Query the annotations
  (list
    ;; Get annotation at "quick"
    (let ((pos (string-search "quick" doc)))
      (list (get-text-property pos 'author doc)
            (get-text-property pos 'category doc)
            (get-text-property pos 'timestamp doc)))
    ;; Get annotation at "jumps"
    (let ((pos (string-search "jumps" doc)))
      (list (get-text-property pos 'author doc)
            (get-text-property pos 'category doc)))
    ;; Get annotation at "lazy"
    (let ((pos (string-search "lazy" doc)))
      (get-text-property pos 'author doc))
    ;; Unannotated region: "The" at position 0
    (get-text-property 0 'author doc)
    ;; Collect all annotated regions with their properties
    (let ((annotations nil)
          (pos 0)
          (len (length doc)))
      (while (< pos len)
        (let ((author (get-text-property pos 'author doc)))
          (if author
              (let ((next (next-single-property-change pos 'author doc len)))
                (push (list (substring-no-properties doc pos next)
                            author
                            (get-text-property pos 'category doc))
                      annotations)
                (setq pos next))
            (let ((next (next-single-property-change pos 'author doc len)))
              (setq pos (or next len))))))
      (nreverse annotations))
    ;; Overwrite annotation: change "quick brown" author
    (progn
      (let ((start (string-search "quick" doc))
            (end (+ (string-search "brown" doc) (length "brown"))))
        (put-text-property start end 'author "dave" doc))
      (get-text-property (string-search "quick" doc) 'author doc))
    ;; Count of distinct annotated regions
    (let ((count 0) (pos 0) (len (length doc)))
      (while (< pos len)
        (when (get-text-property pos 'author doc)
          (setq count (1+ count)))
        (setq pos (or (next-single-property-change pos 'author doc len) len)))
      count)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable start)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: zero-length range, adjacent properties, buffer context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_patterns_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero-length range: START = END, should have no effect
  (let ((s (copy-sequence "hello")))
    (put-text-property 2 2 'face 'bold s)
    (get-text-property 2 'face s))

  ;; Adjacent properties: two different faces side by side
  (let ((s (copy-sequence "aabbcc")))
    (put-text-property 0 2 'face 'bold s)
    (put-text-property 2 4 'face 'italic s)
    (put-text-property 4 6 'face 'underline s)
    (list
      (get-text-property 0 'face s)
      (get-text-property 2 'face s)
      (get-text-property 4 'face s)
      ;; Boundaries
      (next-property-change 0 s)
      (next-property-change 2 s)
      (next-property-change 4 s)))

  ;; Same property value on adjacent ranges merges
  (let ((s (copy-sequence "abcdef")))
    (put-text-property 0 3 'face 'bold s)
    (put-text-property 3 6 'face 'bold s)
    ;; Should be no boundary at position 3 since values are equal
    (next-property-change 0 s))

  ;; Property on single character
  (let ((s (copy-sequence "abcde")))
    (put-text-property 2 3 'face 'bold s)
    (list
      (get-text-property 1 'face s)
      (get-text-property 2 'face s)
      (get-text-property 3 'face s)))

  ;; put-text-property in buffer context
  (with-temp-buffer
    (insert "hello world")
    (put-text-property 1 6 'face 'bold)
    (list
      (get-text-property 1 'face)
      (get-text-property 6 'face)
      (buffer-substring 1 6))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (bold italic underline 2 4 nil) nil (nil bold nil) (bold nil #(\"hello\" 0 5 (face bold))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
