//! Oracle parity tests for GNU DBus inhibitor lock primitives.
//!
//! GNU implements these in `src/dbusbind.c`: the inhibitor-lock registry starts
//! empty, and argument type checks run before DBus side effects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_dbus_inhibitor_lock_argument_checks_and_initial_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (dbus-registered-inhibitor-locks)
 (condition-case err
     (dbus-close-inhibitor-lock "not-a-lock")
   (error (cons (car err) (cdr err))))
 (condition-case err
     (dbus-make-inhibitor-lock 1 "why")
   (error (cons (car err) (cdr err))))
 (condition-case err
     (dbus-make-inhibitor-lock "sleep" 2)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (dbus-make-inhibitor-lock "shutdown" "why")
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function dbus-registered-inhibitor-locks)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_dbus_inhibitor_lock_registry_and_call_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (calls)
  (fset 'dbus-call-method
        (lambda (&rest args)
          (push args calls)
          -1))
  (let* ((what (copy-sequence "shutdown"))
         (lock1 (dbus-make-inhibitor-lock what "why"))
         (reg1 (dbus-registered-inhibitor-locks))
         (lock2 (dbus-make-inhibitor-lock what "why"))
         (reg2 (dbus-registered-inhibitor-locks))
         (copy-mutability
          (progn
            (setcar (car reg1) 99)
            (dbus-registered-inhibitor-locks)))
         (close1 (dbus-close-inhibitor-lock lock1))
         (reg3 (dbus-registered-inhibitor-locks))
         (close2 (dbus-close-inhibitor-lock lock1)))
    (list lock1 lock2 calls reg1 reg2 copy-mutability close1 reg3 close2)))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function dbus-make-inhibitor-lock)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
