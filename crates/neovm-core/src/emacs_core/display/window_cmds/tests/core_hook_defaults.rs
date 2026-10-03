use crate::emacs_core::eval::RedisplayHookPolicyGuard;
use crate::emacs_core::forward::LispFwdType;
use crate::emacs_core::intern::intern;
use crate::emacs_core::{Context, Value};

#[test]
fn configuration_hook_core_default_is_nil_only_under_gnu_policy() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("window-configuration-change-hook");
    {
        let _policy = RedisplayHookPolicyGuard::legacy();
        let eval = Context::new();
        assert_eq!(
            crate::emacs_core::data::default_value_by_id(&eval, symbol),
            None
        );
        assert_eq!(eval.obarray.forward_type(symbol), None);
    }
    {
        let _policy = RedisplayHookPolicyGuard::gnu();
        let mut eval = Context::new();
        // GNU window.c:9383–9398 declares this C-owned DEFVAR_LISP special
        // object and initializes its default to nil before loading window.el.
        assert_eq!(
            crate::emacs_core::data::default_value_by_id(&eval, symbol),
            Some(Value::NIL)
        );
        assert_eq!(eval.obarray.forward_type(symbol), Some(LispFwdType::Obj));
        assert!(eval.obarray.is_special_id(symbol));
        assert_eq!(
            crate::emacs_core::data::default_value(&mut eval, vec![Value::from_sym_id(symbol)])
                .expect("initialized GNU core hook"),
            Value::NIL
        );
    }
}
