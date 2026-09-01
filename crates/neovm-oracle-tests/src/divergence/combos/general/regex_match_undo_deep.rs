//! Deep stress: regex + match-data + save-match-data + undo + textprop + overlay combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_save_match_data_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"smu\")))\n\
         (with-current-buffer buf\n\
         (insert \"foo123bar456baz789\")\n\
         (put-text-property 1 4 'type 'word)\n\
         (put-text-property 4 7 'type 'num)\n\
         (put-text-property 7 10 'type 'word)\n\
         (put-text-property 10 13 'type 'num)\n\
         (put-text-property 13 16 'type 'word)\n\
         (put-text-property 16 19 'type 'num)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"[0-9]+\")\n\
         (let ((m1 (match-data)))\n\
         (save-match-data\n\
         (re-search-forward \"[0-9]+\")\n\
         (let ((m2 (match-data)))\n\
         (replace-match \"XXX\")\n\
         (undo-boundary)\n\
         (list m1 m2 (buffer-string)))))\n\
         (list (match-data) (buffer-string))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 7 'type)\n\
         (get-text-property 13 'type))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_re_search_replace_propertize_undo_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 46 51)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rrp\"))\n\
         (match-log nil))\n\
         (with-current-buffer buf\n\
         (insert \"var1 = 100; var2 = 200; var3 = 300; var4 = 400\")\n\
         (put-text-property 1 6 'kind 'var)\n\
         (put-text-property 7 12 'kind 'assign)\n\
         (put-text-property 12 15 'kind 'val)\n\
         (put-text-property 16 21 'kind 'var)\n\
         (put-text-property 22 27 'kind 'assign)\n\
         (put-text-property 27 30 'kind 'val)\n\
         (put-text-property 31 36 'kind 'var)\n\
         (put-text-property 37 42 'kind 'assign)\n\
         (put-text-property 42 45 'kind 'val)\n\
         (put-text-property 46 51 'kind 'var)\n\
         (put-text-property 52 57 'kind 'assign)\n\
         (put-text-property 57 60 'kind 'val)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (while (re-search-forward \"var\\\\([0-9]+\\\\)\" nil t)\n\
         (let ((num (match-string 1)))\n\
         (push (list (match-beginning 0) (match-end 0) num) match-log)\n\
         (replace-match (concat \"variable_\" num))))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (k1 (get-text-property 1 'kind))\n\
         (k5 (get-text-property 5 'kind))\n\
         (k15 (get-text-property 15 'kind)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s k1 k5 k15\n\
         (buffer-string)\n\
         (get-text-property 1 'kind)\n\
         (get-text-property 7 'kind)\n\
         (get-text-property 12 'kind)\n\
         (length (nreverse match-log))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_match_data_across_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mnu\")))\n\
         (with-current-buffer buf\n\
         (insert \"aaa bbb ccc ddd eee fff ggg hhh\")\n\
         (put-text-property 1 4 'grp 1)\n\
         (put-text-property 5 8 'grp 2)\n\
         (put-text-property 9 12 'grp 3)\n\
         (put-text-property 13 16 'grp 4)\n\
         (put-text-property 17 20 'grp 5)\n\
         (put-text-property 21 24 'grp 6)\n\
         (put-text-property 25 28 'grp 7)\n\
         (put-text-property 29 32 'grp 8)\n\
         (undo-boundary)\n\
         (narrow-to-region 5 20)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"bbb\")\n\
         (let ((m-bbb (match-data)))\n\
         (replace-match \"BBB\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (g5 (get-text-property 5 'grp))\n\
         (g8 (get-text-property 8 'grp)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list m-bbb s g5 g8\n\
         (buffer-string)\n\
         (get-text-property 5 'grp)\n\
         (get-text-property 8 'grp))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_re_place_match_overlay_bounds_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rpm\")))\n\
         (with-current-buffer buf\n\
         (insert \"[link text](url) and [more text](url2)\")\n\
         (let ((ovs nil))\n\
         (goto-char 1)\n\
         (while (re-search-forward \"\\\\[\\\\([^]]*\\\\)\\\\](\\\\([^)]*\\\\))\" nil t)\n\
         (let ((ov (make-overlay (match-beginning 0) (match-end 0))))\n\
         (overlay-put ov 'link-text (match-string 1))\n\
         (overlay-put ov 'link-url (match-string 2))\n\
         (push ov ovs)))\n\
         (let ((ov-data (mapcar (lambda (ov)\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'link-text)\n\
         (overlay-get ov 'link-url)))\n\
         (nreverse ovs))))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (while (re-search-forward \"\\\\[\\\\([^]]*\\\\)\\\\](\\\\([^)]*\\\\))\" nil t)\n\
         (replace-match \"\\\\1\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ov-data s\n\
         (buffer-string)\n\
         (mapcar (lambda (ov)\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'link-text)\n\
         (overlay-get ov 'link-url)))\n\
         (nreverse ovs)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_looking_back_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"lbw\")))\n\
         (with-current-buffer buf\n\
         (insert \"function hello() { return true; }\")\n\
         (put-text-property 1 9 'type 'keyword)\n\
         (put-text-property 10 15 'type 'ident)\n\
         (put-text-property 16 22 'type 'keyword)\n\
         (put-text-property 23 27 'type 'value)\n\
         (undo-boundary)\n\
         (goto-char 23)\n\
         (let ((before (looking-back \"return \" nil)))\n\
         (re-search-forward \"true\")\n\
         (replace-match \"false\")\n\
         (undo-boundary)\n\
         (let ((after-look (looking-back \"false\" nil))\n\
         (s (buffer-string))\n\
         (t23 (get-text-property 23 'type)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list before after-look s t23\n\
         (buffer-string)\n\
         (get-text-property 23 'type))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_skip_chars_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 20 25)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"scw\")))\n\
         (with-current-buffer buf\n\
         (insert \"    \\t  hello world  \\t  \")\n\
         (put-text-property 1 8 'type 'whitespace)\n\
         (put-text-property 8 14 'type 'word)\n\
         (put-text-property 14 15 'type 'space)\n\
         (put-text-property 15 20 'type 'word)\n\
         (put-text-property 20 25 'type 'whitespace)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (skip-chars-forward \" \\t\")\n\
         (let ((after-skip (point)))\n\
         (insert \"(\")\n\
         (goto-char (point-max))\n\
         (skip-chars-backward \" \\t\")\n\
         (insert \")\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list after-skip s\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 8 'type))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_capture_groups_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 34 51)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mcg\")))\n\
         (with-current-buffer buf\n\
         (insert \"2024-01-15 10:30:45 ERROR [main] Something failed\")\n\
         (put-text-property 1 11 'field 'date)\n\
         (put-text-property 12 20 'field 'time)\n\
         (put-text-property 21 26 'field 'level)\n\
         (put-text-property 27 33 'field 'module)\n\
         (put-text-property 34 51 'field 'message)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"\\\\([0-9]+\\\\)-\\\\([0-9]+\\\\)-\\\\([0-9]+\\\\)\")\n\
         (let ((yr (match-string 1))\n\
         (mo (match-string 2))\n\
         (dy (match-string 3)))\n\
         (replace-match (concat mo \"/\" dy \"/\" yr))\n\
         (undo-boundary)\n\
         (re-search-forward \"\\\\([0-9]+\\\\):\\\\([0-9]+\\\\):\\\\([0-9]+\\\\)\")\n\
         (let ((hr (match-string 1))\n\
         (mn (match-string 2))\n\
         (sc (match-string 3)))\n\
         (replace-match (concat hr \"h\" mn \"m\" sc \"s\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'field))\n\
         (f12 (get-text-property 12 'field)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list yr mo dy hr mn sc s f1 f12\n\
         (buffer-string)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 12 'field)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_re_replace_with_overlay_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rot\")))\n\
         (with-current-buffer buf\n\
         (insert \"TODO: fix bug FIXME: update docs TODO: add tests\")\n\
         (let ((ovs nil))\n\
         (goto-char 1)\n\
         (while (re-search-forward \"TODO:\\\\|FIXME:\" nil t)\n\
         (let ((ov (make-overlay (match-beginning 0) (match-end 0))))\n\
         (overlay-put ov 'todo-type\n\
         (if (string= (match-string 0) \"TODO:\") 'todo 'fixme))\n\
         (push ov ovs)))\n\
         (let ((ov-before (mapcar (lambda (ov)\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'todo-type)))\n\
         (nreverse ovs))))\n\
         (undo-boundary)\n\
         (dolist (ov ovs)\n\
         (when (eq (overlay-get ov 'todo-type) 'todo)\n\
         (goto-char (overlay-start ov))\n\
         (delete-region (overlay-start ov) (overlay-end ov))\n\
         (insert \"DONE:\")))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ov-before s\n\
         (buffer-string)\n\
         (mapcar (lambda (ov)\n\
         (list (and (overlay-start ov) t)\n\
         (overlay-get ov 'todo-type)))\n\
         (nreverse ovs)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_whitespace_regex_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"wrp\")))\n\
         (with-current-buffer buf\n\
         (insert \"  if (x) {\\n    return y;\\n  }\\n  for (i=0; i<10; i++) {\\n    print(i);\\n  }\")\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         when (memq (char-after i) '(?\\  ?\\t))\n\
         do (put-text-property i (1+ i) 'whitespace t))\n\
         (let ((ws-count (cl-loop for i from 1 to (buffer-size)\n\
         count (get-text-property i 'whitespace))))\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (while (re-search-forward \"^  \" nil t)\n\
         (replace-match \"\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (new-ws (cl-loop for i from 1 to (buffer-size)\n\
         count (get-text-property i 'whitespace))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ws-count s new-ws\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         count (get-text-property i 'whitespace))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_regex_subexp_with_props_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rsp\")))\n\
         (with-current-buffer buf\n\
         (insert \"<div class=\\\"main\\\">content</div><span id=\\\"x\\\">text</span>\")\n\
         (let ((m-start (copy-marker 1))\n\
         (m-end (copy-marker (point-max))))\n\
         (put-text-property 1 21 'tag 'div)\n\
         (put-text-property 21 28 'tag 'div-close)\n\
         (put-text-property 28 45 'tag 'span)\n\
         (put-text-property 45 53 'tag 'span-close)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (while (re-search-forward \"<\\\\([^>]+\\\\)>\" nil t)\n\
         (let ((tag-content (match-string 1))\n\
         (tag-start (match-beginning 0))\n\
         (tag-end (match-end 0)))\n\
         (put-text-property tag-start tag-end 'html-tag tag-content)))\n\
         (undo-boundary)\n\
         (let ((h1 (get-text-property 1 'html-tag))\n\
         (h22 (get-text-property 22 'html-tag))\n\
         (h28 (get-text-property 28 'html-tag)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list h1 h22 h28\n\
         (buffer-string)\n\
         (get-text-property 1 'tag)\n\
         (get-text-property 21 'tag)\n\
         (get-text-property 28 'tag)\n\
         (marker-position m-start)\n\
         (marker-position m-end))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
