use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::RedisplayHookPolicyGuard;
use neovm_value::forward::LispFwdType;
use neovm_value::intern::intern;
use neovm_value::value::Value;

#[test]
fn configuration_hook_core_default_is_nil_only_under_gnu_policy() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("window-configuration-change-hook");
    {
        let _policy = RedisplayHookPolicyGuard::legacy();
        let eval = Context::new(crate::test_utils::unit_runtime());
        assert_eq!(
            neovm_eval::emacs_core::data::default_value_by_id(&eval, symbol),
            None
        );
        assert_eq!(eval.obarray_view().forward_type(symbol), None);
    }
    {
        let _policy = RedisplayHookPolicyGuard::gnu();
        let mut eval = Context::new(crate::test_utils::unit_runtime());
        // GNU window.c:9383–9398 declares this C-owned DEFVAR_LISP special
        // object and initializes its default to nil before loading window.el.
        assert_eq!(
            neovm_eval::emacs_core::data::default_value_by_id(&eval, symbol),
            Some(Value::NIL)
        );
        assert_eq!(
            eval.obarray_view().forward_type(symbol),
            Some(LispFwdType::Obj)
        );
        assert!(eval.obarray_view().is_special_id(symbol));
        assert_eq!(
            neovm_eval::emacs_core::data::default_value(
                &mut eval,
                vec![Value::from_sym_id(symbol)]
            )
            .expect("initialized GNU core hook"),
            Value::NIL
        );
    }
}
