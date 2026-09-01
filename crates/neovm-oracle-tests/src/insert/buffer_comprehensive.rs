//! Comprehensive oracle parity tests for buffer insertion operations:
//! `insert` with multiple args, `insert-char` with COUNT and INHERIT,
//! `insert-before-markers`, `insert-buffer-substring` with START/END,
//! `insert-buffer-substring-no-properties`, point movement after insert,
//! marker behavior, text properties inheritance, `insert-and-inherit`,
//! and `insert-for-yank`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// insert with multiple args of different types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_multi_arg_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert accepts strings and characters interleaved in any order.
    let form = r#"(with-temp-buffer
      ;; Mix strings and character codes
      (insert "Hello" ?  "World" ?! ?\n "Line2" 65 66 67)
      (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"Hello World!\\nLine2ABC\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_buffer_comp_empty_and_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Edge cases: empty string, single char, many args, unicode chars.
    let form = r#"(with-temp-buffer
      (insert "" "" "")
      (let ((r1 (buffer-string)))
        (insert "a")
        (let ((r2 (buffer-string)))
          (erase-buffer)
          ;; Unicode characters
          (insert "abc" #x3b1 "def" #x3b2 "ghi")
          (let ((r3 (buffer-string)))
            (erase-buffer)
            ;; Many string args
            (insert "a" "b" "c" "d" "e" "f" "g" "h" "i" "j")
            (list r1 r2 r3 (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK (\"\" \"a\" \"abcαdefβghi\" \"abcdefghij\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char with COUNT and INHERIT params
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_insert_char_count_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-char CHAR &optional COUNT INHERIT
    // Inserts COUNT copies of CHAR. INHERIT controls text property inheritance.
    let form = r#"(with-temp-buffer
      ;; Basic: insert 5 copies of ?x
      (insert-char ?x 5)
      (let ((r1 (buffer-string)))
        (erase-buffer)
        ;; COUNT=0 inserts nothing
        (insert-char ?y 0)
        (let ((r2 (buffer-string)))
          (erase-buffer)
          ;; COUNT=1 (default behavior)
          (insert-char ?z 1)
          (let ((r3 (buffer-string)))
            (erase-buffer)
            ;; Unicode char with count
            (insert-char #x2603 3)  ;; snowman
            (let ((r4 (buffer-string)))
              (erase-buffer)
              ;; insert-char with INHERIT=t: inherits text properties from adjacent text
              (insert (propertize "abc" 'face 'bold))
              (goto-char 4)
              (insert-char ?X 2 t)
              (let ((r5 (buffer-string))
                    (r5-props (text-properties-at 4)))
                (list r1 r2 r3 r4 r5 r5-props)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"xxxxx\" \"\" \"z\" \"☃☃☃\" #(\"abcXX\" 0 5 (face bold)) (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-before-markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_insert_before_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-before-markers inserts text and adjusts markers that were
    // at point to stay AFTER the inserted text (unlike regular insert
    // where markers at point stay before).
    let form = r#"(with-temp-buffer
      (insert "abcdef")
      ;; Create a marker at position 4
      (let ((m (make-marker)))
        (set-marker m 4 (current-buffer))
        ;; Insert text at position 4 with regular insert
        (goto-char 4)
        (insert "XX")
        ;; Marker stays at 4 (before inserted text) with regular insert
        (let ((pos-after-insert (marker-position m)))
          ;; Now reset
          (erase-buffer)
          (insert "abcdef")
          (set-marker m 4 (current-buffer))
          ;; Insert at position 4 with insert-before-markers
          (goto-char 4)
          (insert-before-markers "YY")
          ;; Marker moves to after inserted text
          (let ((pos-after-ibm (marker-position m)))
            (list pos-after-insert pos-after-ibm
                  (buffer-string) (point))))))"#;
    let expect = expect_test::expect![[r#""OK (4 6 \"abcYYdef\" 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-buffer-substring with START/END
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-buffer-substring BUFFER &optional START END
    // Inserts a portion of another buffer's contents.
    let form = r#"(let ((src-buf (generate-new-buffer " *test-src*")))
      (unwind-protect
          (progn
            (with-current-buffer src-buf
              (insert "0123456789abcdef"))
            (with-temp-buffer
              ;; Insert entire source buffer
              (insert-buffer-substring src-buf)
              (let ((r1 (buffer-string)))
                (erase-buffer)
                ;; Insert with START only (from position 5 to end)
                (insert-buffer-substring src-buf 5)
                (let ((r2 (buffer-string)))
                  (erase-buffer)
                  ;; Insert with START and END
                  (insert-buffer-substring src-buf 3 8)
                  (let ((r3 (buffer-string)))
                    (erase-buffer)
                    ;; Insert with START=END (empty)
                    (insert-buffer-substring src-buf 5 5)
                    (let ((r4 (buffer-string)))
                      (list r1 r2 r3 r4)))))))
        (kill-buffer src-buf)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"0123456789abcdef\" \"456789abcdef\" \"23456\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-buffer-substring-no-properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-buffer-substring-no-properties strips text properties.
    let form = r#"(let ((src-buf (generate-new-buffer " *test-props-src*")))
      (unwind-protect
          (progn
            (with-current-buffer src-buf
              (insert (propertize "hello" 'face 'bold 'custom 42))
              (insert (propertize " world" 'face 'italic)))
            (with-temp-buffer
              ;; Insert with properties stripped
              (insert-buffer-substring-no-properties src-buf)
              (let ((r1 (buffer-string))
                    (r1-props (text-properties-at 1)))
                (erase-buffer)
                ;; Insert substring without properties
                (insert-buffer-substring-no-properties src-buf 1 6)
                (let ((r2 (buffer-string))
                      (r2-props (text-properties-at 1)))
                  (list r1 r1-props r2 r2-props)))))
        (kill-buffer src-buf)))"#;
    let expect = expect_test::expect![[r#""OK (\"hello world\" nil \"hello\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point movement after insert
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_point_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After insert, point moves to after the inserted text.
    // Verify for various insertion points within existing text.
    let form = r#"(with-temp-buffer
      (insert "abcdef")
      ;; Point is now at end (7)
      (let ((p1 (point)))
        ;; Go to middle and insert
        (goto-char 4)
        (insert "XY")
        (let ((p2 (point))
              (s2 (buffer-string)))
          ;; Go to beginning and insert
          (goto-char 1)
          (insert ">>")
          (let ((p3 (point))
                (s3 (buffer-string)))
            ;; Insert at end
            (goto-char (point-max))
            (insert "<<")
            (let ((p4 (point))
                  (s4 (buffer-string)))
              ;; Multiple inserts track point correctly
              (goto-char 5)
              (insert "A" "B" "C")
              (let ((p5 (point))
                    (s5 (buffer-string)))
                (list p1 p2 s2 p3 s3 p4 s4 p5 s5)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (7 6 \"abcXYdef\" 3 \">>abcXYdef\" 13 \">>abcXYdef<<\" 8 \">>abABCcXYdef<<\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Marker behavior after insert
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_marker_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Markers before insertion point stay put.
    // Markers after insertion point shift forward.
    // Markers at insertion point: depend on insertion type.
    let form = r#"(with-temp-buffer
      (insert "0123456789")
      (let ((m-before (copy-marker 3))
            (m-at (copy-marker 5))
            (m-after (copy-marker 8)))
        ;; Insert at position 5
        (goto-char 5)
        (insert "XXX")
        (let ((r1 (list (marker-position m-before)
                        (marker-position m-at)
                        (marker-position m-after)
                        (buffer-string))))
          ;; Now test with insert-before-markers at position 8
          (goto-char 8)
          (let ((m-at2 (copy-marker 8)))
            (insert-before-markers "YY")
            (let ((r2 (list (marker-position m-at2)
                            (point)
                            (buffer-string))))
              (list r1 r2))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 5 11 \"0123XXX456789\") (10 10 \"0123XXXYY456789\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Text properties inheritance via insert-and-inherit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_insert_and_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-and-inherit inserts text and inherits sticky text properties
    // from the surrounding text.
    let form = r#"(with-temp-buffer
      ;; Set up text with properties
      (insert (propertize "aaa" 'face 'bold))
      (insert (propertize "bbb" 'face 'italic))
      ;; insert-and-inherit between the two propertized regions
      (goto-char 4)
      (insert-and-inherit "XXX")
      (let ((s (buffer-string))
            ;; Check properties at various positions
            (p1 (get-text-property 1 'face))
            (p4 (get-text-property 4 'face))
            (p7 (get-text-property 7 'face)))
        (list s (length s) p1 p4 p7)))"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"aaaXXXbbb\" 0 6 (face bold) 6 9 (face italic)) 9 bold bold italic)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-for-yank
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_insert_for_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-for-yank handles yank-handler text property and
    // processes yank-undo-function. Test basic behavior.
    let form = r#"(with-temp-buffer
      ;; Basic insert-for-yank with plain string
      (insert-for-yank "hello")
      (let ((r1 (buffer-string))
            (r1-point (point)))
        (erase-buffer)
        ;; insert-for-yank with propertized text
        (insert-for-yank (propertize "world" 'face 'bold))
        (let ((r2 (buffer-string))
              (r2-props (get-text-property 1 'face)))
          (erase-buffer)
          ;; insert-for-yank with empty string
          (insert-for-yank "")
          (let ((r3 (buffer-string)))
            ;; insert-for-yank with multiple properties
            (insert-for-yank (propertize "test" 'face 'underline 'help-echo "tip"))
            (let ((r4 (buffer-string))
                  (r4-face (get-text-property 1 'face))
                  (r4-help (get-text-property 1 'help-echo)))
              (list r1 r1-point r2 r2-props r3 r4 r4-face r4-help))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" 6 #(\"world\" 0 4 (face bold) 4 5 (rear-nonsticky t face bold)) bold \"\" #(\"test\" 0 3 (face underline) 3 4 (rear-nonsticky t face underline)) underline nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined operations: insert + narrow + widen interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_buffer_comp_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that insert respects narrowing boundaries and that
    // point/markers behave correctly in narrowed buffers.
    let form = r#"(with-temp-buffer
      (insert "0123456789")
      ;; Narrow to region 4..8 (shows "3456")
      (narrow-to-region 4 8)
      (let ((r1 (buffer-string))
            (r1-pmin (point-min))
            (r1-pmax (point-max)))
        ;; Insert within narrowed region
        (goto-char (point-min))
        (insert ">>")
        (let ((r2 (buffer-string))
              (r2-point (point)))
          ;; Widen and see the full buffer
          (widen)
          (let ((r3 (buffer-string)))
            ;; Narrow again, insert at end
            (narrow-to-region 6 12)
            (goto-char (point-max))
            (insert "<<")
            (let ((r4 (buffer-string)))
              (widen)
              (list r1 r1-pmin r1-pmax r2 r2-point r3 r4 (buffer-string)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"3456\" 4 8 \">>3456\" 6 \"012>>3456789\" \"345678<<\" \"012>>345678<<9\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
