//! Combo: syntax table + regex search + overlay + textprop + narrow + markers.
//! Tests how modifying syntax table entries affects regex search inside narrowed
//! regions with overlays and text properties present.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_syntax_mod_regex_search_narrow_overlay_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sro\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello-world_test.foo@bar\")\n\
         (let ((st (copy-syntax-table)))\n\
         (modify-syntax-entry ?- \"w\" st)\n\
         (modify-syntax-entry ?_ \"w\" st)\n\
         (with-syntax-table st\n\
         (let ((ov (make-overlay 1 11)))\n\
         (overlay-put ov 'face 'bold)\n\
         (put-text-property 1 6 'zone 'first)\n\
         (put-text-property 7 12 'zone 'second)\n\
         (narrow-to-region 1 12)\n\
         (goto-char (point-min))\n\
         (let* ((r1 (progn (re-search-forward \"\\\\sw+\" nil t) (match-string 0)))\n\
         (r2 (progn (re-search-forward \"\\\\sw+\" nil t) (match-string 0)))\n\
         (p1 (match-beginning 1))\n\
         (p2 (match-end 1))\n\
         (z1 (get-text-property p1 'zone))\n\
         (z2 (get-text-property p2 'zone)))\n\
         (widen)\n\
         (list r1 r2 p1 p2 z1 z2\n\
         (overlay-start ov) (overlay-end ov))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_syntax_change_regex_capture_groups_with_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"scg\")))\n\
         (with-current-buffer buf\n\
         (insert \"foo-bar baz-qux\")\n\
         (let ((st (copy-syntax-table))\n\
         (m (make-marker)))\n\
         (modify-syntax-entry ?- \"_\" st)\n\
         (with-syntax-table st\n\
         (goto-char (point-min))\n\
         (re-search-forward \"\\\\(\\\\w+\\\\)-\\\\(\\\\w+\\\\)\")\n\
         (set-marker m (match-end 1))\n\
         (let ((g1 (match-string 1))\n\
         (g2 (match-string 2))\n\
         (mp (marker-position m)))\n\
         (replace-match \"\\\\1_\\\\2\")\n\
         (list g1 g2 mp\n\
         (buffer-string)\n\
         (marker-position m)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_syntax_overlay_narrow_multi_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"som\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha.beta gamma.delta epsilon.zeta\")\n\
         (let ((st (copy-syntax-table)))\n\
         (modify-syntax-entry ?. \"w\" st)\n\
         (with-syntax-table st\n\
         (let ((ov1 (make-overlay 7 18))\n\
         (ov2 (make-overlay 19 33)))\n\
         (overlay-put ov1 'region t)\n\
         (overlay-put ov2 'region t)\n\
         (put-text-property 1 6 'part 'a)\n\
         (put-text-property 7 12 'part 'b)\n\
         (put-text-property 13 18 'part 'c)\n\
         (put-text-property 19 27 'part 'd)\n\
         (put-text-property 28 33 'part 'e)\n\
         (narrow-to-region 7 33)\n\
         (goto-char (point-min))\n\
         (let ((hits nil))\n\
         (while (re-search-forward \"\\\\w+\" nil t)\n\
         (push (list (match-string 0)\n\
         (match-beginning 0)\n\
         (get-text-property (match-beginning 0) 'part))\n\
         hits))\n\
         (widen)\n\
         (list (nreverse hits)\n\
         (overlay-start ov1)\n\
         (overlay-end ov2))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_syntax_undo_replace_with_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sur\")))\n\
         (with-current-buffer buf\n\
         (insert \"aaa-bbb ccc-ddd\")\n\
         (let ((st (copy-syntax-table))\n\
         (m1 (make-marker))\n\
         (m2 (make-marker)))\n\
         (modify-syntax-entry ?- \"w\" st)\n\
         (with-syntax-table st\n\
         (let ((ov (make-overlay 1 8)))\n\
         (overlay-put ov 'type 'special)\n\
         (set-marker m1 4)\n\
         (set-marker m2 8)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"aaa-bbb\")\n\
         (replace-match \"111-222\")\n\
         (let ((bs1 (buffer-string))\n\
         (mp1 (marker-position m1))\n\
         (mp2 (marker-position m2)))\n\
         (undo-boundary)\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list bs1 mp1 mp2\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (overlay-start ov)\n\
         (overlay-end ov))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_syntax_textprop_replace_match_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stm\")))\n\
         (with-current-buffer buf\n\
         (insert \"aaXbbYccZdd\")\n\
         (let ((st (copy-syntax-table)))\n\
         (modify-syntax-entry ?X \"w\" st)\n\
         (modify-syntax-entry ?Y \"w\" st)\n\
         (modify-syntax-entry ?Z \"w\" st)\n\
         (with-syntax-table st\n\
         (put-text-property 1 4 'kind 'pre)\n\
         (put-text-property 4 7 'kind 'mid)\n\
         (put-text-property 7 11 'kind 'post)\n\
         (let ((m (make-marker)))\n\
         (set-marker m 6)\n\
         (narrow-to-region 1 7)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"aaXbbY\")\n\
         (replace-match \"AA-BB\")\n\
         (let ((bs (buffer-string))\n\
         (mp (marker-position m))\n\
         (k (get-text-property 1 'kind))\n\
         (k2 (get-text-property 4 'kind)))\n\
         (widen)\n\
         (list bs mp k k2\n\
         (buffer-string)\n\
         (marker-position m))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
