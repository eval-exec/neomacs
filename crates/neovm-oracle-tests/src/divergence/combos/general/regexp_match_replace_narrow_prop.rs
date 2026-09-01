//! Deep combo: regexp + match-data + replace-match + props + narrowing + markers.
//! Tests search/replace operations in narrowed regions with property interference.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_re_search_forward_in_narrowed_with_prop_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rsn\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA-BBB-CCC-DDD-EEE-FFF\")\n\
         (dotimes (i 6)\n\
         (let ((s (+ 1 (* i 4))))\n\
         (put-text-property s (+ s 3) 'zone i)))\n\
         (narrow-to-region 5 21)\n\
         (goto-char (point-min))\n\
         (let ((hits nil))\n\
         (while (re-search-forward \"[A-Z]\\\\+\" nil t)\n\
         (push (list (match-string 0)\n\
         (match-beginning 0)\n\
         (match-end 0)\n\
         (get-text-property (match-beginning 0) 'zone))\n\
         hits))\n\
         (nreverse hits)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_replace_match_preserves_props_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rmp\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox jumps\")\n\
         (put-text-property 1 4 'pos 'start)\n\
         (put-text-property 5 10 'pos 'adj1)\n\
         (put-text-property 11 16 'pos 'adj2)\n\
         (narrow-to-region 5 16)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"quick\")\n\
         (replace-match \"slow\")\n\
         (list (buffer-string)\n\
         (cl-loop for i from (point-min) to (point-max)\n\
         collect (cons i (get-text-property i 'pos)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_match_data_with_multiple_searches_and_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mdm\")))\n\
         (with-current-buffer buf\n\
         (insert \"foo123bar456baz789\")\n\
         (put-text-property 1 4 'type 'word)\n\
         (put-text-property 4 7 'type 'num)\n\
         (put-text-property 7 10 'type 'word)\n\
         (put-text-property 10 13 'type 'num)\n\
         (goto-char 1)\n\
         (re-search-forward \"[0-9]+\")\n\
         (let ((m1 (match-string 0))\n\
         (mb1 (match-beginning 0))\n\
         (me1 (match-end 0)))\n\
         (re-search-forward \"[0-9]+\")\n\
         (let ((m2 (match-string 0))\n\
         (mb2 (match-beginning 0))\n\
         (me2 (match-end 0)))\n\
         (list m1 mb1 me1 m2 mb2 me2\n\
         (get-text-property mb1 'type)\n\
         (get-text-property mb2 'type)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_query_replace_regexp_simulation_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"qrr\")))\n\
         (with-current-buffer buf\n\
         (insert \"v1-v2-v3-v4-v5-v6-v7-v8\")\n\
         (dotimes (i 8)\n\
         (let ((s (+ 1 (* i 3))))\n\
         (put-text-property s (+ s 2) 'slot i)))\n\
         (narrow-to-region 4 22)\n\
         (goto-char (point-min))\n\
         (let ((replacements 0))\n\
         (while (re-search-forward \"v[0-9]\" nil t)\n\
         (replace-match (format \"r%d\" replacements))\n\
         (setq replacements (1+ replacements)))\n\
         (list replacements\n\
         (buffer-string)\n\
         (cl-loop for i from (point-min) to (point-max)\n\
         collect (cons i (get-text-property i 'slot))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_re_search_backward_in_narrowed_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rsb\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA.BBB.CCC.DDD.EEE\")\n\
         (dotimes (i 5)\n\
         (let ((s (+ 1 (* i 4))))\n\
         (put-text-property s (+ s 3) 'group i)))\n\
         (narrow-to-region 5 17)\n\
         (goto-char (point-max))\n\
         (let ((hits nil))\n\
         (while (re-search-backward \"[A-Z]\\\\+\" nil t)\n\
         (push (list (match-string 0)\n\
         (match-beginning 0)\n\
         (match-end 0)\n\
         (get-text-property (match-beginning 0) 'group))\n\
         hits))\n\
         hits))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_replace_with_backreference_in_prop_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rbr\")))\n\
         (with-current-buffer buf\n\
         (insert \"[hello] [world] [test]\")\n\
         (put-text-property 1 8 'bracket 'first)\n\
         (put-text-property 9 16 'bracket 'second)\n\
         (put-text-property 17 23 'bracket 'third)\n\
         (goto-char 1)\n\
         (re-search-forward \"\\\\[\\\\([a-z]+\\\\)\\\\]\")\n\
         (let ((m1 (match-string 1)))\n\
         (replace-match \"<\\\\1>\")\n\
         (let ((after1 (buffer-string)))\n\
         (re-search-forward \"\\\\[\\\\([a-z]+\\\\)\\\\]\")\n\
         (let ((m2 (match-string 1)))\n\
         (replace-match \"<\\\\1>\")\n\
         (list m1 m2 after1 (buffer-string)\n\
         (get-text-property 1 'bracket)\n\
         (get-text-property 9 'bracket))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_skip_chars_forward_backward_in_narrow_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"scn\")))\n\
         (with-current-buffer buf\n\
         (insert \"abc123def456ghi\")\n\
         (put-text-property 1 4 'class 'alpha)\n\
         (put-text-property 4 7 'class 'digit)\n\
         (put-text-property 7 10 'class 'alpha)\n\
         (put-text-property 10 13 'class 'digit)\n\
         (narrow-to-region 4 13)\n\
         (goto-char (point-min))\n\
         (skip-chars-forward \"0-9\")\n\
         (let ((p1 (point)))\n\
         (skip-chars-forward \"a-z\")\n\
         (let ((p2 (point)))\n\
         (skip-chars-backward \"0-9\")\n\
         (let ((p3 (point)))\n\
         (list p1 p2 p3\n\
         (buffer-string)\n\
         (get-text-property p1 'class)\n\
         (get-text-property p2 'class)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_looking_at_and_match_data_in_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"lan\")))\n\
         (with-current-buffer buf\n\
         (insert \"(defun foo (x) (+ x 1))\")\n\
         (put-text-property 1 7 'syntax 'keyword)\n\
         (put-text-property 8 11 'syntax 'name)\n\
         (narrow-to-region 8 21)\n\
         (goto-char (point-min))\n\
         (let ((r1 (looking-at \"[a-z]+\")))\n\
         (let ((m1 (match-string 0)))\n\
         (forward-char 4)\n\
         (let ((r2 (looking-at \" +\")))\n\
         (let ((m2 (match-string 0)))\n\
         (forward-char 2)\n\
         (let ((r3 (looking-at \"([^)]+)\")))\n\
         (list r1 m1 r2 m2 r3 (match-string 0)\n\
         (buffer-string)))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_re_search_with_save_match_data_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"smr\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha beta gamma delta\")\n\
         (goto-char 1)\n\
         (re-search-forward \"beta\")\n\
         (let ((saved-match (match-string 0))\n\
         (saved-begin (match-beginning 0))\n\
         (saved-end (match-end 0)))\n\
         (save-match-data\n\
         (re-search-forward \"delta\"))\n\
         (let ((restored-match (match-string 0))\n\
         (restored-begin (match-beginning 0))\n\
         (restored-end (match-end 0)))\n\
         (list saved-match saved-begin saved-end\n\
         restored-match restored-begin restored-end))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_regexp_whitespace_classes_in_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rwn\")))\n\
         (with-current-buffer buf\n\
         (insert \"word1  word2\\tword3  word4\")\n\
         (put-text-property 1 6 'w 1)\n\
         (put-text-property 8 13 'w 2)\n\
         (put-text-property 14 19 'w 3)\n\
         (put-text-property 21 26 'w 4)\n\
         (narrow-to-region 6 21)\n\
         (goto-char (point-min))\n\
         (let ((tokens nil))\n\
         (while (re-search-forward \"\\\\S-+\" nil t)\n\
         (push (list (match-string 0)\n\
         (match-beginning 0)\n\
         (get-text-property (match-beginning 0) 'w))\n\
         tokens))\n\
         (nreverse tokens)))\n\
         (kill-buffer buf)))",
        expect,
    );
}
