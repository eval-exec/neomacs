//! Oracle parity tests for default toplevel value accessors.
//!
//! GNU implements `default-toplevel-value` and
//! `set-default-toplevel-value` in `src/eval.c`: they first search the
//! specpdl for an exact `SPECPDL_LET` or `SPECPDL_LET_DEFAULT` entry, and
//! otherwise fall back to `default-value` / `set-default`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_default_toplevel_value_let_default_and_alias_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/eval.c:default_toplevel_binding compares the requested symbol
    // with the saved specpdl symbol.  `specbind` resolves variable aliases to
    // the base symbol, so alias arguments intentionally miss that entry and
    // exercise the fallback path through GNU src/data.c default/set-default.
    let form = r#"
(let ((base 'neomacs--oracle-dtv-base)
      (alias 'neomacs--oracle-dtv-alias)
      (plain 'neomacs--oracle-dtv-plain))
  (dolist (sym (list base alias plain))
    (ignore-errors (fmakunbound sym))
    (ignore-errors (makunbound sym))
    (setplist sym nil))
  (unwind-protect
      (progn
        (defvaralias alias base)
        (set base 'base-global)
        (set plain 'plain-global)
        (list
         (condition-case err
             (default-toplevel-value 'neomacs--oracle-dtv-missing)
           (error (list (car err) (cdr err))))
         (let ((neomacs--oracle-dtv-plain 'plain-let))
           (list
            neomacs--oracle-dtv-plain
            (default-value plain)
            (default-toplevel-value plain)
            (set-default-toplevel-value plain 'plain-top-set)
            neomacs--oracle-dtv-plain
            (default-value plain)
            (default-toplevel-value plain)))
         (symbol-value plain)
         (default-value plain)
         (let ((neomacs--oracle-dtv-alias 'alias-let))
           (list
            neomacs--oracle-dtv-base
            neomacs--oracle-dtv-alias
            (default-value base)
            (default-value alias)
            (default-toplevel-value base)
            (default-toplevel-value alias)))
         (let ((neomacs--oracle-dtv-alias 'alias-let-2))
           (list
            (set-default-toplevel-value alias 'alias-top-set)
            neomacs--oracle-dtv-base
            neomacs--oracle-dtv-alias
            (default-toplevel-value base)
            (default-toplevel-value alias)
            (default-value base)
            (default-value alias)))
         (symbol-value base)
         (symbol-value alias)
         (default-value base)
         (default-value alias)
         (condition-case err
             (default-toplevel-value 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-default-toplevel-value 42 'bad)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (fmakunbound alias))
    (ignore-errors (makunbound alias))
    (ignore-errors (fmakunbound base))
    (ignore-errors (makunbound base))
    (ignore-errors (fmakunbound plain))
    (ignore-errors (makunbound plain))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 40)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
