//! Advanced oracle parity tests for text property manipulation:
//! `put-text-property`, `add-text-properties`, `remove-text-properties`,
//! `set-text-properties`, `text-properties-at`, `text-property-not-all`,
//! `next-single-property-change`, `previous-single-property-change`,
//! and property interval manipulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_tpm_insert_before_markers_copies_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (let ((s (make-string 3 ?x)))
    (put-text-property 0 1 'face 'foo s)
    (put-text-property 1 2 'help-echo "h" s)
    (insert-before-markers s)
    (buffer-string)))"#;
    let expect = expect_test::expect![[r#""OK #(\"xxx\" 0 1 (face foo) 1 2 (help-echo \"h\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// add-text-properties: additive merging across overlapping intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_add_text_properties_overlapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (copy-sequence "0123456789ABCDEF")))
  ;; Layer 1: face on 0..8
  (add-text-properties 0 8 '(face bold) s)
  ;; Layer 2: help-echo on 4..12 (overlaps 4..8 with layer 1)
  (add-text-properties 4 12 '(help-echo "tip") s)
  ;; Layer 3: mouse-face on 2..10 (overlaps both)
  (add-text-properties 2 10 '(mouse-face highlight) s)
  ;; Layer 4: add a second property to 0..4 — should NOT remove face
  (add-text-properties 0 4 '(category test-cat) s)
  ;; Survey all positions
  (let ((result nil))
    (dotimes (i 16)
      (setq result
            (cons (list i
                        (get-text-property i 'face s)
                        (get-text-property i 'help-echo s)
                        (get-text-property i 'mouse-face s)
                        (get-text-property i 'category s))
                  result)))
    (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 bold nil nil test-cat) (1 bold nil nil test-cat) (2 bold nil highlight test-cat) (3 bold nil highlight test-cat) (4 bold \"tip\" highlight nil) (5 bold \"tip\" highlight nil) (6 bold \"tip\" highlight nil) (7 bold \"tip\" highlight nil) (8 nil \"tip\" highlight nil) (9 nil \"tip\" highlight nil) (10 nil \"tip\" nil nil) (11 nil \"tip\" nil nil) (12 nil nil nil nil) (13 nil nil nil nil) (14 nil nil nil nil) (15 nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-text-properties: wholesale replacement with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_set_text_properties_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (copy-sequence (propertize "abcdefghij" 'face 'bold 'help-echo "old" 'priority 10))))
  ;; set-text-properties with nil clears ALL properties in region
  (set-text-properties 2 5 nil s)
  (let ((after-clear
         (list (text-properties-at 1 s)   ;; still has properties
               (text-properties-at 2 s)   ;; cleared
               (text-properties-at 4 s)   ;; cleared
               (text-properties-at 5 s))));; still has properties
    ;; set-text-properties with new plist replaces completely
    (set-text-properties 0 3 '(new-prop 42 another yes) s)
    (let ((after-replace
           (list (get-text-property 0 'face s)      ;; nil (replaced)
                 (get-text-property 0 'new-prop s)  ;; 42
                 (get-text-property 0 'another s)   ;; yes
                 (get-text-property 2 'new-prop s)  ;; 42
                 (get-text-property 3 'new-prop s))));; nil (out of range)
      ;; set-text-properties with empty range (start=end) should be no-op
      (set-text-properties 5 5 '(dummy 1) s)
      (let ((no-op (get-text-property 5 'dummy s)))
        (list after-clear after-replace no-op)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((face bold help-echo \"old\" priority 10) nil nil (face bold help-echo \"old\" priority 10)) (nil 42 yes 42 nil) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// text-property-not-all: find where property changes from given value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_text_property_not_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (copy-sequence "abcdefghij")))
  ;; Set face to bold for 0..5, italic for 5..8, nil for 8..10
  (put-text-property 0 5 'face 'bold s)
  (put-text-property 5 8 'face 'italic s)
  ;; text-property-not-all: find position in range where prop != val
  (list
    ;; All bold in 0..5? Should return nil (all are bold)
    (text-property-not-all 0 5 'face 'bold s)
    ;; All bold in 0..6? Should return 5 (italic starts there)
    (text-property-not-all 0 6 'face 'bold s)
    ;; All bold in 0..10? Should return 5
    (text-property-not-all 0 10 'face 'bold s)
    ;; All italic in 5..8? Should return nil
    (text-property-not-all 5 8 'face 'italic s)
    ;; All italic in 5..10? Should return 8 (nil starts there)
    (text-property-not-all 5 10 'face 'italic s)
    ;; Check for a property that doesn't exist anywhere
    (text-property-not-all 0 10 'help-echo "tip" s)
    ;; All nil face in 8..10? Should return nil (face is nil in 8..10)
    (text-property-not-all 8 10 'face nil s)))"#;
    let expect = expect_test::expect![[r#""OK (nil 5 5 nil 8 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-single-property-change and previous-single-property-change
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_single_property_change_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (copy-sequence "abcdefghijklmnop")))
  ;; Create multiple property intervals
  (put-text-property 0 4 'face 'bold s)
  (put-text-property 4 4 'face 'italic s)  ;; zero-width, no effect
  (put-text-property 4 8 'face 'italic s)
  (put-text-property 8 12 'face 'bold s)
  ;; Positions 12..16 have no face
  (put-text-property 2 10 'help-echo "tip" s)
  ;; Walk forward with next-single-property-change for 'face
  (let ((face-changes nil)
        (pos 0))
    (while pos
      (setq pos (next-single-property-change pos 'face s))
      (when pos
        (setq face-changes (cons (cons pos (get-text-property pos 'face s))
                                 face-changes))))
    ;; Walk backward with previous-single-property-change for 'face
    (let ((face-changes-back nil)
          (pos (length s)))
      (while pos
        (setq pos (previous-single-property-change pos 'face s))
        (when pos
          (setq face-changes-back
                (cons (cons pos (get-text-property pos 'face s))
                      face-changes-back))))
      ;; Walk forward for 'help-echo
      (let ((echo-changes nil)
            (pos 0))
        (while pos
          (setq pos (next-single-property-change pos 'help-echo s))
          (when pos
            (setq echo-changes (cons (cons pos (get-text-property pos 'help-echo s))
                                     echo-changes))))
        (list (nreverse face-changes)
              face-changes-back
              (nreverse echo-changes))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((4 . italic) (8 . bold) (12)) ((4 . italic) (8 . bold) (12)) ((2 . \"tip\") (10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// remove-text-properties with return value (changed-p) and partial overlap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_remove_return_value_and_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (copy-sequence (propertize "abcdefghij" 'face 'bold 'help-echo "tip"))))
  ;; remove-text-properties returns t if any property was actually removed
  (let ((r1 (remove-text-properties 0 5 '(face nil) s))
        ;; Try removing face again from same region — already gone, should return nil
        (r2 (remove-text-properties 0 5 '(face nil) s))
        ;; Remove help-echo from overlapping region
        (r3 (remove-text-properties 3 8 '(help-echo nil) s))
        ;; Remove nonexistent property — should return nil
        (r4 (remove-text-properties 0 10 '(nonexistent nil) s)))
    ;; Survey remaining properties
    (let ((survey nil))
      (dotimes (i 10)
        (setq survey
              (cons (list i
                          (get-text-property i 'face s)
                          (get-text-property i 'help-echo s))
                    survey)))
      (list r1 r2 r3 r4 (nreverse survey)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil t nil ((0 nil \"tip\") (1 nil \"tip\") (2 nil \"tip\") (3 nil nil) (4 nil nil) (5 bold nil) (6 bold nil) (7 bold nil) (8 bold \"tip\") (9 bold \"tip\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_tpm_malformed_plist_validation_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU textprop.c validates add/set plists before mutation, but
    // remove-text-properties first looks for candidate intervals and can
    // return nil without validating an odd plist when no property exists.
    let form = r#"(let ((s (copy-sequence "abc")))
  (list
   (condition-case err
       (add-text-properties 0 1 '(face) s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (remove-text-properties 0 1 '(face) s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (set-text-properties 0 1 '(face) s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (add-text-properties 0 1 '(face . bold) s)
     (error (list (car err) (cdr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error (\"Odd length text property list\")) nil (error (\"Odd length text property list\")) (error (\"Odd length text property list\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_tpm_string_range_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU textprop.c validate_interval_range keeps both original BEGIN and
    // END values for args-out-of-range, even for point queries where the same
    // Lisp_Object is passed as both BEGIN and END.
    let form = r#"(let ((s (copy-sequence "abc")))
  (list
   (condition-case err
       (put-text-property -1 1 'face 'bold s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (put-text-property 2 1 'face 'bold s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (put-text-property 0 4 'face 'bold s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (text-properties-at -1 s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (text-properties-at 3 s)
     (error (list (car err) (cdr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((args-out-of-range (-1 1)) nil (args-out-of-range (0 4)) (args-out-of-range (-1 -1)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_tpm_buffer_range_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU textprop.c uses the same validate_interval_range path for buffers,
    // with buffer positions being 1-based and the end position accepted.
    let form = r#"(with-temp-buffer
  (insert "abc")
  (list
   (point-min)
   (point-max)
   (condition-case err
       (text-properties-at 0)
     (error (list (car err) (cdr err))))
   (condition-case err
       (text-properties-at 4)
     (error (list (car err) (cdr err))))
   (condition-case err
       (put-text-property 0 1 'face 'bold)
     (error (list (car err) (cdr err))))
   (condition-case err
       (put-text-property 1 5 'face 'bold)
     (error (list (car err) (cdr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (1 4 (args-out-of-range (0 0)) nil (args-out-of-range (0 1)) (args-out-of-range (1 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer text properties: put/add/remove/set in a buffer with positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_buffer_text_property_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog")
  ;; Buffer positions are 1-based
  ;; put-text-property on "quick" (5..10)
  (put-text-property 5 10 'face 'bold)
  ;; add-text-properties on "brown fox" (11..20) with multiple props
  (add-text-properties 11 20 '(face italic help-echo "animal"))
  ;; set-text-properties on "jumps" (21..26) — wholesale replace
  (set-text-properties 21 26 '(category verb priority 5))
  ;; Verify buffer text properties
  (let ((r1 (get-text-property 5 'face))        ;; bold
        (r2 (get-text-property 11 'face))       ;; italic
        (r3 (get-text-property 11 'help-echo))  ;; "animal"
        (r4 (get-text-property 21 'category))   ;; verb
        (r5 (get-text-property 21 'priority))   ;; 5
        ;; No properties outside annotated regions
        (r6 (text-properties-at 1))             ;; nil
        (r7 (text-properties-at 30)))           ;; nil
    ;; remove-text-properties in buffer
    (remove-text-properties 11 16 '(face nil))
    (let ((r8 (get-text-property 11 'face))     ;; nil (removed)
          (r9 (get-text-property 11 'help-echo));; "animal" (kept)
          (r10 (get-text-property 16 'face)))   ;; italic (still there)
      ;; next-single-property-change in buffer
      (let ((next-face (next-single-property-change 1 'face))
            (next-from-bold (next-single-property-change 5 'face)))
        (list r1 r2 r3 r4 r5 r6 r7 r8 r9 r10
              next-face next-from-bold)))))"#;
    let expect = expect_test::expect![[
        r#""OK (bold italic \"animal\" verb 5 nil nil nil \"animal\" italic 5 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property interval merging — annotate, merge adjacent intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_interval_merge_annotator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-collect-intervals
    (lambda (s prop)
      "Collect a list of (start end value) for contiguous intervals of PROP in string S."
      (let ((intervals nil)
            (pos 0)
            (len (length s)))
        (while (< pos len)
          (let ((val (get-text-property pos prop s))
                (start pos))
            ;; Find end of this interval
            (let ((end (or (next-single-property-change pos prop s) len)))
              (when val
                (setq intervals (cons (list start end val) intervals)))
              (setq pos end))))
        (nreverse intervals))))

  (unwind-protect
      (let ((s (copy-sequence "aabbccddee")))
        ;; Set face to bold for several disjoint+adjacent regions
        (put-text-property 0 2 'face 'bold s)   ;; aa
        (put-text-property 2 4 'face 'bold s)   ;; bb (adjacent to aa)
        (put-text-property 4 6 'face 'italic s) ;; cc
        (put-text-property 6 8 'face 'bold s)   ;; dd
        (put-text-property 8 10 'face 'bold s)  ;; ee (adjacent to dd)
        ;; Collect face intervals
        (let ((face-intervals (funcall 'neovm--test-collect-intervals s 'face)))
          ;; Now add help-echo overlapping multiple face regions
          (add-text-properties 1 7 '(help-echo "hover") s)
          (let ((echo-intervals (funcall 'neovm--test-collect-intervals s 'help-echo)))
            ;; Count total property boundaries
            (let ((boundaries nil)
                  (pos 0))
              (while pos
                (setq pos (next-property-change pos s))
                (when pos
                  (setq boundaries (cons pos boundaries))))
              (list face-intervals
                    echo-intervals
                    (nreverse boundaries)
                    ;; Verify previous-single-property-change from end
                    (previous-single-property-change 10 'face s)
                    (previous-single-property-change 6 'face s))))))
    (fmakunbound 'neovm--test-collect-intervals)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 4 bold) (4 6 italic) (6 10 bold)) ((1 7 \"hover\")) (1 4 6 7) 6 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property-based token stream with interval splitting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tpm_token_stream_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-tokenize-annotate
    (lambda (code)
      "Tokenize simple code and annotate with text properties."
      (let ((s (copy-sequence code))
            (keywords '("if" "else" "while" "return" "let")))
        (with-temp-buffer
          (insert code)
          (goto-char (point-min))
          ;; Annotate numbers
          (while (re-search-forward "\\b[0-9]+\\b" nil t)
            (put-text-property (1- (match-beginning 0)) (1- (match-end 0))
                               'token-type 'number s))
          ;; Annotate identifiers/keywords
          (goto-char (point-min))
          (while (re-search-forward "\\b[a-z]+\\b" nil t)
            (let* ((word (match-string 0))
                   (start (1- (match-beginning 0)))
                   (end (1- (match-end 0)))
                   (type (if (member word keywords) 'keyword 'identifier)))
              ;; Only annotate if not already annotated as number
              (unless (get-text-property start 'token-type s)
                (put-text-property start end 'token-type type s))))
          ;; Annotate operators
          (goto-char (point-min))
          (while (re-search-forward "[+*/=<>-]+" nil t)
            (let ((start (1- (match-beginning 0)))
                  (end (1- (match-end 0))))
              (unless (get-text-property start 'token-type s)
                (put-text-property start end 'token-type 'operator s)))))
        s)))

  (unwind-protect
      (let ((annotated (funcall 'neovm--test-tokenize-annotate "let x = 42 + y")))
        ;; Collect all token intervals
        (let ((tokens nil)
              (pos 0)
              (len (length annotated)))
          (while (< pos len)
            (let ((type (get-text-property pos 'token-type annotated)))
              (if type
                  (let ((end (or (next-single-property-change pos 'token-type annotated) len)))
                    (setq tokens (cons (list (substring annotated pos end) type) tokens))
                    (setq pos end))
                (setq pos (or (next-single-property-change pos 'token-type annotated) len)))))
          (list (nreverse tokens)
                (length tokens)
                ;; Verify specific positions
                (get-text-property 0 'token-type annotated)   ;; keyword (let)
                (get-text-property 4 'token-type annotated)   ;; identifier (x)
                (get-text-property 8 'token-type annotated)   ;; number (42)
                )))
    (fmakunbound 'neovm--test-tokenize-annotate)))"#;
    let expect = expect_test::expect![[
        r#""OK (((#(\"let\" 0 3 (token-type keyword)) keyword) (#(\"x\" 0 1 (token-type identifier)) identifier) (#(\"=\" 0 1 (token-type operator)) operator) (#(\"42\" 0 2 (token-type number)) number) (#(\"+\" 0 1 (token-type operator)) operator) (#(\"y\" 0 1 (token-type identifier)) identifier)) 1 keyword identifier number)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
