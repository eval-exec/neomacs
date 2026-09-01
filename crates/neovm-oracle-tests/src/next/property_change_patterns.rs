//! Oracle parity tests for `next-property-change` with complex patterns:
//! POSITION argument, OBJECT argument (string or buffer), LIMIT argument,
//! return value semantics, single/multiple properties, iterating through
//! all property changes, and building a property span map.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// next-property-change POSITION argument variations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_position_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test next-property-change with various POSITION values:
    // start of string, middle of a property run, at a boundary,
    // past the last boundary, at the end of string.
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold)
                         (propertize "BBB" 'face 'italic)
                         "CCC"
                         (propertize "DDD" 'help-echo "tip"))))
  (list
    ;; From position 0 (start of first run)
    (next-property-change 0 s)
    ;; From position 1 (middle of first run)
    (next-property-change 1 s)
    ;; From position 2 (last char of first run)
    (next-property-change 2 s)
    ;; From position 3 (exactly at boundary between AAA and BBB)
    (next-property-change 3 s)
    ;; From position 5 (last char of second run)
    (next-property-change 5 s)
    ;; From position 6 (boundary between BBB and CCC)
    (next-property-change 6 s)
    ;; From position 8 (last char of CCC)
    (next-property-change 8 s)
    ;; From position 9 (boundary between CCC and DDD)
    (next-property-change 9 s)
    ;; From position 11 (last char of DDD, at end)
    (next-property-change 11 s)
    ;; From position 12 (at string length, past end)
    (next-property-change 12 s)))"#;
    let expect = expect_test::expect![[r#""OK (3 3 3 6 6 9 9 nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change OBJECT argument (string vs buffer)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_object_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test next-property-change with string objects and buffer objects.
    // Strings use 0-based positions, buffers use 1-based positions.
    let form = r#"(let ((results nil))
  ;; String object (0-based)
  (let ((s (concat (propertize "AB" 'x 1) (propertize "CD" 'x 2) "EF")))
    (let ((boundaries nil)
          (pos 0))
      (while pos
        (setq pos (next-property-change pos s))
        (when pos (push pos boundaries)))
      (push (list :string-boundaries (nreverse boundaries)) results)))

  ;; Buffer object (1-based) -- default when OBJECT is nil
  (with-temp-buffer
    (insert (propertize "AB" 'x 1))
    (insert (propertize "CD" 'x 2))
    (insert "EF")
    (let ((boundaries nil)
          (pos 1))
      (while pos
        (setq pos (next-property-change pos))
        (when (and pos (<= pos (point-max)))
          (push pos boundaries)))
      (push (list :buffer-boundaries (nreverse boundaries)) results)))

  ;; Explicit buffer object
  (with-temp-buffer
    (insert (propertize "Hello" 'face 'bold))
    (insert " ")
    (insert (propertize "World" 'face 'italic))
    (let ((buf (current-buffer))
          (boundaries nil)
          (pos 1))
      (while pos
        (setq pos (next-property-change pos buf))
        (when (and pos (<= pos (point-max)))
          (push pos boundaries)))
      (push (list :explicit-buffer (nreverse boundaries)) results)))

  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((:string-boundaries (2 4)) (:buffer-boundaries (3 5)) (:explicit-buffer (6 7)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change LIMIT argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_limit_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive testing of the LIMIT parameter:
    // - LIMIT before next boundary
    // - LIMIT exactly at next boundary
    // - LIMIT after next boundary
    // - LIMIT beyond string end
    // - LIMIT of nil (no limit)
    // - LIMIT at current position
    let form = r#"(let ((s (concat (propertize "AAAA" 'face 'bold)
                         (propertize "BBBB" 'face 'italic)
                         "CCCC")))
  ;; Boundary at 4 and 8
  (list
    ;; LIMIT before first boundary (4): returns LIMIT
    (next-property-change 0 s 2)
    ;; LIMIT exactly at first boundary: returns 4
    (next-property-change 0 s 4)
    ;; LIMIT after first boundary: returns 4 (boundary found first)
    (next-property-change 0 s 6)
    ;; LIMIT at second boundary from pos 0: returns 4 (first boundary)
    (next-property-change 0 s 8)
    ;; LIMIT beyond string end from pos 0: returns 4
    (next-property-change 0 s 100)
    ;; From pos 4 (at second run), LIMIT before boundary at 8
    (next-property-change 4 s 6)
    ;; From pos 4, LIMIT exactly at boundary
    (next-property-change 4 s 8)
    ;; From pos 4, LIMIT past boundary
    (next-property-change 4 s 10)
    ;; From pos 8 (at third run, no more boundaries), various limits
    (next-property-change 8 s 10)
    (next-property-change 8 s 12)
    (next-property-change 8 s 100)
    ;; LIMIT at current position (should be nil or limit)
    (next-property-change 0 s 0)
    (next-property-change 4 s 4)
    ;; No LIMIT (nil)
    (next-property-change 0 s nil)
    (next-property-change 4 s nil)
    (next-property-change 8 s nil)))"#;
    let expect = expect_test::expect![[r#""OK (2 4 4 4 4 6 8 8 10 12 100 0 4 4 8 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change return value semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify return values: position of next change, or LIMIT, or nil.
    let form = r#"(let ((results nil))
  ;; Uniform property: returns nil (no change)
  (let ((s (propertize "uniform" 'face 'bold)))
    (push (list :uniform-nil (next-property-change 0 s)) results))

  ;; No properties at all: returns nil
  (push (list :plain-nil (next-property-change 0 "plain")) results)

  ;; Property change exists: returns position
  (let ((s (concat (propertize "AB" 'x 1) "CD")))
    (push (list :returns-pos (next-property-change 0 s)) results))

  ;; With LIMIT before change: returns LIMIT
  (let ((s (concat (propertize "ABCDEF" 'x 1) "GHI")))
    (push (list :returns-limit (next-property-change 0 s 3)) results))

  ;; Past all boundaries: returns nil
  (let ((s (concat (propertize "AB" 'x 1) "CD")))
    (push (list :past-all-nil (next-property-change 2 s)) results))

  ;; Empty string
  (push (list :empty-nil (next-property-change 0 "")) results)

  ;; LIMIT past end with no more changes: returns nil
  (let ((s (propertize "hello" 'face 'bold)))
    (push (list :limit-past-end (next-property-change 0 s 100)) results))

  ;; Multiple properties change at same position
  (let ((s (copy-sequence "abcdef")))
    (put-text-property 0 3 'face 'bold s)
    (put-text-property 0 3 'help-echo "tip" s)
    ;; Both properties end at 3
    (push (list :multi-prop-change (next-property-change 0 s)) results))

  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((:uniform-nil nil) (:plain-nil nil) (:returns-pos 2) (:returns-limit 3) (:past-all-nil nil) (:empty-nil nil) (:limit-past-end 100) (:multi-prop-change 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change with single property
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_single_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use next-single-property-change to track a specific property,
    // and compare with next-property-change (which tracks any property).
    let form = r#"(let ((s (copy-sequence "abcdefghijkl")))
  ;; Set up overlapping property regions
  (put-text-property 0 4 'face 'bold s)
  (put-text-property 2 8 'help-echo "info" s)
  (put-text-property 6 10 'face 'italic s)
  ;; Walk face-only boundaries
  (let ((face-bounds nil)
        (pos 0))
    (while pos
      (setq pos (next-single-property-change pos 'face s))
      (when pos (push (cons pos (get-text-property pos 'face s)) face-bounds)))
    ;; Walk help-echo-only boundaries
    (let ((help-bounds nil)
          (pos2 0))
      (while pos2
        (setq pos2 (next-single-property-change pos2 'help-echo s))
        (when pos2 (push (cons pos2 (get-text-property pos2 'help-echo s)) help-bounds)))
      ;; Walk all-property boundaries
      (let ((all-bounds nil)
            (pos3 0))
        (while pos3
          (setq pos3 (next-property-change pos3 s))
          (when pos3 (push pos3 all-bounds)))
        (list :face-boundaries (nreverse face-bounds)
              :help-boundaries (nreverse help-bounds)
              :all-boundaries (nreverse all-bounds)
              ;; All-boundaries should be superset of face and help boundaries
              :face-subset (let ((all-sorted (sort (copy-sequence (nreverse all-bounds)) #'<)))
                             (let ((ok t))
                               (dolist (fb (mapcar #'car (nreverse face-bounds)) ok)
                                 (unless (memq fb all-sorted) (setq ok nil))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (:face-boundaries ((4) (6 . italic) (10)) :help-boundaries ((2 . \"info\") (8)) :all-boundaries (2 4 6 8 10) :face-subset t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change with multiple properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_multiple_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string with many layered properties and walk all boundaries.
    let form = r#"(let ((s (copy-sequence "0123456789abcdef")))
  ;; Layer 1: face on 0-4 and 8-12
  (put-text-property 0 4 'face 'bold s)
  (put-text-property 8 12 'face 'italic s)
  ;; Layer 2: help-echo on 2-6 and 10-14
  (put-text-property 2 6 'help-echo "h1" s)
  (put-text-property 10 14 'help-echo "h2" s)
  ;; Layer 3: display on 4-8
  (put-text-property 4 8 'invisible t s)
  ;; Collect all boundary positions and the properties at each
  (let ((spans nil)
        (pos 0)
        (len (length s)))
    (while (< pos len)
      (let* ((props (text-properties-at pos s))
             (next (or (next-property-change pos s) len)))
        (push (list pos next
                    (plist-get props 'face)
                    (plist-get props 'help-echo)
                    (plist-get props 'invisible))
              spans)
        (setq pos next)))
    (list :span-count (length spans)
          :spans (nreverse spans)
          ;; Verify spans cover the whole string
          :coverage-ok (let ((ok t)
                             (expected 0))
                         (dolist (sp (nreverse spans) ok)
                           (unless (= (car sp) expected) (setq ok nil))
                           (setq expected (cadr sp)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (:span-count 8 :spans ((0 2 bold nil nil) (2 4 bold \"h1\" nil) (4 6 nil \"h1\" t) (6 8 nil nil t) (8 10 italic nil nil) (10 12 italic \"h2\" nil) (12 14 nil \"h2\" nil) (14 16 nil nil nil)) :coverage-ok nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: iterating through all property changes to build index
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_iterate_all_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a rich propertized string and collect every boundary,
    // then verify we can reconstruct the original properties from the spans.
    let form = r#"(progn
  (fset 'neovm--test-collect-all-spans
    (lambda (s)
      "Collect all property spans as (start end . plist) from string S."
      (let ((spans nil)
            (pos 0)
            (len (length s)))
        (while (< pos len)
          (let* ((props (text-properties-at pos s))
                 (next (or (next-property-change pos s) len)))
            (push (cons pos (cons next props)) spans)
            (setq pos next)))
        (nreverse spans))))

  (fset 'neovm--test-verify-spans
    (lambda (s spans)
      "Verify that SPANS correctly describe all properties of S."
      (let ((ok t))
        (dolist (span spans ok)
          (let ((start (car span))
                (end (cadr span))
                (props (cddr span)))
            ;; Check every position in this span has matching properties
            (let ((i start))
              (while (and ok (< i end))
                (let ((actual (text-properties-at i s))
                      (p props))
                  (while (and ok p)
                    (unless (equal (plist-get actual (car p)) (cadr p))
                      (setq ok nil))
                    (setq p (cddr p))))
                (setq i (1+ i)))))))))

  (unwind-protect
      (let ((s (copy-sequence "The quick brown fox jumps over the lazy dog")))
        ;; Apply diverse properties
        (put-text-property 0 3 'face 'font-lock-keyword-face s)
        (put-text-property 4 9 'face 'font-lock-type-face s)
        (put-text-property 4 9 'help-echo "speed" s)
        (put-text-property 10 15 'face 'font-lock-string-face s)
        (put-text-property 16 19 'mouse-face 'highlight s)
        (put-text-property 20 25 'face 'bold s)
        (put-text-property 20 25 'invisible t s)
        (put-text-property 31 34 'face 'italic s)
        (put-text-property 35 39 'help-echo "sleepy" s)
        (put-text-property 40 43 'face 'underline s)
        (let ((spans (funcall 'neovm--test-collect-all-spans s)))
          (list :span-count (length spans)
                :spans spans
                :verified (funcall 'neovm--test-verify-spans s spans)
                ;; Check that first span starts at 0 and last ends at length
                :starts-at-0 (= (caar spans) 0)
                :ends-at-length (= (cadr (car (last spans))) (length s)))))
    (fmakunbound 'neovm--test-collect-all-spans)
    (fmakunbound 'neovm--test-verify-spans)))"#;
    let expect = expect_test::expect![[
        r#""OK (:span-count 15 :spans ((0 3 face font-lock-keyword-face) (3 4) (4 9 help-echo \"speed\" face font-lock-type-face) (9 10) (10 15 face font-lock-string-face) (15 16) (16 19 mouse-face highlight) (19 20) (20 25 invisible t face bold) (25 31) (31 34 face italic) (34 35) (35 39 help-echo \"sleepy\") (39 40) (40 43 face underline)) :verified t :starts-at-0 t :ends-at-length t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a property span map
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_patterns_property_span_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a map from property names to their span ranges,
    // then verify consistency by checking each property individually.
    let form = r#"(progn
  (fset 'neovm--test-build-prop-map
    (lambda (s)
      "Build a map: property-name -> list of (start end value) spans."
      (let ((prop-map nil)  ;; alist: prop-name -> spans
            (pos 0)
            (len (length s)))
        (while (< pos len)
          (let* ((props (text-properties-at pos s))
                 (next (or (next-property-change pos s) len))
                 (p props))
            (while p
              (let* ((name (car p))
                     (val (cadr p))
                     (existing (assq name prop-map))
                     (span (list pos next val)))
                (if existing
                    (setcdr existing (cons span (cdr existing)))
                  (push (cons name (list span)) prop-map)))
              (setq p (cddr p)))
            (setq pos next)))
        ;; Reverse each span list
        (dolist (entry prop-map)
          (setcdr entry (nreverse (cdr entry))))
        prop-map)))

  (unwind-protect
      (let ((s (copy-sequence "abcdefghijklmnopqrst")))
        ;; Create a pattern: face changes every 4 chars, help-echo every 5
        (put-text-property 0 4 'face 'bold s)
        (put-text-property 4 8 'face 'italic s)
        (put-text-property 8 12 'face 'bold s)
        (put-text-property 12 16 'face 'italic s)
        (put-text-property 16 20 'face 'bold s)
        (put-text-property 0 5 'help-echo "h0" s)
        (put-text-property 5 10 'help-echo "h1" s)
        (put-text-property 10 15 'help-echo "h2" s)
        (put-text-property 15 20 'help-echo "h3" s)
        (let ((pmap (funcall 'neovm--test-build-prop-map s)))
          (list :prop-names (sort (mapcar #'car pmap)
                                  (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                :face-spans (cdr (assq 'face pmap))
                :help-spans (cdr (assq 'help-echo pmap))
                ;; Verify face spans cover exactly 0-20
                :face-coverage
                (let ((last-end 0) (ok t))
                  (dolist (sp (cdr (assq 'face pmap)) ok)
                    (unless (= (car sp) last-end) (setq ok nil))
                    (setq last-end (cadr sp))))
                ;; Verify help-echo spans cover exactly 0-20
                :help-coverage
                (let ((last-end 0) (ok t))
                  (dolist (sp (cdr (assq 'help-echo pmap)) ok)
                    (unless (= (car sp) last-end) (setq ok nil))
                    (setq last-end (cadr sp)))))))
    (fmakunbound 'neovm--test-build-prop-map)))"#;
    let expect = expect_test::expect![[
        r#""OK (:prop-names (face help-echo) :face-spans ((0 4 bold) (4 5 italic) (5 8 italic) (8 10 bold) (10 12 bold) (12 15 italic) (15 16 italic) (16 20 bold)) :help-spans ((0 4 \"h0\") (4 5 \"h0\") (5 8 \"h1\") (8 10 \"h1\") (10 12 \"h2\") (12 15 \"h2\") (15 16 \"h3\") (16 20 \"h3\")) :face-coverage t :help-coverage t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
