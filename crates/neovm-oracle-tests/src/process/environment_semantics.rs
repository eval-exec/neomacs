//! Oracle parity tests for GNU `process-environment` semantics.
//!
//! GNU layers `setenv`, `getenv`, and `substitute-env-in-file-name` in
//! `lisp/env.el` over `getenv-internal` from `src/callproc.c`.  The central
//! contract is that Lisp-visible `process-environment` is authoritative for
//! let-bound environment changes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_setenv_mutates_let_bound_process_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_ENV_A" "one")
  (setenv "NEOMACS_ORACLE_ENV_B" "two")
  (list
   (getenv "NEOMACS_ORACLE_ENV_A")
   (getenv "NEOMACS_ORACLE_ENV_B")
   (seq-filter (lambda (entry)
                 (and (stringp entry)
                      (string-match-p "\\`NEOMACS_ORACLE_ENV_[AB]\\(=\\|\\'\\)" entry)))
               process-environment)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"one\" \"two\" (\"NEOMACS_ORACLE_ENV_B=two\" \"NEOMACS_ORACLE_ENV_A=one\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setenv_nil_creates_negative_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_ENV_NEG" "present")
  (let ((before process-environment))
    (setenv "NEOMACS_ORACLE_ENV_NEG")
    (list
     (getenv "NEOMACS_ORACLE_ENV_NEG")
     (car process-environment)
     (getenv-internal "NEOMACS_ORACLE_ENV_NEG" process-environment)
     (not (equal before process-environment)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil \"NEOMACS_ORACLE_ENV_NEG\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_getenv_internal_explicit_env_list_first_match_and_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((env '("A=first" "B" "A=second" "C=")))
  (list
   (getenv-internal "A" env)
   (getenv-internal "B" env)
   (getenv-internal "C" env)
   (getenv-internal "D" env)))
"#;

    let expect = expect_test::expect![[r#""OK (\"first\" t \"\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_getenv_internal_explicit_env_list_strict_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((env '("AA=long" "A=short" 42 "B" ("C=bad") "C=value"
             "D=one" "D" "E=" "=empty-name" "F")))
  (list
   (getenv-internal "A" env)
   (getenv-internal "AA" env)
   (getenv-internal "B" env)
   (getenv-internal "C" env)
   (getenv-internal "D" env)
   (getenv-internal "E" env)
   (getenv-internal "" env)
   (getenv-internal "F" env)
   (getenv-internal "G" env)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"short\" \"long\" t \"value\" \"one\" \"\" \"empty-name\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setenv_internal_mutation_and_scan_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((env (list "A=old" "B=old" 42 "A=late")))
   (list (eq (setenv-internal env "A" "new" t) env)
         env))
 (let ((env (list "A=old" "B=old" 42 "C=late")))
   (list (setenv-internal env "C" "new" t)
         env))
 (let ((env (list "A=old" "B=old" 42 "B=late")))
   (list (setenv-internal env "B" nil nil)
         env))
 (let ((env (list "A=old" "B=old" 42 "A=late")))
   (list (eq (setenv-internal env "A" nil t) env)
         env))
 (let ((env (list "A=old" "B=old")))
   (list (setenv-internal env "C" nil nil)
         env)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((t (\"A=new\" \"B=old\" 42 \"A=late\")) ((\"C=new\" \"A=old\" \"B=old\" 42 \"C=late\") (\"A=old\" \"B=old\" 42 \"C=late\")) ((\"A=old\" 42 \"B=late\") (\"A=old\" 42 \"B=late\")) (t (\"A\" \"B=old\" 42 \"A=late\")) ((\"A=old\" \"B=old\") (\"A=old\" \"B=old\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_substitute_env_in_file_name_uses_lisp_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_ENV_DIR" "/tmp/env-root")
  (setenv "NEOMACS_ORACLE_ENV_LEAF" "leaf")
  (list
   (substitute-env-in-file-name "$NEOMACS_ORACLE_ENV_DIR/$NEOMACS_ORACLE_ENV_LEAF")
   (substitute-env-in-file-name "${NEOMACS_ORACLE_ENV_DIR}/x")
   (substitute-env-in-file-name "$NEOMACS_ORACLE_ENV_MISSING/x")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/env-root/leaf\" \"/tmp/env-root/x\" \"$NEOMACS_ORACLE_ENV_MISSING/x\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_environment_variables_scoping_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_WITH_ENV_A" "outer")
  (setenv "NEOMACS_ORACLE_WITH_ENV_B" "outer-b")
  (list
   (with-environment-variables
       (("NEOMACS_ORACLE_WITH_ENV_A" "inner")
        ("NEOMACS_ORACLE_WITH_ENV_B" nil)
        ("NEOMACS_ORACLE_WITH_ENV_C" "created"))
     (list
      (getenv "NEOMACS_ORACLE_WITH_ENV_A")
      (getenv "NEOMACS_ORACLE_WITH_ENV_B")
      (getenv-internal "NEOMACS_ORACLE_WITH_ENV_B" process-environment)
      (getenv "NEOMACS_ORACLE_WITH_ENV_C")
      (car process-environment)))
   (list
    (getenv "NEOMACS_ORACLE_WITH_ENV_A")
    (getenv "NEOMACS_ORACLE_WITH_ENV_B")
    (getenv "NEOMACS_ORACLE_WITH_ENV_C"))
   (with-environment-variables (("NEOMACS_ORACLE_WITH_ENV_A" "level1"))
     (with-environment-variables (("NEOMACS_ORACLE_WITH_ENV_A" "level2"))
       (getenv "NEOMACS_ORACLE_WITH_ENV_A")))
   (getenv "NEOMACS_ORACLE_WITH_ENV_A")
   (condition-case err
       (macroexpand '(with-environment-variables nil :body))
     (error (list (car err) (cdr err))))
   (condition-case err
       (with-environment-variables "not-a-list" :body)
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 29 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
