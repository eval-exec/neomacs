//! Oracle parity tests for overlay operations in Elisp.
//!
//! Covers: `make-overlay`, `overlayp`, `overlay-start`/`overlay-end`,
//! `overlay-buffer`, `overlay-get`/`overlay-put`, `overlay-properties`,
//! `overlays-at`, `overlays-in`, `next-overlay-change`,
//! `previous-overlay-change`, `overlay-lists`, `delete-overlay`,
//! `move-overlay`, `overlay-recenter`, overlay priority ordering,
//! and face overlays.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic overlay creation and predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_make_and_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Hello, World! This is a test buffer with some content.")
  (let* ((ov (make-overlay 1 6))
         (ov2 (make-overlay 8 13))
         (ov3 (make-overlay 1 1)))
    (list
     ;; overlayp
     (overlayp ov)
     (overlayp ov2)
     (overlayp "not-an-overlay")
     (overlayp nil)
     (overlayp 42)
     ;; overlay-start and overlay-end
     (overlay-start ov)
     (overlay-end ov)
     (overlay-start ov2)
     (overlay-end ov2)
     ;; empty overlay (start = end)
     (overlay-start ov3)
     (overlay-end ov3)
     (= (overlay-start ov3) (overlay-end ov3))
     ;; overlay-buffer returns the buffer
     (eq (overlay-buffer ov) (current-buffer))
     (eq (overlay-buffer ov2) (current-buffer)))))
