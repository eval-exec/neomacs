//! Deep combo: catch/throw + unwind-protect + condition-case + buffer state + markers.
//! Tests nested non-local exits combined with buffer mutations to surface cleanup divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_catch_throw_through_nested_unwind_protect_with_buffer_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ctu\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (put-text-property 1 6 'tag 'front)\n\
         (put-text-property 6 11 'tag 'back)\n\
         (let ((m3 (copy-marker 3))\n\
         (m7 (copy-marker 7)))\n\
         (catch 'abort\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 5)\n\
         (insert \"XXX\")\n\
         (put-text-property 5 8 'tag 'inserted)\n\
         (delete-region 10 13)\n\
         (throw 'abort (list 'thrown (buffer-string)\n\
         (get-text-property 5 'tag)\n\
         (get-text-property 9 'tag)\n\
         (marker-position m3)\n\
         (marker-position m7))))\n\
         (goto-char (point-min))\n\
         (insert \"CLEANUP\"))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_condition_case_with_buffer_mods_and_marker_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-error)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ccb\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789\")\n\
         (let ((m2 (copy-marker 2))\n\
         (m5 (copy-marker 5))\n\
         (m8 (copy-marker 8)))\n\
         (put-text-property 1 6 'zone 'alpha)\n\
         (put-text-property 6 11 'zone 'beta)\n\
         (condition-case err\n\
         (progn\n\
         (delete-region 3 7)\n\
         (put-text-property 1 7 'zone 'merged)\n\
         (signal 'test-error '(\"boom\")))\n\
         (test-error\n\
         (list (cadr err)\n\
         (buffer-string)\n\
         (marker-position m2)\n\
         (marker-position m5)\n\
         (marker-position m8)\n\
         (get-text-property 1 'zone)\n\
         (get-text-property 5 'zone))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_nested_catch_with_insertion_type_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nci\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (let ((m1 (copy-marker 3))\n\
         (m2 (copy-marker 3 t)))\n\
         (set-marker-insertion-type m1 nil)\n\
         (set-marker-insertion-type m2 t)\n\
         (catch 'inner\n\
         (catch 'outer\n\
         (goto-char 3)\n\
         (insert \"XX\")\n\
         (throw 'inner (list 'inner-caught\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-insertion-type m1)\n\
         (marker-insertion-type m2))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_unwind_protect_restores_point_after_error_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-error)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upn\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDDEEEE\")\n\
         (dotimes (i 5)\n\
         (let ((s (+ 1 (* i 4))))\n\
         (put-text-property s (+ s 4) 'block i)))\n\
         (narrow-to-region 5 17)\n\
         (let ((saved-point nil)\n\
         (saved-narrow nil))\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 8)\n\
         (setq saved-point (point))\n\
         (widen)\n\
         (setq saved-narrow (list (point-min) (point-max)))\n\
         (signal 'test-error '(\"narrow-test\")))\n\
         (list saved-point saved-narrow\n\
         (buffer-string)\n\
         (get-text-property 1 'block)\n\
         (get-text-property 8 'block)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_throw_across_buffer_switch_with_overlay_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"ob1\"))\n\
         (buf2 (generate-new-buffer \"ob2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"CONTENT-ONE\")\n\
         (let ((ov (make-overlay 1 9)))\n\
         (overlay-put ov 'role 'primary)\n\
         (put-text-property 1 9 'src 'buf1)\n\
         (catch 'switch\n\
         (with-current-buffer buf2\n\
         (insert \"CONTENT-TWO\")\n\
         (let ((ov2 (make-overlay 1 9)))\n\
         (overlay-put ov2 'role 'secondary)\n\
         (throw 'switch\n\
         (list 'from-buf2\n\
         (buffer-string)\n\
         (overlay-get ov2 'role)\n\
         (with-current-buffer buf1\n\
         (overlay-get ov 'role)))))))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}

#[test]
fn deficiency_deeply_nested_condition_case_with_prop_interval_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" inner-signal)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"dnc\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")\n\
         (dotimes (i 13)\n\
         (let ((s (+ 1 (* i 2))))\n\
         (put-text-property s (+ s 2) 'idx i)))\n\
         (condition-case outer-err\n\
         (condition-case inner-err\n\
         (progn\n\
         (delete-region 5 10)\n\
         (put-text-property 5 15 'idx 'deleted-zone)\n\
         (signal 'inner-signal '(\"inner\")))\n\
         (inner-signal\n\
         (progn\n\
         (goto-char 1)\n\
         (insert \"RESTORED\")\n\
         (signal 'outer-signal '(\"outer\")))))\n\
         (outer-signal\n\
         (list (cadr outer-err)\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'idx))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_catch_from_within_mapcar_with_buffer_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cmw\")))\n\
         (with-current-buffer buf\n\
         (insert \"-----\"  )\n\
         (let ((results (catch 'done\n\
         (mapcar (lambda (n)\n\
         (goto-char (point-max))\n\
         (insert (number-to-string n))\n\
         (put-text-property (- (point) 1) (point) 'val n)\n\
         (when (= n 7)\n\
         (throw 'done\n\
         (list 'early-exit n\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'val))))))\n\
         '(1 2 3 4 5 6 7 8 9)))))\n\
         results)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_unwind_protect_in_recursive_edit_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" recursive-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urs\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA-BBBB-CCCC-DDDD\")\n\
         (let ((ov1 (make-overlay 1 5))\n\
         (ov2 (make-overlay 10 14))\n\
         (cleanup-log nil))\n\
         (overlay-put ov1 'layer 1)\n\
         (overlay-put ov2 'layer 2)\n\
         (put-text-property 1 5 'grp 'a)\n\
         (put-text-property 6 10 'grp 'sep)\n\
         (put-text-property 10 14 'grp 'b)\n\
         (put-text-property 14 19 'grp 'tail)\n\
         (let ((result\n\
         (unwind-protect\n\
         (progn\n\
         (delete-region 6 10)\n\
         (goto-char 6)\n\
         (insert \"XXXX\")\n\
         (put-text-property 6 10 'grp 'replaced)\n\
         (signal 'recursive-exit '(\"simulated\")))\n\
         (push (list 'cleanup\n\
         (buffer-string)\n\
         (overlay-start ov1)\n\
         (overlay-end ov1)\n\
         (overlay-start ov2)\n\
         (overlay-end ov2)\n\
         (get-text-property 6 'grp))\n\
         cleanup-log))))\n\
         (list result cleanup-log)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_throw_past_dolist_with_overlay_evaporation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tdo\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFF\")\n\
         (let ((ovs (cl-loop for i from 0 to 5\n\
         collect (let ((s (+ 1 (* i 3)))\n\
         (ov (make-overlay (+ 1 (* i 3)) (+ 4 (* i 3)))))\n\
         (overlay-put ov 'evaporate t)\n\
         (overlay-put ov 'idx i)\n\
         ov))))\n\
         (catch 'bail\n\
         (dolist (ov ovs)\n\
         (let ((idx (overlay-get ov 'idx)))\n\
         (when (= idx 3)\n\
         (throw 'bail\n\
         (list 'bailed-at idx\n\
         (cl-loop for o in ovs\n\
         collect (if (overlay-start o)\n\
         (list (overlay-get o 'idx)\n\
         (overlay-start o)\n\
         (overlay-end o))\n\
         (list (overlay-get o 'idx) 'gone)))))))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_unwind_protect_layers_with_undo_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mul\")))\n\
         (with-current-buffer buf\n\
         (insert \"INITIAL-TEXT\")\n\
         (put-text-property 1 8 'section 'head)\n\
         (put-text-property 8 13 'section 'tail)\n\
         (let ((m5 (copy-marker 5))\n\
         (layer1-result nil)\n\
         (layer2-result nil))\n\
         (undo-boundary)\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 5)\n\
         (insert \"MMM\")\n\
         (put-text-property 5 8 'section 'mid1)\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 11)\n\
         (delete-region 11 14)\n\
         (setq layer2-result (list 'l2 (buffer-string)\n\
         (get-text-property 5 'section)))\n\
         (signal 'test-error '(\"layer2\")))\n\
         (setq layer1-result (list 'l1-cleanup (buffer-string)\n\
         (marker-position m5)\n\
         (get-text-property 1 'section)\n\
         (get-text-property 8 'section)))))\n\
         (push 'final cleanup: done))\n\
         (list layer1-result layer2-result\n\
         (buffer-string)\n\
         (marker-position m5)\n\
         (get-text-property 1 'section)\n\
         (get-text-property 8 'section))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
