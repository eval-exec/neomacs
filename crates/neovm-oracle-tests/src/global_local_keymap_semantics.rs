//! Oracle parity tests for keymap accessors: `current-global-map`,
//! `current-local-map`, `use-global-map`, `use-local-map`.
//!
//! GNU implements these in `src/keymap.c`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_current_global_map_returns_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(keymapp (current-global-map))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_current_global_map_is_independent_of_dynamic_global_map_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((saved-map (current-global-map))
       (installed-map (make-keymap))
       (rebound-map (copy-keymap installed-map))
       (key (kbd "C-.")))
  (unwind-protect
      (progn
        (use-global-map installed-map)
        (let ((global-map rebound-map))
          (global-set-key key 'neomacs--oracle-global-map-command)
          (list
           (eq (current-global-map) installed-map)
           (eq (current-global-map) global-map)
           (lookup-key installed-map key)
           (lookup-key global-map key))))
    (use-global-map saved-map)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil neomacs--oracle-global-map-command nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "(t nil neomacs--oracle-global-map-command nil)",
        &oracle,
        &neovm,
    );
}

#[test]
fn oracle_current_local_map_nil_by_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(with-temp-buffer (current-local-map))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_use_global_map_returns_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(use-global-map (current-global-map))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_use_local_map_returns_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((m (make-sparse-keymap))) (use-local-map m))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_global_set_key_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (global-set-key (kbd "C-c C-x") 'ignore)
  t)"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_local_set_key_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (local-set-key (kbd "C-c C-y") 'ignore)
  t)"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_global_key_binding_and_unset_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((old-global-map (current-global-map)))
  (unwind-protect
      (let ((map (make-sparse-keymap)))
        (use-global-map map)
        (global-set-key (kbd "C-c x") 'ignore)
        (list (global-key-binding (kbd "C-c x"))
              (global-key-binding (kbd "C-c y"))
              (global-unset-key (kbd "C-c x"))
              (global-key-binding (kbd "C-c x"))))
    (use-global-map old-global-map)))"#;
    let expect = expect_test::expect![[r#""OK (ignore nil nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ignore nil nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_local_key_binding_and_unset_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (let ((map (make-sparse-keymap)))
    (use-local-map map)
    (local-set-key (kbd "C-c y") 'ignore)
    (list (local-key-binding (kbd "C-c y"))
          (local-key-binding (kbd "C-c z"))
          (local-unset-key (kbd "C-c y"))
          (local-key-binding (kbd "C-c y")))))"#;
    let expect = expect_test::expect![[r#""OK (ignore nil nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ignore nil nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_current_local_map_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments current-local-map 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(current-local-map 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}
