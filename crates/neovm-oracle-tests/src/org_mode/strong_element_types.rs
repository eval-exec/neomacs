//! Strong element-types oracle tests — test all org-element types.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn et_headline_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Title\" \"TODO\" 65 (\"tag1\" \"tag2\") 1 1 35 31 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag1:tag2:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h)))))
    (list (org-element-property :raw-value h)
          (org-element-property :todo-keyword h)
          (org-element-property :priority h)
          (org-element-property :tags h)
          (org-element-property :level h)
          (org-element-property :begin h)
          (org-element-property :end h)
          (org-element-property :contents-begin h)
          (org-element-property :contents-end h))))"##,
        expect,
    );
}

#[test]
fn et_paragraph_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil 1 19) (nil 19 36))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "First paragraph.\n\nSecond paragraph.")
  (let* ((tree (org-element-parse-buffer))
         (paras (org-element-map tree 'paragraph
                  (lambda (p)
                    (list (org-element-property :value p)
                          (org-element-property :begin p)
                          (org-element-property :end p))))))
    paras))"##,
        expect,
    );
}

#[test]
fn et_planning_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((timestamp (:standard-properties [24 nil nil nil 36 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (let* ((tree (org-element-parse-buffer))
         (p (car (org-element-map tree 'planning (lambda (p) p)))))
    (list (org-element-property :scheduled p)
          (org-element-property :deadline p)
          (org-element-property :closed p))))"##,
        expect,
    );
}

#[test]
fn et_timestamp_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (active-range 2026 1 15 10 0 2026 1 15 11 30 cumulate 1 week all 3 day)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 Wed 10:00-11:30 +1w -3d>")
  (let* ((tree (org-element-parse-buffer))
         (ts (car (org-element-map tree 'timestamp (lambda (t) t)))))
    (list (org-element-property :type ts)
          (org-element-property :year-start ts)
          (org-element-property :month-start ts)
          (org-element-property :day-start ts)
          (org-element-property :hour-start ts)
          (org-element-property :minute-start ts)
          (org-element-property :year-end ts)
          (org-element-property :month-end ts)
          (org-element-property :day-end ts)
          (org-element-property :hour-end ts)
          (org-element-property :minute-end ts)
          (org-element-property :repeater-type ts)
          (org-element-property :repeater-value ts)
          (org-element-property :repeater-unit ts)
          (org-element-property :warning-type ts)
          (org-element-property :warning-value ts)
          (org-element-property :warning-unit ts))))"##,
        expect,
    );
}

#[test]
fn et_link_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"https\" \"//example.com/path?q=1\" \"https://example.com/path?q=1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com/path?q=1][My Link]]")
  (let* ((tree (org-element-parse-buffer))
         (l (car (org-element-map tree 'link (lambda (l) l)))))
    (list (org-element-property :type l)
          (org-element-property :path l)
          (org-element-property :raw-link l))))"##,
        expect,
    );
}

#[test]
fn et_keyword_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"My Title\") (\"AUTHOR\" \"Author\") (\"OPTIONS\" \"toc:nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: My Title\n#+AUTHOR: Author\n#+OPTIONS: toc:nil")
  (let* ((tree (org-element-parse-buffer))
         (kws (org-element-map tree 'keyword
                (lambda (k)
                  (list (org-element-property :key k)
                        (org-element-property :value k))))))
    kws))"##,
        expect,
    );
}

#[test]
fn et_src_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"emacs-lisp\" \"my-block\" \"-n\" \":results value :exports both\" \"(+ x 1)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: my-block\n#+HEADER: :var x=1\n#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ x 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (b (car (org-element-map tree 'src-block (lambda (b) b)))))
    (list (org-element-property :language b)
          (org-element-property :name b)
          (org-element-property :switches b)
          (org-element-property :parameters b)
          (org-element-property :value b))))"##,
        expect,
    );
}

