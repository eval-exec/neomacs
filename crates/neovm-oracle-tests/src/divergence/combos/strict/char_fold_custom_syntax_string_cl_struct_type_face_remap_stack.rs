//! Strict combo oracle probes, batch 122: char-fold-table customization,
//! syntax-table string syntax variants, cl-struct with :type vector/list,
//! face-remap-add-relative stacking, and display-line-numbers effect.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t6_char_fold_table_customization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((old-table (copy-tree char-fold-table)))
  (set-char-table-range char-fold-table ?a "áàâä")
  (prog1
      (list (char-table-range char-fold-table ?a)
            (length (char-fold-to-regexp "a"))
            (string-match-p (char-fold-to-regexp "a") "á"))
    (setq char-fold-table old-table)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable char-fold-table)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t6_syntax_table_string_syntax_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((table (make-syntax-table)))
  (modify-syntax-entry ?\" "\"" table)
  (modify-syntax-entry ?` "\"" table)
  (modify-syntax-entry ?| "\"" table)
  (with-temp-buffer
    (set-syntax-table table)
    (insert "\"string\" `other` |pipe|")
    (goto-char 1)
    (list (nth 3 (syntax-pp 2))
          (nth 3 (syntax-pp 10))
          (nth 3 (syntax-pp 12))
          (nth 3 (syntax-pp 20))
          (nth 3 (syntax-pp 22))))
"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t6_cl_struct_type_vector_and_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (cl-defstruct (probe-vec-struct (:type vector) :named) a b c)
  (cl-defstruct (probe-list-struct (:type list)) x y z)
  (let ((vs (make-probe-vec-struct :a 1 :b 2 :c 3))
        (ls (make-probe-list-struct :x 10 :y 20 :z 30)))
    (list (vectorp vs)
          (probe-vec-struct-p vs)
          (probe-vec-struct-a vs)
          (probe-vec-struct-c vs)
          (listp ls)
          (probe-list-struct-x ls)
          (probe-list-struct-z ls)
          (progn (setf (probe-vec-struct-b vs) 'changed)
                 (probe-vec-struct-b vs))
          (length vs))))
"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t6_face_remap_add_relative_stacking() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((b (get-buffer-create " *probe-frs*")))
  (unwind-protect
      (with-current-buffer b
        (let ((c1 (face-remap-add-relative 'default :height 1.5))
              (c2 (face-remap-add-relative 'default :weight 'bold)))
          (prog1
              (list (consp c1)
                    (consp c2)
                    (length face-remapping-alist)
                    (assoc 'default face-remapping-alist))
            (face-remap-remove-relative c1)
            (face-remap-remove-relative c2))))
    (kill-buffer b)))
"#;
    let expect = expect_test::expect![[r#""OK (t t 1 (default default))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t6_cl_coerce_type_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list (cl-coerce 65 'character)
      (cl-coerce "abc" 'list)
      (cl-coerce '(1 2 3) 'vector)
      (cl-coerce [1 2 3] 'list)
      (cl-coerce "abc" 'vector)
      (condition-case err (cl-coerce 3.14 'integer) (error (car err)))
      (condition-case err (cl-coerce "ab" 'character) (error (car err)))
      (cl-coerce '(1 2) 'simple-vector))
"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
