//! Strong combo-complex-50 oracle tests — multi-backend export pipelines,
//! element cache stress, org-id creation and retrieval, clock persistence,
//! citation export, org-lint checks, and multi-buffer parse consistency.
//!
//! These target areas where index divergence or behavioral gaps may surface.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Multi-backend export with complex content (html/latex/ascii/md/texinfo)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_multi_backend_export_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:html-has-table 646) (:html-has-bold 386) (:html-has-italic 399) (:html-has-link 499) (:html-has-ul 120) (:latex-has-section 0) (:latex-has-textbf 56) (:latex-has-tabular 315) (:latex-has-href 142) (:ascii-has-heading 2) (:ascii-has-bullet 123) (:md-success t) (:md-has-heading nil) (:md-has-bold nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-html)
  (require 'ox-latex)
  (require 'ox-ascii)
  (require 'ox-md)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Export Test\n")
    (insert "Text with *bold*, /italic/, =code=, +strike+, ~verbatim~.\n")
    (insert "Also a [[https://example.com][link to example]].\n\n")
    (insert "- bullet 1\n- bullet 2\n  - nested 2a\n")
    (insert "| X | Y |\n|---+---|\n| 1 | 2 |\n")
    (let ((r '()))
      ;; HTML export (just verify non-empty and has key patterns)
      (let ((html (org-export-as 'html nil nil t)))
        (push (list :html-has-table (and html (string-match-p "<table" html))) r)
        (push (list :html-has-bold (and html (string-match-p "<b>" html))) r)
        (push (list :html-has-italic (and html (string-match-p "<i>" html))) r)
        (push (list :html-has-link (and html (string-match-p "example.com" html))) r)
        (push (list :html-has-ul (and html (string-match-p "<ul>" html))) r))
      ;; LaTeX export
      (let ((latex (org-export-as 'latex nil nil t)))
        (push (list :latex-has-section (and latex (string-match-p "\\\\section" latex))) r)
        (push (list :latex-has-textbf (and latex (string-match-p "textbf" latex))) r)
        (push (list :latex-has-tabular (and latex (string-match-p "tabular" latex))) r)
        (push (list :latex-has-href (and latex (string-match-p "href" latex))) r))
      ;; ASCII
      (let ((ascii (org-export-as 'ascii nil nil t)))
        (push (list :ascii-has-heading (and ascii (string-match-p "Export Test" ascii))) r)
        (push (list :ascii-has-bullet (and ascii (string-match-p "bullet 1" ascii))) r))
      ;; Markdown
      (let ((md (condition-case err (org-md-export-as-markdown nil nil nil t)
                  (error (error-message-string err)))))
        (push (list :md-success (and md (stringp md) (> (length md) 0))) r)
        (when (stringp md)
          (push (list :md-has-heading (string-match-p "Export Test" md)) r)
          (push (list :md-has-bold (string-match-p "\\*\\*" md)) r)))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-buffer parse consistency: parse buffer A, then B, then A-like
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_multibuffer_parse_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  ;; buffer 1
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** B\nBody.\n* C\n")
    (let ((tree1 (org-element-parse-buffer)))
      (push (list :buf1-headlines (length (org-element-map tree1 'headline #'identity))) r)
      (push (list :buf1-paras (length (org-element-map tree1 'paragraph #'identity))) r)))
  ;; buffer 2 (different content)
  (with-temp-buffer
    (org-mode)
    (insert "| x | y |\n| 1 | 2 |\n")
    (let ((tree2 (org-element-parse-buffer)))
      (push (list :buf2-tables (length (org-element-map tree2 'table #'identity))) r)
      (push (list :buf2-cells (length (org-element-map tree2 'table-cell #'identity))) r)))
  ;; buffer 3 (same shape as buffer 1)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n** Q\nMore body.\n* R\n")
    (let ((tree3 (org-element-parse-buffer)))
      (push (list :buf3-headlines (length (org-element-map tree3 'headline #'identity))) r)
      (push (list :buf3-paras (length (org-element-map tree3 'paragraph #'identity))) r)
      ;; should match buffer 1 counts (2 sections = EACH: all body text in one section per top-level heading)
      (push (list :buf3-matches-buf1-headlines
                  (= (length (org-element-map tree3 'headline #'identity))
                     (car (plist-get (car r) :buf1-headlines)))) r)))
  ;; also check that buffer 1's tree is still usable
  ;; by re-parsing buffer 1 again fresh
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** B\nBody.\n* C\n")
    (push (list :buf1-reparse-headlines
                (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r))
  (nreverse r))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-id: get, create, goto, and store-link round-trip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_org_id_create_goto_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-id)
  (let ((org-id-link-to-org-use-id t))
    (insert "* Target Heading\nBody.\n* Other Heading\n")
    (let ((r '()))
      ;; Get or create ID for target heading
      (goto-char (point-min))
      (let ((id1 (org-id-get-create)))
        (push (list :id1-created (and id1 (stringp id1) (> (length id1) 0))) r)
        (push (list :id1-stored-in-entry (org-entry-get nil "ID")) r)
        ;; get the same ID again
        (let ((id1-again (org-id-get)))
          (push (list :id1-stable (equal id1 id1-again)) r)))
      ;; store-link on that heading
      (goto-char (point-min))
      (let ((link (org-store-link nil)))
        (push (list :store-link-created (and link (stringp link) (> (length link) 0))) r)
        (push (list :store-link-has-id (when (stringp link) (string-match-p "id:" link))) r))
      ;; get ID at other heading (should be nil initially)
      (goto-char (point-min))
      (search-forward "* Other Heading") (beginning-of-line)
      (let ((id2-before (org-id-get)))
        (push (list :other-no-id-before (null id2-before)) r)
        ;; create
        (let ((id2 (org-id-get-create)))
          (push (list :other-id-created (and id2 (stringp id2))) r)
          (push (list :other-has-id (and (org-entry-get nil "ID") t)) r)))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Citation API beyond basic parsing: oc-basic activate and export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_citation_activate_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:citation-count 2) (:ref-count 3) (:c1-refs (\"doe2024\" \"smith2023\")) (:c2-style nil) (:oc-basic-loaded t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'oc)
  (require 'oc-basic)
  (let ((org-cite-global-bibliography nil)
        (org-cite-basic-author-column-suffix nil))
    (insert "[cite:@doe2024; @smith2023]\n")
    (insert "[cite:see @jones2024 for details].\n")
    (let* ((tree (org-element-parse-buffer))
           (cites (org-element-map tree 'citation #'identity))
           (all-refs (org-element-map tree 'citation-reference #'identity))
           (r '()))
      ;; citation count
      (push (list :citation-count (length cites)) r)
      ;; reference count
      (push (list :ref-count (length all-refs)) r)
      ;; first citation reference keys
      (let ((c1 (car cites)))
        (when c1
          (push (list :c1-refs
                      (mapcar (lambda (ref) (org-element-property :key ref))
                              (org-element-map c1 'citation-reference #'identity))) r)))
      ;; second citation style
      (let ((c2 (cadr cites)))
        (when c2
          (push (list :c2-style
                      (when (org-element-property :style c2)
                        (substring-no-properties
                         (or (org-element-interpret-data
                              (org-element-property :style c2)) "")))) r)))
      ;; try to activate: just verify no error when calling oc-basic-register
      (push (list :oc-basic-loaded (fboundp 'org-cite-basic-activate)) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries with various sort types and property sort
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_sort_entries_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n:PROPERTIES:\n:PRIO: 3\n:END:\n")
  (insert "* B\n:PROPERTIES:\n:PRIO: 1\n:END:\n")
  (insert "* C\n:PROPERTIES:\n:PRIO: 2\n:END:\n")
  (insert "* D\n:PROPERTIES:\n:PRIO: 4\n:END:\n")
  (let ((r '()))
    ;; initial order
    (push (list :init (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort by property PRIO numerically
    (goto-char (point-min))
    (org-sort-entries nil ?r ?p "PRIO" nil #'string<)
    (push (list :after-sort (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort alphabetically
    (goto-char (point-min))
    (org-sort-entries nil ?a)
    (push (list :after-alpha (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                     (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; buffer after sorts
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-lint: basic lint checks on buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_org_lint_basic_checks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:lint-result-type cons) (:lint-report-count 3) (:lint-error \"Wrong type argument: listp, 2\") (:org-lint-fboundp t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-lint)
  (insert "* No Content\n* Empty :\n")
  (insert "* Duplicate CUSTOM_ID\n:PROPERTIES:\n:CUSTOM_ID: dup\n:END:\n")
  (insert "** Also Duplicate CUSTOM_ID\n:PROPERTIES:\n:CUSTOM_ID: dup\n:END:\n")
  (insert "* Missing Language\n#+begin_src\ncode\n#+end_src\n")
  (let ((r '()))
    ;; org-lint should return a list of reports
    (condition-case err
        (let ((reports (org-lint)))
          (push (list :lint-result-type (type-of reports)) r)
          (push (list :lint-report-count (length reports)) r)
          (push (list :lint-first-type
                      (when reports (nth 1 (safe-length (car-safe reports))))) r))
      (error (push (list :lint-error (error-message-string err)) r)))
    ;; check that org-lint is callable and it exists
    (push (list :org-lint-fboundp (fboundp 'org-lint)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element: at-point vs context divergence after rapid edits
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_element_at_point_vs_context_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:h-at headline) (:h-ctx headline) (:bold-at paragraph) (:bold-ctx bold) (:after-del-at paragraph) (:after-del-ctx paragraph) (:table-at table-row) (:table-ctx table-cell))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n\nSome *bold* and /italic/ and =code=.\n\n| a | b |\n| 1 | 2 |\n")
  (let ((r '()))
    ;; position on heading
    (goto-char (point-min))
    (push (list :h-at (org-element-type (org-element-at-point))) r)
    (push (list :h-ctx (org-element-type (org-element-context))) r)
    ;; position on bold
    (search-forward "*bold*")
    (backward-char 2)
    (push (list :bold-at (org-element-type (org-element-at-point))) r)
    (push (list :bold-ctx (org-element-type (org-element-context))) r)
    ;; edit: delete bold markers, making it plain text
    (search-backward "*") (delete-char 1)
    (search-forward "bold*") (delete-backward-char 1)
    ;; now context should be plain-text / paragraph
    (search-backward "bold")
    (push (list :after-del-at (org-element-type (org-element-at-point))) r)
    (push (list :after-del-ctx (org-element-type (org-element-context))) r)
    ;; position in table
    (goto-char (point-min))
    (search-forward "| 1 |") (backward-char 1)
    (push (list :table-at (org-element-type (org-element-at-point))) r)
    (push (list :table-ctx (org-element-type (org-element-context))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-recalculate with relative and range edge refs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_table_recalc_relative_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"sum\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | sum | avg |\n|---+---+-----+-----|\n")
  (insert "| 1 | 2 |     |     |\n| 3 | 4 |     |     |\n| 5 | 6 |     |     |\n")
  (insert "#+TBLFM: $3=$1+$2::$4=vmean($1..$2);%.1f\n")
  (let ((r '()))
    ;; recalc
    (goto-char (point-min))
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-recalc (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; get cells from each row
    (push (list :row1-sum (org-table-get "sum" nil)) r)
    (goto-char (point-min))
    (forward-line 2)
    (push (list :row2-sum (org-table-get "sum" nil)) r)
    (forward-line)
    (push (list :row3-sum (org-table-get "sum" nil)) r)
    ;; add an hline before last row
    (org-table-insert-hline)
    (insert " 7 | 8 |   |     ")
    (org-table-align)
    ;; update formula range to include new row
    (goto-char (point-max))
    ;; and recalc once more
    (goto-char (point-min))
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-add-row (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; to-lisp of final
    (goto-char (point-min))
    (push (list :to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with info arg (with-affiliated, no-recurse, first-match)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_element_map_info_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:all-types 14) (:list-types 8) (:no-recurse 14) (:first-match-type table) (:with-affiliated 1) (:tbl-caption (((#(\"Captioned\" 0 9 (:parent (#(\"Captioned\" 0 9 (:parent #7))))))))) (:tbl-name \"my-table\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "#+CAPTION: Captioned\n#+NAME: my-table\n| a | b |\n|---+---|\n| 1 | 2 |\n")
  (let* ((tree (org-element-parse-buffer))
         (r '()))
    ;; standard map: all elements of all types
    (push (list :all-types (length (org-element-map tree t #'identity))) r)
    ;; map with list of types
    (push (list :list-types (length (org-element-map tree '(table table-row table-cell) #'identity))) r)
    ;; map with no-recursion (only top level)
    (push (list :no-recurse (length (org-element-map tree t #'identity nil nil 'no-recursion))) r)
    ;; map with first-match
    (let ((first (org-element-map tree 'table #'identity nil 'first-match)))
      (push (list :first-match-type (when first (org-element-type first))) r))
    ;; map with affiliated
    (let ((with-aff (org-element-map tree 'table #'identity nil nil 'with-affiliated)))
      (push (list :with-affiliated (length with-aff)) r)
      (let ((tbl (car with-aff)))
        (when tbl
          (push (list :tbl-caption (org-element-property :caption tbl)) r)
          (push (list :tbl-name (org-element-property :name tbl)) r))))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// structure template completion via org-try-structure-completion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo50_structure_template_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:keys (\"a\" \"e\" \"q\" \"s\" \"E\")) (:vals (\"export ascii\" \"example\" \"quote\" \"src\" \"export html\")) (:before-completion \"<s\") (:completion-error t) (:after-completion \"<s\") (:q-template \"<q\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-tempo)
  (let ((org-structure-template-alist
         '(("a" . "export ascii")
           ("e" . "example")
           ("q" . "quote")
           ("s" . "src")
           ("E" . "export html")))
        (r '()))
    ;; verify template keys
    (push (list :keys (mapcar #'car org-structure-template-alist)) r)
    (push (list :vals (mapcar #'cdr org-structure-template-alist)) r)
    ;; try structure completion: insert <s and TAB
    (insert "<s")
    (push (list :before-completion (buffer-string)) r)
    ;; org-try-structure-completion may or may not trigger here
    (condition-case nil
        (progn (org-try-structure-completion) t)
      (error (push (list :completion-error t) r)))
    (push (list :after-completion (buffer-string)) r)
    ;; clear and try with <q
    (erase-buffer)
    (insert "<q")
    (condition-case nil
        (progn (org-try-structure-completion) t)
      (error nil))
    (push (list :q-template (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}
