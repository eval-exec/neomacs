//! Oracle parity tests for comprehensive text property operations:
//! propertize, put-text-property, add-text-properties, get-text-property,
//! get-char-property, text-properties-at, text-property-not-all,
//! text-property-any, next/previous-property-change,
//! next/previous-single-property-change, remove-text-properties,
//! remove-list-of-text-properties, and complex multi-property scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// add-text-properties: adding multiple properties to ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_add_text_properties_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // add-text-properties adds without replacing existing properties,
    // unlike put-text-property which overwrites.
    let form = r#"(let ((s (copy-sequence "abcdefghij")))
  ;; Start with face on range 0-5
  (put-text-property 0 5 'face 'bold s)
  ;; add-text-properties: add help-echo and mouse-face on 2-8
  (add-text-properties 2 8 '(help-echo "tip" mouse-face highlight) s)
  ;; add-text-properties: add category on 0-10
  (add-text-properties 0 10 '(category my-cat) s)
  ;; Now verify the overlapping regions
  (list
   ;; Position 0: face=bold, category=my-cat, no help-echo
   (list (get-text-property 0 'face s)
         (get-text-property 0 'help-echo s)
         (get-text-property 0 'mouse-face s)
         (get-text-property 0 'category s))
   ;; Position 3: face=bold, help-echo="tip", mouse-face=highlight, category=my-cat
   (list (get-text-property 3 'face s)
         (get-text-property 3 'help-echo s)
         (get-text-property 3 'mouse-face s)
         (get-text-property 3 'category s))
   ;; Position 6: no face, help-echo="tip", mouse-face=highlight, category=my-cat
   (list (get-text-property 6 'face s)
         (get-text-property 6 'help-echo s)
         (get-text-property 6 'mouse-face s)
         (get-text-property 6 'category s))
   ;; Position 9: only category
   (list (get-text-property 9 'face s)
         (get-text-property 9 'help-echo s)
         (get-text-property 9 'mouse-face s)
         (get-text-property 9 'category s))
   ;; Return value of add-text-properties: t if any changed, nil if none
   (add-text-properties 0 1 '(category my-cat) s)    ;; already set => nil
   (add-text-properties 9 10 '(face italic) s)))"#;
    let expect = expect_test::expect![[
        r#""OK ((bold nil nil my-cat) (bold \"tip\" highlight my-cat) (nil \"tip\" highlight my-cat) (nil nil nil my-cat) nil t)""#
    ]];
    // new => t
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// text-property-not-all and text-property-any
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_not_all_and_any() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // text-property-not-all: returns first pos where property != value (or nil)
    // text-property-any: returns first pos where property is non-nil
    let form = r#"(let ((s (copy-sequence "0123456789")))
  ;; Set face=bold on 0-5, face=italic on 5-8, leave 8-10 without face
  (put-text-property 0 5 'face 'bold s)
  (put-text-property 5 8 'face 'italic s)
  ;; Set help-echo on 3-7
  (put-text-property 3 7 'help-echo "tip" s)
  (list
   ;; text-property-not-all: find first pos in range where face != bold
   (text-property-not-all 0 10 'face 'bold s)      ;; 5 (italic starts)
   (text-property-not-all 0 5 'face 'bold s)       ;; nil (all bold in 0..5)
   (text-property-not-all 0 10 'face 'italic s)    ;; 0 (first pos is bold, not italic)
   (text-property-not-all 5 8 'face 'italic s)     ;; nil (all italic in 5..8)
   (text-property-not-all 0 10 'face nil s)        ;; 0 (face is non-nil at 0)
   (text-property-not-all 8 10 'face nil s)        ;; nil (face is nil in 8..10)
   ;; text-property-any: find first pos in range where property is non-nil
   (text-property-any 0 10 'help-echo s)           ;; 3
   (text-property-any 0 3 'help-echo s)            ;; nil
   (text-property-any 5 10 'help-echo s)           ;; 5
   (text-property-any 7 10 'help-echo s)           ;; nil (help-echo ends at 7)
   (text-property-any 0 10 'face s)                ;; 0
   (text-property-any 8 10 'face s)))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 10)""#]];
    // nil
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// previous-property-change and previous-single-property-change
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_previous_property_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk backward through property boundaries
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold)
                          "BBB"
                          (propertize "CCC" 'face 'italic)
                          "DDD")))
  (list
   ;; previous-property-change: find previous boundary from end
   (previous-property-change 12 s)     ;; 9 (end of italic)
   (previous-property-change 9 s)      ;; 6 (end of plain)
   (previous-property-change 6 s)      ;; 3 (end of bold)
   (previous-property-change 3 s)      ;; 0 (start of string)
   (previous-property-change 0 s)      ;; nil (no more changes before 0)
   ;; With limit parameter
   (previous-property-change 12 s 10)  ;; 10 (limited to 10, boundary at 9 is before)
   (previous-property-change 12 s 9)   ;; 9 (limit = boundary)
   (previous-property-change 12 s 5)   ;; 9 (limit < boundary, returns boundary)
   ;; previous-single-property-change for specific property
   (previous-single-property-change 12 'face s)     ;; 9
   (previous-single-property-change 9 'face s)      ;; 6
   (previous-single-property-change 6 'face s)      ;; 3
   (previous-single-property-change 3 'face s)      ;; 0
   ;; Walk backward collecting all boundaries
   (let ((boundaries nil) (pos (length s)))
     (while pos
       (setq pos (previous-property-change pos s))
       (when pos (setq boundaries (cons pos boundaries))))
     boundaries)))"#;
    let expect = expect_test::expect![[r#""OK (9 6 3 nil nil 10 9 9 9 6 3 nil (3 6 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-single-property-change with all parameter variations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_next_single_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // next-single-property-change only looks at one specific property
    let form = r#"(let ((s (copy-sequence "0123456789ABCDEF")))
  ;; face changes at 3, 7, 12
  (put-text-property 0 3 'face 'bold s)
  (put-text-property 3 7 'face 'italic s)
  (put-text-property 7 12 'face 'underline s)
  ;; help-echo changes at 5, 10
  (put-text-property 5 10 'help-echo "tip" s)
  ;; mouse-face changes at 2, 14
  (put-text-property 2 14 'mouse-face 'highlight s)
  (list
   ;; Track face changes: 0->3->7->12->nil
   (next-single-property-change 0 'face s)       ;; 3
   (next-single-property-change 3 'face s)       ;; 7
   (next-single-property-change 7 'face s)       ;; 12
   (next-single-property-change 12 'face s)      ;; nil (no more)
   ;; Track help-echo changes: 0->5->10->nil
   (next-single-property-change 0 'help-echo s)  ;; 5
   (next-single-property-change 5 'help-echo s)  ;; 10
   (next-single-property-change 10 'help-echo s) ;; nil
   ;; Track mouse-face changes: 0->2->14->nil
   (next-single-property-change 0 'mouse-face s) ;; 2
   (next-single-property-change 2 'mouse-face s) ;; 14
   (next-single-property-change 14 'mouse-face s);; nil
   ;; With limit
   (next-single-property-change 0 'face s 2)     ;; 2 (limit < boundary)
   (next-single-property-change 0 'face s 3)     ;; 3 (limit = boundary)
   (next-single-property-change 0 'face s 5)     ;; 3 (limit > boundary)
   ;; Walk all face boundaries
   (let ((boundaries nil) (pos 0))
     (while pos
       (setq pos (next-single-property-change pos 'face s))
       (when pos (setq boundaries (cons pos boundaries))))
     (nreverse boundaries))))"#;
    let expect = expect_test::expect![[r#""OK (3 7 12 nil 5 10 nil 2 14 nil 2 3 3 (3 7 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// remove-list-of-text-properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_remove_list_of_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // remove-list-of-text-properties removes named properties (keys only, not plist)
    let form = r#"(let ((s (propertize "hello world" 'face 'bold 'help-echo "tip"
                                       'mouse-face 'highlight 'category 'my-cat
                                       'keymap 'some-map)))
  ;; Verify all 5 properties are set
  (let ((before (length (text-properties-at 0 s))))
    ;; Remove face and help-echo (property names, not plist pairs)
    (remove-list-of-text-properties 0 11 '(face help-echo) s)
    (let ((after-first
           (list (get-text-property 0 'face s)
                 (get-text-property 0 'help-echo s)
                 (get-text-property 0 'mouse-face s)
                 (get-text-property 0 'category s)
                 (get-text-property 0 'keymap s))))
      ;; Remove mouse-face and category from partial range
      (remove-list-of-text-properties 0 5 '(mouse-face category) s)
      (let ((after-partial
             (list
              ;; At pos 0: mouse-face and category removed
              (get-text-property 0 'mouse-face s)
              (get-text-property 0 'category s)
              (get-text-property 0 'keymap s)
              ;; At pos 6: mouse-face and category still present
              (get-text-property 6 'mouse-face s)
              (get-text-property 6 'category s)
              (get-text-property 6 'keymap s))))
        (list before after-first after-partial)))))"#;
    let expect = expect_test::expect![[
        r#""OK (10 (nil nil highlight my-cat some-map) (nil nil some-map highlight my-cat some-map))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property walking/iteration collecting property intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_property_interval_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk through a propertized string and collect intervals (start end property-value)
    let form = r#"(let ((s (copy-sequence "The quick brown fox jumps over the lazy dog")))
  ;; Apply various properties to simulate syntax highlighting
  (put-text-property 0 3 'face 'font-lock-keyword-face s)
  (put-text-property 4 9 'face 'font-lock-variable-name-face s)
  (put-text-property 10 15 'face 'font-lock-type-face s)
  (put-text-property 16 19 'face 'font-lock-string-face s)
  (put-text-property 20 25 'face 'font-lock-comment-face s)
  ;; Also add help-echo to some words
  (put-text-property 4 9 'help-echo "fast" s)
  (put-text-property 16 19 'help-echo "animal" s)

  ;; Collect all face intervals
  (let ((face-intervals nil)
        (pos 0)
        (len (length s)))
    (while (< pos len)
      (let ((face-val (get-text-property pos 'face s))
            (next (next-single-property-change pos 'face s)))
        (when face-val
          (setq face-intervals
                (cons (list pos (or next len) face-val) face-intervals)))
        (setq pos (or next len))))
    (setq face-intervals (nreverse face-intervals))

    ;; Collect help-echo intervals
    (let ((echo-intervals nil)
          (pos 0))
      (while (< pos len)
        (let ((echo-val (get-text-property pos 'help-echo s))
              (next (next-single-property-change pos 'help-echo s)))
          (when echo-val
            (setq echo-intervals
                  (cons (list pos (or next len) echo-val) echo-intervals)))
          (setq pos (or next len))))
      (setq echo-intervals (nreverse echo-intervals))

      ;; Count distinct face values
      (let ((faces nil))
        (dolist (interval face-intervals)
          (unless (memq (nth 2 interval) faces)
            (setq faces (cons (nth 2 interval) faces))))

        (list
         face-intervals
         echo-intervals
         (length face-intervals)
         (length echo-intervals)
         (length faces)
         ;; Verify no overlap between adjacent face intervals
         (let ((ok t) (prev-end nil))
           (dolist (interval face-intervals)
             (when (and prev-end (< (car interval) prev-end))
               (setq ok nil))
             (setq prev-end (nth 1 interval)))
           ok))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 3 font-lock-keyword-face) (4 9 font-lock-variable-name-face) (10 15 font-lock-type-face) (16 19 font-lock-string-face) (20 25 font-lock-comment-face)) ((4 9 \"fast\") (16 19 \"animal\")) 5 2 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: syntax highlighting simulation with fontification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_syntax_highlight_full_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a multi-pass syntax highlighter that:
    // 1) Highlights keywords
    // 2) Highlights strings
    // 3) Highlights numbers
    // Then verify the resulting property map
    let form = r#"(progn
  (fset 'neovm--tp-simple-search-all
    (lambda (str pattern)
      "Find all occurrences of PATTERN in STR. Returns list of (start . end)."
      (let ((result nil) (pos 0))
        (while (and pos (< pos (length str)))
          (let ((found (string-match pattern str pos)))
            (if found
                (progn
                  (setq result (cons (cons (match-beginning 0) (match-end 0)) result))
                  (setq pos (match-end 0)))
              (setq pos nil))))
        (nreverse result))))

  (fset 'neovm--tp-highlight
    (lambda (str)
      "Apply syntax highlighting to STR and return the highlighted string."
      (let ((s (copy-sequence str)))
        ;; Pass 1: Keywords (if, let, defun, while, progn)
        (dolist (kw '("if" "let" "defun" "while" "progn"))
          (dolist (match (funcall 'neovm--tp-simple-search-all s kw))
            (put-text-property (car match) (cdr match) 'face 'font-lock-keyword-face s)))
        ;; Pass 2: Numbers
        (dolist (match (funcall 'neovm--tp-simple-search-all s "[0-9]+"))
          (put-text-property (car match) (cdr match) 'face 'font-lock-constant-face s))
        ;; Pass 3: Mark fontified region
        (put-text-property 0 (length s) 'fontified t s)
        s)))

  (unwind-protect
      (let* ((code "(defun add (x y) (+ x 42))")
             (highlighted (funcall 'neovm--tp-highlight code)))
        (list
         ;; "defun" at positions 1-6 should be keyword face
         (get-text-property 1 'face highlighted)
         ;; "42" should be constant face
         (get-text-property 22 'face highlighted)
         ;; Entire string is fontified
         (get-text-property 0 'fontified highlighted)
         (get-text-property 15 'fontified highlighted)
         ;; text-property-any for 'face in whole string
         (text-property-any 0 (length highlighted) 'face highlighted)
         ;; Collect all face regions
         (let ((regions nil) (pos 0) (len (length highlighted)))
           (while (< pos len)
             (let ((face (get-text-property pos 'face highlighted))
                   (next (next-single-property-change pos 'face highlighted)))
               (when face
                 (setq regions (cons (list pos (or next len) face) regions)))
               (setq pos (or next len))))
           (nreverse regions))
         ;; text-property-not-all for fontified=t should be nil (all t)
         (text-property-not-all 0 (length highlighted) 'fontified t highlighted)))
    (fmakunbound 'neovm--tp-simple-search-all)
    (fmakunbound 'neovm--tp-highlight)))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property-based rich text builder with merge semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_rich_text_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a rich text system where properties accumulate rather than
    // overwrite, simulating CSS-like cascading
    let form = r#"(progn
  (fset 'neovm--tp-concat-rich
    (lambda (&rest parts)
      "Concatenate propertized strings preserving all properties."
      (let ((result ""))
        (dolist (part parts)
          (setq result (concat result part)))
        result)))

  (fset 'neovm--tp-span
    (lambda (text &rest props)
      "Create a propertized text span."
      (let ((s (copy-sequence text)))
        (while props
          (put-text-property 0 (length s) (car props) (cadr props) s)
          (setq props (cddr props)))
        s)))

  (fset 'neovm--tp-get-all-props-at
    (lambda (pos str)
      "Get a sorted list of property names at POS."
      (let ((plist (text-properties-at pos str))
            (names nil))
        (while plist
          (setq names (cons (car plist) names))
          (setq plist (cddr plist)))
        (sort names (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((doc (funcall 'neovm--tp-concat-rich
                          (funcall 'neovm--tp-span "Title" 'face 'bold 'level 1)
                          (funcall 'neovm--tp-span ": " 'face 'default)
                          (funcall 'neovm--tp-span "important" 'face 'italic 'priority 'high)
                          (funcall 'neovm--tp-span " text " 'face 'default)
                          (funcall 'neovm--tp-span "here" 'face 'underline 'link "http://example.com"))))
        (list
         ;; "Title" at 0: face=bold, level=1
         (get-text-property 0 'face doc)
         (get-text-property 0 'level doc)
         ;; ": " at 5: face=default
         (get-text-property 5 'face doc)
         (get-text-property 5 'level doc)
         ;; "important" at 7: face=italic, priority=high
         (get-text-property 7 'face doc)
         (get-text-property 7 'priority doc)
         ;; "here" at end: face=underline, link present
         (get-text-property (- (length doc) 2) 'face doc)
         (get-text-property (- (length doc) 2) 'link doc)
         ;; Property names at each region
         (funcall 'neovm--tp-get-all-props-at 0 doc)
         (funcall 'neovm--tp-get-all-props-at 7 doc)
         ;; Total property changes count
         (let ((count 0) (pos 0))
           (while pos
             (setq pos (next-property-change pos doc))
             (when pos (setq count (1+ count))))
           count)
         ;; Walk from end: previous-property-change
         (let ((pos (length doc))
               (boundaries nil))
           (while pos
             (setq pos (previous-property-change pos doc))
             (when pos (setq boundaries (cons pos boundaries))))
           boundaries)))
    (fmakunbound 'neovm--tp-concat-rich)
    (fmakunbound 'neovm--tp-span)
    (fmakunbound 'neovm--tp-get-all-props-at)))"#;
    let expect = expect_test::expect![[
        r#""OK (bold 1 default nil italic high underline \"http://example.com\" (face level) (face priority) 4 (5 7 16 22))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// get-char-property in buffer context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_get_char_property_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // get-char-property works on strings (same as get-text-property for strings)
    // Test with string argument to verify parity
    let form = r#"(let ((s (concat (propertize "abc" 'face 'bold 'custom 1)
                          (propertize "def" 'face 'italic 'custom 2)
                          "ghi")))
  (list
   ;; get-char-property on string (same as get-text-property for strings)
   (get-char-property 0 'face s)
   (get-char-property 0 'custom s)
   (get-char-property 3 'face s)
   (get-char-property 3 'custom s)
   (get-char-property 6 'face s)
   (get-char-property 6 'custom s)
   ;; Boundary positions
   (get-char-property 2 'face s)      ;; bold (last char of first region)
   (get-char-property 5 'face s)      ;; italic (last char of second region)
   ;; Compare with get-text-property (should be identical on strings)
   (equal (get-char-property 0 'face s) (get-text-property 0 'face s))
   (equal (get-char-property 3 'custom s) (get-text-property 3 'custom s))
   (equal (get-char-property 6 'face s) (get-text-property 6 'face s))))"#;
    let expect = expect_test::expect![[r#""OK (bold 1 italic 2 nil nil bold italic t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: remove-text-properties with partial overlapping regions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_props_remove_with_overlapping_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test complex removal scenarios with overlapping property regions
    let form = r#"(let ((s (copy-sequence "0123456789ABCDEF")))
  ;; Layer multiple properties with overlapping ranges
  (put-text-property 0 10 'face 'bold s)
  (put-text-property 3 13 'help-echo "tip" s)
  (put-text-property 6 16 'mouse-face 'highlight s)
  ;; Remove face from 2-8 (partial overlap with 0-10 face range)
  (remove-text-properties 2 8 '(face nil) s)
  ;; Remove help-echo from 5-11 (partial overlap with 3-13 help-echo range)
  (remove-text-properties 5 11 '(help-echo nil) s)
  (list
   ;; face: should be bold at 0-2, nil at 2-8, bold at 8-10, nil at 10+
   (get-text-property 0 'face s)    ;; bold
   (get-text-property 1 'face s)    ;; bold
   (get-text-property 2 'face s)    ;; nil (removed)
   (get-text-property 7 'face s)    ;; nil (removed)
   (get-text-property 8 'face s)    ;; bold (not removed)
   (get-text-property 9 'face s)    ;; bold
   (get-text-property 10 'face s)   ;; nil (was never set past 10)
   ;; help-echo: should be "tip" at 3-5, nil at 5-11, "tip" at 11-13
   (get-text-property 3 'help-echo s)   ;; "tip"
   (get-text-property 4 'help-echo s)   ;; "tip"
   (get-text-property 5 'help-echo s)   ;; nil (removed)
   (get-text-property 10 'help-echo s)  ;; nil (removed)
   (get-text-property 11 'help-echo s)  ;; "tip" (not removed)
   (get-text-property 12 'help-echo s)  ;; "tip"
   ;; mouse-face: untouched, highlight at 6-16
   (get-text-property 5 'mouse-face s)  ;; nil
   (get-text-property 6 'mouse-face s)  ;; highlight
   (get-text-property 15 'mouse-face s) ;; highlight
   ;; Count total property boundaries
   (let ((count 0) (pos 0))
     (while pos
       (setq pos (next-property-change pos s))
       (when pos (setq count (1+ count))))
     count)))"#;
    let expect = expect_test::expect![[
        r#""OK (bold bold nil nil bold bold nil \"tip\" \"tip\" nil nil \"tip\" \"tip\" nil highlight highlight 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
