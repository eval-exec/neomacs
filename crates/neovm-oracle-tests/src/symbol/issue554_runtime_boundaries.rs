//! Positioned-symbol regression forms for runtime helper boundaries.
//!
//! Expected results follow the cited GNU C contracts; the coordinating lane
//! validates these generated forms through sandbox-run before gating them.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

fn check(form: &str, expected: &str) {
    let (oracle, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(expected, &oracle, &neomacs);
}

#[test]
fn issue554_runtime_gnutls_error_and_warning_symbols_follow_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // gnutls.c:1008-1009 EQ,1023-1025 EQ/SYMBOLP,1329 CHECK_SYMBOL.
    check(
        r#"(let ((again (position-symbol 'gnutls-e-again 4))
                 (true (position-symbol t 5))
                 (warning (position-symbol :expired 6)))
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list (gnutls-errorp again)
                        (gnutls-errorp true)
                        (condition-case err (gnutls-error-fatalp true)
                          (error (car err)))
                        (condition-case err
                            (stringp (gnutls-peer-status-warning-describe warning))
                          (error (car err))))))
              '(nil t)))"#,
        "((t t error wrong-type-argument) (nil nil nil t))",
    );
}

#[test]
fn issue554_runtime_gnutls_algorithm_and_descriptor_keys_follow_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // gnutls.c:2521,2800,2890 SYMBOLP and2539,2816,2906 plist_get/EQ.
    // Each runtime supplies its own algorithm ID from its public inventory.
    check(
        r#"(let* ((algorithm (position-symbol 'SHA256 4))
                  (cipher (position-symbol 'AES-256-CBC 5))
                  (key (position-symbol :digest-algorithm-id 6))
                  (id (plist-get (cdr (assq 'SHA256 (gnutls-digests)))
                                 :digest-algorithm-id)))
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list (condition-case err
                            (length (gnutls-hash-digest algorithm "a"))
                          (error (car err)))
                        (condition-case err
                            (length (gnutls-hash-digest (list key id) "a"))
                          (error (car err)))
                        (condition-case err
                            (length (gnutls-hash-mac algorithm "k" "a"))
                          (error (car err)))
                        (condition-case err
                            (length (gnutls-symmetric-encrypt
                                     cipher (make-string 32 ?k)
                                     (make-string 16 ?a) (make-string 16 ?b)))
                          (error (car err))))))
              '(nil t)))"#,
        "((error error error error) (32 32 32 2))",
    );
}

