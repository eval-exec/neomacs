//! Oracle parity tests for next-property-change and text property traversal:
//! boundary stepping, LIMIT parameter, nil-at-end, face property changes,
//! collecting all property runs, and merging adjacent runs with same properties.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// next-property-change stepping through multiple property boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_step_through_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string with 5 distinct property regions and walk every boundary
    let form = r#"(let ((s (concat (propertize "AA" 'face 'bold)
                                   (propertize "BB" 'face 'italic)
                                   "CC"
                                   (propertize "DD" 'help-echo "x")
                                   (propertize "EE" 'face 'underline))))
                   ;; Collect every boundary by iterating next-property-change
                   (let ((boundaries nil)
                         (pos 0))
                     (while pos
                       (setq pos (next-property-change pos s))
                       (when pos
                         (setq boundaries (cons pos boundaries))))
                     (list (nreverse boundaries)
                           ;; Verify properties at each region start
                           (get-text-property 0 'face s)
                           (get-text-property 2 'face s)
                           (get-text-property 4 'face s)
                           (get-text-property 6 'help-echo s)
                           (get-text-property 8 'face s))))"#;
    let expect = expect_test::expect![[r#""OK ((2 4 6 8) bold italic nil \"x\" underline)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change with LIMIT parameter (various cases)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_limit_before_at_and_after_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // String: "AAABBB" where AAA has face=bold, BBB has no properties
    // Boundary is at position 3
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold) "BBB")))
                   (list
                     ;; LIMIT before boundary → returns LIMIT
                     (next-property-change 0 s 2)
                     ;; LIMIT exactly at boundary → returns 3 (boundary)
                     (next-property-change 0 s 3)
                     ;; LIMIT past boundary → returns 3 (boundary found first)
                     (next-property-change 0 s 5)
                     ;; LIMIT past end of string → returns boundary
                     (next-property-change 0 s 100)
                     ;; Start at boundary, LIMIT before next boundary
                     (next-property-change 3 s 4)
                     ;; Start at boundary, LIMIT at end
                     (next-property-change 3 s 6)
                     ;; Start past all boundaries → nil even with limit
                     (next-property-change 6 s 10)
                     ;; LIMIT = 0 (less than pos)
                     (next-property-change 0 s 0)))"#;
    let expect = expect_test::expect![[r#""OK (2 3 3 3 4 6 10 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change returns nil at end of string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_nil_at_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When starting at last position or past all boundaries, should return nil
    let form = r#"(let ((s1 (propertize "hello" 'face 'bold))
                        (s2 (concat (propertize "ab" 'face 'bold) "cd"))
                        (s3 "plain"))
                   (list
                     ;; Uniform string: only one region, no change
                     (next-property-change 0 s1)
                     ;; Start at last boundary — past it returns nil
                     (next-property-change 2 s2)
                     ;; Start at end position
                     (next-property-change 4 s2)
                     ;; Plain string (no properties): nil immediately
                     (next-property-change 0 s3)
                     ;; Empty string
                     (next-property-change 0 "")
                     ;; Start past string length
                     (next-property-change 100 s1)))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 100 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stepping through face property changes specifically
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_face_property_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple face changes plus non-face properties interleaved
    let form = r#"(let ((s (copy-sequence "abcdefghijklmnop")))
                   ;; Layer faces on different ranges
                   (put-text-property 0 3 'face 'bold s)
                   (put-text-property 3 6 'face 'italic s)
                   (put-text-property 6 9 'face 'underline s)
                   ;; Add non-face property spanning multiple face regions
                   (put-text-property 2 10 'help-echo "info" s)
                   ;; Now walk with next-single-property-change for 'face only
                   (let ((face-boundaries nil)
                         (pos 0))
                     (while pos
                       (setq pos (next-single-property-change pos 'face s))
                       (when pos
                         (setq face-boundaries
                               (cons (cons pos (get-text-property pos 'face s))
                                     face-boundaries))))
                     ;; Also walk all-property boundaries for comparison
                     (let ((all-boundaries nil)
                           (pos2 0))
                       (while pos2
                         (setq pos2 (next-property-change pos2 s))
                         (when pos2
                           (setq all-boundaries (cons pos2 all-boundaries))))
                       (list (nreverse face-boundaries)
                             (nreverse all-boundaries)))))"#;
    let expect =
        expect_test::expect![[r#""OK (((3 . italic) (6 . underline) (9)) (2 3 6 9 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: collect all property runs in a propertized string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_collect_all_property_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a multi-property string, then collect (start end plist) for every run
    let form = r#"(progn
  (fset 'neovm--test-collect-runs
    (lambda (s)
      (let ((runs nil)
            (pos 0)
            (len (length s)))
        (while (< pos len)
          (let* ((props (text-properties-at pos s))
                 (next (or (next-property-change pos s) len)))
            (setq runs (cons (list pos next
                                   (substring-no-properties s pos next)
                                   (plist-get props 'face)
                                   (plist-get props 'help-echo))
                             runs))
            (setq pos next)))
        (nreverse runs))))
  (unwind-protect
      (let ((s (copy-sequence "The quick brown fox")))
        ;; Apply layered properties
        (put-text-property 0 3 'face 'font-lock-keyword-face s)
        (put-text-property 4 9 'face 'font-lock-type-face s)
        (put-text-property 4 9 'help-echo "speed" s)
        (put-text-property 10 15 'face 'font-lock-string-face s)
        (put-text-property 16 19 'help-echo "animal" s)
        (funcall 'neovm--test-collect-runs s))
    (fmakunbound 'neovm--test-collect-runs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 3 \"The\" font-lock-keyword-face nil) (3 4 \" \" nil nil) (4 9 \"quick\" font-lock-type-face \"speed\") (9 10 \" \" nil nil) (10 15 \"brown\" font-lock-string-face nil) (15 16 \" \" nil nil) (16 19 \"fox\" nil \"animal\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: merge adjacent runs that have identical properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_merge_adjacent_same_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After building runs, merge consecutive ones with equal property plists
    let form = r#"(progn
  (fset 'neovm--test-plist-equal
    (lambda (a b)
      "Compare two plists for equality (order-independent)."
      (and (= (length a) (length b))
           (let ((result t)
                 (p a))
             (while (and result p)
               (let ((key (car p))
                     (val (cadr p)))
                 (unless (equal val (plist-get b key))
                   (setq result nil)))
               (setq p (cddr p)))
             result))))

  (fset 'neovm--test-collect-and-merge
    (lambda (s)
      (let ((runs nil)
            (pos 0)
            (len (length s)))
        ;; Phase 1: collect raw runs as (start end props-plist)
        (while (< pos len)
          (let* ((props (text-properties-at pos s))
                 (next (or (next-property-change pos s) len)))
            (setq runs (cons (list pos next props) runs))
            (setq pos next)))
        (setq runs (nreverse runs))
        ;; Phase 2: merge adjacent runs with equal properties
        (let ((merged nil)
              (current (car runs))
              (rest (cdr runs)))
          (while rest
            (let ((next-run (car rest)))
              (if (funcall 'neovm--test-plist-equal (nth 2 current) (nth 2 next-run))
                  ;; Merge: extend current run's end
                  (setq current (list (nth 0 current) (nth 1 next-run) (nth 2 current)))
                ;; Emit current, advance
                (setq merged (cons current merged))
                (setq current next-run)))
            (setq rest (cdr rest)))
          (setq merged (cons current merged))
          (nreverse merged)))))

  (unwind-protect
      (let ((s (copy-sequence "aabbccddee")))
        ;; Intentionally create adjacent regions with same face
        (put-text-property 0 2 'face 'bold s)
        (put-text-property 2 4 'face 'bold s)     ;; same as prev → should merge
        (put-text-property 4 6 'face 'italic s)
        (put-text-property 6 8 'face 'italic s)   ;; same as prev → should merge
        ;; 8..10 has no properties
        (let ((raw-runs (let ((runs nil) (pos 0) (len (length s)))
                          (while (< pos len)
                            (let* ((props (text-properties-at pos s))
                                   (next (or (next-property-change pos s) len)))
                              (setq runs (cons (list pos next props) runs))
                              (setq pos next)))
                          (nreverse runs)))
              (merged (funcall 'neovm--test-collect-and-merge s)))
          (list (length raw-runs)    ;; should be 4 (or more if boundaries exist)
                (length merged)      ;; should be fewer after merging
                ;; Verify merged boundaries
                (mapcar (lambda (r) (list (nth 0 r) (nth 1 r)
                                          (plist-get (nth 2 r) 'face)))
                        merged))))
    (fmakunbound 'neovm--test-plist-equal)
    (fmakunbound 'neovm--test-collect-and-merge)))"#;
    let expect = expect_test::expect![[r#""OK (3 3 ((0 4 bold) (4 8 italic) (8 10 nil)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change in buffer context (with-temp-buffer)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_buffer_text_property_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk property boundaries in buffer text (1-indexed positions)
    let form = r#"(with-temp-buffer
  (insert (propertize "hello" 'face 'bold))
  (insert " ")
  (insert (propertize "world" 'face 'italic))
  (insert "!!!")
  ;; Collect boundaries using buffer positions (1-based)
  (let ((boundaries nil)
        (pos 1))
    (while pos
      (setq pos (next-property-change pos))
      (when (and pos (<= pos (point-max)))
        (setq boundaries
              (cons (list pos (get-text-property pos 'face)) boundaries))))
    (list (nreverse boundaries)
          ;; Total length
          (buffer-size)
          ;; Verify specific positions
          (get-text-property 1 'face)
          (get-text-property 6 'face)
          (get-text-property 7 'face)
          (get-text-property 12 'face))))"#;
    let expect =
        expect_test::expect![[r#""OK (((6 nil) (7 italic) (12 nil)) 14 bold nil italic nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change with previous-property-change round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_npc_forward_backward_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk forward collecting boundaries, then backward, verify they match
    let form = r#"(let ((s (concat (propertize "AA" 'x 1)
                                   (propertize "BB" 'x 2)
                                   (propertize "CC" 'x 3)
                                   "DD")))
                   (let ((forward nil)
                         (backward nil)
                         (pos 0))
                     ;; Forward walk
                     (while pos
                       (setq pos (next-property-change pos s))
                       (when pos (setq forward (cons pos forward))))
                     (setq forward (nreverse forward))
                     ;; Backward walk from end
                     (setq pos (length s))
                     (while pos
                       (setq pos (previous-property-change pos s))
                       (when pos (setq backward (cons pos backward))))
                     ;; forward and backward should contain the same set of positions
                     (list forward
                           backward
                           (equal (sort (copy-sequence forward) #'<)
                                  (sort (copy-sequence backward) #'<)))))"#;
    let expect = expect_test::expect![[r#""OK ((2 4 6) (2 4 6) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
