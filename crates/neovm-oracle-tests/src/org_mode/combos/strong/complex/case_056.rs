//! Strong combo-complex-56 oracle tests — extreme multi-step chains:
//! table formulas combining vsum+vprod+vmean+vmin+vmax, element
//! adopt-extract complex cycles, deep structural mutations with full
//! verification, export all backends sequentially with no buffer
//! corruption, org-lint with specific checkers, multi-layer property
//! inheritance across global keywords, babel with multi-level NOWEB,
//! clock persistence, macro expansion with all params, and full
//! document lifecycle: create→edit→export→reedit→reexport.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo56_table_all_functions_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c | d | e |\n|---+---+---+---+---|\n")
  (insert "| 1 | 2 | 3 | 4 | 5 |\n| 6 | 7 | 8 | 9 |10 |\n")
  (insert "|   |   |   |   |   |\n")
  (insert "#+TBLFM: @>$1=vsum(@2$1..@-1$1)::@>$2=vprod(@2$2..@-1$2)")
  (insert "::@>$3=vmean(@2$3..@-1$3)::@>$4=vmax(@2$4..@-1$4)")
  (insert "::@>$5=vmin(@2$5..@-1$5)\n")
  (let ((r '()))
    (goto-char (point-min))
    (org-table-recalculate t) (org-table-align)
    ;; sum row
    (goto-char (point-min)) (forward-line 3)  ;; sum row
    (push (list :sum (org-table-get "a" nil)) r)
    (push (list :prod (org-table-get "b" nil)) r)
    (push (list :mean (org-table-get "c" nil)) r)
    (push (list :max (org-table-get "d" nil)) r)
    (push (list :min (org-table-get "e" nil)) r)
    ;; add a row and re-execute
    (goto-char (point-min))
    (forward-line 2)  ;; after first data row
    (org-table-insert-row)
    (insert " 3 | 5 | 7 | 2 | 9 ")
    (org-table-align)
    ;; update formula ranges to include new row
    (goto-char (point-max))
    (search-backward "#+TBLFM:")
    (kill-line)
    (insert "#+TBLFM: @>$1=vsum(@2$1..@-1$1)::@>$2=vprod(@2$2..@-1$2)")
    (insert "::@>$3=vmean(@2$3..@-1$3)::@>$4=vmax(@2$4..@-1$4)")
    (insert "::@>$5=vmin(@2$5..@-1$5)\n")
    (org-table-recalculate t) (org-table-align)
    ;; re-read after new row
    (goto-char (point-min))
    (forward-line 4)  ;; updated sum row
    (push (list :sum-after (org-table-get "a" nil)) r)
    (push (list :prod-after (org-table-get "b" nil)) r)
    ;; to-lisp
    (goto-char (point-min))
    (push (list :to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_element_adopt_extract_deep_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-adopt-element)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* P\n** C1\nC1 body.\n** C2\nC2 body.\n* Q\n** D1\nD1 body.\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (p (car (org-element-map tree 'headline
                    (lambda (h) (when (equal "P" (org-element-property :raw-value h)) h)))))
           (q (car (org-element-map tree 'headline
                    (lambda (h) (when (equal "Q" (org-element-property :raw-value h)) h)))))
           (c2 (and p (cadr (org-element-map p 'headline #'identity)))))
      ;; extract C2 from P
      (when c2
        (org-element-extract-element c2)
        (push (list :after-extract-p-children (length (org-element-map p 'headline #'identity))) r))
      ;; adopt C2 under Q
      (when c2
        (org-element-adopt-element q c2)
        (push (list :after-adopt-q-children (length (org-element-map q 'headline #'identity))) r)
        (push (list :q-children-names (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                              (org-element-map q 'headline #'identity))) r))
      ;; now extract C2 from Q and adopt back under P
      (when c2
        (org-element-extract-element c2)
        (org-element-adopt-element p c2)
        (push (list :after-readopt-p-children (length (org-element-map p 'headline #'identity))) r))
      ;; interpret the final tree
      (push (list :interpret-ok (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_deep_structural_mutations_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot move past superior level or buffer limit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n** C\n* D\n** E\n* F\n")
  (let ((r '()))
    ;; initial
    (push (list :init (mapcar (lambda (h) (list (org-element-property :level h)
                                                (substring-no-properties (org-element-property :raw-value h))))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; promote B (make it level 1 sibling of A)
    (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
    (org-metaleft)
    ;; demote F under D
    (goto-char (point-min)) (search-forward "* F") (beginning-of-line)
    (org-metaright) (org-metaright)
    ;; promote C
    (goto-char (point-min)) (search-forward "** C") (beginning-of-line)
    (org-metaleft)
    ;; move D down
    (goto-char (point-min)) (search-forward "* D") (beginning-of-line)
    (org-metadown) (org-metadown)
    ;; after all mutations
    (push (list :after-mutations
                (mapcar (lambda (h) (list (org-element-property :level h)
                                          (substring-no-properties (org-element-property :raw-value h))))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_export_all_backends_no_corruption() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ok t) (:ok t) (:ok t) (:ok t) (:ok t) (:ok t) (:ok nil) (:buffer-unchanged t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (require 'ox-html)
  (require 'ox-latex)
  (require 'ox-md)
  (require 'ox-texinfo)
  (require 'ox-man)
  (require 'ox-icalendar)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72)
        (org-icalendar-combined-agenda-file "/dev/null"))
    (insert "* Export Test\nBody.\n")
    (let* ((orig (buffer-substring-no-properties (point-min) (point-max)))
           (r '()))
      ;; export to many backends
      (dolist (backend '(ascii html latex md texinfo man icalendar))
        (condition-case nil
            (let ((out (org-export-as backend nil nil t)))
              (push (list :ok (and out (> (length out) 0))) r))
          (error (push (list :backend-error t) r))))
      ;; verify buffer unchanged after all exports
      (push (list :buffer-unchanged
                  (equal orig (buffer-substring-no-properties (point-min) (point-max)))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo56_org_lint_specific_checkers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:report-count 3) (:first-type [#(\"3\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate CUSTOM_ID property \\\"dup\\\"\" #s(org-lint-checker duplicate-custom-id \"Report duplicate CUSTOM_ID properties\" org-lint-duplicate-custom-id nil (link))]) (:lint-fbound t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-lint)
  (insert "* Duplicate ID\n:PROPERTIES:\n:CUSTOM_ID: dup\n:END:\n")
  (insert "** Also Dupe\n:PROPERTIES:\n:CUSTOM_ID: dup\n:END:\n")
  (insert "* Empty Src\n#+begin_src\n#+end_src\n")
  (insert "* Invalid block\n#+BEGIN_OLD_BLOCK\n#+END_OLD_BLOCK\n")
  (let ((r '()))
    (condition-case nil
        (let ((reports (org-lint)))
          (push (list :report-count (length reports)) r)
          ;; first report type
          (when reports
            (let ((first (car reports)))
              (push (list :first-type (nth 1 first)) r))))
      (error (push (list :lint-error t) r)))
    (push (list :lint-fbound (fboundp 'org-lint)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_multi_layer_property_global_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:l1-var \"y=2\") (:l1-header-args nil) (:l2-var \"z=3\") (:l3-var \"y=2 z=3\") (:l3-header-args nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: Header-Args :eval never-export\n")
  (insert "#+PROPERTY: var x=1\n")
  (insert "* L1\n:PROPERTIES:\n:var: y=2\n:END:\n")
  (insert "** L2\n:PROPERTIES:\n:var+: z=3\n:END:\n")
  (insert "*** L3\n")
  (let ((r '()))
    ;; check property values at each level
    (goto-char (point-min))
    (search-forward "* L1") (beginning-of-line)
    (push (list :l1-var (org-entry-get nil "var")) r)
    (push (list :l1-header-args (org-entry-get nil "Header-Args")) r)
    ;; L2
    (search-forward "** L2") (beginning-of-line)
    (push (list :l2-var (org-entry-get nil "var")) r)
    ;; L3 inherits
    (search-forward "*** L3") (beginning-of-line)
    (push (list :l3-var (org-entry-get nil "var" t)) r)
    (push (list :l3-header-args (org-entry-get nil "Header-Args" t)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_babel_multi_level_noweb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable wrapper-result)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: base\n")
    (insert "#+begin_src emacs-lisp :results value\n(setq base-val \"hello\")\n#+end_src\n\n")
    (insert "#+name: wrapper\n")
    (insert "#+begin_src emacs-lisp :results value :noweb yes\n<<base>>\n(concat base-val \" world\")\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :noweb yes\n<<wrapper>>\n(concat wrapper-result \"!\")\n#+end_src\n")
    (let ((r '()))
      ;; execute base
      (goto-char (point-min)) (search-forward "#+name: base")
      (search-forward "#+begin_src emacs-lisp") (org-babel-execute-src-block)
      ;; execute wrapper
      (search-forward "#+begin_src emacs-lisp") (push (org-babel-execute-src-block) r)
      ;; execute final
      (search-forward "#+begin_src emacs-lisp") (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo56_macro_all_params_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:macro-count 3) (:has-greet 121) (:has-sum nil) (:has-mult nil) (:no-macros nil))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello, $1!\n")
  (insert "#+MACRO: sum The sum of $1 and $2 is (eval (+ $1 $2))\n")
  (insert "#+MACRO: mult (eval (* $1 $2))\n")
  (insert "\n{{{greet(World)}}}\n")
  (insert "{{{sum(3,7)}}}\n")
  (insert "Product: {{{mult(6,7)}}}.\n")
  (let ((r '()))
    ;; macro keywords count
    (push (list :macro-count (length (org-element-map (org-element-parse-buffer) 'keyword
                                      (lambda (k) (when (equal "MACRO" (org-element-property :key k)) k))))) r)
    ;; interpret (macros expanded)
    (let ((tree (org-element-parse-buffer)))
      (condition-case nil
          (let ((interpreted (substring-no-properties (org-element-interpret-data tree))))
            (push (list :has-greet (string-match-p "World" interpreted)) r)
            (push (list :has-sum (string-match-p "10" interpreted)) r)
            (push (list :has-mult (string-match-p "42" interpreted)) r)
            (push (list :no-macros (not (string-match-p "{{{" interpreted))) r))
        (error (push (list :interpret-error t) r))))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo56_document_lifecycle_create_edit_export_reedit_reexport() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:create-headlines (\"Doc\" \"Section A\" \"Section B\")) (:export1-has-A 44) (:export1-has-B 88) (:edit-headlines (\"Doc\" \"Section A\" \"Section X\" \"Section B\")) (:export2-has-X 61) (:export2-has-A 44) (:export3-no-B t) (:export3-has-A 44))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    ;; 1. Create
    (insert "* Doc\n** Section A\nContent A.\n** Section B\nContent B.\n")
    (let ((r '()))
      (push (list :create-headlines
                  (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                          (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
      ;; 2. First export
      (let ((e1 (org-export-as 'ascii nil nil t)))
        (push (list :export1-has-A (and e1 (string-match-p "Content A" e1))) r)
        (push (list :export1-has-B (and e1 (string-match-p "Content B" e1))) r))
      ;; 3. Edit: add new heading, modify content
      (goto-char (point-min))
      (search-forward "** Section B") (beginning-of-line)
      (insert "** Section X\nNew content.\n")
      (let ((tree (org-element-parse-buffer)))
        (push (list :edit-headlines
                    (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                            (org-element-map tree 'headline #'identity))) r))
      ;; 4. Second export
      (let ((e2 (org-export-as 'ascii nil nil t)))
        (push (list :export2-has-X (and e2 (string-match-p "Section X" e2))) r)
        (push (list :export2-has-A (and e2 (string-match-p "Content A" e2))) r))
      ;; 5. Delete Section B, add TODO
      (goto-char (point-min))
      (search-forward "** Section B") (beginning-of-line)
      (let ((start (point)))
        (org-end-of-subtree)
        (delete-region start (point)))
      ;; 6. Third export
      (goto-char (point-min))
      (search-forward "** Section A") (beginning-of-line)
      (let ((e3 (org-export-as 'ascii nil nil t)))
        (push (list :export3-no-B (and e3 (not (string-match-p "Content B" e3)))) r)
        (push (list :export3-has-A (and e3 (string-match-p "Content A" e3))) r))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo56_clock_persistence_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:total-clocks 3) (:a-sum 0) (:b-sum 0) (:c-sum 0) (:logbooks 3) (:buffer \"* A\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n* B\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n* C\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil)
        (org-clock-out-remove-zero-clock-sum t))
    (insert "* A\n* B\n* C\n")
    (let ((r '()))
      ;; clock in/out on each
      (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
      (search-forward "* B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
      (search-forward "* C") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
      ;; total clock count
      (goto-char (point-min))
      (push (list :total-clocks (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
      ;; clock sum on each
      (push (list :a-sum (org-clock-sum-current-item)) r)
      (search-forward "* B") (beginning-of-line)
      (push (list :b-sum (org-clock-sum-current-item)) r)
      (search-forward "* C") (beginning-of-line)
      (push (list :c-sum (org-clock-sum-current-item)) r)
      ;; logbook entries
      (goto-char (point-min))
      (push (list :logbooks (length (org-element-map (org-element-parse-buffer) 'drawer
                                      (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))) r)
      (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}