"####;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil 1 6 8 13 1 1 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// overlay-put / overlay-get / overlay-properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_put_get_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Property testing for overlays in a buffer.")
  (let ((ov (make-overlay 1 20)))
    ;; Set various properties
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'priority 10)
    (overlay-put ov 'invisible t)
    (overlay-put ov 'before-string "[")
    (overlay-put ov 'after-string "]")
    (overlay-put ov 'custom-data '(a b c))
    (overlay-put ov 'evaporate nil)
    (overlay-put ov 'intangible t)
    (list
     ;; Retrieve properties
     (overlay-get ov 'face)
     (overlay-get ov 'priority)
     (overlay-get ov 'invisible)
     (overlay-get ov 'before-string)
     (overlay-get ov 'after-string)
     (overlay-get ov 'custom-data)
     (overlay-get ov 'evaporate)
     (overlay-get ov 'intangible)
     ;; Non-existent property returns nil
     (overlay-get ov 'nonexistent)
     ;; Overwrite a property
     (progn (overlay-put ov 'priority 99)
            (overlay-get ov 'priority))
     ;; overlay-properties returns a plist
     (let ((props (overlay-properties ov)))
       (list (plist-get props 'face)
             (plist-get props 'priority)
             (plist-get props 'invisible))))))
"####;
    let expect =
        expect_test::expect![[r#""OK (bold 10 t \"[\" \"]\" (a b c) nil t nil 99 (bold 99 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// overlays-at and overlays-in
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_overlays_at_and_in() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (let ((ov1 (make-overlay 1 6))    ;; covers a-e
        (ov2 (make-overlay 3 10))   ;; covers c-i
        (ov3 (make-overlay 8 15))   ;; covers h-n
        (ov4 (make-overlay 20 25))) ;; covers t-x
    ;; Tag them for identification
    (overlay-put ov1 'name "ov1")
    (overlay-put ov2 'name "ov2")
    (overlay-put ov3 'name "ov3")
    (overlay-put ov4 'name "ov4")
    (list
     ;; overlays-at position 1 (inside ov1 only)
     (length (overlays-at 1))
     ;; overlays-at position 4 (inside ov1 and ov2)
     (length (overlays-at 4))
     ;; overlays-at position 9 (inside ov2 and ov3)
     (length (overlays-at 9))
     ;; overlays-at position 22 (inside ov4 only)
     (length (overlays-at 22))
     ;; overlays-at position 18 (inside none)
     (length (overlays-at 18))
     ;; overlays-in range 1 to 6 (ov1 entirely, ov2 partially)
     (length (overlays-in 1 6))
     ;; overlays-in range 1 to 26 (all overlays)
     (length (overlays-in 1 26))
     ;; overlays-in range 20 to 25 (ov4 only)
     (length (overlays-in 20 25))
     ;; overlays-in range 16 to 19 (none)
     (length (overlays-in 16 19))
     ;; Verify names of overlays-at position 4
     (sort (mapcar (lambda (o) (overlay-get o 'name))
                   (overlays-at 4))
           'string<))))
"####;
    let expect = expect_test::expect![[r#""OK (1 2 2 1 0 2 4 1 0 (\"ov1\" \"ov2\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-overlay
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Delete overlay testing buffer content here.")
  (let ((ov1 (make-overlay 1 10))
        (ov2 (make-overlay 5 15))
        (ov3 (make-overlay 10 20)))
    (overlay-put ov1 'name "first")
    (overlay-put ov2 'name "second")
    (overlay-put ov3 'name "third")
    (let ((before-count (length (overlays-in 1 42))))
      ;; Delete the middle overlay
      (delete-overlay ov2)
      (let ((after-count (length (overlays-in 1 42))))
        ;; Deleted overlay: buffer becomes nil, start/end become markers at 1
        (list
         before-count
         after-count
         (overlay-buffer ov2)
         ;; Remaining overlays still work
         (overlay-get ov1 'name)
         (overlay-get ov3 'name)
         (overlay-start ov1)
         (overlay-end ov3)
         ;; Deleting an already-deleted overlay is a no-op
         (progn (delete-overlay ov2) t)
         ;; overlayp still returns t for deleted overlay
         (overlayp ov2))))))
"####;
    let expect = expect_test::expect![[r#""OK (3 2 nil \"first\" \"third\" 1 20 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-overlay
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Move overlay around in this buffer with enough text.")
  (let ((ov (make-overlay 1 10)))
    (overlay-put ov 'name "mobile")
    (let ((orig-start (overlay-start ov))
          (orig-end (overlay-end ov)))
      ;; Move to new position in same buffer
      (move-overlay ov 20 30)
      (let ((new-start (overlay-start ov))
            (new-end (overlay-end ov))
            (same-buf (eq (overlay-buffer ov) (current-buffer))))
        ;; Move to zero-width
        (move-overlay ov 15 15)
        (let ((zero-start (overlay-start ov))
              (zero-end (overlay-end ov)))
          ;; Move to encompass entire buffer
          (move-overlay ov (point-min) (point-max))
          (list
           orig-start orig-end
           new-start new-end same-buf
           zero-start zero-end
           (overlay-start ov)
           (overlay-end ov)
           ;; Property survives move
           (overlay-get ov 'name)))))))
"####;
    let expect = expect_test::expect![[r#""OK (1 10 20 30 t 15 15 1 53 \"mobile\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-overlay to a different buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_move_to_other_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((buf1 (generate-new-buffer " *test-ov-buf1*"))
      (buf2 (generate-new-buffer " *test-ov-buf2*")))
  (unwind-protect
      (progn
        (with-current-buffer buf1
          (insert "Buffer one content here."))
        (with-current-buffer buf2
          (insert "Buffer two content here."))
        (let ((ov (with-current-buffer buf1
                    (make-overlay 1 10))))
          (overlay-put ov 'tag 'moved)
          (let ((in-buf1 (eq (overlay-buffer ov) buf1))
                (count-buf1-before (with-current-buffer buf1
                                     (length (overlays-in 1 25)))))
            ;; Move overlay to buf2
            (move-overlay ov 5 15 buf2)
            (let ((in-buf2 (eq (overlay-buffer ov) buf2))
                  (count-buf1-after (with-current-buffer buf1
                                      (length (overlays-in 1 25))))
                  (count-buf2 (with-current-buffer buf2
                                (length (overlays-in 1 25)))))
              (list in-buf1 count-buf1-before
                    in-buf2 count-buf1-after count-buf2
                    (overlay-start ov)
                    (overlay-end ov)
                    (overlay-get ov 'tag))))))
    (kill-buffer buf1)
    (kill-buffer buf2)))
"####;
    let expect = expect_test::expect![[r#""OK (t 1 t 0 1 5 15 moved)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-overlay-change / previous-overlay-change
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_change_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "0123456789abcdefghijklmnopqrstuvwxyz")
  (let ((ov1 (make-overlay 5 10))
        (ov2 (make-overlay 15 20))
        (ov3 (make-overlay 25 30)))
    (list
     ;; next-overlay-change from position 1 -> 5 (start of ov1)
     (next-overlay-change 1)
     ;; next-overlay-change from position 5 -> 10 (end of ov1)
     (next-overlay-change 5)
     ;; next-overlay-change from position 10 -> 15 (start of ov2)
     (next-overlay-change 10)
     ;; next-overlay-change from position 21 -> 25 (start of ov3)
     (next-overlay-change 21)
     ;; next-overlay-change past all overlays -> point-max
     (next-overlay-change 31)
     ;; previous-overlay-change from position 36 -> 30 (end of ov3)
     (previous-overlay-change 36)
     ;; previous-overlay-change from position 25 -> 20 (end of ov2)
     (previous-overlay-change 25)
     ;; previous-overlay-change from position 15 -> 10 (end of ov1)
     (previous-overlay-change 15)
     ;; previous-overlay-change from position 5 -> 5 (start of ov1)
     (previous-overlay-change 5)
     ;; previous-overlay-change from position 1 -> 1 (point-min)
     (previous-overlay-change 1))))
"####;
    let expect = expect_test::expect![[r#""OK (5 10 15 25 37 30 20 10 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// overlay-lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Overlay lists test: enough content for multiple overlays here.")
  ;; overlay-lists returns (BEFORE . AFTER) relative to point
  (make-overlay 1 5)
  (make-overlay 10 15)
  (make-overlay 20 25)
  (make-overlay 30 35)
  (goto-char 18)
  (let ((ol (overlay-lists)))
    (list
     ;; overlay-lists returns a cons
     (consp ol)
     ;; Total overlays = (length before) + (length after)
     (+ (length (car ol)) (length (cdr ol)))
     ;; All returned items are overlays
     (cl-every #'overlayp (car ol))
     (cl-every #'overlayp (cdr ol))
     ;; With no overlays in a fresh buffer
     (with-temp-buffer
       (insert "no overlays")
       (let ((ol2 (overlay-lists)))
         (list (length (car ol2)) (length (cdr ol2))))))))
"####;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// overlay-recenter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_recenter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert (make-string 200 ?x))
  (let ((ovs nil))
    ;; Create many overlays
    (dotimes (i 20)
      (let ((start (1+ (* i 10)))
            (end (+ 5 (* i 10))))
        (push (make-overlay start end) ovs)))
    ;; overlay-recenter is mainly for performance, should not change semantics
    (let ((before-count (length (overlays-in 1 200))))
      (overlay-recenter 100)
      (let ((after-count (length (overlays-in 1 200))))
        (list
         before-count
         after-count
         (= before-count after-count)
         ;; Overlays still accessible
         (length (overlays-at 50))
         ;; recenter at point-min
         (progn (overlay-recenter 1) (length (overlays-in 1 200)))
         ;; recenter at point-max
         (progn (overlay-recenter (point-max)) (length (overlays-in 1 200))))))))
"####;
    let expect = expect_test::expect![[r#""OK (20 20 t 0 20 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// overlay priority ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_priority_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Priority testing with overlapping overlays for ordering.")
  ;; Create overlapping overlays with different priorities
  (let ((ov-low (make-overlay 1 30))
        (ov-mid (make-overlay 1 30))
        (ov-high (make-overlay 1 30))
        (ov-nil (make-overlay 1 30)))
    (overlay-put ov-low 'priority 1)
    (overlay-put ov-mid 'priority 50)
    (overlay-put ov-high 'priority 100)
    ;; ov-nil has no priority (nil)
    (overlay-put ov-low 'name "low")
    (overlay-put ov-mid 'name "mid")
    (overlay-put ov-high 'name "high")
    (overlay-put ov-nil 'name "nil-pri")
    (let ((at-5 (overlays-at 5)))
      (list
       ;; All 4 overlays present at position 5
       (length at-5)
       ;; Check priorities
       (overlay-get ov-low 'priority)
       (overlay-get ov-mid 'priority)
       (overlay-get ov-high 'priority)
       (overlay-get ov-nil 'priority)
       ;; Negative priority
       (progn
         (overlay-put ov-low 'priority -5)
         (overlay-get ov-low 'priority))
       ;; Change priority and verify
       (progn
         (overlay-put ov-mid 'priority 200)
         (overlay-get ov-mid 'priority))
       ;; Sorting overlays by priority manually
       (let ((sorted (sort (copy-sequence at-5)
                           (lambda (a b)
                             (let ((pa (or (overlay-get a 'priority) 0))
                                   (pb (or (overlay-get b 'priority) 0)))
                               (< pa pb))))))
         (mapcar (lambda (o) (overlay-get o 'name)) sorted))))))
"####;
    let expect = expect_test::expect![[
        r#""OK (4 1 50 100 nil -5 200 (\"low\" \"nil-pri\" \"high\" \"mid\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overlay with face property
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_face_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "Face overlay testing with various face specs applied.")
  (let ((ov1 (make-overlay 1 10))
        (ov2 (make-overlay 10 20))
        (ov3 (make-overlay 20 30))
        (ov4 (make-overlay 30 40)))
    ;; Different face specifications
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face '(:foreground "red" :background "blue"))
    (overlay-put ov3 'face '(bold italic))
    (overlay-put ov4 'face '(:weight bold :slant italic :underline t))
    (list
     (overlay-get ov1 'face)
     (overlay-get ov2 'face)
     (overlay-get ov3 'face)
     (overlay-get ov4 'face)
     ;; Can set face to nil
     (progn (overlay-put ov1 'face nil)
            (overlay-get ov1 'face))
     ;; Multiple properties on same overlay
     (progn
       (overlay-put ov1 'face 'underline)
       (overlay-put ov1 'help-echo "tooltip text")
       (overlay-put ov1 'mouse-face 'highlight)
       (list (overlay-get ov1 'face)
             (overlay-get ov1 'help-echo)
             (overlay-get ov1 'mouse-face))))))
"####;
    let expect = expect_test::expect![[
        r#""OK (bold (:foreground \"red\" :background \"blue\") (bold italic) (:weight bold :slant italic :underline t) nil (underline \"tooltip text\" highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overlay evaporate property
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "abcdefghij0123456789")
  ;; Evaporate overlay: deleted when region becomes empty
  (let ((ov-evap (make-overlay 5 10))
        (ov-normal (make-overlay 5 10)))
    (overlay-put ov-evap 'evaporate t)
    (overlay-put ov-evap 'name "evap")
    (overlay-put ov-normal 'name "normal")
    (let ((count-before (length (overlays-in 1 20))))
      ;; Delete the text under both overlays
      (delete-region 5 10)
      (let ((count-after (length (overlays-in 1 20))))
        (list
         count-before
         count-after
         ;; The evaporating overlay should be gone
         (overlay-buffer ov-evap)
         ;; The normal overlay should still exist (zero-width)
         (overlayp ov-normal)
         (overlay-buffer ov-normal)
         (= (overlay-start ov-normal) (overlay-end ov-normal)))))))
"####;
    let expect = expect_test::expect![[r#""OK (2 1 nil t #<killed buffer> t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex overlay scenario: nested, adjacent, and boundary overlays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_overlay_complex_nested_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghij")
  (let ((outer (make-overlay 1 40))
        (inner (make-overlay 10 20))
        (left-adj (make-overlay 1 10))
        (right-adj (make-overlay 20 30))
        (point-ov (make-overlay 15 15))
        (full (make-overlay 1 46)))
    (overlay-put outer 'name "outer")
    (overlay-put inner 'name "inner")
    (overlay-put left-adj 'name "left")
    (overlay-put right-adj 'name "right")
    (overlay-put point-ov 'name "point")
    (overlay-put full 'name "full")
    (list
     ;; At position 5: outer, left-adj, full
     (sort (mapcar (lambda (o) (overlay-get o 'name)) (overlays-at 5)) 'string<)
     ;; At position 15: outer, inner, full (point-ov is zero-width)
     (sort (mapcar (lambda (o) (overlay-get o 'name)) (overlays-at 15)) 'string<)
     ;; At position 25: outer, right-adj, full
     (sort (mapcar (lambda (o) (overlay-get o 'name)) (overlays-at 25)) 'string<)
     ;; At position 35: outer, full
     (sort (mapcar (lambda (o) (overlay-get o 'name)) (overlays-at 35)) 'string<)
     ;; At position 42: full only
     (sort (mapcar (lambda (o) (overlay-get o 'name)) (overlays-at 42)) 'string<)
     ;; overlays-in 10 to 20 should include inner, outer, left-adj (ends at 10),
     ;; right-adj (starts at 20), point-ov (at 15), full
     (length (overlays-in 10 20))
     ;; Total overlays in buffer
     (length (overlays-in 1 46)))))
"####;
    let expect = expect_test::expect![[
        r#""OK ((\"full\" \"left\" \"outer\") (\"full\" \"inner\" \"outer\") (\"full\" \"outer\" \"right\") (\"full\" \"outer\") (\"full\") 4 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
