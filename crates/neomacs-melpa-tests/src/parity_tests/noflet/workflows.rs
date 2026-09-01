use expect_test::expect;

use super::ParityBatchCase;

/// The surface: the macro, its expander, and the base marker function,
/// plus the payload.
fn the_macro_surface_and_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_macro_surface_and_payload",
        r####"(list
 :source (nf7ae-test-source-state)
 :macro (macrop 'noflet)
 :expander (fboundp 'noflet|expand)
 :base (list :value (noflet|base)
             :doc (documentation-property 'noflet|base
                                          'function-documentation)))"####,
        expect![[
            r#"OK (:source (:upstream-tree "06ef64caedc804601aba7df0638d386f23803848" :feature t :version "20141102.1454") :macro t :expander t :base (:value :noflet :doc nil))"#
        ]],
    )
}

/// A basic override replaces the function inside `noflet' and the
/// original definition is restored after the body.
fn a_basic_override_applies_and_restores() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_basic_override_applies_and_restores",
        r####"(let ((before (symbol-function 'user-login-name)))
  (let ((inside (noflet ((user-login-name () "parity-user"))
                  (user-login-name)))
        (after-in (progn
                    (noflet ((user-full-name () "Parity User"))
                      (user-full-name))))
        (after (user-login-name)))
    (list :inside inside
          :after-in after-in
          :after after
          :restored (eq before (symbol-function 'user-login-name)))))"####,
        expect![[
            r#"OK (:inside "parity-user" :after-in "Parity User" :after "melpa-test" :restored t)"#
        ]],
    )
}

/// `this-fn' delegates to the original: the override intercepts some
/// calls and passes others straight through.
fn this_fn_delegates_to_the_original() -> ParityBatchCase {
    ParityBatchCase::value(
        "this_fn_delegates_to_the_original",
        r####"(noflet ((expand-file-name (name &optional default)
                (if (string-prefix-p "/sandbox/" name)
                    (concat "[SANDBOX]" name)
                  (funcall this-fn name default))))
  (list :intercepted (expand-file-name "/sandbox/file.el")
        :delegated (expand-file-name "file.el" "/home")))"####,
        expect![[r#"OK (:intercepted "[SANDBOX]/sandbox/file.el" :delegated "/home/file.el")"#]],
    )
}

/// Multiple simultaneous bindings apply together, nest with the inner
/// winning, and each level restores on exit.
fn multiple_and_nested_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "multiple_and_nested_bindings",
        r####"(noflet ((user-login-name () "outer-user")
              (user-real-login-name () "outer-real"))
  (let ((outer (list (user-login-name) (user-real-login-name))))
    (let ((inner
           (noflet ((user-login-name () "inner-user"))
             (list (user-login-name) (user-real-login-name)))))
      (list :outer outer
            :inner inner
            :after-inner (user-login-name)))))"####,
        expect![[
            r#"OK (:outer ("outer-user" "outer-real") :inner ("inner-user" "outer-real") :after-inner "outer-user")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_macro_surface_and_payload(),
        a_basic_override_applies_and_restores(),
        this_fn_delegates_to_the_original(),
        multiple_and_nested_bindings(),
    ]
}
