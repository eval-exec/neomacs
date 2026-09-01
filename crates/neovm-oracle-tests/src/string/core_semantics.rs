//! Oracle parity tests for GNU core string and sequence construction semantics.
//!
//! GNU implements `string-equal`, `string-lessp`, `concat`, `vconcat`,
//! `copy-sequence`, `substring`, and `substring-no-properties` in `src/fns.c`.
//! These tests focus on symbol coercion, text-property behavior, negative
//! subarray validation, vector substrings, and character-sequence validation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_string_comparison_symbol_coercion_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (string-equal 'alpha "alpha")
 (string= 'alpha 'alpha)
 (string-equal (propertize "alpha" 'face 'bold) "alpha")
 (string-lessp 'alpha "beta")
 (string-lessp "beta" 'alpha)
 (string< 'alpha 'beta)
 (condition-case err
     (string-equal 42 "42")
   (error (list (car err) (cdr err))))
 (condition-case err
     (string-lessp "x" 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t t nil t (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_properties_and_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (propertize "abcdef" 'face 'bold 'tag 'source))
       (sub (substring s 1 5))
       (plain (substring-no-properties s 1 5)))
  (list
   sub
   (get-text-property 0 'face sub)
   (get-text-property 3 'tag sub)
   (text-properties-at 0 sub)
   plain
   (text-properties-at 0 plain)
   (equal-including-properties sub plain)
   (string= sub plain)))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"bcde\" 0 4 (tag source face bold)) bold source (tag source face bold) \"bcde\" nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_vector_negative_and_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (substring [a b c d e] -4 -1)
 (substring [a b c d e] 2 nil)
 (substring "aébcd" -4 -1)
 (condition-case err
     (substring [a b c] 'bad 2)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring [a b c] 0 4)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring-no-properties [a b c] 0 1)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ([b c d] [c d e] \"ébc\" (wrong-type-argument (integerp bad)) (args-out-of-range ([a b c] 0 4)) (wrong-type-argument (stringp [a b c])))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_rejects_record_without_crashing_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Fsubstring starts with CHECK_VECTOR_OR_STRING, so records are
    // rejected with `arrayp` and must not be treated as vector storage.
    let form = r#"
(condition-case err
    (substring (record 'a 1 2) 0 1)
  (error (list (car err) (cdr err))))
"#;

    let expect = expect_test::expect![[r#""OK (wrong-type-argument (arrayp #s(a 1 2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_rejects_bool_vector_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:Fsubstring calls CHECK_VECTOR_OR_STRING, whose
    // src/lisp.h definition accepts only VECTORP and STRINGP.  Bool-vectors
    // are arrays for `aref`, but not valid `substring` inputs.
    let form = r#"
(condition-case err
    (substring (bool-vector t nil t) 0 2)
  (error (list (car err) (cdr err))))
"#;

    let expect = expect_test::expect![[r#""OK (wrong-type-argument (arrayp #&3\"\u{5}\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_rejects_char_table_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Fsubstring uses CHECK_VECTOR_OR_STRING; char-tables are
    // rejected here even though `copy-sequence` has a char-table-specific path.
    let form = r#"
(condition-case err
    (substring (make-char-table 'generic 65) 0 1)
  (error (list (car err) (cdr err))))
"#;

    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument (arrayp #^[65 nil generic 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substring_no_properties_rejects_vectorlike_objects_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Fsubstring_no_properties uses CHECK_STRING, unlike
    // Fsubstring's CHECK_VECTOR_OR_STRING gate.  Vectorlike objects must signal
    // `stringp` here rather than being treated as arrays.
    let form = r#"
(list
 (condition-case err
     (substring-no-properties (make-char-table 'generic 65) 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring-no-properties (make-bool-vector 3 t) 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring-no-properties (record 'tag 1 2) 0 1)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (stringp #^[65 nil generic 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65])) (wrong-type-argument (stringp #&3\"\u{7}\")) (wrong-type-argument (stringp #s(tag 1 2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_concat_and_vconcat_character_sequence_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (propertize "ab" 'face 'bold))
       (joined (concat s '(?c ?d) [?e ?f]))
       (vec (vconcat "ab" '(c d) [e f])))
  (list
   joined
   (get-text-property 0 'face joined)
   (get-text-property 1 'face joined)
   (get-text-property 2 'face joined)
   vec
   (condition-case err
       (concat '(?a bad ?c))
     (error (list (car err) (cdr err))))
   (condition-case err
       (concat [65 4194304])
     (error (list (car err) (cdr err))))
   (condition-case err
       (vconcat 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"abcdef\" 0 2 (face bold)) bold bold nil [97 98 c d e f] (wrong-type-argument (characterp bad)) (wrong-type-argument (characterp 4194304)) (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_text_properties_and_shallow_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((cell (list 'shared))
       (lst (list cell))
       (lst-copy (copy-sequence lst))
       (str (propertize "abc" 'face 'bold))
       (str-copy (copy-sequence str))
       (vec (vector cell))
       (vec-copy (copy-sequence vec)))
  (setcar cell 'changed)
  (list
   (eq lst lst-copy)
   (eq (car lst) (car lst-copy))
   lst-copy
   (eq str str-copy)
   str-copy
   (text-properties-at 0 str-copy)
   (eq vec vec-copy)
   (eq (aref vec 0) (aref vec-copy 0))
   vec-copy
   (copy-sequence nil)
   (condition-case err
       (copy-sequence 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t ((changed)) nil #(\"abc\" 0 3 (face bold)) (face bold) nil t [(changed)] nil (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_vectorlike_type_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Fcopy_sequence has explicit record, char-table, and
    // bool-vector branches, but no closure branch.
    let form = r#"
(let ((bv (make-bool-vector 3 t))
      (rec (record 'tag 1 2))
      (table (make-char-table 'generic 65)))
  (list
   (bool-vector-p (copy-sequence bv))
   (equal bv (copy-sequence bv))
   (recordp (copy-sequence rec))
   (equal rec (copy-sequence rec))
   (char-table-p (copy-sequence table))
   (char-table-range (copy-sequence table) ?A)
   (condition-case err
       (copy-sequence (lambda (x) x))
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t t t 65 (wrong-type-argument (sequencep (closure (t) (x) x))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_char_table_deep_subtables_shallow_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/chartab.c:copy_char_table recursively copies sub-char-tables,
    // but directly shares the parent/default/purpose/extra-slot values.
    let form = r#"
(let* ((payload (list 'shared))
       (table (make-char-table 'generic 'default))
       (parent (make-char-table 'generic 'parent-default)))
  (set-char-table-range parent ?A 'parent-a)
  (set-char-table-parent table parent)
  (set-char-table-extra-slot table 0 payload)
  (set-char-table-range table '(#x0100 . #x01ff) 'latin-extended)
  (set-char-table-range table #x0101 'special-101)
  (let ((copy (copy-sequence table)))
    (set-char-table-range copy #x0101 'copy-101)
    (setcar payload 'mutated)
    (list
     (eq table copy)
     (eq (char-table-parent table) (char-table-parent copy))
     (eq (char-table-extra-slot table 0) (char-table-extra-slot copy 0))
     (list (char-table-range table #x0100)
           (char-table-range table #x0101)
           (char-table-range table #x0102))
     (list (char-table-range copy #x0100)
           (char-table-range copy #x0101)
           (char-table-range copy #x0102))
     (list (char-table-range table ?A)
           (char-table-range copy ?A))
     (list (char-table-extra-slot table 0)
           (char-table-extra-slot copy 0)))))
"#;

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[default #^[parent-default nil generic #^^[3 0 parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-a parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default] #^^[1 0 #^^[2 0 #^^[3 0 parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-a parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default] parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default] parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default] parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default parent-default] generic default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default default] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_record_allocates_shallow_slot_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_sequence copies records with Frecord over the
    // original vectorlike contents.  The record object and slots are new, but
    // objects stored in slots remain shared.
    let form = r#"
(let* ((cell (list 'shared))
       (rec (record 'neovm--copy-sequence-record cell 'tail))
       (copy (copy-sequence rec)))
  (aset copy 2 'copy-tail)
  (setcar cell 'mutated)
  (list
   (eq rec copy)
   (recordp copy)
   (eq (aref rec 1) (aref copy 1))
   (list (aref rec 0) (aref copy 0))
   (list (aref rec 1) (aref copy 1))
   (list (aref rec 2) (aref copy 2))
   (equal rec copy)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t t (neovm--copy-sequence-record neovm--copy-sequence-record) ((mutated) (mutated)) (tail copy-tail) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_circular_and_improper_list_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_sequence copies conses with FOR_EACH_TAIL and
    // then CHECK_LIST_END.  Circular data is normalized here by probing the
    // signaled cycle tail instead of printing the circular object directly.
    let form = r#"
(list
 (condition-case err
     (copy-sequence '(a b . c))
   (wrong-type-argument (list (car err) (cdr err))))
 (let ((c (list 1 2 3)))
   (setcdr (last c) c)
   (condition-case err
       (copy-sequence c)
     (circular-list
      (list (car err)
            (consp (cadr err))
            (safe-length (cadr err))
            (car (cadr err))))))
 (let ((l (list 'p0 'p1 'c0 'c1)))
   (setcdr (last l) (nthcdr 2 l))
   (condition-case err
       (copy-sequence l)
     (circular-list
      (list (car err)
            (consp (cadr err))
            (safe-length (cadr err))
            (car (cadr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp c)) (circular-list t 5 1) (circular-list t 4 c1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
