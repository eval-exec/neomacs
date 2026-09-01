//! Deep stress: marker + point + region + undo across narrow/widen cycles.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_marker_after_narrow_widen_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 7 7)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mnu\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFFGGG\")\n\
         (put-text-property 1 4 'grp 'a)\n\
         (put-text-property 4 7 'grp 'b)\n\
         (put-text-property 7 10 'grp 'c)\n\
         (put-text-property 10 13 'grp 'd)\n\
         (put-text-property 13 16 'grp 'e)\n\
         (put-text-property 16 19 'grp 'f)\n\
         (put-text-property 19 22 'grp 'g)\n\
         (let ((ma (copy-marker 2))\n\
         (mb (copy-marker 6))\n\
         (mc (copy-marker 12))\n\
         (md (copy-marker 20)))\n\
         (undo-boundary)\n\
         (narrow-to-region 4 19)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"XX\")\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"YY\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (pa (marker-position ma))\n\
         (pb (marker-position mb))\n\
         (pc (marker-position mc))\n\
         (pd (marker-position md)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s pa pb pc pd\n\
         (buffer-string)\n\
         (marker-position ma)\n\
         (marker-position mb)\n\
         (marker-position mc)\n\
         (marker-position md)\n\
         (get-text-property 1 'grp)\n\
         (get-text-property 7 'grp))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_relocate_after_kill_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mrk\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (let ((m1 (copy-marker 3))\n\
         (m2 (copy-marker 7))\n\
         (m3 (copy-marker 10)))\n\
         (undo-boundary)\n\
         (kill-region 3 7)\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (p1 (marker-position m1))\n\
         (p2 (marker-position m2))\n\
         (p3 (marker-position m3)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s1 p1 p2 p3\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-position m3))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_point_marker_after_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pmr\")))\n\
         (with-current-buffer buf\n\
         (insert \"function hello() { return true; }\")\n\
         (put-text-property 1 9 'token 'keyword)\n\
         (put-text-property 10 15 'token 'ident)\n\
         (put-text-property 16 22 'token 'keyword)\n\
         (put-text-property 23 27 'token 'value)\n\
         (let ((mp (copy-marker 23)))\n\
         (undo-boundary)\n\
         (goto-char 23)\n\
         (re-search-forward \"true\")\n\
         (replace-match \"false\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p (marker-position mp))\n\
         (t23 (get-text-property 23 'token)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s p t23\n\
         (buffer-string)\n\
         (marker-position mp)\n\
         (get-text-property 23 'token))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_in_visible_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mvr\")))\n\
         (with-current-buffer buf\n\
         (insert \"SHOW1HIDE1SHOW2HIDE2SHOW3\")\n\
         (put-text-property 1 6 'vis t)\n\
         (put-text-property 6 11 'vis nil)\n\
         (put-text-property 11 16 'vis t)\n\
         (put-text-property 16 21 'vis nil)\n\
         (put-text-property 21 26 'vis t)\n\
         (let ((m-hide1-start (copy-marker 6))\n\
         (m-hide1-end (copy-marker 11))\n\
         (m-show2-start (copy-marker 11)))\n\
         (undo-boundary)\n\
         (delete-region 6 11)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ph1s (marker-position m-hide1-start))\n\
         (ph1e (marker-position m-hide1-end))\n\
         (ps2s (marker-position m-show2-start)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s ph1s ph1e ps2s\n\
         (buffer-string)\n\
         (marker-position m-hide1-start)\n\
         (marker-position m-hide1-end)\n\
         (marker-position m-show2-start)\n\
         (get-text-property 6 'vis)\n\
         (get-text-property 11 'vis))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_markers_different_insert_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mmd\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAAAAAAAAAA\")\n\
         (let ((m-nil (copy-marker 5))\n\
         (m-t (copy-marker 5))\n\
         (m-nil2 (copy-marker 10))\n\
         (m-t2 (copy-marker 10)))\n\
         (set-marker-insertion-type m-nil nil)\n\
         (set-marker-insertion-type m-t t)\n\
         (set-marker-insertion-type m-nil2 nil)\n\
         (set-marker-insertion-type m-t2 t)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"CCCC\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p-nil (marker-position m-nil))\n\
         (p-t (marker-position m-t))\n\
         (p-nil2 (marker-position m-nil2))\n\
         (p-t2 (marker-position m-t2)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s p-nil p-t p-nil2 p-t2\n\
         (buffer-string)\n\
         (marker-position m-nil)\n\
         (marker-position m-t)\n\
         (marker-position m-nil2)\n\
         (marker-position m-t2))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_set_buffer_dead_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"sb1\"))\n\
         (buf2 (generate-new-buffer \"sb2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"BUFFER1\")\n\
         (let ((m (copy-marker 4)))\n\
         (set-marker m 5 buf2)\n\
         (list (marker-position m)\n\
         (marker-buffer m)\n\
         (eq (marker-buffer m) buf2)\n\
         (with-current-buffer buf2\n\
         (insert \"BUFFER2\")\n\
         (= (marker-position m) 0))\n\
         (set-marker m nil)\n\
         (marker-position m))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}

#[test]
fn deficiency_region_narrow_marker_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rni\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA BBB CCC DDD EEE\")\n\
         (put-text-property 1 4 'word 1)\n\
         (put-text-property 5 8 'word 2)\n\
         (put-text-property 9 12 'word 3)\n\
         (put-text-property 13 16 'word 4)\n\
         (put-text-property 17 20 'word 5)\n\
         (let ((m1 (copy-marker 5))\n\
         (m2 (copy-marker 13)))\n\
         (undo-boundary)\n\
         (narrow-to-region 5 16)\n\
         (let ((s (buffer-string))\n\
         (min (point-min))\n\
         (max (point-max)))\n\
         (goto-char min)\n\
         (insert \"XX\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s2 (buffer-string))\n\
         (p1 (marker-position m1))\n\
         (p2 (marker-position m2)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s min max s2 p1 p2\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (get-text-property 5 'word)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_after_goto_char_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mgu\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789\")\n\
         (let ((markers (cl-loop for i from 1 to 10 collect (copy-marker i))))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"AAA\")\n\
         (undo-boundary)\n\
         (goto-char 8)\n\
         (insert \"BBB\")\n\
         (undo-boundary)\n\
         (let ((pos-before (mapcar #'marker-position markers)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (let ((pos-after (mapcar #'marker-position markers)))\n\
         (list pos-before pos-after\n\
         (buffer-string)\n\
         (= (buffer-size) 10)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_point_max_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pma\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA\")\n\
         (let ((pm1 (point-max)))\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"BBBB\")\n\
         (let ((pm2 (point-max)))\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"CCCC\")\n\
         (let ((pm3 (point-max)))\n\
         (undo-boundary)\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list pm1 pm2 pm3\n\
         (point-max)\n\
         (buffer-string)\n\
         (= (point-max) pm1)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_at_point_min_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mpm\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (let ((m0 (copy-marker 1))\n\
         (m1 (copy-marker (point-min)))\n\
         (m2 (copy-marker (point-max))))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (undo-boundary)\n\
         (let ((p0 (marker-position m0))\n\
         (p1 (marker-position m1))\n\
         (p2 (marker-position m2)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list p0 p1 p2\n\
         (buffer-string)\n\
         (marker-position m0)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (= (point-min) 1)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
