//! Strong zetta combo oracle tests — maximum multi-operation.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Zetta: full document lifecycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"P\" \"TODO\") (\"T1\" \"TODO\") (\"T2\" \"TODO\") (\"D\" nil) (\"S1\" \"DONE\") (\"S2\" \"TODO\")) ((\"P\" \"TODO\" nil) (\"T1\" \"DONE\" (\"d\")) (\"T2\" \"TODO\" nil) (\"D\" nil nil) (\"S1\" \"DONE\" nil) (\"S2\" \"TODO\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: L\n* TODO P\n** TODO T1\n** TODO T2\n* D\n** DONE S1\n** TODO S2")
  (let* ((tree (org-element-parse-buffer))
         (h1 (org-element-map tree 'headline
               (lambda (h) (list (org-element-property :raw-value h)
                                 (org-element-property :todo-keyword h))))))
    (goto-char (point-min))
    (search-forward "T1")
    (org-todo 'done)
    (org-set-tags '("d"))
    (goto-char (point-min))
    (let* ((tree2 (org-element-parse-buffer))
           (h2 (org-element-map tree2 'headline
                 (lambda (h) (list (org-element-property :raw-value h)
                                   (org-element-property :todo-keyword h)
                                   (org-element-property :tags h))))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: property chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pchain() {
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
// Zetta: table fst
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_tfst() {
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
// Zetta: checkbox stats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cbs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* T [0%]\" \"* T [33%]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [%]\n- [ ] a\n  - [ ] a1\n  - [ ] a2\n- [ ] b\n  - [ ] b1\n- [ ] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 2)
    (org-toggle-checkbox)
    (forward-line 1)
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
// Zetta: sparse tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_spt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"T1\" \"T2\" \"T3\" \"T4\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1 :w:\n* T2 :p:\n* T3 :w:u:\n* T4")
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
// Zetta: headline edit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_hle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Orig :old:\n:PROPERTIES:\n:V: val\n:END:\nBody")
  (goto-char (point-min))
  (let ((c1 (list (org-get-heading t t t t) (org-get-todo-state)
                  (org-get-priority (char-after)) (org-get-tags nil t)
                  (org-entry-get nil "V"))))
    (org-edit-headline "New")
    (org-set-tags '("n"))
    (list c1 (list (org-get-heading t t t t) (org-get-todo-state)
                   (org-get-priority (char-after)) (org-get-tags nil t)
                   (org-entry-get nil "V")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: clock effort
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_ce() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n:PROPERTIES:\n:EFFORT: 2:00\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-15 10:00]--[2026-01-15 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "EFFORT")
        (org-clock-sum-current-entry)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: link attr
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_la() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"i.png\" (((#(\"C\" 0 1 (:parent (#(\"C\" 0 1 (:parent #6)))))))) (\":width 300\") \"n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: C\n#+ATTR_HTML: :width 300\n#+NAME: n\n[[file:i.png]]")
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
// Zetta: element chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_ec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\" :pri 65 :tags (\"tag\") :var \"val\") (:todo \"DONE\" :pri 66 :tags (\"n\") :var \"n\" :title \"C\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T :tag:\n:PROPERTIES:\n:V: val\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (p1 (list :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el)
                   :var (org-entry-get nil "V"))))
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("n"))
    (org-entry-put nil "V" "n")
    (org-edit-headline "C")
    (let* ((el2 (org-element-at-point))
           (p2 (list :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :var (org-entry-get nil "V")
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: multi-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_mb() {
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
// Zetta: planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pl() {
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
// Zetta: block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_bl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"emacs-lisp\" \"-n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value\n(+ 1 2)\n#+END_SRC")
  (org-element-map (org-element-parse-buffer) 'src-block
    (lambda (b)
      (list (org-element-property :language b)
            (org-element-property :switches b)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: timestamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((active-range 2026 15) (active-range 2026 16))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 10:00-11:30>\n<2026-01-16>--<2026-01-20>")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (t)
      (list (org-element-property :type t)
            (org-element-property :year-start t)
            (org-element-property :day-start t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: link types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_lt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//x\") (\"file\" \"f\") (\"id\" \"i\") (\"elisp\" \"(+ 1)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[https://x][w]] [[file:f][f]] [[id:i][i]] [[elisp:(+ 1)][e]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: footnote
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fn() {
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
// Zetta: outline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_ol() {
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
// Zetta: visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_vi() {
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
// Zetta: sparse dates
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sd() {
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
// Zetta: macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_mc() {
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
// Zetta: dynamic block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_db() {
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
// Zetta: structure template
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_st() {
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
// Zetta: comment fixed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cf() {
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
// Zetta: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pc() {
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
// Zetta: colview
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cv() {
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
// Zetta: entity radio
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_er() {
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
// Zetta: inline task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_in() {
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
// Zetta: keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_kw() {
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
// Zetta: agenda
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_ag() {
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
// Zetta: refile
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_rf() {
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
// Zetta: statistics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sts() {
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
// Zetta: property inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pi() {
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
// Zetta: hierarchy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_hi() {
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
