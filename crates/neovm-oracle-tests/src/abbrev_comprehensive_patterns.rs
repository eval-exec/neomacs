//! Oracle parity tests for the abbrev system:
//! `define-abbrev-table`, `abbrev-table-p`, `make-abbrev-table`, `define-abbrev`,
//! `abbrev-symbol`, `abbrev-expansion`, `abbrev-table-name-list`,
//! `clear-abbrev-table`, `insert-abbrev-table-description`, `abbrev-get`,
//! `abbrev-put`. Covers complex abbrev tables, nested lookups, properties,
//! system abbrev tables, and edge cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic abbrev table creation and predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_table_creation_and_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--test-abbrev-tbl1 nil)
  (defvar neovm--test-abbrev-tbl2 nil)
  (unwind-protect
      (progn
        (setq neovm--test-abbrev-tbl1 (make-abbrev-table))
        (setq neovm--test-abbrev-tbl2 (make-abbrev-table '(:case-fixed t)))
        (list
         ;; make-abbrev-table returns an obarray (vector)
         (abbrev-table-p neovm--test-abbrev-tbl1)
         ;; Also an abbrev table with properties
         (abbrev-table-p neovm--test-abbrev-tbl2)
         ;; Non-tables are not abbrev tables
         (abbrev-table-p nil)
         (abbrev-table-p 42)
         (abbrev-table-p "hello")
         (abbrev-table-p (make-vector 10 0))
         ;; An obarray created with make-abbrev-table has abbrev-table property
         (abbrev-table-p (make-abbrev-table '(:enable-function ignore)))))
    (makunbound 'neovm--test-abbrev-tbl1)
    (makunbound 'neovm--test-abbrev-tbl2)))"#;
    let expect = expect_test::expect![r#""OK (t t nil nil nil nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-abbrev-table and abbrev-table-name-list membership
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_define_abbrev_table_and_name_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-alpha nil
          :case-fixed nil)
        (list
         ;; The symbol should now be in abbrev-table-name-list
         (not (null (memq 'neovm--test-abt-alpha abbrev-table-name-list)))
         ;; The value of the symbol should be an abbrev table
         (abbrev-table-p (symbol-value 'neovm--test-abt-alpha))
         ;; Defining again with different docstring should not duplicate
         (progn
           (define-abbrev-table 'neovm--test-abt-alpha nil)
           (let ((count 0))
             (dolist (s abbrev-table-name-list)
               (when (eq s 'neovm--test-abt-alpha)
                 (setq count (1+ count))))
             count))))
    (makunbound 'neovm--test-abt-alpha)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-alpha abbrev-table-name-list))))"#;
    let expect = expect_test::expect![r#""OK (t t 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-abbrev, abbrev-symbol, abbrev-expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_define_and_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-beta nil)
        (let ((tbl (symbol-value 'neovm--test-abt-beta)))
          ;; Define several abbreviations
          (define-abbrev tbl "btw" "by the way")
          (define-abbrev tbl "afaik" "as far as I know")
          (define-abbrev tbl "imo" "in my opinion" nil :count 5)
          (list
           ;; abbrev-symbol returns the symbol for "btw"
           (not (null (abbrev-symbol "btw" tbl)))
           ;; abbrev-expansion returns the expansion string
           (abbrev-expansion "btw" tbl)
           (abbrev-expansion "afaik" tbl)
           (abbrev-expansion "imo" tbl)
           ;; Non-existent abbreviation
           (abbrev-expansion "xyz" tbl)
           ;; Symbol name matches the abbreviation
           (symbol-name (abbrev-symbol "btw" tbl))
           ;; Count property for "imo"
           (let ((sym (abbrev-symbol "imo" tbl)))
              (symbol-value sym)))))
    (makunbound 'neovm--test-abt-beta)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-beta abbrev-table-name-list))))"#;
    let expect = expect_test::expect![[
        r#""OK (t \"by the way\" \"as far as I know\" \"in my opinion\" nil \"btw\" \"in my opinion\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// abbrev-get and abbrev-put for custom properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_get_put_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-gamma nil)
        (let ((tbl (symbol-value 'neovm--test-abt-gamma)))
          (define-abbrev tbl "hw" "hello world")
          (let ((sym (abbrev-symbol "hw" tbl)))
            ;; Set custom properties via abbrev-put
            (abbrev-put sym :custom-tag 'important)
            (abbrev-put sym :priority 42)
            (abbrev-put sym :author "test")
            (list
             ;; Retrieve them with abbrev-get
             (abbrev-get sym :custom-tag)
             (abbrev-get sym :priority)
             (abbrev-get sym :author)
             ;; Non-existent property returns nil
             (abbrev-get sym :nonexistent)
             ;; Overwrite a property
             (progn (abbrev-put sym :priority 99)
                    (abbrev-get sym :priority))
             ;; The :count property is set by define-abbrev
             (abbrev-get sym :count)))))
    (makunbound 'neovm--test-abt-gamma)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-gamma abbrev-table-name-list))))"#;
    let expect = expect_test::expect![[r#""OK (important 42 \"test\" nil 99 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// clear-abbrev-table empties the table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_clear_abbrev_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-delta nil)
        (let ((tbl (symbol-value 'neovm--test-abt-delta)))
          (define-abbrev tbl "aaa" "alpha alpha alpha")
          (define-abbrev tbl "bbb" "beta beta beta")
          (define-abbrev tbl "ccc" "gamma gamma gamma")
          (let ((before-clear
                 (list (abbrev-expansion "aaa" tbl)
                       (abbrev-expansion "bbb" tbl)
                       (abbrev-expansion "ccc" tbl))))
            (clear-abbrev-table tbl)
            (let ((after-clear
                   (list (abbrev-expansion "aaa" tbl)
                         (abbrev-expansion "bbb" tbl)
                         (abbrev-expansion "ccc" tbl))))
              (list before-clear after-clear)))))
    (makunbound 'neovm--test-abt-delta)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-delta abbrev-table-name-list))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alpha alpha alpha\" \"beta beta beta\" \"gamma gamma gamma\") (nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parent table (chained) lookups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_parent_table_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; Create parent table with some abbreviations
        (define-abbrev-table 'neovm--test-abt-parent nil)
        (define-abbrev (symbol-value 'neovm--test-abt-parent) "pg" "parent greeting")
        (define-abbrev (symbol-value 'neovm--test-abt-parent) "pf" "parent farewell")
        ;; Create child table that inherits from parent
        (define-abbrev-table 'neovm--test-abt-child nil
          :parents (list (symbol-value 'neovm--test-abt-parent)))
        (define-abbrev (symbol-value 'neovm--test-abt-child) "cg" "child greeting")
        (let ((child (symbol-value 'neovm--test-abt-child))
              (parent (symbol-value 'neovm--test-abt-parent)))
          (list
           ;; Child's own abbreviation is found
           (abbrev-expansion "cg" child)
           ;; Parent's abbreviation found through inheritance
           (abbrev-expansion "pg" child)
           (abbrev-expansion "pf" child)
           ;; Non-existent in either
           (abbrev-expansion "zz" child)
           ;; Parent itself still works directly
           (abbrev-expansion "pg" parent))))
    (makunbound 'neovm--test-abt-parent)
    (makunbound 'neovm--test-abt-child)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-parent
                (delq 'neovm--test-abt-child abbrev-table-name-list)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"child greeting\" \"parent greeting\" \"parent farewell\" nil \"parent greeting\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// System abbrevs: :system flag behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_system_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-sys nil)
        (let ((tbl (symbol-value 'neovm--test-abt-sys)))
          ;; Define a system abbreviation
          (define-abbrev tbl "sys1" "system one" nil :system t)
          ;; Define a user abbreviation
          (define-abbrev tbl "usr1" "user one")
          (let ((sys-sym (abbrev-symbol "sys1" tbl))
                (usr-sym (abbrev-symbol "usr1" tbl)))
            (list
             ;; Both expand
             (abbrev-expansion "sys1" tbl)
             (abbrev-expansion "usr1" tbl)
             ;; System property differs
             (abbrev-get sys-sym :system)
             (abbrev-get usr-sym :system)
             ;; After clear, system abbrevs survive (Emacs behavior)
             ;; clear-abbrev-table removes user abbrevs but keeps system ones
             (progn
               (clear-abbrev-table tbl)
               (list (abbrev-expansion "sys1" tbl)
                     (abbrev-expansion "usr1" tbl)))))))
    (makunbound 'neovm--test-abt-sys)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-sys abbrev-table-name-list))))"#;
    let expect = expect_test::expect![[r#""OK (\"system one\" \"user one\" t nil (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-abbrev-table-description produces Lisp-readable output
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_insert_abbrev_table_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-desc nil)
        (let ((tbl (symbol-value 'neovm--test-abt-desc)))
          (define-abbrev tbl "fn" "function")
          (define-abbrev tbl "var" "variable")
          (with-temp-buffer
            (insert-abbrev-table-description 'neovm--test-abt-desc nil)
            ;; The buffer should contain a Lisp form
            (let ((content (buffer-string)))
              (list
               ;; Should mention the table name
               (not (null (string-match "neovm--test-abt-desc" content)))
               ;; Should contain the abbreviations
               (not (null (string-match "fn" content)))
               (not (null (string-match "var" content)))
               ;; Content should be non-empty
               (> (length content) 0))))))
    (makunbound 'neovm--test-abt-desc)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-desc abbrev-table-name-list))))"#;
    let expect = expect_test::expect![r#""OK (t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple operations sequence: define, expand, redefine, clear
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_lifecycle_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        (define-abbrev-table 'neovm--test-abt-life nil)
        (let ((tbl (symbol-value 'neovm--test-abt-life))
              (results nil))
          ;; Step 1: Define and check
          (define-abbrev tbl "abc" "alpha bravo charlie")
          (push (abbrev-expansion "abc" tbl) results)
          ;; Step 2: Redefine same abbreviation with new expansion
          (define-abbrev tbl "abc" "always be coding")
          (push (abbrev-expansion "abc" tbl) results)
          ;; Step 3: Add more, check count
          (define-abbrev tbl "def" "delta echo foxtrot")
          (define-abbrev tbl "ghi" "golf hotel india")
          (let ((count 0))
            (mapatoms (lambda (s)
                        (when (and (symbol-name s) (> (length (symbol-name s)) 0))
                          (setq count (1+ count))))
                      tbl)
            (push count results))
          ;; Step 4: Clear and verify empty
          (clear-abbrev-table tbl)
          (push (abbrev-expansion "abc" tbl) results)
          (push (abbrev-expansion "def" tbl) results)
          ;; Return results in order
          (nreverse results)))
    (makunbound 'neovm--test-abt-life)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-life abbrev-table-name-list))))"#;
    let expect =
        expect_test::expect![[r#""OK (\"alpha bravo charlie\" \"always be coding\" 3 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-abbrev-table with properties and define-abbrev-table with initial defs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abbrev_table_with_initial_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; define-abbrev-table with initial abbrev definitions
        (define-abbrev-table 'neovm--test-abt-init
          '(("mon" "Monday" nil :count 1)
            ("tue" "Tuesday" nil :count 2)
            ("wed" "Wednesday" nil :count 3)
            ("thu" "Thursday" nil :count 0)
            ("fri" "Friday" nil :count 10)))
        (let ((tbl (symbol-value 'neovm--test-abt-init)))
          (list
           (abbrev-table-p tbl)
           (abbrev-expansion "mon" tbl)
           (abbrev-expansion "tue" tbl)
           (abbrev-expansion "wed" tbl)
           (abbrev-expansion "thu" tbl)
           (abbrev-expansion "fri" tbl)
           ;; Non-existent
           (abbrev-expansion "sat" tbl)
           ;; Count property preserved
           (let ((sym (abbrev-symbol "fri" tbl)))
             (abbrev-get sym :count)))))
    (makunbound 'neovm--test-abt-init)
    (setq abbrev-table-name-list
          (delq 'neovm--test-abt-init abbrev-table-name-list))))"#;
    let expect = expect_test::expect![[
        r#""OK (t \"Monday\" \"Tuesday\" \"Wednesday\" \"Thursday\" \"Friday\" nil 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
