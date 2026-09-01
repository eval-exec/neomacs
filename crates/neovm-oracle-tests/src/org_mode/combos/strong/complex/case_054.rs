//! Strong combo-complex-54 oracle tests — extreme multi-step
//! divergence-prone workflows: nested block parsing (quote inside
//! center inside example), element swap & restructure, element-create
//! for all abstract types, footnote create/delete/renumber/resort
//! cycle, citation complex style parsing, dynamic block multi-type
//! create/update, multi-backend export sequence with no cross-
//! contamination, narrow/edit/widen/narrow-again/stress, multi-temp-
//! buffer (5 buffers) isolation, and 3x parse/modify/reparse
//! stability check.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Nested block parsing: quote inside center inside example
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_nested_blocks_deep_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:example-type example-block) (:center-type nil) (:quote-type nil) (:bold-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_EXAMPLE\n")
  (insert "#+BEGIN_CENTER\n")
  (insert "#+BEGIN_QUOTE\n")
  (insert "Deeply nested content *bold*.\n")
  (insert "#+END_QUOTE\n")
  (insert "#+END_CENTER\n")
  (insert "#+END_EXAMPLE\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (example (car (org-element-map tree 'example-block #'identity)))
           (center (car (org-element-map tree 'center-block #'identity)))
           (quote (car (org-element-map tree 'quote-block #'identity)))
           (bolds (org-element-map tree 'bold #'identity)))
      (push (list :example-type (when example (org-element-type example))) r)
      (push (list :center-type (when center (org-element-type center))) r)
      (push (list :quote-type (when quote (org-element-type quote))) r)
      (push (list :bold-count (length bolds)) r)
      ;; lineage of bold should go: bold → paragraph → quote-block → center-block → example-block → section → headline → org-data
      (when (car bolds)
        (push (list :bold-lineage (mapcar #'org-element-type (org-element-lineage (car bolds)))) r)))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element swap & restructure across sections
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_element_swap_restructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:table-count-before 2) (:before-order (section section)) (:after-swap-order (section section)) (:interpretable t) (:table-count-after 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* A\nPara A.\n| 1 | 2 |\n* B\nPara B.\n| 3 | 4 |\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (tables (org-element-map tree 'table #'identity)))
      (push (list :table-count-before (length tables)) r)
      (push (list :before-order
                  (mapcar (lambda (tbl) (org-element-type (org-element-property :parent tbl))) tables)) r)
      ;; swap the two table elements
      (when (>= (length tables) 2)
        (org-element-swap-A-B (nth 0 tables) (nth 1 tables))
        (push (list :after-swap-order
                    (mapcar (lambda (tbl) (org-element-type (org-element-property :parent tbl)))
                            (org-element-map tree 'table #'identity))) r)
        ;; interpret the restructured tree
        (push (list :interpretable (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r)
        ;; table count unchanged
        (push (list :table-count-after (length (org-element-map tree 'table #'identity))) r))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element create for multiple types, interpret, and verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_element_create_multi_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:hl-todo \"TODO\") (:hl-priority 65) (:hl-tags (\"test\")) (:para-has-bold t) (:has-table t) (:re-headlines 1) (:re-tables 1) (:re-bolds 1) (:re-italics 1) (:interpreted-has-header 0) (:interpreted-length t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (let* ( ;; create elements of various types
         (hl (org-element-create 'headline
               '(:level 1 :raw-value "Created" :todo-keyword "TODO" :priority ?A :tags ("test"))))
         (para (org-element-create 'paragraph nil
                 (org-element-create 'bold nil "bold")
                 " plain "
                 (org-element-create 'italic nil "italic")))
         (table (org-element-create 'table '(:type org)
                  (org-element-create 'table-row '(:type standard)
                    (org-element-create 'table-cell nil "A")
                    (org-element-create 'table-cell nil "B"))))
         ;; assemble into org-data
         (data (org-element-create 'org-data nil hl
                                    (org-element-create 'section nil para)
                                    (org-element-create 'section nil table)))
         (interpreted (substring-no-properties (org-element-interpret-data data)))
         (r '()))
    (push (list :hl-todo (org-element-property :todo-keyword hl)) r)
    (push (list :hl-priority (org-element-property :priority hl)) r)
    (push (list :hl-tags (org-element-property :tags hl)) r)
    (push (list :para-has-bold (> (length (org-element-map data 'bold #'identity)) 0)) r)
    (push (list :has-table (> (length (org-element-map data 'table #'identity)) 0)) r)
    ;; reparse the interpreted string
    (let ((reparsed (with-temp-buffer (org-mode)
                      (insert interpreted)
                      (goto-char (point-min))
                      (org-element-parse-buffer))))
      (push (list :re-headlines (length (org-element-map reparsed 'headline #'identity))) r)
      (push (list :re-tables (length (org-element-map reparsed 'table #'identity))) r)
      (push (list :re-bolds (length (org-element-map reparsed 'bold #'identity))) r)
      (push (list :re-italics (length (org-element-map reparsed 'italic #'identity))) r))
    ;; interpreted contains expected text
    (push (list :interpreted-has-header (string-match-p "^\\*" interpreted)) r)
    (push (list :interpreted-length (> (length interpreted) 20)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Footnote: create multiple, delete some, renumber, resort, verify gaps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_footnote_multi_create_delete_renumber() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-renumber-fn-n)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Notes\n")
  (insert "Alpha[fn:aaa] Beta[fn:bbb] Gamma[fn:ccc] Delta[fn:ddd].\n")
  (insert "[fn:aaa] A.\n[fn:bbb] B.\n[fn:ccc] C.\n[fn:ddd] D.\n")
  (let ((r '()))
    ;; initial labels
    (push (list :init-refs (mapcar (lambda (fr) (org-element-property :label fr))
                                   (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; delete ref bbb and ddd
    (goto-char (point-min))
    (search-forward "[fn:bbb]") (replace-match "")
    (goto-char (point-min))
    (search-forward "[fn:ddd]") (replace-match "")
    ;; after delete
    (push (list :after-del-refs (mapcar (lambda (fr) (org-element-property :label fr))
                                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; renumber
    (org-footnote-renumber-fn-n)
    (push (list :after-renumber (mapcar (lambda (fr) (org-element-property :label fr))
                                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; normalize
    (org-footnote-normalize 'sort)
    (push (list :after-normalize (mapcar (lambda (fr) (list (org-element-property :label fr)
                                                            (org-element-property :type fr)))
                                         (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Citation: complex style parsing with multiple refs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_citation_complex_style_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:citation-count 3) (:style nil :ref-count 2 :ref-keys (\"doe2024\" \"smith2023\") :ref-prefixes (\"see \" \" \")) (:style \"text\" :ref-count 1 :ref-keys (\"jones2024\") :ref-prefixes (\"as mentioned in \")) (:style \"nocite\" :ref-count 1 :ref-keys (\"unreferenced\") :ref-prefixes (nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'oc)
  (insert "[cite:see @doe2024 for details; @smith2023, pp. 10-15].\n")
  (insert "[cite/text:as mentioned in @jones2024].\n")
  (insert "[cite/nocite:@unreferenced].\n")
  (let* ((tree (org-element-parse-buffer))
         (cites (org-element-map tree 'citation #'identity))
         (r '()))
    (push (list :citation-count (length cites)) r)
    (dolist (c cites)
      (let ((style (org-element-property :style c))
            (refs (org-element-map c 'citation-reference #'identity)))
        (push (list :style
                    (when style (substring-no-properties
                                 (or (org-element-interpret-data style) "")))
                    :ref-count (length refs)
                    :ref-keys (mapcar (lambda (ref) (org-element-property :key ref)) refs)
                    :ref-prefixes
                    (mapcar (lambda (ref)
                              (when (org-element-property :prefix ref)
                                (substring-no-properties
                                 (or (org-element-interpret-data
                                      (org-element-property :prefix ref)) ""))))
                            refs))
              r)))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Dynamic block: multiple types create/update
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_dynamic_block_multi_type_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:after-clocktable \"#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\\n* Task A\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n\") (:dblock-count 1) (:table-count 1))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task A\n** Sub A1\n")
    ;; add some clocks for clocktable
    (goto-char (point-min))
    (org-clock-in nil) (org-clock-out nil nil)
    (goto-char (point-min))
    (search-forward "** Sub A1") (beginning-of-line)
    (org-clock-in nil) (org-clock-out nil nil)
    ;; create clocktable
    (goto-char (point-min))
    (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:\n")
    (let ((r '()))
      ;; update dynamic block
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable") (beginning-of-line)
      (org-dblock-update)
      (push (list :after-clocktable (buffer-substring-no-properties (point-min) (point-max))) r)
      ;; element counts after update
      (push (list :dblock-count (length (org-element-map (org-element-parse-buffer) 'dynamic-block #'identity))) r)
      (push (list :table-count (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-backend export sequence with no cross-contamination
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_multi_backend_export_no_cross_contamination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (require 'ox-html)
  (require 'ox-latex)
  (require 'ox-md)
  (require 'ox-texinfo)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Multi Export\nBody text with *bold* and /italic/.\n")
    (let ((r '()))
      ;; export to all backends in sequence
      (let ((a (org-export-as 'ascii nil nil t)))
        (push (list :ascii-ok (and a (> (length a) 0))) r))
      (let ((h (org-export-as 'html nil nil t)))
        (push (list :html-ok (and h (> (length h) 0))) r))
      (let ((l (org-export-as 'latex nil nil t)))
        (push (list :latex-ok (and l (> (length l) 0))) r))
      (let ((m (condition-case nil (org-md-export-as-markdown nil nil nil t)
                 (error nil))))
        (push (list :md-ok (and m (stringp m) (> (length m) 0))) r))
      (let ((ti (condition-case nil (org-texinfo-export-to-info nil nil nil t)
                  (error nil))))
        (push (list :texinfo-ok (and ti (stringp ti) (> (length ti) 0))) r))
      ;; re-verify buffer unchanged by all the exporting
      (push (list :buffer-intact (string-match-p "Body text" (buffer-substring-no-properties (point-min) (point-max)))) r)
      (nreverse r))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-temp-buffer (5 buffers) isolation stress test
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_multibuffer_5_isolation_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  ;; buffer 1: headlines
  (with-temp-buffer (org-mode) (insert "* A\n** B\n* C\n")
    (push (list :b1-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r))
  ;; buffer 2: table
  (with-temp-buffer (org-mode) (insert "| x | y |\n| 1 | 2 |\n")
    (push (list :b2-cells (length (org-element-map (org-element-parse-buffer) 'table-cell #'identity))) r))
  ;; buffer 3: list
  (with-temp-buffer (org-mode) (insert "- a\n- b\n- c\n")
    (push (list :b3-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r))
  ;; buffer 4: mixed
  (with-temp-buffer (org-mode) (insert "* H\nText.\n| 9 | 8 |\n- item\n")
    (push (list :b4-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :b4-tables (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
    (push (list :b4-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r))
  ;; buffer 5: src block
  (with-temp-buffer (org-mode) (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (push (list :b5-src-blocks (length (org-element-map (org-element-parse-buffer) 'src-block #'identity))) r))
  ;; verify all results are correct
  (push (list :all-ok (and (= (plist-get (car (last r 5)) :b1-headlines) 3)
                           (= (plist-get (nth 3 r) :b2-cells) 4)
                           (= (plist-get (nth 2 r) :b3-items) 3)
                           (= (plist-get (nth 1 r) :b4-headlines) 1)
                           (= (plist-get (nth 0 r) :b5-src-blocks) 1))) r)
  (nreverse r))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 3x parse → modify → reparse → reinterpret stability check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_3x_parse_modify_reparse_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:iter1-headlines 2) (:iter1-after-todo 2) (:iter2-headlines 3) (:iter3-rows 2) (:final-headlines 3) (:final-cells 10) (:interpret-ok t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\nBody.\n* B\n| 1 | 2 |\n")
  (let ((r '()))
    ;; iteration 1
    (push (list :iter1-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (goto-char (point-min))
    (org-todo "DONE")
    (push (list :iter1-after-todo (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; iteration 2
    (goto-char (point-max))
    (insert "\n** C\nNew child.\n")
    (push (list :iter2-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; iteration 3
    (goto-char (point-min))
    (search-forward "| 1 | 2 |") (beginning-of-line)
    (org-table-insert-row)
    (insert "| 3 | 4 |")
    (org-table-align)
    (push (list :iter3-rows (length (org-element-map (org-element-parse-buffer) 'table-row #'identity))) r)
    ;; verify stability: headline count and cell count consistent
    (let* ((tree (org-element-parse-buffer))
           (hl-count (length (org-element-map tree 'headline #'identity)))
           (cell-count (length (org-element-map tree 'table-cell #'identity))))
      (push (list :final-headlines hl-count) r)
      (push (list :final-cells cell-count) r)
      (push (list :interpret-ok (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entities replacement in buffer and export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo54_entity_replacement_buffer_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:entities-before 5) (:latex-frags-before 0) (:buffer-after-toggle \"* Math symbols: \\\\alpha, \\\\beta, \\\\rightarrow\\nLaTeX: \\\\sum_{i=1}^{n} and \\\\int_{a}^{b}f(x)dx\\n\") (:export-has-alpha nil) (:export-has-beta nil) (:export-ok t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Math symbols: \\alpha, \\beta, \\rightarrow\n")
    (insert "LaTeX: \\sum_{i=1}^{n} and \\int_{a}^{b}f(x)dx\n")
    (let ((r '()))
      ;; before toggle: count entities
      (let ((tree1 (org-element-parse-buffer)))
        (push (list :entities-before (length (org-element-map tree1 'entity #'identity))) r)
        (push (list :latex-frags-before (length (org-element-map tree1 'latex-fragment #'identity))) r))
      ;; toggle pretty entities
      (goto-char (point-min))
      (condition-case nil
          (org-toggle-pretty-entities)
        (error nil))
      ;; after toggle: look at buffer
      (push (list :buffer-after-toggle (buffer-substring-no-properties (point-min) (point-max))) r)
      ;; export to ascii
      (let ((out (org-export-as 'ascii nil nil t)))
        (push (list :export-has-alpha (and out (string-match-p "\\\\alpha" out))) r)
        (push (list :export-has-beta (and out (string-match-p "\\\\beta" out))) r)
        (push (list :export-ok (and out (> (length out) 0))) r))
      (nreverse r))))"##,
        expect,
    );
}
