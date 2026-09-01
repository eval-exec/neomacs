//! Divergence tests: EIEIO dispatch + buffer local + hash + cl-struct deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_defgeneric_multiple_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (60 t 90 t 150 t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-disp-base () ((val :initarg :val :accessor test-disp-val)))
  (defclass test-disp-a (test-disp-base) ((a-flag :initform t)))
  (defclass test-disp-b (test-disp-base) ((b-flag :initform t)))
  (defclass test-disp-ab (test-disp-a test-disp-b) ((ab-flag :initform t)))
  (cl-defgeneric test-disp-compute (obj factor)
    "Compute something.")
  (cl-defmethod test-disp-compute ((obj test-disp-a) factor)
    (* (test-disp-val obj) factor 2))
  (cl-defmethod test-disp-compute ((obj test-disp-b) factor)
    (* (test-disp-val obj) factor 3))
  (cl-defmethod test-disp-compute ((obj test-disp-ab) factor)
    (* (test-disp-val obj) factor 5))
  (let ((oa (make-instance 'test-disp-a :val 10))
        (ob (make-instance 'test-disp-b :val 10))
        (oab (make-instance 'test-disp-ab :val 10)))
    (list (test-disp-compute oa 3) (= (test-disp-compute oa 3) 60)
          (test-disp-compute ob 3) (= (test-disp-compute ob 3) 90)
          (test-disp-compute oab 3) (= (test-disp-compute oab 3) 150)
          (child-of-class-p (class-of oab) 'test-disp-a)
          (child-of-class-p (class-of oab) 'test-disp-b)
          (cl-typep oa 'test-disp-base)
          (cl-typep oab 'test-disp-a)
          (cl-typep oab 'test-disp-b)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-undo-slot ()
    ((name :initarg :name :accessor test-undo-name)
     (data :initarg :data :accessor test-undo-data :initform nil)))
  (let ((obj (make-instance 'test-undo-slot :name "test")))
    (with-temp-buffer
      (insert "AAA-BBB-CCC")
      (put-text-property 1 3 'owner obj)
      (put-text-property 5 7 'owner obj)
      (put-text-property 9 11 'owner obj)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "BBB" nil t)
      (replace-match "XXX")
      (let ((s (buffer-string))
            (p1 (get-text-property 1 'owner))
            (p2 (get-text-property 5 'owner)))
        (primitive-undo 1 buffer-undo-list)
        (setf (test-undo-data obj) '(1 2 3))
        (list s
              (buffer-string)
              (string= (buffer-string) "AAA-BBB-CCC")
              (object-of-class-p p1 'test-undo-slot)
              (object-of-class-p p2 'test-undo-slot)
              (test-undo-data obj) (equal (test-undo-data obj) '(1 2 3))
              (test-undo-name obj) (string= (test-undo-name obj) "test")
              (get-text-property 1 'owner)
              (object-of-class-p (get-text-property 1 'owner) 'test-undo-slot)
              (get-text-property 5 'owner)
              (object-of-class-p (get-text-property 5 'owner) 'test-undo-slot)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_struct_nested_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-nst (:constructor test-nst-make))
    label tokens props)
  (let ((s1 (test-nst-make :label "alpha" :tokens '("a" "b" "c") :props '((a . 1))))
        (s2 (test-nst-make :label "beta" :tokens '("d" "e") :props '((b . 2))))
        (s3 (test-nst-make :label "gamma" :tokens '("f") :props '((c . 3)))))
    (with-temp-buffer
      (insert "TOKEN1 TOKEN2 TOKEN3 TOKEN4 TOKEN5")
      (put-text-property 1 6 'struct s1)
      (put-text-property 8 13 'struct s2)
      (put-text-property 15 20 'struct s3)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "TOKEN1" nil t)
      (replace-match "REPLACED")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "TOKEN3" nil t)
      (replace-match "CHANGED")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "TOKEN1 TOKEN2 TOKEN3 TOKEN4 TOKEN5")
              (test-nst-label (get-text-property 1 'struct))
              (string= (test-nst-label (get-text-property 1 'struct)) "alpha")
              (test-nst-label (get-text-property 8 'struct))
              (string= (test-nst-label (get-text-property 8 'struct)) "beta")
              (test-nst-label (get-text-property 15 'struct))
              (string= (test-nst-label (get-text-property 15 'struct)) "gamma")
              (equal (test-nst-tokens (get-text-property 1 'struct)) '("a" "b" "c"))))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_eql_eq_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (found-sym t found-num t nil nil found-sym t nil t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht-eql (make-hash-table :test 'eql))
        (ht-eq (make-hash-table :test 'eq))
        (sym1 'hello)
        (sym2 'hello)
        (num1 42)
        (num2 42)
        (str1 "world")
        (str2 "world"))
    (puthash sym1 'found-sym ht-eql)
    (puthash num1 'found-num ht-eql)
    (puthash str1 'found-str ht-eql)
    (puthash sym1 'found-sym ht-eq)
    (puthash str1 'found-str ht-eq)
    (list (gethash sym2 ht-eql) (eq (gethash sym2 ht-eql) 'found-sym)
          (gethash num2 ht-eql) (eq (gethash num2 ht-eql) 'found-num)
          (gethash str2 ht-eql) (eq (gethash str2 ht-eql) 'found-str)
          (gethash sym2 ht-eq) (eq (gethash sym2 ht-eq) 'found-sym)
          (gethash str2 ht-eq)
          (= (hash-table-count ht-eql) 3)
          (eq sym1 sym2)
          (equal str1 str2)
          (eq str1 str2)))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_over_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq-local 'my-test-val 10)
  (setq-local 'my-test-trace nil)
  (let ((add-fn (lambda (n) (setq my-test-val (+ my-test-val n))))
        (mul-fn (lambda (n) (setq my-test-val (* my-test-val n))))
        (trace-fn (lambda () (push my-test-val my-test-trace))))
    (funcall add-fn 5)
    (funcall trace-fn)
    (funcall mul-fn 3)
    (funcall trace-fn)
    (funcall add-fn 7)
    (funcall trace-fn)
    (list my-test-val (= my-test-val 52)
          my-test-trace (equal my-test-trace '(52 45 15)))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_filter_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 t 10 t (\"after:15\" \"before:15\") 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-advice-target (x) (* x 2))
  (let ((log nil))
    (advice-add 'test-advice-target :before
      (lambda (x) (push (format "before:%d" x) log)))
    (advice-add 'test-advice-target :after
      (lambda (x) (push (format "after:%d" x) log)))
    (advice-add 'test-advice-target :filter-args
      (lambda (args) (list (+ (car args) 10))))
    (let ((result (test-advice-target 5)))
      (advice-remove 'test-advice-target
        (lambda (x) (push (format "before:%d" x) log)))
      (advice-remove 'test-advice-target
        (lambda (x) (push (format "after:%d" x) log)))
      (advice-remove 'test-advice-target
        (lambda (args) (list (+ (car args) 10))))
      (let ((result2 (test-advice-target 5)))
        (list result (= result 30)
              result2 (= result2 10)
              log (length log)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_flet_labels_interplay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-cl-top-level-fn (x) (+ x 100))
  (let ((result1 (test-cl-top-level-fn 5)))
    (cl-flet ((test-cl-top-level-fn (x) (+ x 200)))
      (let ((result2 (test-cl-top-level-fn 5)))
        (cl-labels ((test-cl-top-level-fn (x) (+ x 300)))
          (let ((result3 (test-cl-top-level-fn 5)))
            (list result1 (= result1 105)
                  result2 (= result2 205)
                  result3 (= result3 305)
                  (test-cl-top-level-fn 10) (= (test-cl-top-level-fn 10) 110))))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_condition_case_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAA-XXXBBBB-CCCC-DDDD\" 0 3 (zone a) 8 11 (zone b) 13 16 (zone c) 18 21 (zone d)) #(\"AAAA-BBBB-CCCC-DDDD\" 0 3 (zone a) 5 8 (zone b) 10 13 (zone c) 15 18 (zone d)) t t a t b t c t d t all t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (put-text-property 1 4 'zone 'a)
  (put-text-property 6 9 'zone 'b)
  (put-text-property 11 14 'zone 'c)
  (put-text-property 16 19 'zone 'd)
  (let ((m (copy-marker 6 t))
        (ov (make-overlay 1 19)))
    (overlay-put ov 'scope 'all)
    (undo-boundary)
    (condition-case nil
        (progn
          (narrow-to-region 6 14)
          (goto-char (point-min))
          (insert "XXX")
          (undo-boundary)
          (signal 'error '("test error")))
      (error nil))
    (widen)
    (let ((s1 (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s1
            (buffer-string)
            (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD")
            (= (marker-position m) 6)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'a)
            (get-text-property 6 'zone) (eq (get-text-property 6 'zone) 'b)
            (get-text-property 11 'zone) (eq (get-text-property 11 'zone) 'c)
            (get-text-property 16 'zone) (eq (get-text-property 16 'zone) 'd)
            (overlay-get ov 'scope) (eq (overlay-get ov 'scope) 'all))))) "#,
        expect,
    );
}

#[test]
fn divergence_propertized_string_concat_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"GOODBYE WORLD\" 7 8 (separator t) 8 13 (face italic)) #(\"HELLO WORLD\" 0 5 (face bold) 5 6 (separator t) 6 11 (face italic)) t bold t t t italic t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((s1 (propertize "HELLO" 'face 'bold))
        (s2 (propertize " " 'separator t))
        (s3 (propertize "WORLD" 'face 'italic)))
    (insert (concat s1 s2 s3))
    (let ((f1 (get-text-property 1 'face))
          (f5 (get-text-property 6 'face))
          (sep (get-text-property 6 'separator))
          (f7 (get-text-property 7 'face))
          (ov (make-overlay 1 11)))
      (overlay-put ov 'combined t)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "HELLO" nil t)
      (replace-match "GOODBYE")
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "HELLO WORLD")
              (get-text-property 1 'face) (eq (get-text-property 1 'face) 'bold)
              (get-text-property 6 'separator) (eq (get-text-property 6 'separator) t)
              (get-text-property 7 'face) (eq (get-text-property 7 'face) 'italic)
              (overlay-get ov 'combined) (eq (overlay-get ov 'combined) t)))))) "#,
        expect,
    );
}

#[test]
fn divergence_multibyte_propertized_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 23 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "café résumé naïve déjeuner")
  (put-text-property 1 4 'word 'w1)
  (put-text-property 5 6 'accent 'e-acute)
  (put-text-property 7 12 'word 'w2)
  (put-text-property 15 19 'word 'w3)
  (put-text-property 21 22 'accent 'e-acute-2)
  (put-text-property 23 30 'word 'w4)
  (let ((m (copy-marker 1 t))
        (ov (make-overlay 1 30)))
    (overlay-put ov 'lang 'french)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "caf" nil t)
    (replace-match "restaurant")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "na" nil t)
    (replace-match "simplistic")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "café résumé naïve déjeuner")
            (= (marker-position m) 1)
            (get-text-property 1 'word) (eq (get-text-property 1 'word) 'w1)
            (get-text-property 5 'accent) (eq (get-text-property 5 'accent) 'e-acute)
            (get-text-property 7 'word) (eq (get-text-property 7 'word) 'w2)
            (overlay-get ov 'lang) (eq (overlay-get ov 'lang) 'french))))) "#,
        expect,
    );
}
