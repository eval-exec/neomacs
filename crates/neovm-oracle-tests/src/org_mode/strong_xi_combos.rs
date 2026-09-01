//! Strong xi combo oracle tests — comprehensive coverage.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Xi: document
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_doc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 8) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: D\n* TODO H1 :t:\nSCHEDULED: <2026-01-15>\n:PROPERTIES:\n:V: v\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (types (org-element-map tree (lambda (el) (org-element-type el)))))
    types))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_prop() {
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
// Xi: table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_tbl() {
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
// Xi: checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_cb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* T [0%]\" \"* T [33%]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [%]\n- [ ] a\n  - [ ] a1\n- [ ] b\n- [ ] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 2)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (list h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: sparse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_sp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"T1\" \"T2\" \"T3\" \"T4\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1 :w:\n* T2 :p:\n* T3 :w:\n* T4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "w")
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
// Xi: headline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_hl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" 65 (\"t\") (timestamp (:standard-properties [30 nil nil nil 42 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T :t:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:V: v\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (p (car (org-element-map (org-element-contents h) 'planning
                   (lambda (p) p)))))
    (list (org-element-property :todo-keyword h)
          (org-element-property :priority h)
          (org-element-property :tags h)
          (org-element-property :scheduled p)
          (org-element-property :deadline p))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_exp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"T\" 0 1 (:parent (#(\"T\" 0 1 (:parent #4)))))) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+OPTIONS: toc:nil\n* H")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title) (plist-get info :with-toc))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: element chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_ec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\" :pri 65 :tags (\"t\")) (:todo \"DONE\" :pri 66 :tags (\"n\") :title \"C\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T :t:\n:PROPERTIES:\n:V: v\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (p1 (list :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el))))
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("n"))
    (org-edit-headline "C")
    (let* ((el2 (org-element-at-point))
           (p2 (list :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: multi-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_mb() {
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

// ═══════════════════════════════════════════════════════════════════════
// Xi: planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_pl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((cumulate nil) (nil cumulate))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w -3d>\n* TODO M\nDEADLINE: <2026-01-20 +1m -1w>")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p)
      (let ((s (org-element-property :scheduled p))
            (d (org-element-property :deadline p)))
        (list (when s (org-element-property :repeater-type s))
              (when d (org-element-property :repeater-type d)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_bl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"emacs-lisp\" \"-n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n\n(+ 1 2)\n#+END_SRC")
  (org-element-map (org-element-parse-buffer) 'src-block
    (lambda (b) (list (org-element-property :language b)
                      (org-element-property :switches b)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: timestamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((active-range 2026 15) (active-range 2026 16))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 10:00-11:30>\n<2026-01-16>--<2026-01-20>")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (t) (list (org-element-property :type t)
                      (org-element-property :year-start t)
                      (org-element-property :day-start t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_lnk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"https\" \"//x\") (\"file\" \"f\") (\"id\" \"i\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[https://x][w]] [[file:f][f]] [[id:i][i]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: footnote
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\") (\"1\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] *bold*")
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
// Xi: outline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_ol() {
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
// Xi: visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_vi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (let ((s '()))
    (org-set-startup-visibility 'overview)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (org-set-startup-visibility 'content)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (org-set-startup-visibility 'all)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (nreverse s)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: sparse dates
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_sd() {
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
// Xi: macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_mc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Undefined Org macro: g; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: g H $1 $2!\n{{{g(A, B)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: dynamic block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_db() {
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
// Xi: structure template
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_st() {
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
// Xi: comment fixed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_cf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"C\") (\"F\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# C\n: F\nN")
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
// Xi: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_pc() {
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
// Xi: colview
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_cv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %PRIORITY\n* TODO [#A] T")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: entity radio
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_er() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\alpha \\\\beta\\n<<<t>>>\\nSee t\" \"\\\\alpha \\\\beta\\n<<<t>>>\\nSee t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta\n<<<t>>>\nSee t")
  (let ((b (buffer-string)))
    (org-toggle-pretty-entities)
    (list b (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: inline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_in() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-inlinetask)
  (insert "B\n*************** TODO Inline\n*************** END\nM")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (when (= (org-element-property :level h) 15)
        (list (org-element-property :raw-value h)
              (org-element-property :todo-keyword h))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_kw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"T\") (\"AUTHOR\" \"A\") (\"OPTIONS\" \"toc:nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A\n#+OPTIONS: toc:nil")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: agenda
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_ag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE T2\n* TODO T3")
  (org-map-entries
    (lambda ()
      (list (org-get-heading t t t t)
            (org-get-todo-state)))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: refile
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_rf() {
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
// Xi: statistics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_sts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T [66%]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [%]\n- [X] a\n- [ ] b\n- [X] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: property inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_pi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: V 1\n* L1\n:PROPERTIES:\n:V: 2\n:END:\n** L2\n*** L3")
  (goto-char (point-min))
  (search-forward "L3")
  (list (org-entry-get nil "V" 'inherit)
        (org-entry-get nil "V" nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Xi: hierarchy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_xi_hi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"L1\" 2) (2 \"L2a\" 2) (3 \"L3a\" 0) (3 \"L3b\" 0) (2 \"L2b\" 0) (1 \"L1b\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n* L1b")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (list (org-element-property :level h)
            (org-element-property :raw-value h)
            (length (org-element-contents h))))))"##,
        expect,
    );
}
