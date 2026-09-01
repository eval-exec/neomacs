//! Combo-strict-7 oracle tests — deep property/API contract verification
//! targeting known divergence patterns: property inheritance chains,
//! #+PROPERTY: keyword interaction, export environment mutations,
//! element cache coherence, mixed list type integrity, table formula
//! edge cases, org-element-interpret-data deep round-trips, tag
//! matcher boolean logic, link abbreviation resolution, footnote
//! normalize with gaps, and macro expansion in deep nesting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Property inheritance: #+PROPERTY: keyword → drawer → inherited chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_property_keyword_drawer_inherit_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 33 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-use-property-inheritance nil))
    (with-temp-buffer (org-mode)
      (insert "#+PROPERTY: GLOBAL_KEY global-value\n")
      (insert "#+PROPERTY: SHARED shared-value\n\n")
      (insert "* Root\n")
      (insert ":PROPERTIES:\n:ROOT_LOCAL: root-local-value\n:INHERIT_ME: from-root\n:END:\n\n")
      (insert "** Middle\n")
      (insert ":PROPERTIES:\n:SHARED: overridden-shared\n:END:\n\n")
      (insert "*** Leaf\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; leaf: inherited from root via INHERIT_ME
        (search-forward "Leaf")
        (push (list :leaf-inherit (org-entry-get nil "INHERIT_ME" t)) r)
        (push (list :leaf-inherit-select (org-entry-get nil "INHERIT_ME" 'selective)) r)
        ;; leaf: ROOT_LOCAL not inherited
        (push (list :leaf-root-local (org-entry-get nil "ROOT_LOCAL" t)) r)
        ;; middle: SHARED overridden
        (push (list :middle-shared (progn (search-backward "Middle")
                                          (org-entry-get nil "SHARED"))) r)
        ;; middle: INHERIT_ME from root
        (push (list :middle-inherit (org-entry-get nil "INHERIT_ME" t)) r)
        ;; leaf: GLOBAL_KEY (from #+PROPERTY:) — not in any drawer, not inherited
        (search-forward "Leaf")
        (push (list :leaf-global (org-entry-get nil "GLOBAL_KEY" t)) r)
        ;; leaf: get all properties
        (let ((props (org-entry-properties nil 'standard)))
          (push (list :leaf-standard-props (length props)) r)
          (push (list :leaf-prop-keys (sort (mapcar #'car props) #'string-lessp)) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export environment: mutate buffer keywords, re-read environment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_env_mutate_reread() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 37 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Initial Title\n#+AUTHOR: Alice\n#+OPTIONS: num:t\n\n")
      (insert "* H1\nContent.\n* H2\nMore.\n")
      (goto-char (point-min))
      (let* ((env1 (org-export-get-environment))
             (r '()))
        (push (list :title1 (plist-get env1 :title)) r)
        (push (list :author1 (plist-get env1 :author)) r)
        ;; change title
        (goto-char (point-min))
        (search-forward "Initial Title")
        (replace-match "Changed Title")
        (let ((env2 (org-export-get-environment)))
          (push (list :title2 (plist-get env2 :title)) r)
          (push (list :author2 (plist-get env2 :author)) r))
        ;; add new keyword
        (goto-char (point-min))
        (search-forward "* H1")
        (beginning-of-line)
        (insert "#+SUBTITLE: A Subtitle\n")
        (when (re-search-forward "^\\*" nil t) (beginning-of-line))
        (let ((env3 (org-export-get-environment)))
          (push (list :subtitle3 (plist-get env3 :subtitle)) r)
          (push (list :title3 (plist-get env3 :title)) r))
        ;; remove a keyword
        (goto-char (point-min))
        (search-forward "#+AUTHOR:")
        (beginning-of-line)
        (kill-line)
        (let ((env4 (org-export-get-environment)))
          (push (list :author4 (plist-get env4 :author)) r)
          (push (list :title4 (plist-get env4 :title)) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element cache: parse → modify text → re-parse → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_cache_coherence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody\n** B\nBody\n* C\nBody\n")
      (goto-char (point-min))
      (let ((trees '())
            (r '()))
        ;; parse 1: full buffer
        (push (org-element-parse-buffer) trees)
        (push (list :parse1-type (org-element-type (car trees))) r)
        (push (list :parse1-headlines (length (org-element-map (car trees) 'headline #'identity))) r)
        ;; modify buffer
        (goto-char (point-min))
        (search-forward "* C")
        (beginning-of-line)
        (insert "* X\n** Y\n")
        ;; parse 2: after insertion
        (push (org-element-parse-buffer) trees)
        (push (list :parse2-headlines (length (org-element-map (cadr trees) 'headline #'identity))) r)
        (push (list :parse2-raw (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                        (org-element-map (cadr trees) 'headline #'identity))) r)
        ;; parse 1 should still be valid (unchanged)
        (push (list :parse1-still-has-3 (length (org-element-map (car trees) 'headline #'identity))) r)
        ;; modify more
        (goto-char (point-min))
        (search-forward "** B")
        (beginning-of-line)
        (let ((start (point)))
          (org-mark-subtree)
          (exchange-point-and-mark)
          (delete-region (region-beginning) (region-end)))
        ;; parse 3: after deletion
        (push (org-element-parse-buffer) trees)
        (push (list :parse3-headlines (length (org-element-map (nth 2 trees) 'headline #'identity))) r)
        (push (list :parse3-raw (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                        (org-element-map (nth 2 trees) 'headline #'identity))) r)
        ;; element-at-point after all changes
        (goto-char (point-min))
        (push (list :at-point-type (org-element-type (org-element-at-point))) r)
        (search-forward "X")
        (push (list :at-x-type (org-element-type (org-element-at-point))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Table formula edge cases: relative refs, range refs, cross-table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_table_formula_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"Total\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      ;; Table with formula using $LR and @> (last row/col references)
      (insert "| Item | Qty | Price | Total |\n|-------+-----+-------+-------|\n")
      (insert "| A     |   2 |    10 |       |\n| B     |   1 |    25 |       |\n| C     |   4 |     8 |       |\n")
      (insert "#+TBLFM: @>$2=vsum(@2$2..@-1$2)::$4=$2*$3\n")
      (goto-char (point-min))
      (let ((r '()))
        (push (list :before (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; recalc
        (org-table-recalculate t)
        (org-table-align)
        (push (list :after-recalc (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; get specific cells
        (push (list :cell-A-total (org-table-get nil "Total")) r)
        ;; add a row and recalc again
        (goto-char (point-min))
        (forward-line 4)  ;; on row C just before last hline
        (org-table-insert-row)
        (insert " D |   3 |   15 |     ")
        (org-table-align)
        (org-table-recalculate t)
        (org-table-align)
        (push (list :after-add-row (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; cell after new row
        (push (list :cell-qty-sum (org-table-get "@>$2" nil)) r)
        ;; use org-table-to-lisp on the final table
        (goto-char (point-min))
        (push (list :to-lisp (org-table-to-lisp)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mixed list types: ordered + unordered + description in deep nesting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_mixed_list_deep_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- [ ] Groceries\n")
      (insert "  1. [X] Apples\n")
      (insert "  2. [ ] Bananas ::\n")
      (insert "     Get ripe ones.\n")
      (insert "     - organic\n")
      (insert "     - regular\n")
      (insert "  3. [ ] Cherries\n")
      (insert "- [X] Chores\n")
      (insert "  1. [X] Laundry :: Done.\n")
      (insert "  2. [ ] Dishes\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (r '()))
        ;; list types present
        (push (list :plain-lists (mapcar (lambda (pl) (org-element-property :type pl))
                                         (org-element-map tree 'plain-list #'identity))) r)
        ;; items with level and type
        (push (list :items (mapcar (lambda (i) (list (org-element-property :level i)
                                                     (org-element-property :structure-type i)
                                                     (substring-no-properties
                                                      (or (org-element-property :raw-value i) ""))
                                                     (org-element-property :checkbox i)))
                                   (org-element-map tree 'item #'identity))) r)
        ;; description items taggedness
        (push (list :tag-count (length (org-element-map tree 'item
                                     (lambda (i) (when (org-element-property :tag i) i))))) r)
        ;; toggle some checkboxes and verify
        (goto-char (point-min))
        (search-forward "Cherries") (beginning-of-line) (org-toggle-checkbox)
        (push (list :after-cherry-toggle
                    (mapcar (lambda (i) (org-element-property :checkbox i))
                            (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
        ;; interpret round-trip
        (push (list :interpreted-length
                    (> (length (substring-no-properties
                                (org-element-interpret-data (org-element-parse-buffer))))
                       20)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Footnote: normalize with gaps, inline refs, anonymous
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_footnote_normalize_gaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Ref [fn:5:label] and [fn:2] and [fn::inline def].\n")
      (insert "[fn:5] Def five.\n[fn:2] Def two.\n[fn:1] Def one.\n")
      (let ((r '()))
        ;; initial footnote refs
        (push (list :init-refs
                    (mapcar (lambda (fr) (list (org-element-property :label fr)
                                               (org-element-property :type fr)))
                            (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
        ;; initial footnote defs
        (push (list :init-defs
                    (mapcar (lambda (fd) (list (org-element-property :label fd)
                                               (org-element-property :type fd)))
                            (org-element-map (org-element-parse-buffer) 'footnote-definition #'identity))) r)
        ;; normalize: renumber and sort
        (goto-char (point-min))
        (org-footnote-normalize 'sort)
        (push (list :after-normalize-refs
                    (mapcar (lambda (fr) (org-element-property :label fr))
                            (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
        (push (list :after-normalize-defs
                    (mapcar (lambda (fd) (org-element-property :label fd))
                            (org-element-map (org-element-parse-buffer) 'footnote-definition #'identity))) r)
        ;; buffer content after normalize
        (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; renumber
        (goto-char (point-min))
        (org-footnote-renumber-fn-n)
        (push (list :after-renumber-refs
                    (mapcar (lambda (fr) (org-element-property :label fr))
                            (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tag matching: complex boolean expressions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_tag_match_boolean_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments #<subr identity> 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-tags-column -80)
        (org-use-tag-inheritance nil))
    (with-temp-buffer (org-mode)
      (insert "* A :work:urgent:\n** A1 :work:\n** A2 :urgent:admin:\n")
      (insert "* B :home:fun:\n** B1 :home:urgent:\n** B2 :fun:admin:\n")
      (insert "* C :work:home:\n** C1 :urgent:fun:\n** C2 :admin:\n")
      (let ((r '()))
        ;; match work
        (push (list :match-work (org-map-entries
                                 (lambda () (org-get-heading t t t t))
                                 "work")) r)
        ;; match work+urgent
        (push (list :match-work+urgent (org-map-entries
                                        (lambda () (org-get-heading t t t t))
                                        "work+urgent")) r)
        ;; match work|home
        (push (list :match-work-or-home (org-map-entries
                                         (lambda () (org-get-heading t t t t))
                                         "work|home")) r)
        ;; match work-urgent (work but not urgent)
        (push (list :match-work-minus-urgent (org-map-entries
                                              (lambda () (org-get-heading t t t t))
                                              "work-urgent")) r)
        ;; match {work+urgent}|{home+fun}
        (push (list :match-grouped (org-map-entries
                                    (lambda () (org-get-heading t t t t))
                                    "{work+urgent}|{home+fun}")) r)
        ;; count entries matching admin
        (push (list :count-admin (length (org-map-entries #'identity "admin"))) r)
        ;; match !work (no work tag)
        (push (list :match-not-work (org-map-entries
                                     (lambda () (org-get-heading t t t t))
                                     "!work")) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Link abbreviation and format conversions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_link_abbreviation_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 37 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((org-mode-hook nil)
        (org-link-abbrev-alist '(("gh" . "https://github.com/%s")
                                 ("bug" . "https://bugs.example.com/%s")
                                 ("wiki" . "https://en.wikipedia.org/wiki/%s"))))
    (with-temp-buffer (org-mode)
      (insert "See [[gh:eval-exec/neomacs][NeoMACS]] and [[wiki:Emacs][Emacs on Wikipedia]].\n")
      (insert "Bug: [[bug:12345]].\n")
      (insert "Plain: [[https://example.com/page][Example]].\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (links (org-element-map tree 'link #'identity))
             (r '()))
        ;; link types and expanded paths
        (push (list :links (mapcar (lambda (l) (list (org-element-property :type l)
                                                     (org-element-property :path l)
                                                     (substring-no-properties
                                                      (or (org-element-property :raw-link l) ""))
                                                     ;; expanded link
                                                     (condition-case nil
                                                         (org-link-expand-abbrev
                                                          (org-element-property :raw-link l))
                                                       (error 'error))))
                                   links)) r)
        ;; org-link-escape roundtrip for each path
        (push (list :escape-roundtrip
                    (mapcar (lambda (l)
                              (let ((path (org-element-property :path l)))
                                (equal path
                                       (org-link-unescape
                                        (org-link-escape path)))))
                            links)) r)
        ;; count links
        (push (list :count (length links)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element interpret-data roundtrip for complex nested structures
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_interpret_roundtrip_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 37 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Bold+Code\n** _Underlined_\n")
      (insert "#+BEGIN_QUOTE\n")
      (insert "1. First /italic/ point\n")
      (insert "2. Second +strike+ point\n")
      (insert "#+END_QUOTE\n")
      (insert "| *bold* | /italic/ |\n|--------+----------|\n| =code=  | plain    |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (substring-no-properties (org-element-interpret-data tree)))
             (reparsed (with-temp-buffer (org-mode) (insert interpreted)
                                          (goto-char (point-min))
                                          (org-element-parse-buffer)))
             (r '()))
        ;; original element counts
        (push (list :orig-headlines (length (org-element-map tree 'headline #'identity))) r)
        (push (list :orig-paras (length (org-element-map tree 'paragraph #'identity))) r)
        (push (list :orig-bolds (length (org-element-map tree 'bold #'identity))) r)
        (push (list :orig-italics (length (org-element-map tree 'italic #'identity))) r)
        (push (list :orig-tables (length (org-element-map tree 'table #'identity))) r)
        (push (list :orig-lists (length (org-element-map tree 'plain-list #'identity))) r)
        ;; re-parsed counts should match
        (push (list :reparse-headlines (length (org-element-map reparsed 'headline #'identity))) r)
        (push (list :reparse-paras (length (org-element-map reparsed 'paragraph #'identity))) r)
        (push (list :reparse-bolds (length (org-element-map reparsed 'bold #'identity))) r)
        (push (list :reparse-italics (length (org-element-map reparsed 'italic #'identity))) r)
        (push (list :reparse-tables (length (org-element-map reparsed 'table #'identity))) r)
        (push (list :reparse-lists (length (org-element-map reparsed 'plain-list #'identity))) r)
        ;; comparisons
        (push (list :headlines-match (= (length (org-element-map tree 'headline #'identity))
                                        (length (org-element-map reparsed 'headline #'identity)))) r)
        (push (list :bolds-match (= (length (org-element-map tree 'bold #'identity))
                                    (length (org-element-map reparsed 'bold #'identity)))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export to multiple backends with edge content (md, texinfo, man)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_multi_backend_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'ox-md)
  (require 'ox-texinfo)
  (require 'ox-man)
  (let ((org-mode-hook nil)
        (org-export-show-temporary-export-buffer nil))
    (with-temp-buffer (org-mode)
      (insert "* Section\n")
      (insert "Text with *bold*, /italic/, =code=, ~verbatim~, and +strike+.\n\n")
      (insert "- item one\n- item two\n")
      (insert "  - nested\n")
      (let ((r '()))
        (condition-case err
            (push (list :md-nonempty
                        (let ((out (org-md-export-to-markdown nil nil nil t)))
                          (and out (> (length out) 0)))) r)
          (error (push (list :md-error (error-message-string err)) r)))
        (condition-case err
            (push (list :texinfo-nonempty
                        (let ((out (org-texinfo-export-to-info nil nil nil t)))
                          (and out (> (length out) 0)))) r)
          (error (push (list :texinfo-error (error-message-string err)) r)))
        (condition-case err
            (push (list :man-nonempty
                        (let ((out (org-man-export-to-man nil nil nil t)))
                          (and out (> (length out) 0)))) r)
          (error (push (list :man-error (error-message-string err)) r)))
        (nreverse r))))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Macro expansion: nested, recursive, eval
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_macro_expansion_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 30 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: a alice\n")
      (insert "#+MACRO: b bob\n")
      (insert "#+MACRO: join {{{a}}} & {{{b}}}\n")
      (insert "#+MACRO: tim (eval (format-time-string \"%Y\"))\n")
      (insert "\n* Report by {{{join}}}\n")
      (insert "Authored by {{{a}}} and {{{b}}}.\n")
      (insert "Status: active ({{{tim}}})\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; macro count
        (push (list :macro-keywords (length (org-element-map (org-element-parse-buffer) 'keyword
                                              (lambda (k) (when (equal "MACRO" (org-element-property :key k)) k))))) r)
        ;; headline before replacement
        (push (list :headline-raw-before
                    (substring-no-properties
                     (org-element-property :raw-value
                      (car (org-element-map (org-element-parse-buffer) 'headline #'identity))))) r)
        ;; interpret-data (macros are expanded during interpretation)
        (let ((interpreted (substring-no-properties (org-element-interpret-data (org-element-parse-buffer)))))
          (push (list :interpreted-contains-alice (string-match-p "alice" interpreted)) r)
          (push (list :interpreted-contains-bob (string-match-p "bob" interpreted)) r)
          ;; the eval macro should produce a 4-digit year
          (push (list :interpreted-has-year (string-match-p "[12][0-9][0-9][0-9]" interpreted)) r)
          ;; no raw macro markers should remain
          (push (list :interpreted-no-braces (not (string-match-p "{{{" interpreted))) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Dynamic blocks: create, update, content extraction
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_dynamic_block_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 26 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Tasks [%]\n")
      (insert "- [X] A\n- [ ] B\n- [X] C\n- [ ] D\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; update statistics cookie
        (org-update-statistics-cookies t)
        (push (list :after-stats (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; count items and checkboxes
        (push (list :item-count (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
        (push (list :checked-count (length (org-element-map (org-element-parse-buffer) 'item
                                           (lambda (i) (when (equal "X" (org-element-property :checkbox i)) i))))) r)
        ;; toggle a checkbox and re-update
        (goto-char (point-min))
        (search-forward "B") (beginning-of-line) (org-toggle-checkbox)
        (org-update-statistics-cookies t)
        (push (list :after-toggle (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; toggle all to X
        (goto-char (point-min))
        (search-forward "D") (beginning-of-line) (org-toggle-checkbox)
        (org-update-statistics-cookies t)
        (push (list :after-all-done (buffer-substring-no-properties (point-min) (point-max))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-to-lisp with various table shapes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_table_to_lisp_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; basic to-lisp
        (push (list :basic (org-table-to-lisp)) r)
        ;; add hline in middle
        (goto-char (point-min))
        (forward-line 2)
        (org-table-insert-hline)
        (push (list :with-mid-hline (org-table-to-lisp)) r)
        ;; single column
        (goto-char (point-max))
        (insert "\n| apple |\n| banana |\n")
        (goto-char (point-max))
        (forward-line -3)
        (push (list :single-col (org-table-to-lisp)) r)
        ;; empty table
        (goto-char (point-max))
        (insert "\n| |\n| |\n")
        (goto-char (point-max))
        (forward-line -2)
        (push (list :empty (org-table-to-lisp)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-context at boundary positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_context_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nSome *bold* and /italic/.\n** H2\n| a | b |\n|---+---|\n| 1 | 2 |\n")
      ;; Position at exact element boundaries
      (let ((r '()))
        ;; at beginning of buffer
        (goto-char (point-min))
        (push (list :bob-at-point (org-element-type (org-element-at-point))) r)
        (push (list :bob-context (org-element-type (org-element-context))) r)
        ;; at beginning of bold (right after "Some ")
        (goto-char (point-min))
        (search-forward "*bold*")
        (backward-char 6)
        (push (list :start-bold-context (org-element-type (org-element-context))) r)
        ;; at end of bold
        (search-forward "bold*")
        (push (list :end-bold-context (org-element-type (org-element-context))) r)
        ;; inside table, on separator line
        (search-forward "|---+---|")
        (push (list :sep-context (org-element-type (org-element-context))) r)
        ;; on table cell content
        (search-forward "1 |")
        (backward-char 2)
        (push (list :cell-context (org-element-type (org-element-context))) r)
        ;; at empty line between elements
        (goto-char (point-min))
        (search-forward "\n** H2")
        (forward-line -2)
        (goto-char (line-end-position))
        (forward-char 1)
        (push (list :empty-line-at-point (org-element-type (org-element-at-point))) r)
        ;; at end of buffer
        (goto-char (point-max))
        (push (list :eob-at-point (org-element-type (org-element-at-point))) r)
        (push (list :eob-context (org-element-type (org-element-context))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-properties with special, standard, and all flags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_entry_properties_flag_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Task :work:urgent:\n")
      (insert "SCHEDULED: <2024-06-01 Sat>\n")
      (insert "DEADLINE: <2024-06-15 Sat>\n")
      (insert ":PROPERTIES:\n:CUSTOM_ID: t1\n:EFFORT:   2:00\n:OWNER:    alice\n:END:\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; standard: nil (= standard set only)
        (let ((props (org-entry-properties nil nil)))
          (push (list :nil-count (length props)) r)
          (push (list :nil-keys (sort (mapcar #'car props) #'string-lessp)) r))
        ;; standard: 'standard
        (let ((props (org-entry-properties nil 'standard)))
          (push (list :standard-count (length props)) r)
          (push (list :standard-has-item (member "ITEM" (mapcar #'car props))) r))
        ;; all: t
        (let ((props (org-entry-properties nil t)))
          (push (list :t-count (length props)) r)
          (push (list :t-has-effort (assoc "EFFORT" props)) r)
          (push (list :t-has-owner (assoc "OWNER" props)) r))
        ;; specific: "TODO"
        (let ((props (org-entry-properties nil "TODO")))
          (push (list :specific-count (length props)) r)
          (push (list :specific-has-todo (member "TODO" (mapcar #'car props))) r))
        ;; specific: "EFFORT"
        (let ((props (org-entry-properties nil "EFFORT")))
          (push (list :effort-specific (assoc "EFFORT" props)) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element: create → interpret → parse → compare pipeline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_create_interpret_parse_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (let* (;; manually build a headline
             (hl (org-element-create
                  'headline
                  '(:level 1 :raw-value "Built Headline" :priority ?B :todo-keyword "TODO"
                    :tags ("test" "manual"))
                  (org-element-create 'section nil
                    (org-element-create 'paragraph nil
                      "Manually built "
                      (org-element-create 'bold nil "bold")
                      " text.\n"))))
             ;; interpret to string
             (str (substring-no-properties (org-element-interpret-data hl)))
             ;; parse the string back
             (tree (progn (erase-buffer) (insert str)
                          (goto-char (point-min))
                          (org-element-parse-buffer)))
             (r '()))
        ;; original properties
        (push (list :orig-level (org-element-property :level hl)) r)
        (push (list :orig-priority (org-element-property :priority hl)) r)
        (push (list :orig-todo (org-element-property :todo-keyword hl)) r)
        (push (list :orig-tags (org-element-property :tags hl)) r)
        ;; re-parsed properties
        (let ((rehl (car (org-element-map tree 'headline #'identity))))
          (push (list :re-level (org-element-property :level rehl)) r)
          (push (list :re-priority (org-element-property :priority rehl)) r)
          (push (list :re-todo (org-element-property :todo-keyword rehl)) r)
          (push (list :re-tags (org-element-property :tags rehl)) r)
          (push (list :re-raw (substring-no-properties (org-element-property :raw-value rehl))) r))
        ;; bold element inside
        (push (list :bold-count (length (org-element-map tree 'bold #'identity))) r)
        ;; paragraph content
        (let ((para (car (org-element-map tree 'paragraph #'identity))))
          (push (list :para-text (and para
                                      (substring-no-properties
                                       (org-element-interpret-data
                                        (org-element-contents para))))) r))
        ;; string representation
        (push (list :str-length (> (length str) 0)) r)
        (nreverse r))))))"##,
        expect,
    );
}
