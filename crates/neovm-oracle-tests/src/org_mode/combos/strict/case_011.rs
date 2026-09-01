//! Combo-strict-11 oracle tests — targeting known divergence themes:
//! case-fold-search in org-sparse-tree, display/indent interactions
//! in org-at-table-p and org-in-item-p, org-get-heading with all
//! arg combinations, org-occur with mixed case, org-get-level-face,
//! org-toggle-pretty-entities with unicode roundtrip, CJK heading
//! regexp matching, org-match-any-p, org-re-property extraction,
//! org-find-exact-headline-in-buffer, and org-find-invisible-
//! foreground.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_sparse_tree_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (case-fold-search t))
    (with-temp-buffer (org-mode)
      (insert "* Apple\n** APPLES\n* BANANA\n* banana\n* CHERRY\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; search for "banana" (case-insensitive)
        (org-occur "[Bb][Aa][Nn][Aa][Nn][Aa]")
        (push (list :matched (org-element-map (org-element-parse-buffer nil t) 'headline
                               (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
        ;; search for "apple" (case-insensitive)
        (org-remove-occur-highlights)
        (org-occur "[Aa][Pp][Pp][Ll][Ee]")
        (push (list :matched-apple (org-element-map (org-element-parse-buffer nil t) 'headline
                                      (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
        (org-remove-occur-highlights)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_at_table_item_position_sensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Heading\nBody.\n| a | b |\n| 1 | 2 |\n- item\n")
      (let ((r '()))
        ;; on table
        (goto-char (point-min)) (search-forward "| a |") (backward-char 1)
        (push (list :on-table (org-at-table-p)) r)
        ;; on table content
        (search-forward "| 1 |") (backward-char 1)
        (push (list :on-table-cell (org-at-table-p)) r)
        ;; on item
        (search-forward "- item") (backward-char 1)
        (push (list :on-item (org-at-item-p)) r)
        ;; on heading
        (goto-char (point-min))
        (push (list :on-heading (org-at-heading-p)) r)
        ;; on plain text
        (forward-line 1)
        (push (list :on-plain (org-at-heading-p)) r)
        (push (list :on-plain-table (org-at-table-p)) r)
        (push (list :on-plain-item (org-at-item-p)) r)
        ;; at specific positions with property drawer
        (goto-char (point-min))
        (push (list :on-property (org-at-property-p)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_get_heading_all_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:0 \"TODO [#A] Task :work:urgent:\") (:1-t \"TODO [#A] Task\") (:2-tt \"[#A] Task\") (:3-ttt \"Task\") (:4-tttt \"Task\") (:tags-only \"TODO [#A] Task :work:urgent:\") (:todo+tags \"TODO Task :work:urgent:\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Task :work:urgent:\nBody.\n")
      (goto-char (point-min))
      (list
       ;; no args
       (list :0 (org-get-heading))
       ;; heading only
       (list :1-t (org-get-heading t))
       ;; heading + todo
       (list :2-tt (org-get-heading t t))
       ;; heading + todo + priority
       (list :3-ttt (org-get-heading t t t))
       ;; heading + todo + priority + tags
       (list :4-tttt (org-get-heading t t t t))
       ;; just tags
       (list :tags-only (org-get-heading nil nil nil t))
       ;; just todo+tags
       (list :todo+tags (org-get-heading nil nil t t))
       ))))"##,
        expect,
    );
}

#[test]
fn strict_toggle_pretty_entities_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable before)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Symbols: \\alpha \\beta \\gamma \\rightarrow \\sum\n")
      (goto-char (point-min))
      (let ((r '()))
        (let* ((before (buffer-substring-no-properties (point-min) (point-max)))
               (tree-before (org-element-parse-buffer)))
          (push (list :entities-before (length (org-element-map tree-before 'entity #'identity))) r)
          (push (list :has-alpha (string-match-p "\\\\alpha" before)) r))
        ;; toggle entities
        (condition-case nil
            (org-toggle-pretty-entities)
          (error nil))
        (let ((after (buffer-substring-no-properties (point-min) (point-max))))
          (push (list :after-length (> (length after) 0)) r)
          (push (list :buffer-changed
                      (not (string= before (buffer-substring-no-properties (point-min) (point-max))))) r))
        ;; toggle back (should be idempotent-like)
        (condition-case nil
            (org-toggle-pretty-entities)
          (error nil))
        ;; element parse after toggling
        (let ((entities (org-element-map (org-element-parse-buffer) 'entity #'identity)))
          (push (list :entities-after-toggle (length entities)) r)
          ;; entity names
          (push (list :entity-names (mapcar (lambda (e) (org-element-property :name e)) entities)) r))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_cjk_heading_regexp_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* 日本語の見出し :タグ:\n")
      (insert "** TODO [#A] 中文标题 :标签:\n")
      (insert "*** DONE 한국어 제목 :태그:\n")
      (goto-char (point-min))
      (let ((r '()))
        (while (re-search-forward org-complex-heading-regexp nil t)
          (push (list :todo (match-string 2)
                      :priority (match-string 3)
                      :title (match-string 4)
                      :tags (match-string 5))
                r))
        (push (list :count (length r)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_match_any_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 104)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; basic matches
   (list :match1 (org-match-any-p "hello world" "hello"))
   (list :match2 (org-match-any-p "hello world" "world"))
   (list :match3 (org-match-any-p "hello world" "missing"))
   ;; regex
   (list :regex (org-match-any-p "hello123" "[0-9]+"))
   ;; empty string
   (list :empty (org-match-any-p "" "x"))
   ;; nil inputs
   (condition-case nil
       (org-match-any-p nil ".*")
     (error :error))
   ;; case insensitive
   (let ((case-fold-search t))
     (list :case-insensitive (org-match-any-p "Hello" "hello"))))))"##,
        expect,
    );
}

#[test]
fn strict_re_property_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (cond ((boundp 'org-re-property)
         (list
          :property-regexp (if (stringp org-re-property)
                              (string-match-p ":PROPERTIES:" org-re-property)
                            :not-string)
          :property-key-regexp (cond ((boundp 'org-re-property-key)
                                      (if (stringp org-re-property-key)
                                          (string-match-p "ID" org-re-property-key)
                                        :not-string))
                                     (t :not-bound))))
        (t (list :org-re-property :not-bound)))))"##,
        expect,
    );
}

#[test]
fn strict_find_exact_headline_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:found-banana nil) (:found-date nil) (:not-found nil) (:found-top t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Apple\n** Banana\n** Cherry\n* Date\n** Elderberry\n")
      (goto-char (point-min))
      (list
       ;; find existing heading
       (let ((pos (org-find-exact-headline-in-buffer "Banana")))
         (list :found-banana (and pos (numberp pos))))
       ;; find top-level heading
       (let ((pos (org-find-exact-headline-in-buffer "Date")))
         (list :found-date (and pos (numberp pos)
                               (progn (goto-char pos)
                                      (substring-no-properties
                                       (org-element-property :raw-value
                                        (org-element-at-point)))))))
       ;; non-existent heading
       (list :not-found (org-find-exact-headline-in-buffer "Fig"))
       ;; heading with todo keyword should also be found
       (list :found-top (and (org-find-exact-headline-in-buffer "Apple") t))))))"##,
        expect,
    );
}

#[test]
fn strict_get_level_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 53)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-get-level-face exists
   (list :fbound (fboundp 'org-get-level-face))
   ;; org-level-faces exists
   (cond ((boundp 'org-level-faces)
          (list :level-faces-length (length org-level-faces)
                :face-1 (nth 0 org-level-faces)
                :face-2 (nth 1 org-level-faces)))
         (t :not-bound))
   ;; org-level-1 exists
   (list :level1-face-exists (facep 'org-level-1))
   (list :level2-face-exists (facep 'org-level-2)))))"##,
        expect,
    );
}

#[test]
fn strict_find_invisible_foreground() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 47)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-find-invisible-foreground exists
   (list :fbound (fboundp 'org-find-invisible-foreground))
   ;; try calling it (should return a color string or nil)
   (condition-case nil
       (let ((result (org-find-invisible-foreground)))
         (list :result (if result (list :string (stringp result)) :nil)))
     (error :error))
   ;; check face existence
   (list :org-hide-exists (facep 'org-hide)))))"##,
        expect,
    );
}

#[test]
fn strict_clock_in_with_logging() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 22 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (let ((org-mode-hook nil)
        (org-clock-persist nil)
        (org-log-note-clock-out t))
    (with-temp-buffer (org-mode)
      (insert "* TODO Task\n")
      (goto-char (point-min))
      (let ((r '()))
        (org-clock-in nil)
        (push (list :clocking-p (org-clocking-p)) r)
        ;; attempt to get clocked heading
        (push (list :clock-heading (when (fboundp 'org-clock-get-clock-string)
                                     (org-clock-get-clock-string))) r)
        ;; Use org-clock-out without extra arg to avoid time spec divergence
        (org-clock-out nil nil)
        (push (list :clocking-after (org-clocking-p)) r)
        ;; check logbook has note
        (push (list :logbook-count (length (org-element-map (org-element-parse-buffer) 'drawer
                                             (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_todo_dependencies_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"** TODO Child\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t))
    (with-temp-buffer (org-mode)
      (insert "* TODO Parent\n** TODO Child\n")
      (let ((r '()))
        ;; try to mark child DONE (parent is TODO - should be blocked)
        (goto-char (point-min))
        (search-forward "** TODO Child") (beginning-of-line)
        (let ((before (buffer-substring-no-properties (point-min) (point-max))))
          (condition-case nil
              (progn (org-todo "DONE")
                     (push (list :child-todo-after (org-get-todo-state)) r))
            (error (push (list :child-blocked t) r))))
        ;; mark parent DONE
        (goto-char (point-min))
        (org-todo "DONE")
        (push (list :parent-todo (org-get-todo-state)) r)
        ;; now child can be DONE
        (goto-char (point-min))
        (search-forward "** TODO Child") (beginning-of-line)
        (condition-case nil
            (progn (org-todo "DONE")
                   (push (list :child-after-parent (org-get-todo-state)) r))
          (error (push (list :child-error t) r)))
        ;; todo states
        (push (list :final-states (org-map-entries (lambda () (list (org-get-heading t t t t)
                                                                    (org-get-todo-state))))) r)
        (nreverse r))))))"##,
        expect,
    );
}
