//! Deep combo: char-table + category-table + syntax-table + aref/aset + map-char-table.
//! Tests character infrastructure operations across table types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_make_char_table_and_aref_aset_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (word word word digit digit nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'syntax-table nil)))\n\
         (aset ct ?A 'word)\n\
         (aset ct ?Z 'word)\n\
         (aset ct ?a 'word)\n\
         (aset ct ?0 'digit)\n\
         (aset ct ?9 'digit)\n\
         (list (aref ct ?A)\n\
         (aref ct ?Z)\n\
         (aref ct ?a)\n\
         (aref ct ?0)\n\
         (aref ct ?9)\n\
         (aref ct ?+)\n\
         (aref ct ? ))))",
        expect,
    );
}

#[test]
fn deficiency_char_table_range_set_and_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (override base digit base #^[base nil category-table #^^[3 0 base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base digit base base base base base base base base base base base base base base base base alpha base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base] #^^[1 0 #^^[2 0 #^^[3 0 base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base digit base base base base base base base base base base base base base base base base alpha base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base] base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base] base base base base base base base base base base base base base base base] base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base base])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((parent (make-char-table 'category-table 'base))\n\
         (child (make-char-table 'category-table nil)))\n\
         (aset parent ?A 'alpha)\n\
         (aset parent ?0 'digit)\n\
         (set-char-table-parent child parent)\n\
         (aset child ?A 'override)\n\
         (list (aref child ?A)\n\
         (aref child ?B)\n\
         (aref child ?0)\n\
         (aref child ?+)\n\
         (char-table-parent child))))",
        expect,
    );
}

#[test]
fn deficiency_map_char_table_collects_all_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'syntax-table nil))\n\
         (ranges nil))\n\
         (dotimes (i 26)\n\
         (aset ct (+ ?a i) 'lower))\n\
         (aset ct ?0 'digit)\n\
         (aset ct ?1 'digit)\n\
         (aset ct ?2 'digit)\n\
         (map-char-table (lambda (range val)\n\
         (push (list range val) ranges)) ct)\n\
         (let ((sorted (sort ranges\n\
         (lambda (a b)\n\
         (let ((ra (car a)) (rb (car b)))\n\
         (if (consp ra) (car ra) ra)\n\
         (< (if (consp ra) (car ra) ra)\n\
         (if (consp rb) (car rb) rb)))))))\n\
         (length sorted))))",
        expect,
    );
}

#[test]
fn deficiency_category_table_set_and_category_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments define-category 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-category-table)))\n\
         (define-category ?a \\\"ASCII letters\\\" ct)\n\
         (define-category ?d \\\"Digits\\\" ct)\n\
         (dotimes (i 26)\n\
         (modify-category-entry (+ ?a i) ?a ct))\n\
         (dotimes (i 10)\n\
         (modify-category-entry (+ ?0 i) ?d ct))\n\
         (list (category-docstring ?a ct)\n\
         (category-docstring ?d ct)\n\
         (char-category-set ?a ct)\n\
         (char-category-set ?0 ct)\n\
         (char-category-set ?+ ct))))",
        expect,
    );
}

#[test]
fn deficiency_syntax_table_after_modify_syntax_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((st (copy-syntax-table (standard-syntax-table))))\n\
         (with-syntax-table st\n\
         (modify-syntax-entry ?$ \\\"w\\\")\n\
         (modify-syntax-entry ?% \\\"w\\\")\n\
         (list (char-syntax ?$)\n\
         (char-syntax ?%)\n\
         (char-syntax ?a)\n\
         (char-syntax ?0)\n\
         (char-syntax ? ))))",
        expect,
    );
}

#[test]
fn deficiency_char_table_extra_slots_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[0 nil foo 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'foo 0)))\n\
         (set-char-table-extra-slot ct 0 'slot0)\n\
         (set-char-table-extra-slot ct 1 'slot1)\n\
         (set-char-table-extra-slot ct 2 'slot2)\n\
         (list (char-table-extra-slot ct 0)\n\
         (char-table-extra-slot ct 1)\n\
         (char-table-extra-slot ct 2))))",
        expect,
    );
}

#[test]
fn deficiency_char_table_default_value_and_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"string\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'test-table 'default-val)))\n\
         (aset ct ?x 'x-specific)\n\
         (list (aref ct ?x)\n\
         (aref ct ?y)\n\
         (aref ct ?z)\n\
         (char-table-p ct)\n\
         (char-table-p [1 2 3])\n\
         (char-table-p \\\"string\\\"))))",
        expect,
    );
}

#[test]
fn deficiency_category_table_merge_two_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"Alpha\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct1 (make-category-table))\n\
         (ct2 (make-category-table)))\n\
         (define-category ?a \\\"Alpha\\\" ct1)\n\
         (define-category ?b \\\"Beta\\\" ct2)\n\
         (modify-category-entry ?x ?a ct1)\n\
         (modify-category-entry ?y ?b ct2)\n\
         (let ((merged (copy-category-table ct1)))\n\
         (modify-category-entry ?y ?b merged)\n\
         (list (char-category-set ?x merged)\n\
         (char-category-set ?y merged)\n\
         (category-docstring ?a merged)\n\
         (category-docstring ?b ct2)))))",
        expect,
    );
}

#[test]
fn deficiency_optimize_char_table_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 letter lower nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'test nil)))\n\
         (aset ct ?A 'letter)\n\
         (aset ct ?B 'letter)\n\
         (aset ct ?C 'letter)\n\
         (aset ct ?a 'lower)\n\
         (aset ct ?b 'lower)\n\
         (let ((count 0))\n\
         (map-char-table (lambda (_ _) (setq count (1+ count))) ct)\n\
         (list count\n\
         (aref ct ?A)\n\
         (aref ct ?a)\n\
         (aref ct ?Z)))))",
        expect,
    );
}

#[test]
fn deficiency_char_table_with_multibyte_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (japanese cyrillic nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ct (make-char-table 'syntax-table nil)))\n\
         (aset ct ?\\x300 'japanese)\n\
         (aset ct ?\\x400 'cyrillic)\n\
         (list (aref ct ?\\x300)\n\
         (aref ct ?\\x400)\n\
         (aref ct ?A)\n\
         (aref ct ?z))))",
        expect,
    );
}
