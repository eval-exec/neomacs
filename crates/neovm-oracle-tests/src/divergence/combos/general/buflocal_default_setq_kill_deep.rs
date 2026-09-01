//! Deep combo: buffer-local + default-value + setq-default + kill-local-variable + with-current-buffer.
//! Tests variable scoping across buffer switches with local/default interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_make_variable_buffer_local_across_3_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar bv-test-var nil)\n\
         (make-variable-buffer-local 'bv-test-var)\n\
         (let ((b1 (generate-new-buffer \"bv1\"))\n\
         (b2 (generate-new-buffer \"bv2\"))\n\
         (b3 (generate-new-buffer \"bv3\")))\n\
         (setq-default bv-test-var 'global-default)\n\
         (with-current-buffer b1 (setq bv-test-var 'buf1-val))\n\
         (with-current-buffer b2 (setq bv-test-var 'buf2-val))\n\
         (let ((results (list\n\
         (cons 'default (default-value 'bv-test-var))\n\
         (cons 'b1 (buffer-local-value 'bv-test-var b1))\n\
         (cons 'b2 (buffer-local-value 'bv-test-var b2))\n\
         (cons 'b3 (buffer-local-value 'bv-test-var b3)))))\n\
         (with-current-buffer b2 (kill-local-variable 'bv-test-var))\n\
         (list results\n\
         (default-value 'bv-test-var)\n\
         (buffer-local-value 'bv-test-var b1)\n\
         (buffer-local-value 'bv-test-var b2)\n\
         (buffer-local-value 'bv-test-var b3))))\n\
         (kill-buffer b1) (kill-buffer b2) (kill-buffer b3)))",
        expect,
    );
}

#[test]
fn deficiency_setq_default_propagates_to_buffers_without_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar sdp-test-var 10)\n\
         (make-variable-buffer-local 'sdp-test-var)\n\
         (let ((b1 (generate-new-buffer \"sdp\"))\n\
         (b2 (generate-new-buffer \"sdp2\")))\n\
         (with-current-buffer b1 (setq sdp-test-var 100))\n\
         (setq-default sdp-test-var 50)\n\
         (list (default-value 'sdp-test-var)\n\
         (buffer-local-value 'sdp-test-var b1)\n\
         (buffer-local-value 'sdp-test-var b2)\n\
         (with-current-buffer b1\n\
         (kill-local-variable 'sdp-test-var)\n\
         sdp-test-var)\n\
         (default-value 'sdp-test-var)))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_local_value_in_closure_captures_correctly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar blc-var 0)\n\
         (make-variable-buffer-local 'blc-var)\n\
         (let ((b1 (generate-new-buffer \"blc\"))\n\
         (b2 (generate-new-buffer \"blc2\")))\n\
         (with-current-buffer b1 (setq blc-var 'alpha))\n\
         (with-current-buffer b2 (setq blc-var 'beta))\n\
         (let ((f1 (with-current-buffer b1\n\
         (lambda () blc-var)))\n\
         (f2 (with-current-buffer b2\n\
         (lambda () blc-var))))\n\
         (setq-default blc-var 'gamma)\n\
         (list (funcall f1)\n\
         (funcall f2)\n\
         (with-current-buffer b1 blc-var)\n\
         (with-current-buffer b2 blc-var))))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn deficiency_make_local_variable_vs_setq_default_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar mlv-var 'init)\n\
         (let ((b1 (generate-new-buffer \"mlv\"))\n\
         (b2 (generate-new-buffer \"mlv2\")))\n\
         (setq-default mlv-var 'default-val)\n\
         (with-current-buffer b1\n\
         (make-local-variable 'mlv-var)\n\
         (setq mlv-var 'local-val))\n\
         (list (default-value 'mlv-var)\n\
         (buffer-local-value 'mlv-var b1)\n\
         (buffer-local-value 'mlv-var b2)\n\
         (with-current-buffer b1\n\
         (kill-local-variable 'mlv-var)\n\
         mlv-var)\n\
         (buffer-local-value 'mlv-var b2))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_local_binding_with_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar bll-var 'outer)\n\
         (make-variable-buffer-local 'bll-var)\n\
         (let ((b1 (generate-new-buffer \"bll\")))\n\
         (with-current-buffer b1\n\
         (setq bll-var 'buf-local)\n\
         (let ((bll-var 'let-bound))\n\
         (list bll-var\n\
         (buffer-local-value 'bll-var b1))))\n\
         (list (buffer-local-value 'bll-var b1)\n\
         (default-value 'bll-var)))\n\
         (kill-buffer b1)))",
        expect,
    );
}

