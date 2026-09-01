//! Complex combo batch 93 — char table ranges and parent chains,
//! category tables, char-property search across whole buffer, and
//! `with-temp-buffer-window` / `with-help-window` semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx93_char_table_parent_inheritance_and_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:child-a :default :default :parent-a t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((parent (make-char-table 'neo-cx93-parent :default))
       (child (make-char-table 'neo-cx93-child :default)))
  (aset parent ?a :parent-a)
  (aset parent ?b :parent-b)
  (set-char-table-parent child parent)
  (aset child ?a :child-a)
  (list (aref child ?a)
        (aref child ?b)
        (aref child ?z)
        (aref parent ?a)
        (char-table-p child)
        (eq (char-table-parent child) parent)))
"##,
        expect,
    );
}

#[test]
fn div_cx93_char_table_range_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:default :lower :upper :digit :underscore :default :lower)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx93-range nil)))
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
    );
}

#[test]
fn div_cx93_category_table_define_and_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-category-table)))
      (define-category ?l "letter" ct)
      (define-category ?d "digit" ct)
      (modify-category-entry ?a ?l ct)
      (modify-category-entry ?b ?l ct)
      (modify-category-entry ?0 ?d ct)
      (list (category-docstring ?l ct)
            (category-docstring ?d ct)
            (char-category-set ?a ct)
            (char-category-set ?0 ct)
            (category-set-mnemonics (char-category-set ?a ct))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx93_with_temp_buffer_window_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"*My Temp Buffer*\" \"content in temp window\\nsecond line\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf-name "*My Temp Buffer*"))
      (with-temp-buffer-window buf-name nil nil
        (princ "content in temp window\n")
        (princ "second line"))
      (let ((buf (get-buffer buf-name))
            (exists (get-buffer-window buf-name)))
        (prog1 (list (buffer-live-p buf)
                     (buffer-name buf)
                     (with-current-buffer buf (buffer-string)))
          (when buf (kill-buffer buf)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx93_with_help_window_isolated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((help-window-select nil))
      (with-help-window (help-buffer)
        (princ "help content here"))
      (let* ((help-buf-name "*Help*")
             (help-buf (get-buffer help-buf-name)))
        (prog1 (list (buffer-live-p help-buf)
                     (when help-buf (buffer-string)))
          (when help-buf (kill-buffer help-buf)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx93_char_table_map_collect_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:special . 1) (:vowel-or-low . 1) (:vowel-or-up . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx93-mapped nil)))
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
    );
}

#[test]
fn div_cx93_char_table_extra_slots_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-char-table 'neo-cx93-extra nil 4)))
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
    );
}

#[test]
fn div_cx93_syntax_table_inheritance_and_per_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 95)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx93-st-a*"))
      (buf-b (get-buffer-create " *neo-cx93-st-b*"))
      (standard-st (copy-syntax-table (syntax-table))))
  (with-current-buffer buf-a
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "w"))
  (with-current-buffer buf-b
    (set-syntax-table standard-st))
  (let ((at-a (with-current-buffer buf-a (char-syntax ?@)))
        (at-b (with-current-buffer buf-b (char-syntax ?@))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list at-a at-b)))
"##,
        expect,
    );
}

#[test]
fn div_cx93_char_property_search_with_overlay_priority_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic italic nil 3 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 3 8)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'priority 0)
    (let ((at-1 (get-char-property 1 'face))
          (at-3 (get-char-property 3 'face))
          (at-5 (get-char-property 5 'face))
          (at-8 (get-char-property 8 'face)))
      (prog1 (list at-1 at-3 at-5 at-8
                   (next-single-char-property-change 1 'face)
                   (next-single-char-property-change 5 'face))
        (delete-overlay ov)))))
"##,
        expect,
    );
}

#[test]
fn div_cx93_char_table_default_value_via_aset_first_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:override :initial :initial :initial :initial :initial)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx93-default :initial)))
  (aset ct ?a :override)
  (list (aref ct ?a)
        (aref ct ?b)
        (aref ct ?A)
        (aref ct ?1)
        (aref ct ? )
        (char-table-range ct nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx93_window_text_pixel_dimensions_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-text-pixel-width win)
        (window-text-pixel-height win)
        (window-max-chars-per-line)
        (window-body-width win 'pixels)
        (window-body-height win 'pixels)))
"##,
        expect,
    );
}

#[test]
fn div_cx93_char_table_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx93-mega :default)))
  (aset ct ?a :letter)
  (aset ct ?b :letter)
  (set-char-table-range ct '(?0 . ?9) :digit)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "abc 123 def")
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "w")
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
    );
}
