//! Strong state-capture oracle tests — capture deep mutable state.
//!
//! These tests capture multiple pieces of mutable state after
//! operations to surface divergences. Every test returns structured
//! data, never bare booleans.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// State capture: headline + todo + tags + priority after edit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_headline_edit_all_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Original :old:\nBody")
  (goto-char (point-min))
  (let ((s1 (list (org-get-heading t t t t) (org-get-todo-state)
                  (org-get-priority (char-after)) (org-get-tags nil t))))
    (org-edit-headline "Changed")
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("new"))
    (let ((s2 (list (org-get-heading t t t t) (org-get-todo-state)
                    (org-get-priority (char-after)) (org-get-tags nil t))))
      (list s1 s2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: property put/get/delete cycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_property_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1\" \"2\" \"3\" nil ((\"CATEGORY\" . \"???\") (\"B\" . \"3\") (\"A\" . \"2\")) ((\"CATEGORY\" . \"???\") (\"B\" . \"3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  (let ((v1 (org-entry-get nil "A")))
    (org-entry-put nil "A" "2")
    (org-entry-put nil "B" "3")
    (let ((v2 (org-entry-get nil "A"))
          (v3 (org-entry-get nil "B"))
          (p1 (org-entry-properties nil 'standard)))
      (org-entry-delete nil "A")
      (let ((v4 (org-entry-get nil "A"))
            (p2 (org-entry-properties nil 'standard)))
        (list v1 v2 v3 v4 p1 p2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: property inheritance across levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_property_inherit_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"l1\" \"l1\" nil \"l1\" \"l3\" \"l3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: V root\n* L1\n:PROPERTIES:\n:V: l1\n:END:\n** L2\n*** L3\n:PROPERTIES:\n:V: l3\n:END:")
  (goto-char (point-min))
  (search-forward "L3")
  (let ((v3i (org-entry-get nil "V" 'inherit))
        (v3 (org-entry-get nil "V" nil)))
    (search-backward "L2")
    (let ((v2i (org-entry-get nil "V" 'inherit))
          (v2 (org-entry-get nil "V" nil)))
      (search-backward "L1")
      (let ((v1i (org-entry-get nil "V" 'inherit))
            (v1 (org-entry-get nil "V" nil)))
        (list v1 v1i v2 v2i v3 v3i)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: table formula recalc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_table_formula_recalc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((#(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)))) #(\"3\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) ((#(\"10\" 0 2 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"12\" 0 2 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)))) #(\"12\" 0 2 (face org-table)) #(\"7\" 0 1 (face org-table)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 1 | 2 |\n| 3 | 4 |\n#+TBLFM: $3=$1+$2")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((d1 (org-table-to-lisp))
        (r1 (org-table-get 1 3))
        (r2 (org-table-get 2 3)))
    (org-table-put 1 1 "10")
    (org-table-recalculate 'all)
    (let ((d2 (org-table-to-lisp))
          (r3 (org-table-get 1 3))
          (r4 (org-table-get 2 3)))
      (list d1 r1 r2 d2 r3 r4))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: checkbox toggle + statistics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_checkbox_stats_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* T [0%]\" \"* T [33%]\" \"* T [0%]\")""#]];
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
    (let ((h1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (forward-line 3)
      (org-toggle-checkbox)
      (org-update-statistics-cookies t)
      (goto-char (point-min))
      (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
        (list h0 h1 h2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: sparse tree visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_sparse_tree_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"D\" \"E\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n** TODO D\n** DONE E")
  (goto-char (point-min))
  (org-match-sparse-tree nil "TODO")
  (let ((vis '()) (hid '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((h (org-get-heading t t t t)))
        (when h
          (if (get-char-property (point) 'invisible)
              (push h hid) (push h vis))))
      (forward-line))
    (list (nreverse vis) (nreverse hid))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: element parse + modify + reparse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_element_parse_modify_reparse() {
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
// State capture: export environment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_export_environment() {
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
// State capture: link parse with attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_link_parse_attributes() {
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
// State capture: planning with repeaters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_planning_repeaters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((cumulate 1 week) nil) (nil (cumulate 1 month)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w -3d>\n* TODO M\nDEADLINE: <2026-01-20 +1m -1w>")
  (let* ((tree (org-element-parse-buffer))
         (plan (org-element-map tree 'planning
                 (lambda (p)
                   (let ((s (org-element-property :scheduled p))
                         (d (org-element-property :deadline p)))
                     (list (when s (list (org-element-property :repeater-type s)
                                         (org-element-property :repeater-value s)
                                         (org-element-property :repeater-unit s)))
                           (when d (list (org-element-property :repeater-type d)
                                         (org-element-property :repeater-value d)
                                         (org-element-property :repeater-unit d)))))))))
    plan))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: timestamp with repeater
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_timestamp_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active-range 2026 15 10 0 2026 15 nil nil nil) (active-range 2026 16 nil nil 2026 20 nil nil nil) (active 2026 25 nil nil 2026 25 cumulate 1 week))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 10:00-11:30>\n<2026-01-16>--<2026-01-20>\n<2026-01-25 Wed +1w>")
  (let* ((tree (org-element-parse-buffer))
         (ts (org-element-map tree 'timestamp
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
                       (org-element-property :repeater-unit t))))))
    ts))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: drawer with multiple types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_drawer_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"LOGBOOK\" 30 53) (\"MYDRAWER\" 53 77))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\n:MYDRAWER:\n- Data\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (dr (org-element-map tree 'drawer
               (lambda (d)
                 (list (org-element-property :drawer-name d)
                       (org-element-property :begin d)
                       (org-element-property :end d))))))
    dr))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: block with switches
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_block_switches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"emacs-lisp\" \"-n\" \":results value :exports both\") (example-block nil \"-n\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE -n\nExample\n#+END_EXAMPLE")
  (let* ((tree (org-element-parse-buffer))
         (bl (org-element-map tree '(src-block example-block)
               (lambda (b)
                 (list (org-element-type b)
                       (org-element-property :language b)
                       (org-element-property :switches b)
                       (org-element-property :parameters b))))))
    bl))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: footnote with markup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_footnote_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") ((\"1\" 24 47) (\"2\" 47 68)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] *bold* /italic/\n[fn:2] [[link][desc]]")
  (let* ((tree (org-element-parse-buffer))
         (fn (org-element-map tree 'footnote-reference
               (lambda (f) (org-element-property :label f))))
         (fd (org-element-map tree 'footnote-definition
               (lambda (d)
                 (list (org-element-property :label d)
                       (org-element-property :begin d)
                       (org-element-property :end d))))))
    (list fn fd)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: inline task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_inline_task() {
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
    tasks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: hierarchy with contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_hierarchy_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"L1\" 2) (2 \"L2a\" 2) (3 \"L3a\" 0) (3 \"L3b\" 0) (2 \"L2b\" 1) (3 \"L3c\" 0) (1 \"L1b\" 1) (2 \"L2c\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n*** L3c\n* L1b\n** L2c")
  (let* ((tree (org-element-parse-buffer))
         (struct (org-element-map tree 'headline
                   (lambda (h)
                     (list (org-element-property :level h)
                           (org-element-property :raw-value h)
                           (length (org-element-contents h)))))))
    struct))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: visibility cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_visibility_cycle() {
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
// State capture: outline path
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_outline_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"P\" \"T1\" \"S1\") 4 \"SS1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P\n** T1\n*** S1\n**** SS1\n** T2")
  (goto-char (point-min))
  (search-forward "SS1")
  (list (org-get-outline-path)
        (org-current-level)
        (org-get-heading t t t t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: agenda entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_agenda_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* WAITING D")
  (org-map-entries
    (lambda ()
      (list (org-get-heading t t t t)
            (org-get-todo-state)
            (org-entry-get nil "PRIORITY")))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: colview format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_colview_format() {
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
// State capture: macro expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_macro_expansion() {
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
// State capture: dynamic block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_dynamic_block() {
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
// State capture: entity replacement
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_entity_replacement() {
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
// State capture: radio targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_radio_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"target1\" \"target2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target1>>>\n<<<target2>>>\nSee target1 and target2")
  (let* ((tree (org-element-parse-buffer))
         (targets (org-element-map tree 'radio-target
                    (lambda (rt) (org-element-property :value rt)))))
    targets))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: structure template
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_structure_template() {
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
// State capture: comment and fixed-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_comment_fixed() {
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
// State capture: link types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_link_types() {
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
// State capture: keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_keywords() {
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
// State capture: refile targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_refile_targets() {
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
// State capture: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_pcomplete() {
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
// State capture: sparse tree dates
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_sparse_dates() {
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
          (if (get-char-property (point) 'invisible)
              (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// State capture: multi-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sc_multi_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"A1\") (\"B\" \"B1\" \"B2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** A1\nBodyA")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h)))
          r))
  (with-temp-buffer
    (org-mode)
    (insert "* B\n** B1\n** B2\nBodyB")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h)))
          r))
  (nreverse r))"##,
        expect,
    );
}
