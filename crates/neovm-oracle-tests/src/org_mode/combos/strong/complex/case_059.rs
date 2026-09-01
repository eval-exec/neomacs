//! Strong combo-complex-59 oracle tests — error recovery and
//! resilience workflows: babel NOWEB to non-existent, clock with
//! missing drawer, note store on arbitrary positions, table formula
//! error fix cycle, headline indentation after delete+insert,
//! id collision handling, nested block toggle visibility,
//! sort with mixed heading levels, dynamic block update failures,
//! and org-occur with special regex chars.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo59_babel_noweb_nonexistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 :executed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value :noweb yes\n<<ghost-block>>\n(+ 1 2)\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src")
      (condition-case e
          (progn (push (org-babel-execute-src-block) r)
                 (push :executed r))
        (error (push (list :error (car e)) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo59_clock_note_store_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:clocking-before-note t) (:note-result nil) (:clocking-after nil) (:clock-count 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task\n")
    (let ((r '()))
      (goto-char (point-min))
      (org-clock-in nil)
      ;; attempt to add note while clocking
      (push (list :clocking-before-note (org-clocking-p)) r)
      (condition-case e
          (let ((note (org-clock-out nil nil)))
            (push (list :note-result note) r))
        (error (push (list :note-error (car e)) r)))
      (push (list :clocking-after (org-clocking-p)) r)
      (push (list :clock-count (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo59_table_formula_error_refix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:first-result #(\"| a | b | a + b |\\n| 1 | 2 |     3 |\\n| 3 | 4 |     7 |\\n#+TBLFM: $3=$1+$2\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 18 (face org-table-row) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table rear-nonsticky t display (space :relative-width 1)) 28 32 (face org-table) 32 33 (face org-table) 33 34 (face org-table display (space :relative-width 1.001)) 34 35 (face org-table) 35 36 (face org-table-row) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table rear-nonsticky t display (space :relative-width 1)) 42 43 (face org-table) 43 44 (face org-table display (space :relative-width 1.001)) 44 45 (face org-table) 45 46 (face org-table rear-nonsticky t display (space :relative-width 1)) 46 50 (face org-table) 50 51 (face org-table) 51 52 (face org-table display (space :relative-width 1.001)) 52 53 (face org-table) 53 54 (face org-table-row))) (:fixed-result #(\"| a | b | a + b | c |\\n| 1 | 2 |     3 |   |\\n| 3 | 4 |     7 |   |\\n#+TBLFM: $3=$1+$2\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 21 (face org-table) 21 22 (face org-table-row) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table rear-nonsticky t display (space :relative-width 1)) 28 29 (face org-table) 29 30 (face org-table display (space :relative-width 1.001)) 30 31 (face org-table) 31 32 (face org-table rear-nonsticky t display (space :relative-width 1)) 32 36 (face org-table) 36 37 (face org-table) 37 38 (face org-table display (space :relative-width 1.001)) 38 39 (face org-table) 39 40 (face org-table rear-nonsticky t display (space :relative-width 1)) 40 41 (face org-table) 41 42 (face org-table display (space :relative-width 1.001)) 42 43 (face org-table) 43 44 (face org-table-row) 44 45 (face org-table) 45 46 (face org-table rear-nonsticky t display (space :relative-width 1)) 46 47 (face org-table) 47 48 (face org-table display (space :relative-width 1.001)) 48 49 (face org-table) 49 50 (face org-table rear-nonsticky t display (space :relative-width 1)) 50 51 (face org-table) 51 52 (face org-table display (space :relative-width 1.001)) 52 53 (face org-table) 53 54 (face org-table rear-nonsticky t display (space :relative-width 1)) 54 58 (face org-table) 58 59 (face org-table) 59 60 (face org-table display (space :relative-width 1.001)) 60 61 (face org-table) 61 62 (face org-table rear-nonsticky t display (space :relative-width 1)) 62 63 (face org-table) 63 64 (face org-table display (space :relative-width 1.001)) 64 65 (face org-table) 65 66 (face org-table-row))) (:to-lisp ((#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table)) #(\"a + b\" 0 5 (face org-table)) #(\"c\" 0 1 (face org-table))) (#(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table)) \"\") (#(\"3\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) \"\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |\n")
  (let ((r '()))
    ;; first attempt: bad formula
    (insert "#+TBLFM: $3=$1+$2\n")
    (goto-char (point-min))
    (condition-case e
        (progn (org-table-recalculate t) (org-table-align)
               (push (list :first-result (buffer-string)) r))
      (error (push (list :first-error (car e)) r)))
    ;; fix: insert missing column
    (goto-char (point-min))
    (search-forward "| a |") (end-of-line)
    (insert " c |")
    (goto-char (point-min))
    (forward-line 1)
    (end-of-line) (insert "   |")
    (forward-line)
    (end-of-line) (insert "   |")
    ;; recalc after fix
    (condition-case e
        (progn (org-table-recalculate t) (org-table-align)
               (push (list :fixed-result (buffer-string)) r))
      (error (push (list :fixed-error (car e)) r)))
    (goto-char (point-min))
    (push (list :to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_headline_indent_after_delete_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((1 \"A\") (2 \"B\") (2 \"C\") (1 \"D\"))) (:after-delete-insert ((1 \"A\") (2 \"C\") (2 \"X\") (1 \"D\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n** C\n* D\n")
  (let ((r '()))
    ;; initial
    (push (list :init (mapcar (lambda (h) (list (org-element-property :level h)
                                               (substring-no-properties (org-element-property :raw-value h))))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; delete B
    (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
    (let ((start (point)))
      (org-end-of-subtree)
      (delete-region start (point)))
    ;; insert new heading after A
    (goto-char (point-min))
    (search-forward "* A") (org-end-of-subtree)
    (insert "\n** X\nNew.\n")
    (push (list :after-delete-insert
                (mapcar (lambda (h) (list (org-element-property :level h)
                                          (substring-no-properties (org-element-property :raw-value h))))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_id_collision_double_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-id)
  (let ((org-id-link-to-org-use-id t)
        (org-id-track-globally nil))
    (insert "* A\n* B\n")
    (let ((r '()))
      ;; get/create ID on A
      (goto-char (point-min))
      (let ((idA (org-id-get-create)))
        (push (list :idA idA) r))
      ;; get/create ID on B
      (search-forward "* B") (beginning-of-line)
      (let ((idB (org-id-get-create)))
        (push (list :idB idB) r))
      ;; verify uniqueness
      (push (list :ids-unique (not (equal (plist-get (car (last r)) :idA)
                                         (plist-get (car r) :idB)))) r)
      ;; manually force non-unique (should be handled)
      (goto-char (point-min))
      (condition-case nil
          (progn (org-entry-put nil "ID" "manual-dup")
                 (push (list :manual-id (org-entry-get nil "ID")) r))
        (error nil))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo59_nested_block_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:quote-type (quote-block)) (:src-count 1) (:bold-count 1) (:src-lang \"emacs-lisp\") (:src-value \"(+ 1 2)\\n\") (:quote-has-src t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_QUOTE\n")
  (insert "*bold text*\n")
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n")
  (insert "#+END_QUOTE\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (quotes (org-element-map tree 'quote-block #'identity))
           (srcs (org-element-map tree 'src-block #'identity))
           (bolds (org-element-map tree 'bold #'identity)))
      (push (list :quote-type (mapcar #'org-element-type quotes)) r)
      (push (list :src-count (length srcs)) r)
      (push (list :bold-count (length bolds)) r)
      ;; src:language
      (when (car srcs)
        (push (list :src-lang (org-element-property :language (car srcs))) r)
        (push (list :src-value (org-element-property :value (car srcs))) r))
      ;; quote contents
      (when (car quotes)
        (push (list :quote-has-src (> (length (org-element-map (car quotes) 'src-block #'identity)) 0)) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_sort_mixed_level_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"Zebra\" \"Apple\" \"Mango\" \"Banana\" \"Cherry\" \"Date\")) (:after-alpha (\"Zebra\" \"Apple\" \"Mango\" \"Banana\" \"Cherry\" \"Date\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n** Apple\n** Mango\n* Banana\n** Cherry\n** Date\n")
  (let ((r '()))
    (push (list :init (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort top-level only by alphabetical
    (goto-char (point-min))
    (org-sort-entries nil ?a)
    (push (list :after-alpha (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                     (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_dynamic_block_failure_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:updated t) (:dblock-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n")
  ;; malformed dynamic block (missing :maxlevel)
  (insert "#+BEGIN: clocktable\n#+END:\n")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "#+BEGIN: clocktable") (beginning-of-line)
    (condition-case e
        (progn (org-dblock-update)
               (push (list :updated t) r))
      (error (push (list :update-error (car e)) r)))
    ;; element parse after attempted update
    (push (list :dblock-count (length (org-element-map (org-element-parse-buffer) 'dynamic-block #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_occur_special_regex_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:matched-bracket (\"Data [and] (parens) {braces}\" \"Line with | pipe and \\\\ backslash\")) (:matched-pipe (\"Data [and] (parens) {braces}\" \"Line with | pipe and \\\\ backslash\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Data [and] (parens) {braces}\n")
  (insert "** More \"quoted\" text\n")
  (insert "* Line with | pipe and \\ backslash\n")
  (let ((r '()))
    ;; match literal bracket
    (goto-char (point-min))
    (condition-case nil
        (progn (org-occur "\\[and\\]")
               (push (list :matched-bracket (org-element-map (org-element-parse-buffer nil t) 'headline
                                              (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r))
      (error (push (list :bracket-error t) r)))
    (org-remove-occur-highlights)
    ;; match pipe
    (condition-case nil
        (progn (org-occur "|")
               (push (list :matched-pipe (org-element-map (org-element-parse-buffer nil t) 'headline
                                           (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r))
      (error (push (list :pipe-error t) r)))
    (org-remove-occur-highlights)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo59_property_with_semicolons_newlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:notes nil) (:url nil) (:multi nil) (:config \"x=1;y=2;z=3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n:PROPERTIES:\n:NOTES: line1\nline2\nline3\n:URL: https://a.com;b=c;d=e\n:END:\n")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :notes (org-entry-get nil "NOTES")) r)
    (push (list :url (org-entry-get nil "URL")) r)
    ;; set newline-containing property
    (org-entry-put nil "MULTI" "a\nb\nc")
    (push (list :multi (org-entry-get nil "MULTI")) r)
    ;; set semicolon property
    (org-entry-put nil "CONFIG" "x=1;y=2;z=3")
    (push (list :config (org-entry-get nil "CONFIG")) r)
    (nreverse r)))"##,
        expect,
    );
}
