//! Deep stress: defmacro expansion + eval-and-compile + load + symbol combos + undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_defmacro_expansion_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defmacro with-tagged-insert (tag &rest body)\n\
         (list 'let '((start (point)))\n\
         (append (list 'progn) body)\n\
         (list 'put-text-property 'start '(point) ''tag tag)))\n\
         (let ((buf (generate-new-buffer \"dmb\")))\n\
         (with-current-buffer buf\n\
         (with-tagged-insert 'header\n\
         (insert \"HEADER TEXT\"))\n\
         (with-tagged-insert 'body\n\
         (insert \"BODY TEXT\"))\n\
         (with-tagged-insert 'footer\n\
         (insert \"FOOTER TEXT\"))\n\
         (list (buffer-string)\n\
         (get-text-property 1 'tag)\n\
         (get-text-property 12 'tag)\n\
         (get-text-property 21 'tag)\n\
         (= (buffer-size) 30))))\n\
         (kill-buffer (get-buffer \"dmb\")))",
        expect,
    );
}

#[test]
fn deficiency_eval_and_compile_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (eval-and-compile\n\
         (defun my-buffer-processor (buf fn)\n\
         (with-current-buffer buf\n\
         (let ((start (point)))\n\
         (funcall fn)\n\
         (put-text-property start (point) 'processed t))))\n\
         (defvar my-processor-version 1))\n\
         (let ((buf (generate-new-buffer \"eab\")))\n\
         (with-current-buffer buf\n\
         (my-buffer-processor buf (lambda () (insert \"PROCESSED\")))\n\
         (my-buffer-processor buf (lambda () (insert \" MORE\")))\n\
         (list (buffer-string)\n\
         (get-text-property 1 'processed)\n\
         (get-text-property 10 'processed)\n\
         my-processor-version)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eval_when_compile_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 7 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (eval-when-compile\n\
         (defvar my-compile-time-val 42))\n\
         (let ((buf (generate-new-buffer \"ewb\")))\n\
         (with-current-buffer buf\n\
         (insert (format \"value=%d\" my-compile-time-val))\n\
         (put-text-property 1 7 'field 'label)\n\
         (put-text-property 7 10 'field 'value)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (delete-region 7 10)\n\
         (insert \"99\")\n\
         (put-text-property 7 9 'field 'value)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'field))\n\
         (f7 (get-text-property 7 'field)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s f1 f7\n\
         (buffer-string)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 7 'field)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_symbol_function_plist_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-test-fn () \"test\")\n\
         (put 'my-test-fn 'custom 'data)\n\
         (let ((buf (generate-new-buffer \"sfp\")))\n\
         (with-current-buffer buf\n\
         (insert (format \"fn=%S plist=%S\"\n\
         (symbol-function 'my-test-fn)\n\
         (symbol-plist 'my-test-fn)))\n\
         (put-text-property 1 4 'kind 'fn)\n\
         (put-text-property 5 17 'kind 'plist)\n\
         (undo-boundary)\n\
         (put 'my-test-fn 'extra 'more)\n\
         (erase-buffer)\n\
         (insert (format \"fn=%S plist=%S\"\n\
         (symbol-function 'my-test-fn)\n\
         (symbol-plist 'my-test-fn)))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s\n\
         (buffer-string)\n\
         (get 'my-test-fn 'custom)\n\
         (get 'my-test-fn 'extra)))))\n\
         (kill-buffer buf)\n\
         (fmakunbound 'my-test-fn)))",
        expect,
    );
}

#[test]
fn deficiency_obarray_mapatoms_buffer_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable text)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((my-ob (make-vector 31 0))\n\
         (buf (generate-new-buffer \"omb\")))\n\
         (intern \"alpha\" my-ob)\n\
         (intern \"beta\" my-ob)\n\
         (intern \"gamma\" my-ob)\n\
         (intern \"delta\" my-ob)\n\
         (intern \"epsilon\" my-ob)\n\
         (with-current-buffer buf\n\
         (mapatoms (lambda (sym)\n\
         (insert (format \"%s \" sym)))\n\
         my-ob)\n\
         (let ((text (buffer-string))\n\
         (words (split-string text \" +\" t)))\n\
         (let ((sorted (sort words #'string<)))\n\
         (list sorted\n\
         (= (length sorted) 5)\n\
         (equal sorted '(\"alpha\" \"beta\" \"delta\" \"epsilon\" \"gamma\"))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_gensym_uniqueness_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((syms (cl-loop for i from 1 to 5 collect (gensym \"g\"))))\n\
         (let ((buf (generate-new-buffer \"gub\")))\n\
         (with-current-buffer buf\n\
         (dolist (s syms)\n\
         (insert (format \"%s \" s))\n\
         (put-text-property\n\
         (- (point) (length (format \"%s \" s)))\n\
         (1- (point))\n\
         'symbol s))\n\
         (let ((all-unique (= (length (delete-dups (copy-sequence syms)))\n\
         (length syms))))\n\
         (list (buffer-string)\n\
         all-unique\n\
         (= (length syms) 5)\n\
         (get-text-property 1 'symbol)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_defsubst_buffer_call_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defsubst my-insert-tagged (tag text)\n\
         (let ((start (point)))\n\
         (insert text)\n\
         (put-text-property start (point) 'tag tag)))\n\
         (let ((buf (generate-new-buffer \"dsb\")))\n\
         (with-current-buffer buf\n\
         (my-insert-tagged 'first \"AAAA\")\n\
         (my-insert-tagged 'second \"BBBB\")\n\
         (my-insert-tagged 'third \"CCCC\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 9)\n\
         (insert \"XXXX\")\n\
         (put-text-property 5 9 'tag 'modified)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (get-text-property 1 'tag))\n\
         (t5 (get-text-property 5 'tag))\n\
         (t9 (get-text-property 9 'tag)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s t1 t5 t9\n\
         (buffer-string)\n\
         (get-text-property 1 'tag)\n\
         (get-text-property 5 'tag)\n\
         (get-text-property 9 'tag)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_compiler_macro_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable executed)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defmacro my-with-buffer-props (buf &rest body)\n\
         (list 'with-current-buffer buf\n\
         (list 'let '((my-start (point)))\n\
         (append (list 'progn) body)\n\
         (list 'put-text-property 'my-start '(point) 'executed t))))\n\
         (let ((buf (generate-new-buffer \"cmb\")))\n\
         (my-with-buffer-props buf\n\
         (insert \"FIRST\")\n\
         (put-text-property 1 6 'order 1))\n\
         (my-with-buffer-props buf\n\
         (insert \"SECOND\")\n\
         (put-text-property 6 12 'order 2))\n\
         (with-current-buffer buf\n\
         (list (buffer-string)\n\
         (get-text-property 1 'executed)\n\
         (get-text-property 1 'order)\n\
         (get-text-property 6 'order))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_recursive_macro_buffer_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defmacro insert-numbered-line (num text)\n\
         (list 'progn\n\
         (list 'insert (format \"[%d] \" num))\n\
         (list 'insert text)\n\
         (list 'insert \"\\n\")))\n\
         (let ((buf (generate-new-buffer \"rmb\"))\n\
         (items '(\"alpha\" \"beta\" \"gamma\" \"delta\")))\n\
         (with-current-buffer buf\n\
         (cl-loop for item in items\n\
         for i from 1\n\
         do (insert-numbered-line i item))\n\
         (put-text-property 1 20 'section 'content)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'section)\n\
         (= (count-lines 1 (point-max)) 4))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_macro_with_undo_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defmacro my-atomic-insert (&rest body)\n\
         (list 'progn\n\
         '(undo-boundary)\n\
         (append (list 'progn) body)\n\
         '(undo-boundary)))\n\
         (let ((buf (generate-new-buffer \"mab\")))\n\
         (with-current-buffer buf\n\
         (my-atomic-insert\n\
         (insert \"AAA\")\n\
         (put-text-property 1 4 'batch 1))\n\
         (my-atomic-insert\n\
         (insert \"BBB\")\n\
         (put-text-property 4 7 'batch 2))\n\
         (my-atomic-insert\n\
         (insert \"CCC\")\n\
         (put-text-property 7 10 'batch 3))\n\
         (let ((ul (length buffer-undo-list))\n\
         (scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'batch))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ul scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'batch))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