#[test]
fn et_example_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"-n\" \"Some example\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_EXAMPLE -n\nSome example\n#+END_EXAMPLE")
  (let* ((tree (org-element-parse-buffer))
         (b (car (org-element-map tree 'example-block (lambda (b) b)))))
    (list (org-element-property :switches b)
          (org-element-property :value b))))"##,
        expect,
    );
}

#[test]
fn et_quote_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_QUOTE\nQuoted text\n#+END_QUOTE")
  (let* ((tree (org-element-parse-buffer))
         (b (car (org-element-map tree 'quote-block (lambda (b) b)))))
    (org-element-property :value b))"##,
        expect,
    );
}

#[test]
fn et_center_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_CENTER\nCentered text\n#+END_CENTER")
  (let* ((tree (org-element-parse-buffer))
         (b (car (org-element-map tree 'center-block (lambda (b) b)))))
    (org-element-property :value b))"##,
        expect,
    );
}

#[test]
fn et_plain_list_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\n  - sub 1\n  - sub 2")
  (let* ((tree (org-element-parse-buffer))
         (pl (car (org-element-map tree 'plain-list (lambda (l) l)))))
    (list (org-element-property :type pl)
          (length (org-element-contents pl))))"##,
        expect,
    );
}

#[test]
fn et_item_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] Checkbox item\n- Plain item\n- [ ] Unchecked")
  (let* ((tree (org-element-parse-buffer))
         (items (org-element-map tree 'item
                  (lambda (it)
                    (list (org-element-property :bullet it)
                          (org-element-property :checkbox it))))))
    items)"##,
        expect,
    );
}

#[test]
fn et_drawer_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"LOGBOOK\" 30 53))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d)
                      (list (org-element-property :drawer-name d)
                            (org-element-property :begin d)
                            (org-element-property :end d))))))
    drawers))"##,
        expect,
    );
}

#[test]
fn et_footnote_reference_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] First\n[fn:2] Second")
  (let* ((tree (org-element-parse-buffer))
         (refs (org-element-map tree 'footnote-reference
                 (lambda (f)
                   (list (org-element-property :label f)
                         (org-element-property :type f))))))
    refs)"##,
        expect,
    );
}

#[test]
fn et_footnote_definition_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Definition *bold*")
  (let* ((tree (org-element-parse-buffer))
         (defs (org-element-map tree 'footnote-definition
                 (lambda (d)
                   (list (org-element-property :label d)
                         (org-element-property :begin d)
                         (org-element-property :end d))))))
    defs)"##,
        expect,
    );
}

#[test]
fn et_radio_target_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<my target>>>\nSee my target here")
  (let* ((tree (org-element-parse-buffer))
         (targets (org-element-map tree 'radio-target
                    (lambda (rt) (org-element-property :value rt)))))
    targets)"##,
        expect,
    );
}

#[test]
fn et_statistics_cookie_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [2/3]\n- [X] a\n- [ ] b\n- [X] c")
  (let* ((tree (org-element-parse-buffer))
         (cookies (org-element-map tree 'statistics-cookie
                    (lambda (sc)
                      (list (org-element-property :value sc)
                            (org-element-property :begin sc))))))
    cookies)"##,
        expect,
    );
}

#[test]
fn et_horizontal_rule_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Above\n-----\nBelow")
  (let* ((tree (org-element-parse-buffer))
         (hrs (org-element-map tree 'horizontal-rule
                (lambda (hr) (org-element-property :begin hr)))))
    hrs)"##,
        expect,
    );
}

#[test]
fn et_table_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n#+TBLFM: $3=$1+$2")
  (let* ((tree (org-element-parse-buffer))
         (tbl (car (org-element-map tree 'table (lambda (t) t)))))
    (list (org-element-property :tblfm tbl)
          (length (org-element-contents tbl))))"##,
        expect,
    );
}

#[test]
fn et_table_row_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (let* ((tree (org-element-parse-buffer))
         (rows (org-element-map tree 'table-row
                 (lambda (tr)
                   (list (org-element-property :type tr)
                         (length (org-element-contents tr)))))))
    rows)"##,
        expect,
    );
}

#[test]
fn et_table_cell_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (let* ((tree (org-element-parse-buffer))
         (cells (org-element-map tree 'table-cell
                  (lambda (c) (org-element-property :value c)))))
    cells)"##,
        expect,
    );
}

