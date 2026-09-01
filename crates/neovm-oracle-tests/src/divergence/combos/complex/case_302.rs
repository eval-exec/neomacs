//! Complex combo batch 302 — `char-table` parent inheritance and
//! override, `char-table-range` with `nil`/char/range, extra-slots
//! lifecycle, `map-char-table` collect and count, syntax-table category
//! table per-buffer interaction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx302_char_table_parent_inheritance_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:in-child :child-default :child-default :in-parent t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((parent (make-char-table 'neo-cx302-parent :parent-default))
       (child (make-char-table 'neo-cx302-child :child-default)))
  (aset parent ?a :in-parent)
  (aset parent ?b :in-parent)
  (set-char-table-parent child parent)
  (aset child ?a :in-child)
  (list (aref child ?a)
        (aref child ?b)
        (aref child ?z)
        (aref parent ?a)
        (char-table-p child)
        (eq (char-table-parent child) parent)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_range_chains_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:default :lower :upper :digit :underscore :default :lower)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx302-range nil)))
  (set-char-table-range ct nil :default)
  (set-char-table-range ct '(?a . ?z) :lower)
  (set-char-table-range ct '(?A . ?Z) :upper)
  (set-char-table-range ct '(?0 . ?9) :digit)
  (set-char-table-range ct ?_ :underscore)
  (list (char-table-range ct nil)
        (char-table-range ct ?a)
        (char-table-range ct ?A)
        (char-table-range ct ?5)
        (char-table-range ct ?_)
        (char-table-range ct ?!)
        (aref ct ?a)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_extra_slots_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-char-table 'neo-cx302-extra nil 4)))
      (set-char-table-extra-slot ct 0 :slot-0)
      (set-char-table-extra-slot ct 1 :slot-1)
      (set-char-table-extra-slot ct 2 99)
      (set-char-table-extra-slot ct 3 '("list" "of" "data"))
      (list (char-table-extra-slot ct 0)
            (char-table-extra-slot ct 1)
            (char-table-extra-slot ct 2)
            (char-table-extra-slot ct 3)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx302_map_char_table_collect_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:special . 1) (:vowel-or-low . 1) (:vowel-or-up . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx302-map nil)))
  (set-char-table-range ct '(?a . ?e) :vowel-or-low)
  (set-char-table-range ct '(?A . ?E) :vowel-or-up)
  (set-char-table-range ct ?x :special)
  (let (counts)
    (map-char-table
     (lambda (key val)
       (when val
         (let ((entry (assq val counts)))
           (if entry (setcdr entry (1+ (cdr entry)))
             (push (cons val 1) counts)))))
     ct)
    (sort counts (lambda (a b)
                   (string< (symbol-name (car a)) (symbol-name (car b)))))))
"##,
        expect,
    )
}

#[test]
fn div_cx302_syntax_table_per_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx302-st-a*"))
      (buf-b (get-buffer-create " *neo-cx302-st-b*")))
  (with-current-buffer buf-a
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "w"))
  (with-current-buffer buf-b
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "."))
  (let ((at-a (with-current-buffer buf-a (char-syntax ?@)))
        (at-b (with-current-buffer buf-b (char-syntax ?@))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list at-a at-b)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_category_table_define_and_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-category-table)))
      (define-category ?l "letter" ct)
      (define-category ?d "digit" ct)
      (modify-category-entry ?a ?l ct)
      (modify-category-entry ?0 ?d ct)
      (list (category-docstring ?l ct)
            (category-docstring ?d ct)
            (char-category-set ?a ct)
            (char-category-set ?0 ct)
            (category-set-mnemonics (char-category-set ?a ct))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_default_via_nil_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:override :initial :initial :initial :initial :initial)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx302-def :initial)))
  (aset ct ?a :override)
  (list (aref ct ?a)
        (aref ct ?b)
        (aref ct ?A)
        (aref ct ?1)
        (aref ct ? )
        (char-table-range ct nil)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_subtype_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t neo-cx302-subtype t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx302-subtype nil)))
  (list (char-table-p ct)
        (char-table-subtype ct)
        (eq (char-table-subtype ct) 'neo-cx302-subtype)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_default_value_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:p-default :p-default :override-child :override-child :p-default)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((parent (make-char-table 'neo-cx302-p :p-default))
       (child (make-char-table 'neo-cx302-c nil)))
  (set-char-table-parent child parent)
  (list (aref child ?a)
        (aref child ?b)
        (aset child ?a :override-child)
        (aref child ?a)
        (aref parent ?a)))
"##,
        expect,
    )
}

#[test]
fn div_cx302_char_table_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx302-mega :default)))
  (aset ct ?a :letter)
  (aset ct ?b :letter)
  (set-char-table-range ct '(?0 . ?9) :digit)
  (with-temp-buffer
    (buffer-enable-undo)
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "w")
    (insert "abc 123 def")
    (put-text-property 1 4 'face 'bold)
    (let ((m (set-marker (make-marker) 5))
          (ov (make-overlay 3 8)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 1 9)
      (let ((state (list (aref ct ?a)
                         (char-table-range ct '(?0 . ?9))
                         (char-syntax ?@)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
