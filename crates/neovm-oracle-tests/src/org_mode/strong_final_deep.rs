//! Strong final-deep oracle tests — comprehensive deep state.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Final: complete document
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_complete_document() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Complete\" 0 8 (:parent (#(\"Complete\" 0 8 (:parent #4)))))) ((\"Phase 1\" \"TODO\") (\"Task 1\" \"TODO\") (\"Task 2\" \"TODO\") (\"Phase 2\" \"DONE\") (\"Sub 1\" \"DONE\") (\"Sub 2\" \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Complete\n#+AUTHOR: Author\n* TODO Phase 1\n** TODO Task 1\n** TODO Task 2\n* DONE Phase 2\n** DONE Sub 1\n** TODO Sub 2")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (h (org-element-map tree 'headline
              (lambda (h) (list (org-element-property :raw-value h)
                                (org-element-property :todo-keyword h))))))
    (list (plist-get info :title) h)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: property chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_property_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"CATEGORY\" . \"???\") (\"B\" . \"2\") (\"A\" . \"1\")) ((\"CATEGORY\" . \"???\") (\"C\" . \"3\") (\"A\" . \"1\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (goto-char (point-min))
  (let ((p1 (org-entry-properties nil 'standard)))
    (org-entry-put nil "C" "3")
    (org-entry-delete nil "B")
    (list p1 (org-entry-properties nil 'standard))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: table all ops
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_table_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-transpose)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 3 | c |\n| 1 | a |\n| 2 | b |\n|---|\n#+TBLFM: $3=$1*10")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((d1 (org-table-to-lisp)))
    (org-table-sort-lines nil ?N)
    (let ((d2 (org-table-to-lisp)))
      (org-table-transpose)
      (list d1 d2 (org-table-to-lisp)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: checkbox stats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_checkbox_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* T [0%]\" \"* T [33%]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [%]\n- [ ] a\n  - [ ] a1\n  - [ ] a2\n- [ ] b\n- [ ] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 2)
    (org-toggle-checkbox)
    (forward-line 1)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (list h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: sparse tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"D\" \"E\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n** TODO D\n** DONE E")
  (goto-char (point-min))
  (org-match-sparse-tree nil "TODO")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible) (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: element parse modify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_element_parse_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"T\" \"TODO\" 65 (\"tag\")) (\"New\" \"DONE\" 66 (\"newtag\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T :tag:\nBody")
  (let* ((tree1 (org-element-parse-buffer))
         (h1 (car (org-element-map tree1 'headline
                    (lambda (h) (list (org-element-property :raw-value h)
                                      (org-element-property :todo-keyword h)
                                      (org-element-property :priority h)
                                      (org-element-property :tags h)))))))
    (goto-char (point-min))
    (org-edit-headline "New")
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (let* ((tree2 (org-element-parse-buffer))
           (h2 (car (org-element-map tree2 'headline
                      (lambda (h) (list (org-element-property :raw-value h)
                                        (org-element-property :todo-keyword h)
                                        (org-element-property :priority h)
                                        (org-element-property :tags h)))))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: export environment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_export_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"My Title\" 0 8 (:parent (#(\"My Title\" 0 8 (:parent #4)))))) (#(\"Author\" 0 6 (:parent (#(\"Author\" 0 6 (:parent #4)))))) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: My Title\n#+AUTHOR: Author\n#+OPTIONS: toc:nil num:nil\n* H1\n** H2")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :author)
          (plist-get info :with-toc)
          (plist-get info :with-numbers))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: link attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_link_attr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"img.png\" (((#(\"Cap\" 0 3 (:parent (#(\"Cap\" 0 3 (:parent #6)))))))) (\":width 300\") \"fig\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: Cap\n#+ATTR_HTML: :width 300\n#+NAME: fig\n[[file:img.png]]")
  (let* ((tree (org-element-parse-buffer))
         (l (car (org-element-map tree 'link (lambda (l) l))))
         (p (org-element-property :parent l)))
    (list (org-element-property :path l)
          (org-element-property :caption p)
          (org-element-property :attr_html p)
          (org-element-property :name p))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: planning repeaters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_planning_repeaters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w -3d>\n* TODO M\nDEADLINE: <2026-01-20 +1m -1w>")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p)
      (let ((s (org-element-property :scheduled p))
            (d (org-element-property :deadline p)))
        (list (when s (list (org-element-property :repeater-type s)
                            (org-element-property :repeater-value s)
                            (org-element-property :repeater-unit s)))
              (when d (list (org-element-property :repeater-type d)
                            (org-element-property :repeater-value d)
                            (org-element-property :repeater-unit d)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: timestamp repeater
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_timestamp_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active-range 2026 15 10 0 2026 15 nil nil nil) (active-range 2026 16 nil nil 2026 20 nil nil nil) (active 2026 25 nil nil 2026 25 cumulate 1 week))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 10:00-11:30>\n<2026-01-16>--<2026-01-20>\n<2026-01-25 Wed +1w>")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (t)
      (list (org-element-property :type t)
            (org-element-property :year-start t)
            (org-element-property :day-start t)
            (org-element-property :hour-start t)
            (org-element-property :minute-start t)
            (org-element-property :year-end t)
            (org-element-property :day-end t)
            (org-element-property :repeater-type t)
            (org-element-property :repeater-value t)
            (org-element-property :repeater-unit t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: drawer types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_drawer_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"LOGBOOK\" \"MYDRAWER\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\n:MYDRAWER:\n- Data\n:END:\nBody")
  (org-element-map (org-element-parse-buffer) 'drawer
    (lambda (d) (org-element-property :drawer-name d))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: block switches
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_block_switches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"emacs-lisp\" \"-n\" \":results value :exports both\") (example-block nil \"-n\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE -n\nExample\n#+END_EXAMPLE")
  (org-element-map (org-element-parse-buffer) '(src-block example-block)
    (lambda (b)
      (list (org-element-type b)
            (org-element-property :language b)
            (org-element-property :switches b)
            (org-element-property :parameters b)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: footnote markup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_footnote_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] *bold* /italic/\n[fn:2] [[link][desc]]")
  (let* ((tree (org-element-parse-buffer))
         (fn (org-element-map tree 'footnote-reference
               (lambda (f) (org-element-property :label f))))
         (fd (org-element-map tree 'footnote-definition
               (lambda (d) (org-element-property :label d)))))
    (list fn fd)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: inline task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_inline_task() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-inlinetask)
  (insert "Body\n*************** TODO Inline\n*************** END\nMore")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (when (= (org-element-property :level h) 15)
        (list (org-element-property :raw-value h)
              (org-element-property :todo-keyword h))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: hierarchy contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_hierarchy_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"L1\" 2) (2 \"L2a\" 2) (3 \"L3a\" 0) (3 \"L3b\" 0) (2 \"L2b\" 1) (3 \"L3c\" 0) (1 \"L1b\" 1) (2 \"L2c\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n*** L3c\n* L1b\n** L2c")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (list (org-element-property :level h)
            (org-element-property :raw-value h)
            (length (org-element-contents h))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (let ((s '()))
    (org-set-startup-visibility 'overview)
    (push (list :overview
                (get-char-property (search-forward "H2") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          s)
    (org-set-startup-visibility 'content)
    (push (list :content
                (get-char-property (search-forward "H2") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          s)
    (org-set-startup-visibility 'all)
    (push (list :all
                (get-char-property (search-forward "H2") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          s)
    (nreverse s)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: outline path
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_outline_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"P\" \"T1\" \"S1\") 4 \"SS1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P\n** T1\n*** S1\n**** SS1\n** T2")
  (goto-char (point-min))
  (search-forward "SS1")
  (list (org-get-outline-path) (org-current-level) (org-get-heading t t t t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: agenda entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_agenda_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* WAITING D")
  (org-map-entries
    (lambda () (list (org-get-heading t t t t) (org-get-todo-state)))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: colview format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_colview_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %V\n* TODO [#A] T :tag:\n:PROPERTIES:\n:V: val\n:END:")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: macro expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_macro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greet; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello $1 and $2!\n{{{greet(Alice, Bob)}}}\n{{{greet(World, 42)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: dynamic block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_dynamic_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+BEGIN: clocktable :maxlevel 2\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\" 83 84 (face org-table) 84 85 (face org-table rear-nonsticky t display (space :relative-width 1)) 85 93 (face org-table) 93 97 (face org-table) 97 98 (face org-table display (space :relative-width 1.001)) 98 99 (face org-table) 99 100 (face org-table rear-nonsticky t display (space :relative-width 1)) 100 104 (face org-table) 104 106 (face org-table) 106 107 (face org-table display (space :relative-width 1.001)) 107 108 (face org-table) 108 109 (face org-table-row) 109 110 (face org-table) 110 134 (face org-table) 134 135 (face org-table-row) 135 136 (face org-table) 136 137 (face org-table rear-nonsticky t display (space :relative-width 1)) 137 149 (org-emphasis t font-lock-multiline t face (bold org-table)) 149 150 (face org-table display (space :relative-width 1.001)) 150 151 (face org-table) 151 152 (face org-table rear-nonsticky t display (space :relative-width 1)) 152 158 (org-emphasis t font-lock-multiline t face (bold org-table)) 158 159 (face org-table display (space :relative-width 1.001)) 159 160 (face org-table) 160 161 (face org-table-row))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN: clocktable :maxlevel 2\n#+END:")
  (goto-char (point-min))
  (org-dblock-update)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: entity replacement
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_entity_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\alpha \\\\beta \\\\gamma\" \"\\\\alpha \\\\beta \\\\gamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta \\gamma")
  (let ((before (buffer-string)))
    (org-toggle-pretty-entities)
    (list before (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: radio targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_radio_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"target1\" \"target2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target1>>>\n<<<target2>>>\nSee target1 and target2")
  (org-element-map (org-element-parse-buffer) 'radio-target
    (lambda (rt) (org-element-property :value rt))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: structure template
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_structure_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-try-structure-completion)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<s")
  (org-try-structure-completion)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: comment fixed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_comment_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"Comment\") (\"Fixed\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment\n: Fixed\nNormal")
  (let* ((tree (org-element-parse-buffer))
         (c (org-element-map tree 'comment
              (lambda (c) (org-element-property :value c))))
         (f (org-element-map tree 'fixed-width
              (lambda (f) (org-element-property :value f)))))
    (list c f)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: link types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_link_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//x\" \"https://x\") (\"file\" \"f\" \"file:f\") (\"id\" \"i\" \"id:i\") (\"elisp\" \"(+ 1)\" \"elisp:(+ 1)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[https://x][w]] [[file:f][f]] [[id:i][i]] [[elisp:(+ 1)][e]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)
                      (org-element-property :raw-link l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"T\") (\"AUTHOR\" \"A\") (\"EMAIL\" \"e\") (\"OPTIONS\" \"toc:nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A\n#+EMAIL: e\n#+OPTIONS: toc:nil")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: refile targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"P1\" \"P2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P1\n** T1\n* P2\n** T2")
  (mapcar 'car (org-refile-get-targets nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_pcomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\agr")
  (length (all-completions "\\ag" (pcomplete-entries))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: sparse dates
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_sparse_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"T1\" \"T2\" \"T3\" \"T4\") (\"T1\" \"T2\" \"T3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\nSCHEDULED: <2026-01-15>\n* T2\nSCHEDULED: <2026-01-20>\n* T3\nSCHEDULED: <2026-02-01>\n* T4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "SCHEDULED<=\"<2026-01-31>\"")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible) (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: multi-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_multi_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"A1\") (\"B\" \"B1\" \"B2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** A1\nBodyA")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (with-temp-buffer
    (org-mode)
    (insert "* B\n** B1\n** B2\nBodyB")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (nreverse r))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: deferred chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\" :pri 65 :tags (\"tag\") :var \"val\" :title \"Orig\") (:todo \"DONE\" :pri 66 :tags (\"newtag\") :var \"newval\" :title \"Changed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Orig :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (p1 (list :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el)
                   :var (org-entry-get nil "VAR")
                   :title (org-element-property :raw-value el))))
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (org-entry-put nil "VAR" "newval")
    (org-edit-headline "Changed")
    (let* ((el2 (org-element-at-point))
           (p2 (list :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :var (org-entry-get nil "VAR")
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: narrow widen
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* H1\\nBody 1\\n** H2\\nSub\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody 1\n** H2\nSub\n* H2b\nBody 2")
  (goto-char (point-min))
  (org-narrow-to-subtree)
  (let ((narrowed (buffer-string)))
    (widen)
    (list narrowed)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: end of subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (31 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((p1 (progn (org-end-of-subtree) (point))))
    (goto-char (point-min))
    (search-forward "H2a")
    (beginning-of-line)
    (let ((p2 (progn (org-end-of-subtree) (point))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: mark subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_mark_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n* H1b")
  (goto-char (point-min))
  (org-mark-subtree)
  (let ((m (mark))
        (p (point)))
    (list (< p m) p m)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: clone subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n** Sub1\n** Sub2")
  (goto-char (point-min))
  (org-clone-subtree 2)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: sort entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n* Apple\n* Mango\n* Banana")
  (org-sort-entries nil ?a)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: move subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_move_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\" \"D\") (\"A\" \"C\" \"B\" \"D\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C\n* D")
  (goto-char (point-min))
  (let ((o1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    (forward-line 1)
    (org-move-subtree-down)
    (let ((o2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      (list o1 o2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Final: copy paste subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fd_copy_paste_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\") (2 \"Sub1\") (1 \"H2\") (2 \"Sub2\") (1 \"H1\") (2 \"Sub1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** Sub1\n* H2\n** Sub2")
  (goto-char (point-min))
  (org-copy-subtree)
  (goto-char (point-max))
  (org-paste-subtree 1)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}
