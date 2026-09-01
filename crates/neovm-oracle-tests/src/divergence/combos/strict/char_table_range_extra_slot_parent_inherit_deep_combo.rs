//! Strict combo oracle probes, batch 169: char-table deep. single + range set
//! and range queries, char-table-range over a uniform span, extra-slot access,
//! parent inheritance (child override vs parent fallback incl range), and
//! char-table-p / char-table-subtype.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_table_range_set_query_uniform_span() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ct (make-char-table 'foo nil)))
  (aset ct ?a 'a-val)
  (aset ct ?z 'z-val)
  (set-char-table-range ct '(?A . ?Z) 'upper)
  (set-char-table-range ct ?0 'zero)
  (set-char-table-range ct '(?0 . ?9) 'digit)
  (list (char-table-range ct ?a)
        (char-table-range ct ?z)
        (char-table-range ct ?M)
        (char-table-range ct ?B)
        (char-table-range ct ?G)
        (char-table-range ct ?5)
        (char-table-range ct '(?A . ?Z))
        (char-table-range ct '(?0 . ?9))
        (char-table-p ct)
        (char-table-subtype ct)
        (char-table-range ct ?!)))
"##;
    let expect = expect_test::expect![[
        r#""OK (a-val z-val upper upper upper digit upper digit t foo nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_table_extra_slot_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((ct (make-char-table 'foo nil))
       (child (make-char-table 'foo 'child-default)))
  (aset ct ?a 'parent-a)
  (aset ct ?z 'parent-z)
  (set-char-table-range ct '(?A . ?Z) 'parent-upper)
  (set-char-table-extra-slot ct 0 'parent-extra0)
  (set-char-table-parent child ct)
  (aset child ?a 'child-a)
  (list (char-table-range child ?a)
        (char-table-range child ?z)
        (char-table-range child ?B)
        (char-table-range child ? )
        (char-table-range parent--unused--no nil)
        (eq (char-table-parent child) ct)
        (char-table-extra-slot ct 0)
        (char-table-extra-slot child 0)
        (char-table-extra-slot ct 5)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil foo #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper nil nil nil nil nil nil parent-a nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil parent-z nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper parent-upper nil nil nil nil nil nil parent-a nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil parent-z nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_table_optimize_map_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ct (make-char-table 'foo nil)))
  (aset ct ?a 'va)
  (aset ct ?b 'vb)
  (aset ct ?c 'vc)
  (set-char-table-range ct '(?x . ?z) 'vxz)
  (let ((collected nil))
    (map-char-table (lambda (key val) (push (cons key val) collected)) ct)
    (sort (mapcar (lambda (p)
                    (if (consp (car p))
                        (cons (cons (car (car p)) (cdr (car p))) (cdr p))
                      p))
                  collected)
          (lambda (p q)
            (let ((pk (if (consp (car p)) (car (car p)) (car p)))
                  (qk (if (consp (car q)) (car (car q)) (car q))))
              (< pk qk))))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((97 . va) (98 . vb) (99 . vc) ((123 . 4194303) . vxz))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