#[test]
fn deficiency_kill_local_variable_restores_default_in_multiple_rounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar klv-var 'original)\n\
         (make-variable-buffer-local 'klv-var)\n\
         (setq-default klv-var 'default-0)\n\
         (let ((b1 (generate-new-buffer \"klv\"))\n\
         (results nil))\n\
         (with-current-buffer b1\n\
         (setq klv-var 'round-1)\n\
         (push (cons 'set-1 klv-var) results)\n\
         (kill-local-variable 'klv-var)\n\
         (push (cons 'kill-1 klv-var) results)\n\
         (setq klv-var 'round-2)\n\
         (push (cons 'set-2 klv-var) results)\n\
         (kill-local-variable 'klv-var)\n\
         (push (cons 'kill-2 klv-var) results)\n\
         (setq-default klv-var 'new-default)\n\
         (push (cons 'after-default-change klv-var) results))\n\
         (nreverse results))\n\
         (kill-buffer b1)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_local_with_with_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (default temp-local outer-local default)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar bwt-var 'global)\n\
         (make-variable-buffer-local 'bwt-var)\n\
         (setq-default bwt-var 'default)\n\
         (let ((outer-buf (current-buffer)))\n\
         (with-current-buffer outer-buf (setq bwt-var 'outer-local))\n\
         (with-temp-buffer\n\
         (let ((temp-val bwt-var))\n\
         (setq bwt-var 'temp-local)\n\
         (list temp-val\n\
         bwt-var\n\
         (buffer-local-value 'bwt-var outer-buf)\n\
         (default-value 'bwt-var))))))",
        expect,
    );
}

#[test]
fn deficiency_default_value_and_set_after_buffer_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 42 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar dak-var 0)\n\
         (make-variable-buffer-local 'dak-var)\n\
         (setq-default dak-var 42)\n\
         (let ((b1 (generate-new-buffer \"dak\")))\n\
         (with-current-buffer b1 (setq dak-var 99))\n\
         (let ((before (buffer-local-value 'dak-var b1)))\n\
         (kill-buffer b1)\n\
         (list before\n\
         (default-value 'dak-var)\n\
         dak-var))))",
        expect,
    );
}

#[test]
fn deficiency_buffer_local_inherited_on_clone_indirect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar bli-var 'base)\n\
         (make-variable-buffer-local 'bli-var)\n\
         (let ((base (generate-new-buffer \"bli\")))\n\
         (with-current-buffer base\n\
         (setq bli-var 'base-val))\n\
         (let ((ind (make-indirect-buffer base \"bli-ind\")))\n\
         (list (buffer-local-value 'bli-var base)\n\
         (buffer-local-value 'bli-var ind)\n\
         (with-current-buffer ind bli-var)\n\
         (with-current-buffer ind\n\
         (setq bli-var 'ind-local)\n\
         (list bli-var\n\
         (buffer-local-value 'bli-var base)\n\
         (buffer-local-value 'bli-var ind))))\n\
         (kill-buffer ind)\n\
         (kill-buffer base))))",
        expect,
    );
}

#[test]
fn deficiency_multibyte_buffer_local_string_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar mbs-var \"default\")\n\
         (make-variable-buffer-local 'mbs-var)\n\
         (setq-default mbs-var \"\\xc3\\xa9\\xc3\\xa0\\xc3\\bc\")\n\
         (let ((b1 (generate-new-buffer \"mbs\"))\n\
         (b2 (generate-new-buffer \"mbs2\")))\n\
         (with-current-buffer b1\n\
         (setq mbs-var \"\\xe6\\x97\\xa5\\xe6\\x9c\\xac\\xe8\\xaa\\x9e\"))\n\
         (list (length (default-value 'mbs-var))\n\
         (length (buffer-local-value 'mbs-var b1))\n\
         (length (buffer-local-value 'mbs-var b2))\n\
         (substring (buffer-local-value 'mbs-var b1) 0 1)\n\
         (default-value 'mbs-var))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}
