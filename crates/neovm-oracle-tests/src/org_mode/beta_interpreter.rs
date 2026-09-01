//! Beta-strict combo tests for org-mode interpreter round-trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Beta: Block interpreter round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn beta_interpret_center_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+begin_center\\nText\\n#+end_center\\n\" 15 20 (:parent (paragraph (:standard-properties [16 16 16 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (center-block (:standard-properties [1 1 16 21 33 0 nil top-comment nil nil nil 16 21 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 33 33 0 nil first-section nil nil nil 1 33 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 33 33 0 nil org-data nil nil nil 3 33 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"Text\\n\" 0 5 (:parent #2)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\nText\n#+END_CENTER")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\":DRAWER:\\nContents\\n:END:\\n\" 9 18 (:parent (paragraph (:standard-properties [10 10 10 19 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (drawer (:standard-properties [1 1 10 19 24 0 nil top-comment nil nil nil 11 19 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 24 24 0 nil first-section nil nil nil 1 24 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 24 24 0 nil org-data nil nil nil 3 24 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)] :pre-blank 0 :drawer-name \"DRAWER\") #2)]) #(\"Contents\\n\" 0 9 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert ":DRAWER:\nContents\n:END:")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_dynamic_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+begin: myblock :param val\\nContent\\n#+end:\\n\" 28 36 (:parent (paragraph (:standard-properties [29 29 29 37 37 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (dynamic-block (:standard-properties [1 1 29 37 43 0 nil top-comment nil nil nil 29 37 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 43 43 0 nil first-section nil nil nil 1 43 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 43 43 0 nil org-data nil nil nil 3 43 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)] :block-name \"myblock\" :arguments \":param val\") #2)]) #(\"Content\\n\" 0 8 (:parent #2)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: myblock :param val\nContent\n#+END:")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_footnote_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"[fn:1] Definition.\\n\" 7 18 (:parent (paragraph (:standard-properties [8 8 8 19 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (footnote-definition (:standard-properties [1 1 8 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)] :label \"1\" :pre-blank 0) #2)]) #(\"Definition.\" 0 11 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[fn:1] Definition.")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"* Headline\\n\" 2 10 (:parent (headline (:standard-properties [1 1 nil nil 11 0 (:title) first-section nil nil nil nil nil 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 11 11 0 nil org-data nil nil nil 3 11 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #2)] :pre-blank 0 :raw-value \"Headline\" :title (#(\"Headline\" 0 8 (:parent #2))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Headline")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_inlinetask() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"*************** Inline task\\nBody\\n*************** end\\n\" 16 27 (:parent (inlinetask (:standard-properties [1 1 29 34 53 0 (:title) top-comment nil nil nil 31 32 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 53 53 0 nil first-section nil nil nil 1 53 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 53 53 0 nil org-data nil nil nil 3 53 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)] :pre-blank 0 :raw-value \"Inline task\" :title (#(\"Inline task\" 0 11 (:parent #2))) :level 15 :priority nil :tags nil :todo-keyword nil :todo-type nil :archivedp nil :commentedp nil :footnote-section-p nil) (paragraph (:standard-properties [29 29 29 34 34 0 nil planning nil nil nil nil nil nil #<killed buffer> nil nil #2]) #(\"Body\\n\" 0 5 (:parent #3))))) 28 33 (:parent (paragraph (:standard-properties [29 29 29 34 34 0 nil planning nil nil nil nil nil nil #<killed buffer> nil nil (inlinetask (:standard-properties [1 1 29 34 53 0 (:title) top-comment nil nil nil 31 32 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 53 53 0 nil first-section nil nil nil 1 53 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 53 53 0 nil org-data nil nil nil 3 53 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)] :pre-blank 0 :raw-value \"Inline task\" :title (#(\"Inline task\" 0 11 (:parent #5))) :level 15 :priority nil :tags nil :todo-keyword nil :todo-type nil :archivedp nil :commentedp nil :footnote-section-p nil) #2)]) #(\"Body\\n\" 0 5 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "*************** Inline task\nBody\n*************** END")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_plain_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"- Item 1\\n- Item 2\\n\" 2 3 (:parent (paragraph (:standard-properties [3 3 3 10 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [1 1 3 10 10 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (plain-list (:standard-properties [1 1 1 18 18 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (section (:standard-properties [1 1 1 18 18 0 nil first-section nil nil nil 1 18 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 18 18 0 nil org-data nil nil nil 3 18 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type unordered) #5 (item (:standard-properties [10 10 12 18 18 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) #8] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [12 12 12 18 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"Item 2\" 0 6 (:parent #10)))))] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #2)]) #(\"Item 1\\n\" 0 7 (:parent #2)))) 3 8 (:parent (paragraph (:standard-properties [3 3 3 10 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [1 1 3 10 10 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (plain-list (:standard-properties [1 1 1 18 18 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (section (:standard-properties [1 1 1 18 18 0 nil first-section nil nil nil 1 18 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 18 18 0 nil org-data nil nil nil 3 18 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type unordered) #5 (item (:standard-properties [10 10 12 18 18 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) #8] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [12 12 12 18 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"Item 2\" 0 6 (:parent #10)))))] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #2)]) #(\"Item 1\\n\" 0 7 (:parent #2)))) 11 12 (:parent (paragraph (:standard-properties [12 12 12 18 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [10 10 12 18 18 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (plain-list (:standard-properties [1 1 1 18 18 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (section (:standard-properties [1 1 1 18 18 0 nil first-section nil nil nil 1 18 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 18 18 0 nil org-data nil nil nil 3 18 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type unordered) (item (:standard-properties [1 1 3 10 10 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) #8] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [3 3 3 10 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"Item 1\\n\" 0 7 (:parent #10)))) #5)] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #2)]) #(\"Item 2\" 0 6 (:parent #2)))) 12 17 (:parent (paragraph (:standard-properties [12 12 12 18 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [10 10 12 18 18 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (plain-list (:standard-properties [1 1 1 18 18 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) (section (:standard-properties [1 1 1 18 18 0 nil first-section nil nil nil 1 18 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 18 18 0 nil org-data nil nil nil 3 18 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type unordered) (item (:standard-properties [1 1 3 10 10 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((1 0 \"- \" nil nil nil 10) (10 0 \"- \" nil nil nil 18)) #8] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [3 3 3 10 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"Item 1\\n\" 0 7 (:parent #10)))) #5)] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #2)]) #(\"Item 2\" 0 6 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Item 1\n- Item 2")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_quote_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+begin_quote\\nQuoted\\n#+end_quote\\n\" 14 21 (:parent (paragraph (:standard-properties [15 15 15 22 22 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (quote-block (:standard-properties [1 1 15 22 33 0 nil top-comment nil nil nil 15 22 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 33 33 0 nil first-section nil nil nil 1 33 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 33 33 0 nil org-data nil nil nil 3 33 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"Quoted\\n\" 0 7 (:parent #2)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_QUOTE\nQuoted\n#+END_QUOTE")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_special_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+begin_SPECIAL\\nContent\\n#+end_SPECIAL\\n\" 16 24 (:parent (paragraph (:standard-properties [17 17 17 25 25 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (special-block (:standard-properties [1 1 17 25 38 0 nil top-comment nil nil nil 17 25 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 38 38 0 nil first-section nil nil nil 1 38 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)] :type \"SPECIAL\" :parameters nil) #2)]) #(\"Content\\n\" 0 8 (:parent #2)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SPECIAL\nContent\n#+END_SPECIAL")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_babel_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+call: test()\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CALL: test()")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"CLOCK: [2024-01-15 Mon 10:00]--[2024-01-15 Mon 11:00] =>  1:00\\n\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "CLOCK: [2024-01-15 Mon 10:00]--[2024-01-15 Mon 11:00] =>  1:00")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"# Comment\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "# Comment")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_comment_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+begin_comment\\nTest\\n#+end_comment\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_COMMENT\nTest\n#+END_COMMENT")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_diary_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"%%(org-anniversary 1956 5 14) Arthur Dent is %d years old\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "%%(org-anniversary 1956 5 14) Arthur Dent is %d years old")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_example_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#+begin_example\\n  Test\\n#+end_example\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_EXAMPLE\nTest\n#+END_EXAMPLE")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_export_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#+begin_export HTML\\n<p>Text</p>\\n#+end_export\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_EXPORT HTML\n<p>Text</p>\n#+END_EXPORT")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_fixed_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \": Test\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert ": Test")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_horizontal_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-----\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "-------")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+keyword: value\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+KEYWORD: value")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"\\\\begin{equation}\\n1+1=2\\n\\\\end{equation}\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\begin{equation}\n1+1=2\n\\end{equation}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"* Headline\\nDEADLINE: <2012-03-29 Thu> SCHEDULED: <2012-03-29 Thu> CLOSED: [2012-03-29 Thu]\\n\" 2 10 (:parent (headline (:standard-properties [1 1 12 79 79 0 (:title) first-section nil nil nil nil nil 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 79 79 0 nil org-data nil nil nil 3 79 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #2)] :pre-blank 0 :raw-value \"Headline\" :title (#(\"Headline\" 0 8 (:parent #2))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil :deadline (timestamp (:standard-properties [22 nil nil nil 35 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2012-03-29>\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil)) :scheduled (timestamp (:standard-properties [46 nil nil nil 59 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2012-03-29>\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil)) :closed (timestamp (:standard-properties [67 nil nil nil 79 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive :range-type nil :raw-value \"[2012-03-29]\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil))) (section (:standard-properties [12 12 12 79 79 0 nil section nil nil nil 12 79 nil #<killed buffer> nil nil #2]) (planning (:standard-properties [12 12 nil nil 79 0 nil planning nil nil nil nil nil nil #<killed buffer> nil nil #3] :closed (timestamp (:standard-properties [67 nil nil nil 79 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive :range-type nil :raw-value \"[2012-03-29]\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil)) :deadline (timestamp (:standard-properties [22 nil nil nil 35 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2012-03-29>\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil)) :scheduled (timestamp (:standard-properties [46 nil nil nil 59 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2012-03-29>\" :year-start 2012 :month-start 3 :day-start 29 :hour-start nil :minute-start nil :year-end 2012 :month-end 3 :day-end 29 :hour-end nil :minute-end nil))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Headline\nDEADLINE: <2012-03-29> SCHEDULED: <2012-03-29> CLOSED: [2012-03-29]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_property_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"* H\\n:PROPERTIES:\\n:prop:     value\\n:END:\\n\" 2 3 (:parent (headline (:standard-properties [1 1 5 36 36 0 (:title) first-section nil nil nil nil nil 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 36 36 0 nil org-data nil nil nil 3 36 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #2)] :pre-blank 0 :raw-value \"H\" :title (#(\"H\" 0 1 (:parent #2))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil :PROP \"value\") (section (:standard-properties [5 5 5 36 36 0 nil section nil nil nil 5 36 nil #<killed buffer> nil nil #2]) (property-drawer (:standard-properties [5 5 18 31 36 0 nil planning nil nil nil nil nil nil #<killed buffer> nil nil #3]) (node-property (:standard-properties [18 18 nil nil 31 0 nil node-property nil nil nil nil nil nil #<killed buffer> nil nil #4] :key \"prop\" :value \"value\")))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:prop: value\n:END:")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+begin_src emacs-lisp :results silent\\n  (+ 1 1)\\n#+end_src\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC emacs-lisp :results silent\n(+ 1 1)\n#+END_SRC")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm nil :value nil) #5 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"c\" 0 1 (:parent #10))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"d\" 0 1 (:parent #10)))))] :type standard) #2 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #5]) #(\"b\" 0 1 (:parent #6))))]) #(\"a\" 0 1 (:parent #2)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm nil :value nil) #5 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"c\" 0 1 (:parent #10))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"d\" 0 1 (:parent #10)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #5]) #(\"a\" 0 1 (:parent #6))) #2)]) #(\"b\" 0 1 (:parent #2)))) 12 13 (:parent (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"a\" 0 1 (:parent #10))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"b\" 0 1 (:parent #10)))) #5)] :type standard) #2 (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #5]) #(\"d\" 0 1 (:parent #6))))]) #(\"c\" 0 1 (:parent #2)))) 16 17 (:parent (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"a\" 0 1 (:parent #10))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"b\" 0 1 (:parent #10)))) #5)] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #5]) #(\"c\" 0 1 (:parent #6))) #2)]) #(\"d\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n| c | d |")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_table_with_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| 2 |\\n| 4 |\\n| 3 |\\n#+TBLFM: @3=vmean(@1..@2)\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) #5 (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"4\" 0 1 (:parent #10)))) (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"3\" 0 1 (:parent #10)))))] :type standard) #2)]) #(\"2\" 0 1 (:parent #2)))) 8 9 (:parent (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"2\" 0 1 (:parent #10)))) #5 (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"3\" 0 1 (:parent #10)))))] :type standard) #2)]) #(\"4\" 0 1 (:parent #2)))) 14 15 (:parent (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"2\" 0 1 (:parent #10)))) (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"4\" 0 1 (:parent #10)))) #5)] :type standard) #2)]) #(\"3\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 2 |\n| 4 |\n| 3 |\n#+TBLFM: @3=vmean(@1..@2)")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_verse_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+begin_verse\\nTest\\n#+end_verse\\n\" 14 19 (:parent (verse-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"Test\\n\" 0 5 (:parent #2)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_VERSE\nTest\n#+END_VERSE")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Beta: Object interpreter round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn beta_interpret_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"*text*\\n\" 1 5 (:parent (bold (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"text\" 0 4 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "*text*")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_citation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[cite:@key]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[cite:@key]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"~text~\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "~text~")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"\\\\alpha text\\n\" 7 11 (:parent (paragraph (:standard-properties [1 1 1 12 12 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 12 12 0 nil first-section nil nil nil 1 12 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 12 12 0 nil org-data nil nil nil 3 12 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) (entity (:standard-properties [1 nil nil nil 8 1 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p nil)) #(\"text\" 0 4 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\alpha text")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_export_snippet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"@@backend:contents@@\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "@@backend:contents@@")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_footnote_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Text[fn:1]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 11 11 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 11 11 0 nil first-section nil nil nil 1 11 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 11 11 0 nil org-data nil nil nil 3 11 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"Text\" 0 4 (:parent #2)) (footnote-reference (:standard-properties [5 nil nil nil 11 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :label \"1\" :type standard)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_footnote_reference_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Text[fn:label]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 15 15 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 15 15 0 nil first-section nil nil nil 1 15 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 15 15 0 nil org-data nil nil nil 3 15 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"Text\" 0 4 (:parent #2)) (footnote-reference (:standard-properties [5 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :label \"label\" :type standard)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:label]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_footnote_reference_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Text[fn:label:def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"Text\" 0 4 (:parent #2)) (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #3))))) 14 17 (:parent (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"Text\" 0 4 (:parent #5)) #2)] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:label:def]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_footnote_reference_anonymous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Text[fn::def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"Text\" 0 4 (:parent #2)) (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :label nil :type inline) #(\"def\" 0 3 (:parent #3))))) 9 12 (:parent (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"Text\" 0 4 (:parent #5)) #2)] :label nil :type inline) #(\"def\" 0 3 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn::def]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_inline_babel_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"call_test()\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "call_test()")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_inline_babel_call_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"call_test(x=2)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "call_test(x=2)")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_inline_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"src_emacs-lisp{(+ 1 1)}\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "src_emacs-lisp{(+ 1 1)}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"/text/\\n\" 1 5 (:parent (italic (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"text\" 0 4 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "/text/")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_fragment_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\\command{}\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\command{}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_fragment_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"$x$\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "$x$")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_fragment_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"$$x+y$$\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "$$x+y$$")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_fragment_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\\(x+y\\\\)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\(x+y\\)")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_latex_fragment_bracket() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\\[x+y\\\\]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\[x+y\\]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_line_break() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"First line \\\\\\\\\\nSecond line\\n\" 0 11 (:parent (paragraph (:standard-properties [1 1 1 26 26 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 26 26 0 nil first-section nil nil nil 1 26 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 26 26 0 nil org-data nil nil nil 3 26 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"First line \" 0 11 (:parent #2)) (line-break (:standard-properties [12 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2])) #(\"Second line\" 0 11 (:parent #2)))) 14 25 (:parent (paragraph (:standard-properties [1 1 1 26 26 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 26 26 0 nil first-section nil nil nil 1 26 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 26 26 0 nil org-data nil nil nil 3 26 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"First line \" 0 11 (:parent #2)) (line-break (:standard-properties [12 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2])) #(\"Second line\" 0 11 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "First line \\\\\nSecond line")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_no_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[https://orgmode.org]]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[https://orgmode.org]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_with_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"[[https://orgmode.org][Org mode]]\\n\" 23 31 (:parent (link (:standard-properties [1 nil 24 32 34 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 34 34 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 34 34 0 nil first-section nil nil nil 1 34 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 34 34 0 nil org-data nil nil nil 3 34 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)] :type \"https\" :type-explicit-p t :path \"//orgmode.org\" :format bracket :raw-link \"https://orgmode.org\" :application nil :search-option nil) #(\"Org mode\" 0 8 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[https://orgmode.org][Org mode]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[file:todo.org::*task]]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[file:todo.org::*task]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[id:aaaa]]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[id:aaaa]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_custom_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[#id]]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[#id]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_coderef() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[(ref)]]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[(ref)]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_plain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"https://orgmode.org\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "https://orgmode.org")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_angular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<https://orgmode.org>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<https://orgmode.org>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_link_pathological() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"[[file://path][%s]]\\n\" 16 17 (:parent (link (:standard-properties [1 nil 16 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)] :type \"file\" :type-explicit-p t :path \"//path\" :format bracket :raw-link \"file://path\" :application nil :search-option nil) #(\"%s\" 0 2 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[file://path][%s]]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_macro_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"{{{test}}}\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "{{{test}}}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_macro_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"{{{test(arg1,arg2)}}}\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "{{{test(arg1,arg2)}}}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_radio_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"<<<some text>>>\\n\" 3 12 (:parent (radio-target (:standard-properties [1 nil 4 13 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)] :value \"some text\") #(\"some text\" 0 9 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<<some text>>>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_statistics_cookie_fraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[0/1]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[0/1]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_statistics_cookie_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[66%]\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[66%]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_strike_through() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"+target+\\n\" 1 7 (:parent (strike-through (:standard-properties [1 nil 2 8 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 9 9 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 9 9 0 nil first-section nil nil nil 1 9 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 9 9 0 nil org-data nil nil nil 3 9 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"target\" 0 6 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "+target+")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_subscript() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"a_b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"a\" 0 1 (:parent #2)) (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) 2 3 (:parent (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"a\" 0 1 (:parent #5)) #2)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "a_b")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_subscript_braces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"a_{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"a\" 0 1 (:parent #2)) (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) 3 4 (:parent (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"a\" 0 1 (:parent #5)) #2)] :use-brackets-p t) #(\"b\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "a_{b}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_superscript() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"a^b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"a\" 0 1 (:parent #2)) (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) 2 3 (:parent (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"a\" 0 1 (:parent #5)) #2)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "a^b")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_superscript_braces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"a^{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5)]) #2)]) #(\"a\" 0 1 (:parent #2)) (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) 3 4 (:parent (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #(\"a\" 0 1 (:parent #5)) #2)] :use-brackets-p t) #(\"b\" 0 1 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "a^{b}")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<<target>>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<target>>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"_text_\\n\" 1 5 (:parent (underline (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #8)]) #5)]) #2)]) #(\"text\" 0 4 (:parent #2)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "_text_")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_verbatim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"=text=\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "=text=")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Beta: Timestamp interpreter round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn beta_interpret_timestamp_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2012-03-29 Thu 16:40>\\n\"""#]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu 16:40>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[2012-03-29 Thu 16:40]\\n\"""#]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[2012-03-29 Thu 16:40]")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_active_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"<2012-03-29 Thu 16:40>--<2012-03-29 Thu 16:41>\\n\"""#]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu 16:40>--<2012-03-29 Thu 16:41>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_active_timerange() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2012-03-29 Thu 16:40-16:41>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu 16:40-16:41>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_diary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<%%(diary-float t 4 2)>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<%%(diary-float t 4 2)>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_diary_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<%%(diary-float t 4 2) 12:00>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<%%(diary-float t 4 2) 12:00>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2012-03-29 Thu +1y>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu +1y>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2012-03-29 Thu -1y>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu -1y>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_repeater_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2012-03-29 Thu +1y -1y>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu +1y -1y>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}

#[test]
fn beta_interpret_timestamp_range_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"<2012-03-29 Thu +1y>--<2012-03-30 Fri +1y>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2012-03-29 Thu +1y>--<2012-03-30 Fri +1y>")
      (org-element-interpret-data (org-element-parse-buffer)))))"##,
        expect,
    );
}
