//! Strong combo-complex-9 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element map with predicate → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_map_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D\n* WAITING E")
  (let ((r ''))
    ;; all
    (push (list :all (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; TODO only
    (push (list :todo (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "TODO")))) r)
    ;; DONE only
    (push (list :done (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "DONE")))) r)
    ;; first match
    (push (list :first (org-element-map (org-element-parse-buffer) 'headline
                          (lambda (h) (org-element-property :raw-value h))
                          nil 'first-match)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex parent chain → verify lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"italic\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold /italic/ inside* text")
  (search-forward "italic")
  (let* ((obj (org-element-context))
         (chain '()))
    (let ((p obj))
      (while p
        (push (list (org-element-type p)
                    (when (org-element-property :contents-begin p)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin p)
                                  (org-element-property :contents-end p)))))
              chain)
        (setq p (org-element-property :parent p))))
    (list :chain (nreverse chain))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element properties → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_element_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:begin 1 :end 19 :post-blank 0 :contents-begin 5 :contents-end 19 :level 1 :raw-value \"H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n** H2\nSub")
  (goto-char (point-min))
  (let ((h1 (org-element-at-point)))
    (list :begin (org-element-property :begin h1)
          :end (org-element-property :end h1)
          :post-blank (org-element-property :post-blank h1)
          :contents-begin (org-element-property :contents-begin h1)
          :contents-end (org-element-property :contents-end h1)
          :level (org-element-property :level h1)
          :raw-value (org-element-property :raw-value h1))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element cache → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (let ((r ''))
    ;; initial cache
    (let ((s (org-element-cache-status)))
      (push (list :init-size (plist-get s :size)) r))
    ;; cache active
    (push (list :active (org-element-cache-active-p)) r)
    ;; modify buffer
    (insert "\nNew line")
    (let ((s (org-element-cache-status)))
      (push (list :after-mod (plist-get s :size)) r))
    ;; parse after modification
    (push (list :types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)) r)
    ;; reset cache
    (org-element-cache-reset)
    (let ((s (org-element-cache-status)))
      (push (list :after-reset (plist-get s :size)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex indent → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n*** H3\nDeep")
  (let ((r ''))
    ;; initial
    (push (list :init (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; indent mode
    (org-indent-mode 1)
    (let ((indents '()))
      (goto-char (point-min))
      (while (not (eobp))
        (let ((indent (get-char-property (point) 'line-prefix)))
          (when indent (push (list (line-number-at-pos) indent) indents)))
        (forward-line))
      (push (list :indents (nreverse indents)) r))
    ;; indent buffer
    (org-indent-indent-buffer)
    (let ((indents '()))
      (goto-char (point-min))
      (while (not (eobp))
        (let ((indent (get-char-property (point) 'line-prefix)))
          (when indent (push (list (line-number-at-pos) indent) indents)))
        (forward-line))
      (push (list :buffer-indents (nreverse indents)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex lint → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_lint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <invalid>\nBody [[broken]]")
  (let ((r ''))
    ;; lint
    (push (list :lint-count (length (org-lint))) r)
    ;; verify buffer unchanged
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex sort → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO C\n* DONE A\n* TODO B\n* DONE D")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :raw-value h)
                                          (org-element-property :todo-keyword h))))) r)
    ;; sort by todo
    (org-sort-entries nil ?o)
    (push (list :after-todo-sort (org-element-map (org-element-parse-buffer) 'headline
                                    (lambda (h) (list (org-element-property :raw-value h)
                                                      (org-element-property :todo-keyword h))))) r)
    ;; sort alphabetically
    (org-sort-entries nil ?a)
    (push (list :after-alpha-sort (org-element-map (org-element-parse-buffer) 'headline
                                    (lambda (h) (org-element-property :raw-value h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex clone → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n** Sub1\n** Sub2")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    ;; clone 2 times
    (goto-char (point-min))
    (org-clone-subtree 2)
    (push (list :after-clone (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h))))) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex toggle → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; toggle H2 to list
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-heading)
    (push (list :after-toggle (org-element-map (org-element-parse-buffer) '(headline plain-list item)
                                (lambda (e) (list (org-element-type e)
                                                  (org-element-property :raw-value e))))) r)
    ;; toggle back
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-heading)
    (push (list :after-restore (org-element-map (org-element-parse-buffer) 'headline
                                  (lambda (h) (org-element-property :raw-value h)))) r)
    ;; toggle H3 to item
    (goto-char (point-min))
    (forward-line 2)
    (org-toggle-heading)
    (push (list :after-toggle2 (org-element-map (org-element-parse-buffer) '(headline plain-list item)
                                  (lambda (e) (list (org-element-type e)
                                                    (org-element-property :raw-value e))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex move subtree → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo9_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n* B\n** B1\n* C\n** C1")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    ;; move A down
    (goto-char (point-min))
    (org-metadown)
    (push (list :after-down (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h))))) r)
    ;; move C up
    (goto-char (point-min))
    (search-forward "C\n")
    (beginning-of-line)
    (org-metaup)
    (push (list :after-up (org-element-map (org-element-parse-buffer) 'headline
                            (lambda (h) (list (org-element-property :level h)
                                              (org-element-property :raw-value h))))) r)
    (nreverse r)))"##,
        expect,
    );
}