#[test]
fn issue554_runtime_autoload_head_and_macro_type_use_eq_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // eval.c:2502 EQ(head,autoload),2506-2507 EQ(macro_only/type,macro).
    // Delete the freshly reserved path so attempting the implicit load always
    // reports file-missing; neither runtime can pick up a package accidentally.
    check(
        r#"(let* ((file (make-temp-file "issue554-autoload-"))
                  (head (position-symbol 'autoload 4))
                  (type (position-symbol 'macro 5))
                  (head-form (list head file nil nil nil))
                  (type-form (list 'autoload file nil nil type)))
             (delete-file file)
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list (condition-case err
                            (eq (autoload-do-load head-form) head-form)
                          (error (car err)))
                        (condition-case err
                            (eq (autoload-do-load type-form nil 'macro) type-form)
                          (error (car err))))))
              '(nil t)))"#,
        "((t t) (file-missing file-missing))",
    );
}

#[test]
fn issue554_runtime_autoload_function_argument_and_type_storage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // eval.c:Fautoload CHECK_SYMBOL(function);FUNCTIONP3109-3116 tests raw
    // NILP on the stored autoload TYPE, rather than SYMBOLP or EQ on its name.
    check(
        r#"(let ((function (position-symbol 'issue554-runtime-autoload 4))
                 (type (position-symbol 'macro 5)))
             (unwind-protect
                 (list (let ((symbols-with-pos-enabled nil))
                         (condition-case err
                             (autoload function "issue554-runtime-never-loaded"
                                       nil nil type)
                           (error (car err))))
                       (let ((symbols-with-pos-enabled t))
                         (autoload function "issue554-runtime-never-loaded" nil nil type)
                         (eq (nth 4 (symbol-function function)) type))
                       (let ((symbols-with-pos-enabled nil))
                         (functionp 'issue554-runtime-autoload))
                       (let ((symbols-with-pos-enabled t))
                         (functionp 'issue554-runtime-autoload)))
               (fmakunbound 'issue554-runtime-autoload)))"#,
        "(wrong-type-argument t nil nil)",
    );
}

#[test]
fn issue554_runtime_keyboard_descriptions_and_nested_lucid_events() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // keymap.c:2341 SYMBOLP;keyboard.c:7850,7862,7882 SYMBOLP.
    check(
        r#"(let ((left (position-symbol 'left 4))
                 (control (position-symbol 'control 5))
                 (base (position-symbol 'a 6)))
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list (condition-case err (single-key-description left)
                          (error (car err)))
                        (condition-case err (event-convert-list (list control base))
                          (error (car err)))
                        (condition-case err
                            (key-description (vector (list control base)))
                          (error (car err))))))
              '(nil t)))"#,
        "((error error error) (\"<left>\" 1 \"C-a\"))",
    );
}

#[test]
fn issue554_positioned_bindings_check_policy_after_rhs() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(mapcar
  (lambda (kind)
    (mapcar
     (lambda (initial)
       (let* ((swp (position-symbol 'issue554-binding-n 7))
              (rhs (list 'progn
                         (list 'setq 'symbols-with-pos-enabled (null initial))
                         7))
              (form (if (eq kind 'setq)
                        (list 'setq 'issue554-binding-n 7)
                      (list kind (list (list 'issue554-binding-n 7))
                            'issue554-binding-n)))
              (fun (list 'lambda nil form)))
         ;; Warm a bare-symbol body, then mutate its existing binder.
         ;; This exercises the live checks of an already compiled Tier-I body.
         (funcall fun)
         (if (eq kind 'setq)
             (progn (setcar (cdr form) swp)
                    (setcar (cdr (cdr form)) rhs))
           (setcar (car (car (cdr form))) swp)
           (if (eq kind 'let)
               (setcdr (car (cdr form))
                       (list (list 'issue554-binding-switch rhs)))
             (setcar (cdr (car (car (cdr form)))) rhs)))
         (let ((symbols-with-pos-enabled initial))
           (condition-case err (funcall fun)
             (error (car err))))))
     '(nil t)))
  '(setq let let*))"#,
        "((7 wrong-type-argument) (7 wrong-type-argument) (7 wrong-type-argument))",
    );
}

#[test]
fn issue554_positioned_setq_rhs_runs_before_symbol_error() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Exercise dynamic binding explicitly inside the lexical oracle wrapper.
    check(
        r#"(eval '(let ((symbols-with-pos-enabled nil)
      (ran nil)
      (swp (position-symbol 'issue554-binding-n 8)))
  (let ((result (condition-case err
                    (eval (list 'setq swp '(setq ran t)))
                  (error (car err)))))
    (list result ran))) nil)"#,
        "(wrong-type-argument t)",
    );
}

#[test]
fn issue554_positioned_let_initializers_run_before_symbol_error() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Exercise dynamic binding explicitly inside the lexical oracle wrapper.
    check(
        r#"(eval '(let ((symbols-with-pos-enabled nil)
      (ran nil)
      (swp (position-symbol 'issue554-binding-n 8)))
  (let ((result (condition-case err
                    (eval (list 'let (list (list swp 7)
                                          '(issue554-binding-switch (setq ran t)))
                                'issue554-binding-n))
                  (error (car err)))))
    (list result ran))) nil)"#,
        "(wrong-type-argument t)",
    );
}

#[test]
fn issue554_positioned_catch_matches_throw_policy_before_unwind() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(let ((symbols-with-pos-enabled nil)
      (swp (position-symbol 'issue554-binding-catch 8)))
  (list
   (catch 'issue554-binding-catch
     (let ((symbols-with-pos-enabled t)) (throw swp 7)))
   (catch swp
     (let ((symbols-with-pos-enabled nil)) (throw swp 9)))))"#,
        "(7 9)",
    );
}

#[test]
fn issue554_runtime_coding_conversion_and_destination_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU coding.c:9601–9605 CHECK_CODING_SYSTEM/EQ/setup;9621 original argument.
    check(
        r#"(let ((coding (position-symbol 'utf-8-unix 4))
                 (destination (position-symbol t 5)))
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list
                   (condition-case err
                       (equal (decode-coding-string "abc" coding) "abc")
                     (error (car err)))
                   (condition-case err
                       (equal (encode-coding-string "abc" coding) "abc")
                     (error (car err)))
                   (condition-case err
                       (equal (decode-coding-string "abc" 'utf-8-unix nil destination)
                              "abc")
                     (error (car err))))))
              '(nil t)))"#,
        "((wrong-type-argument wrong-type-argument wrong-type-argument) (t t t))",
    );
}

#[test]
fn issue554_runtime_coding_identity_records_original_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU coding.c:9601–9605 CHECK_CODING_SYSTEM/EQ/setup;9621 original argument.
    check(
        r#"(let ((coding (position-symbol 'utf-8-unix 4))
                 (symbols-with-pos-enabled t))
             (decode-coding-string "abc" coding)
             (symbol-with-pos-p last-coding-system-used))"#,
        "t",
    );
}
