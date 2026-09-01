//! Divergence tests: keymap + command + advice + error recovery combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_keymap_lookup_with_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-kla-xxx () (interactive) "original")
  (advice-add 'test-kla-xxx :filter-return
               (lambda (r) (concat r "+advised")))
  (let ((map (make-sparse-keymap)))
    (define-key map "x" 'test-kla-xxx)
    (list (lookup-key map "x")
          (eq (lookup-key map "x") 'test-kla-xxx)
          (commandp (lookup-key map "x"))
          (funcall (lookup-key map "x"))
          (string= (funcall (lookup-key map "x")) "original+advised")
          (advice-remove 'test-kla-xxx
                          (lambda (r) (concat r "+advised"))))) "#,
        expect,
    );
}

#[test]
fn divergence_command_error_recovery_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 21 75)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-cer-xxx () (interactive) (error "command error"))
  (advice-add 'test-cer-xxx :around
               (lambda (fn &rest args)
                 (condition-case e
                     (apply fn args)
                   (error (format "caught: %s" (cadr e))))))
  (let ((map (make-sparse-keymap)))
    (define-key map "e" 'test-cer-xxx)
    (list (condition-case e
              (funcall (lookup-key map "e"))
            (error 'not-caught))
          (string= (condition-case e
                      (funcall (lookup-key map "e"))
                    (error 'not-caught))
                   "caught: command error")
          (advice-remove 'test-cer-xxx
                          (lambda (fn &rest args)
                            (condition-case e
                                (apply fn args)
                              (error (format "caught: %s" (cadr e)))))))) #"#,
        expect,
    );
}

#[test]
fn divergence_keymap_parent_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (test-kp2-xxx t test-kp1-xxx t test-kp1-xxx t \"child\" t \"parent\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-kp1-xxx () "parent")
  (defun test-kp2-xxx () "child")
  (let ((parent (make-sparse-keymap))
        (child (make-sparse-keymap)))
    (define-key parent "a" 'test-kp1-xxx)
    (define-key parent "b" 'test-kp1-xxx)
    (set-keymap-parent child parent)
    (define-key child "a" 'test-kp2-xxx)
    (list (lookup-key child "a")
          (eq (lookup-key child "a") 'test-kp2-xxx)
          (lookup-key child "b")
          (eq (lookup-key child "b") 'test-kp1-xxx)
          (lookup-key parent "a")
          (eq (lookup-key parent "a") 'test-kp1-xxx)
          (funcall (lookup-key child "a"))
          (string= (funcall (lookup-key child "a")) "child")
          (funcall (lookup-key child "b"))
          (string= (funcall (lookup-key child "b")) "parent")))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_prefix_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t test-pm1-xxx t test-pm2-xxx t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-pm1-xxx () "cmd1")
  (defun test-pm2-xxx () "cmd2")
  (let ((prefix-map (make-sparse-keymap))
        (global-map (make-sparse-keymap)))
    (define-key prefix-map "a" 'test-pm1-xxx)
    (define-key prefix-map "b" 'test-pm2-xxx)
    (define-key global-map "\C-c" prefix-map)
    (list (keymapp (lookup-key global-map "\C-c"))
          (lookup-key (lookup-key global-map "\C-c") "a")
          (eq (lookup-key (lookup-key global-map "\C-c") "a") 'test-pm1-xxx)
          (lookup-key (lookup-key global-map "\C-c") "b")
          (eq (lookup-key (lookup-key global-map "\C-c") "b") 'test-pm2-xxx)
          (lookup-key global-map "a")
          (null (lookup-key global-map "a"))))) "#,
        expect,
    );
}

#[test]
fn divergence_where_is_internal_with_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t test-wi-xxx t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-wi-xxx () (interactive) "test")
  (let ((map (make-sparse-keymap)))
    (define-key map "t" 'test-wi-xxx)
    (let ((bindings (where-is-internal 'test-wi-xxx map)))
      (list (listp bindings)
            (>= (length bindings) 1)
            (lookup-key map "t")
          (eq (lookup-key map "t") 'test-wi-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_menu_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-mi-cmd-xxx () (interactive) "menu")
  (let ((map (make-sparse-keymap)))
    (define-key map [menu-bar test-menu]
      (cons "Test" (make-sparse-keymap "Test Menu")))
    (list (keymapp map)
          (keymapp (lookup-key map [menu-bar test-menu]))
          (string= (car-safe (lookup-key map [menu-bar test-menu])) "Test")))) "#,
        expect,
    );
}

#[test]
fn divergence_commandp_with_lambda_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t \"lambda-cmd\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((cmd1 (lambda () (interactive) "lambda-cmd"))
        (cmd2 (lambda () "non-interactive")))
    (let ((map (make-sparse-keymap)))
      (define-key map "a" cmd1)
      (define-key map "b" cmd2)
      (list (commandp cmd1)
            (null (commandp cmd2))
            (commandp (lookup-key map "a"))
            (null (commandp (lookup-key map "b")))
            (funcall (lookup-key map "a"))
            (string= (funcall (lookup-key map "a")) "lambda-cmd"))))) "#,
        expect,
    );
}

#[test]
fn divergence_accessible_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t test-ak1-xxx t test-ak2-xxx t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-ak1-xxx () "1")
  (defun test-ak2-xxx () "2")
  (let ((parent (make-sparse-keymap))
        (child (make-sparse-keymap)))
    (define-key parent "a" 'test-ak1-xxx)
    (define-key child "b" 'test-ak2-xxx)
    (set-keymap-parent child parent)
    (let ((accessible (accessible-keymaps child)))
      (list (listp accessible)
            (>= (length accessible) 1)
            (lookup-key child "a")
            (eq (lookup-key child "a") 'test-ak1-xxx)
            (lookup-key child "b")
            (eq (lookup-key child "b") 'test-ak2-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_unbind_rebind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"second\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-ur1-xxx () "first")
  (defun test-ur2-xxx () "second")
  (let ((map (make-sparse-keymap)))
    (define-key map "x" 'test-ur1-xxx)
    (let ((b1 (lookup-key map "x")))
      (define-key map "x" nil)
      (let ((b2 (lookup-key map "x")))
        (define-key map "x" 'test-ur2-xxx)
        (let ((b3 (lookup-key map "x")))
          (list (eq b1 'test-ur1-xxx)
                (null b2)
                (eq b3 'test-ur2-xxx)
                (funcall b3)
                (string= (funcall b3) "second"))))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_copy_and_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (test-cm1-xxx t test-cm2-xxx t test-cm1-xxx t test-cm1-xxx t \"copy\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-cm1-xxx () "orig")
  (defun test-cm2-xxx () "copy")
  (let ((orig (make-sparse-keymap)))
    (define-key orig "a" 'test-cm1-xxx)
    (define-key orig "b" 'test-cm1-xxx)
    (let ((copy (copy-keymap orig)))
      (define-key copy "a" 'test-cm2-xxx)
      (list (lookup-key orig "a")
            (eq (lookup-key orig "a") 'test-cm1-xxx)
            (lookup-key copy "a")
            (eq (lookup-key copy "a") 'test-cm2-xxx)
            (lookup-key orig "b")
            (eq (lookup-key orig "b") 'test-cm1-xxx)
            (lookup-key copy "b")
            (eq (lookup-key copy "b") 'test-cm1-xxx)
            (funcall (lookup-key copy "a"))
            (string= (funcall (lookup-key copy "a")) "copy"))))) "#,
        expect,
    );
}