#[test]
fn et_bold_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text *bold* more")
  (let* ((tree (org-element-parse-buffer))
         (bold (car (org-element-map tree 'bold (lambda (b) b)))))
    (org-element-property :value bold))"##,
        expect,
    );
}

#[test]
fn et_italic_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text /italic/ more")
  (let* ((tree (org-element-parse-buffer))
         (italic (car (org-element-map tree 'italic (lambda (i) i)))))
    (org-element-property :value italic))"##,
        expect,
    );
}

#[test]
fn et_code_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text =code= more")
  (let* ((tree (org-element-parse-buffer))
         (code (car (org-element-map tree 'code (lambda (c) c)))))
    (org-element-property :value code))"##,
        expect,
    );
}

#[test]
fn et_verbatim_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text ~verbatim~ more")
  (let* ((tree (org-element-parse-buffer))
         (verb (car (org-element-map tree 'verbatim (lambda (v) v)))))
    (org-element-property :value verb))"##,
        expect,
    );
}

#[test]
fn et_underline_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text _underlined_ more")
  (let* ((tree (org-element-parse-buffer))
         (u (car (org-element-map tree 'underline (lambda (u) u)))))
    (org-element-property :value u))"##,
        expect,
    );
}

#[test]
fn et_strikethrough_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text +strikethrough+ more")
  (let* ((tree (org-element-parse-buffer))
         (s (car (org-element-map tree 'strike (lambda (s) s)))))
    (org-element-property :value s))"##,
        expect,
    );
}

#[test]
fn et_superscript_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text^{super} more")
  (let* ((tree (org-element-parse-buffer))
         (sup (car (org-element-map tree 'superscript (lambda (s) s)))))
    (org-element-property :value sup))"##,
        expect,
    );
}

#[test]
fn et_subscript_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text_{sub} more")
  (let* ((tree (org-element-parse-buffer))
         (sub (car (org-element-map tree 'subscript (lambda (s) s)))))
    (org-element-property :value sub))"##,
        expect,
    );
}

#[test]
fn et_entity_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta \\gamma")
  (let* ((tree (org-element-parse-buffer))
         (entities (org-element-map tree 'entity
                     (lambda (e)
                       (list (org-element-property :name e)
                             (org-element-property :value e))))))
    entities)"##,
        expect,
    );
}

#[test]
fn et_latex_fragment_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $E=mc^2$ more")
  (let* ((tree (org-element-parse-buffer))
         (frag (car (org-element-map tree 'latex-fragment (lambda (f) f)))))
    (org-element-property :value frag))"##,
        expect,
    );
}

#[test]
fn et_macro_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello!\n{{{greet}}}")
  (let* ((tree (org-element-parse-buffer))
         (mac (car (org-element-map tree 'macro (lambda (m) m)))))
    (list (org-element-property :key mac)
          (org-element-property :value mac)
          (org-element-property :args mac)))"##,
        expect,
    );
}

#[test]
fn et_export_snippet_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "@@html:<b>bold</b>@@")
  (let* ((tree (org-element-parse-buffer))
         (snippet (car (org-element-map tree 'export-snippet (lambda (s) s)))))
    (list (org-element-property :back-end snippet)
          (org-element-property :value snippet)))"##,
        expect,
    );
}

#[test]
fn et_export_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_EXPORT html\n<b>bold</b>\n#+END_EXPORT")
  (let* ((tree (org-element-parse-buffer))
         (block (car (org-element-map tree 'export-block (lambda (b) b)))))
    (list (org-element-property :type block)
          (org-element-property :value block)))"##,
        expect,
    );
}

#[test]
fn et_call_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CALL: my-func()")
  (let* ((tree (org-element-parse-buffer))
         (call (car (org-element-map tree 'babel-call (lambda (c) c)))))
    (list (org-element-property :call call)
          (org-element-property :inside-header call)))"##,
        expect,
    );
}

#[test]
fn test_clock_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (clock (car (org-element-map tree 'clock (lambda (c) c)))))
    (list (org-element-property :value clock)
          (org-element-property :duration clock)))"##,
        expect,
    );
}

#[test]
fn et_comment_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment line\n# Another comment")
  (let* ((tree (org-element-parse-buffer))
         (comments (org-element-map tree 'comment
                     (lambda (c) (org-element-property :value c)))))
    comments)"##,
        expect,
    );
}

#[test]
fn et_fixed_width_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": Fixed width\n: Another line")
  (let* ((tree (org-element-parse-buffer))
         (fw (org-element-map tree 'fixed-width
               (lambda (f) (org-element-property :value f)))))
    fw)"##,
        expect,
    );
}

#[test]
fn et_diary_sexp_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "%%(diary-float 1 2 3)")
  (let* ((tree (org-element-parse-buffer))
         (sexp (car (org-element-map tree 'diary-sexp (lambda (s) s)))))
    (org-element-property :value sexp))"##,
        expect,
    );
}

#[test]
fn et_inlinetask_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-inlinetask)
  (insert "Body\n*************** TODO Inline\n*************** END\nMore")
  (let* ((tree (org-element-parse-buffer))
         (tasks (org-element-map tree 'headline
                  (lambda (h)
                    (when (= (org-element-property :level h) 15)
                      (list (org-element-property :raw-value h)
                            (org-element-property :todo-keyword h)
                            (org-element-property :level h)))))))
    tasks)"##,
        expect,
    );
}

#[test]
fn et_property_drawer_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (pd (car (org-element-map tree 'property-drawer (lambda (pd) pd)))))
    (list (org-element-property :begin pd)
          (org-element-property :end pd)))"##,
        expect,
    );
}

#[test]
fn et_node_property_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (props (org-element-map tree 'node-property
                  (lambda (np)
                    (list (org-element-property :key np)
                          (org-element-property :value np))))))
    props)"##,
        expect,
    );
}
