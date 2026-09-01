//! Divergence tests: EIEIO + buffer local + advice + cl-struct + hash deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_buffer_local_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"alpha-42\" 0 4 (owner #s(test-bl-slot \"alpha\" 99))) t 99 nil 99 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-bl-slot ()
    ((name :initarg :name :accessor test-bl-name)
     (value :initarg :value :accessor test-bl-value :initform 0)))
  (let ((obj (make-instance 'test-bl-slot :name "alpha" :value 42)))
    (with-temp-buffer
      (insert (format "%s-%d" (test-bl-name obj) (test-bl-value obj)))
      (put-text-property 1 5 'owner obj)
      (let ((s (buffer-string))
            (p (get-text-property 1 'owner)))
        (setf (test-bl-value obj) 99)
        (list s
              (object-of-class-p p 'test-bl-slot)
              (test-bl-value p)
              (= (test-bl-value p) 42)
              (test-bl-value obj)
              (= (test-bl-value obj) 99)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_struct_with_textprops_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-sp (:constructor test-sp-make))
    label start end)
  (insert "LABEL1----LABEL2----LABEL3----LABEL4----LABEL5")
  (let ((sp1 (test-sp-make :label "one" :start 1 :end 7))
        (sp2 (test-sp-make :label "two" :start 12 :end 18))
        (sp3 (test-sp-make :label "three" :start 23 :end 29))
        (sp4 (test-sp-make :label "four" :start 34 :end 40))
        (sp5 (test-sp-make :label "five" :start 45 :end 50)))
    (dolist (sp (list sp1 sp2 sp3 sp4 sp5))
      (put-text-property (test-sp-start sp) (test-sp-end sp) 'struct sp))
    (let ((ov (make-overlay 1 50)))
      (overlay-put ov 'spans (list sp1 sp2 sp3 sp4 sp5))
      (undo-boundary)
      (goto-char 12)
      (insert "QQQQ")
      (undo-boundary)
      (goto-char 1)
      (while (re-search-forward "LABEL" nil t)
        (replace-match "MARK"))
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "LABEL1----LABEL2----LABEL3----LABEL4----LABEL5")
              (= (test-sp-start sp1) 1)
              (= (test-sp-start sp2) 12)
              (test-sp-label sp1) (string= (test-sp-label sp1) "one")
              (test-sp-label sp2) (string= (test-sp-label sp2) "two")
              (overlay-get ov 'spans) (consp (overlay-get ov 'spans))
              (= (length (overlay-get ov 'spans)) 5)))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_as_text_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t #(\"AAA-BBB-CCC-DDD-EEE\" 0 2 (meta #s(hash-table test equal data (\"a\" 1 \"b\" 2 \"c\" 3)))) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE")
  (let ((h (make-hash-table :test 'equal)))
    (puthash "a" 1 h) (puthash "b" 2 h) (puthash "c" 3 h)
    (put-text-property 1 3 'meta h)
    (let ((h2 (get-text-property 1 'meta)))
      (list (hash-table-p h2)
            (= (gethash "a" h2) 1)
            (= (gethash "b" h2) 2)
            (= (gethash "c" h2) 3)
            (= (hash-table-count h2) 3)
            (eq h h2)
            (buffer-string)
            (string= (buffer-string) "AAA-BBB-CCC-DDD-EEE"))))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_around_text_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"X X X X X\" 5 t #(\"hello world foo bar baz\" 0 4 (word w1) 6 10 (word w2) 12 14 (word w3) 16 18 (word w4) 20 22 (word w5)) t w1 t w2 t w3 t w4 t w5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world foo bar baz")
  (put-text-property 1 5 'word 'w1)
  (put-text-property 7 11 'word 'w2)
  (put-text-property 13 15 'word 'w3)
  (put-text-property 17 19 'word 'w4)
  (put-text-property 21 23 'word 'w5)
  (let ((call-count 0))
    (advice-add 'replace-match :around
      (lambda (oldfn &rest args)
        (setq call-count (+ call-count 1))
        (apply oldfn args)))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "\\b\\w+\\b" nil t)
      (replace-match "X"))
    (let ((s (buffer-string))
          (cnt call-count))
      (advice-remove 'replace-match
        (lambda (oldfn &rest args)
          (setq call-count (+ call-count 1))
          (apply oldfn args)))
      (primitive-undo 1 buffer-undo-list)
      (list s cnt (> cnt 0)
            (buffer-string)
            (string= (buffer-string) "hello world foo bar baz")
            (get-text-property 1 'word) (eq (get-text-property 1 'word) 'w1)
            (get-text-property 7 'word) (eq (get-text-property 7 'word) 'w2)
            (get-text-property 13 'word) (eq (get-text-property 13 'word) 'w3)
            (get-text-property 17 'word) (eq (get-text-property 17 'word) 'w4)
            (get-text-property 21 'word) (eq (get-text-property 21 'word) 'w5))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_closure_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Attempting to set a non-symbol: 'my-test-counter\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq-local 'my-test-counter 0)
  (setq-local 'my-test-trace nil)
  (let ((step1 (lambda () (setq my-test-counter (+ my-test-counter 1))))
        (step2 (lambda () (setq my-test-counter (* my-test-counter 2))))
        (step3 (lambda () (push my-test-counter my-test-trace))))
    (funcall step1) (funcall step2) (funcall step3)
    (funcall step1) (funcall step2) (funcall step3)
    (funcall step1) (funcall step2) (funcall step3)
    (list my-test-counter (= my-test-counter 14)
          my-test-trace (equal my-test-trace '(14 6 2))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_polymorphic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a 42) t (b \"hello\") t x t y t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-poly-base ()
    ((tag :initarg :tag :reader test-poly-tag)))
  (defclass test-poly-a (test-poly-base)
    ((val :initarg :val :reader test-poly-val)))
  (defclass test-poly-b (test-poly-base)
    ((str :initarg :str :reader test-poly-str)))
  (cl-defgeneric test-poly-process (obj)
    "Process OBJ.")
  (cl-defmethod test-poly-process ((obj test-poly-a))
    (list 'a (test-poly-val obj)))
  (cl-defmethod test-poly-process ((obj test-poly-b))
    (list 'b (test-poly-str obj)))
  (let ((oa (make-instance 'test-poly-a :tag 'x :val 42))
        (ob (make-instance 'test-poly-b :tag 'y :str "hello")))
    (list (test-poly-process oa) (equal (test-poly-process oa) '(a 42))
          (test-poly-process ob) (equal (test-poly-process ob) '(b "hello"))
          (test-poly-tag oa) (eq (test-poly-tag oa) 'x)
          (test-poly-tag ob) (eq (test-poly-tag ob) 'y)))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "one two three four five six seven eight nine ten")
  (let ((words (split-string (buffer-string))))
    (erase-buffer)
    (cl-loop for w in words
             for i from 1
             do (progn
                  (insert (format "%d:%s " i w))
                  (put-text-property
                   (point) (- (point) (length w) -1)
                   'word-num i)))
    (let ((s (buffer-string))
          (nums (cl-loop for i from 1 to 10
                         collect (get-text-property
                                  (+ 1 (* (- i 1) (+ 1 (length (number-to-string i)) (length (nth (- i 1) words))))) 'word-num))))
      (list s
            (cl-every #'numberp nums)
            (= (car nums) 1)
            (= (car (last nums)) 10)))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_undo_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1111 2222 3333 4444 5555\" #(\"AAAA BBBB CCCC DDDD EEEE\" 0 3 (g 1) 5 8 (g 2) 10 13 (g 3) 15 18 (g 4) 20 23 (g 5)) t nil nil nil nil nil 1 t 2 t 3 t 4 t 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA BBBB CCCC DDDD EEEE")
  (put-text-property 1 4 'g 1)
  (put-text-property 6 9 'g 2)
  (put-text-property 11 14 'g 3)
  (put-text-property 16 19 'g 4)
  (put-text-property 21 24 'g 5)
  (let ((m1 (copy-marker 1 t)) (m2 (copy-marker 6 t))
        (m3 (copy-marker 11 t)) (m4 (copy-marker 16 t))
        (m5 (copy-marker 21 t)))
    (undo-boundary)
    (goto-char 1) (re-search-forward "AAAA" nil t) (replace-match "1111")
    (undo-boundary)
    (goto-char 1) (re-search-forward "BBBB" nil t) (replace-match "2222")
    (undo-boundary)
    (goto-char 1) (re-search-forward "CCCC" nil t) (replace-match "3333")
    (undo-boundary)
    (goto-char 1) (re-search-forward "DDDD" nil t) (replace-match "4444")
    (undo-boundary)
    (goto-char 1) (re-search-forward "EEEE" nil t) (replace-match "5555")
    (let ((s (buffer-string)))
      (primitive-undo 5 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "AAAA BBBB CCCC DDDD EEEE")
            (= (marker-position m1) 1)
            (= (marker-position m2) 6)
            (= (marker-position m3) 11)
            (= (marker-position m4) 16)
            (= (marker-position m5) 21)
            (get-text-property 1 'g) (= (get-text-property 1 'g) 1)
            (get-text-property 6 'g) (= (get-text-property 6 'g) 2)
            (get-text-property 11 'g) (= (get-text-property 11 'g) 3)
            (get-text-property 16 'g) (= (get-text-property 16 'g) 4)
            (get-text-property 21 'g) (= (get-text-property 21 'g) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_eieio_interop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-hash-obj ()
    ((key :initarg :key :accessor test-hash-key)
     (data :initarg :data :accessor test-hash-data)))
  (let ((ht (make-hash-table :test 'eql))
        (objs (cl-loop for i from 1 to 10
                       collect (make-instance 'test-hash-obj :key i :data (* i i)))))
    (dolist (o objs)
      (puthash (test-hash-key o) o ht))
    (let ((all-found t)
          (sum 0))
      (maphash (lambda (k v)
                 (unless (= (test-hash-key v) k)
                   (setq all-found nil))
                 (setq sum (+ sum (test-hash-data v))))
               ht)
      (list all-found
            (= sum 385)
            (= (hash-table-count ht) 10)
            (= (test-hash-data (gethash 5 ht)) 25)
            (= (test-hash-data (gethash 10 ht)) 100))))) "#,
        expect,
    );
}

#[test]
fn divergence_multi_buffer_undo_with_shared_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer "test-mb1"))
        (buf2 (generate-new-buffer "test-mb2")))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 4 'buf 'one)
      (put-text-property 6 9 'buf 'two)
      (put-text-property 11 13 'buf 'three)
      (let ((m (copy-marker 6 t))
            (ov (make-overlay 1 13)))
        (overlay-put ov 'buf 'first)
        (undo-boundary)
        (goto-char 6) (insert "XXX")
        (undo-boundary)
        (re-search-forward "BBBB" nil t) (replace-match "YYYY")
        (let ((s1 (buffer-string)))
          (with-current-buffer buf2
            (insert "DDDD-EEEE-FFFF")
            (put-text-property 1 4 'buf 'four)
            (put-text-property 6 9 'buf 'five)
            (put-text-property 11 13 'buf 'six)
            (undo-boundary)
            (goto-char 1) (re-search-forward "DDDD" nil t) (replace-match "GGGG")
            (let ((s2 (buffer-string)))
              (primitive-undo 1 buffer-undo-list)
              (list s1 s2
                    (buffer-string)
                    (string= (buffer-string) "DDDD-EEEE-FFFF")
                    (get-text-property 1 'buf) (eq (get-text-property 1 'buf) 'four)
                    (get-text-property 6 'buf) (eq (get-text-property 6 'buf) 'five)))))
        (primitive-undo 2 buffer-undo-list)
        (list (buffer-string)
              (string= (buffer-string) "AAAA-BBBB-CCCC")
              (get-text-property 1 'buf) (eq (get-text-property 1 'buf) 'one)
              (get-text-property 6 'buf) (eq (get-text-property 6 'buf) 'two)
              (marker-position m) (= (marker-position m) 6)
              (overlay-get ov 'buf) (eq (overlay-get ov 'buf) 'first))))
    (kill-buffer buf1)
    (kill-buffer buf2))) "#,
        expect,
    );
}
