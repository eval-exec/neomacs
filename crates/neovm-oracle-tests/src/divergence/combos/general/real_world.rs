//! Divergence tests: real-world editing scenario simulations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_json_like_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"Alice\" \"scores\" \"active\") (\"name\" \"Alice\" \"scores\" \"active\") (\"scores\" \"active\") (\"active\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"{\\\"name\\\": \\\"Alice\\\", \\\"scores\\\": [95, 87, 92], \\\"active\\\": true}\")
  (goto-char 1)
  (let ((pairs nil))
    (while (re-search-forward \"\\\\\\\"\\\\([^\\\\\\\"]+\\\\)\\\\\\\"\" nil t)
      (push (match-string 1) pairs))
    (let ((all (nreverse pairs)))
      (list all
            (member \"name\" all)
            (member \"scores\" all)
            (member \"active\" all)
            (= (length all) 4))))) ",
        expect,
    );
}

#[test]
fn divergence_csv_processing_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"name,age,city\" 3 (\"name\" \"age\" \"city\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"name,age,city\\nAlice,30,NYC\\nBob,25,LA\\nCarol,35,Chicago\")
  (goto-char 1)
  (let ((header (buffer-substring (line-beginning-position) (line-end-position))))
    (forward-line 1)
    (let ((rows nil))
      (while (not (eobp))
        (let ((line (buffer-substring (line-beginning-position) (line-end-position))))
          (push line rows)
          (forward-line 1))
        rows)
      (list header
            (length (nreverse rows))
            (split-string header \",\" t)
            (= (length (split-string header \",\" t)) 3))))) ",
        expect,
    );
}

#[test]
fn divergence_refactor_rename_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (2 \"function newName() { return newName.helper(); }\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"function oldName() { return oldName.helper(); }\")
  (goto-char 1)
  (let ((count 0))
    (while (re-search-forward \"\\\\<oldName\\\\>\" nil t)
      (cl-incf count)
      (replace-match \"newName\" t))
    (list count (buffer-string)
          (string= (buffer-string)
                   \"function newName() { return newName.helper(); }\")))) ",
        expect,
    );
}

#[test]
fn divergence_org_like_heading_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"* \" \"** \" \"* \") (1 2 1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"* Heading 1\\nSome text\\n** Heading 2\\nMore text\\n* Heading 3\\nFinal text\")
  (goto-char 1)
  (let ((headings nil)
        (levels nil))
    (while (re-search-forward \"^\\\\(*+\\\\) \" nil t)
      (push (match-string 0) headings)
      (push (length (match-string 1)) levels))
    (list (nreverse headings)
          (nreverse levels)
          (equal (nreverse levels) '(1 2 1))))) ",
        expect,
    );
}

#[test]
fn duplication_code_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 1 t 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"def foo():\\n  return 42\\ndef bar():\\n  return 42\\ndef baz():\\n  return 99\")
  (goto-char 1)
  (let ((lines (split-string (buffer-string) \"\\n\" t))
        (seen (make-hash-table :test 'equal))
        (dupes nil))
    (dolist (l lines)
      (if (gethash l seen)
          (push l dupes)
        (puthash l t seen)))
    (list (length lines)
          (length dupes)
          (>= (length dupes) 1)
          (hash-table-count seen)))) ",
        expect,
    );
}

#[test]
fn divergence_template_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"Hello Alice, your order #12345 is shipped.\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((template \"Hello {{name}}, your order #{{id}} is {{status}}.\")
        (bindings '((name . \"Alice\") (id . \"12345\") (status . \"shipped\"))))
  (let ((result template))
    (dolist (b bindings)
      (setq result (replace-regexp-in-string
                    (format \"{{%s}}\" (car b))
                    (cdr b) result)))
    (list result
          (string= result \"Hello Alice, your order #12345 is shipped.\")))) ",
        expect,
    );
}

#[test]
fn divergence_log_analysis_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1 2 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"[ERROR] 2024-01-15 Connection failed\\n\")
  (insert \"[INFO] 2024-01-15 Retry succeeded\\n\")
  (insert \"[WARN] 2024-01-15 Slow response\\n\")
  (insert \"[ERROR] 2024-01-16 Timeout\\n\")
  (insert \"[INFO] 2024-01-16 All clear\")
  (let ((errors 0) (warnings 0) (infos 0))
    (goto-char 1)
    (while (re-search-forward \"\\\\[\\\\(ERROR\\\\|WARN\\\\|INFO\\\\)\\\\]\" nil t)
      (cond ((string= (match-string 1) \"ERROR\") (cl-incf errors))
            ((string= (match-string 1) \"WARN\") (cl-incf warnings))
            ((string= (match-string 1) \"INFO\") (cl-incf infos))))
    (list errors warnings infos
          (= errors 2) (= warnings 1) (= infos 2)))) ",
        expect,
    );
}

#[test]
fn divergence_extract_and_restructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"John\" \"Doe\" \"30\" \"NYC\" 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"firstName: John\\nlastName: Doe\\nage: 30\\ncity: NYC\")
  (goto-char 1)
  (let ((data (make-hash-table :test 'equal)))
    (while (re-search-forward \"\\\\([a-zA-Z]+\\\\): \\\\([^\\n]+\\\\)\" nil t)
      (puthash (match-string 1) (match-string 2) data))
    (list (gethash \"firstName\" data)
          (gethash \"lastName\" data)
          (gethash \"age\" data)
          (gethash \"city\" data)
          (hash-table-count data)
          (= (hash-table-count data) 4)))) ",
        expect,
    );
}

#[test]
fn divergence_whitespace_normalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world \\n\\n foo bar \\n baz\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"  hello   world   \\n\\n   foo  bar  \\n  baz  \")
  (let ((result
         (replace-regexp-in-string
          \"[ \\t]+\" \" \"
          (string-trim (buffer-string)))))
    (list result
          (not (string-match \"  \" result))
          (string= (substring result 0 1) \"h\")))) ",
        expect,
    );
}

#[test]
fn divergence_code_comment_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"// line1\\n// line2\\n// line3\" \"// line1\\n// line2\\n// line3\" 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq comment-start \"// \")
  (setq comment-end \"\")
  (insert \"line1\\nline2\\nline3\")
  (comment-region 1 18)
  (let ((commented (buffer-string)))
    (uncomment-region 1 (point-max))
    (list commented
          (buffer-string)
          (string-match \"//\" commented)
          (not (string-match \"//\" (buffer-string)))))) ",
        expect,
    );
}
