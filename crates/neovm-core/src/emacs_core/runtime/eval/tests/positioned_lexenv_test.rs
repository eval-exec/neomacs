//! Position-symbol lexical bindings follow GNU's EQ-based alist lookup.

use super::{Context, SpecBinding};
use crate::emacs_core::{intern::intern, print::print_value, value::Value};

fn answer(source: &str) -> String {
    let mut context = Context::new();
    print_value(&context.eval_str(source).expect("positioned lexical form"))
}

#[test]
fn positioned_lexenv_eval_uses_positioned_key() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (eval 'n (list (cons (position-symbol 'n 0) 7))))"
        ),
        "7"
    );
}

#[test]
fn positioned_lexenv_setq_mutates_the_original_binding() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (let ((env (list (cons (position-symbol 'n 0) 7))))
                 (list (eval '(progn (setq n 8) n) env) (cdr (car env)))))"
        ),
        "(8 8)"
    );
}

#[test]
fn positioned_lexenv_special_declaration_selects_dynamic_binding() {
    assert_eq!(
        answer(
            "(progn
               (setq positioned-lexenv-var 10)
               (let ((symbols-with-pos-enabled t))
                 (eval '(let ((positioned-lexenv-var 20))
                          (list positioned-lexenv-var
                                (symbol-value 'positioned-lexenv-var)))
                       (list (position-symbol 'positioned-lexenv-var 0)))))"
        ),
        "(20 20)"
    );
}

#[test]
fn positioned_lexenv_cache_keeps_bare_mode_answers_across_flag_changes() {
    let mut context = Context::new();
    context.lexenv = context
        .eval_str("(list (cons (position-symbol 'n 0) 7) (cons 'n 9))")
        .expect("lexical environment");
    let symbol = intern("n");
    context.set_variable("symbols-with-pos-enabled", Value::bool_val(false));
    assert_eq!(
        context.lexenv_lookup_cached_in(context.lexenv, symbol),
        Some(Value::fixnum(9))
    );
    context.set_variable("symbols-with-pos-enabled", Value::bool_val(true));
    assert_eq!(
        context.lexenv_lookup_cached_in(context.lexenv, symbol),
        Some(Value::fixnum(7))
    );
    context.set_variable("symbols-with-pos-enabled", Value::bool_val(false));
    assert_eq!(
        context.lexenv_lookup_cached_in(context.lexenv, symbol),
        Some(Value::fixnum(9))
    );
}

#[test]
fn positioned_lexenv_cached_absence_and_special_declarations_follow_flag() {
    let mut context = Context::new();
    context.lexenv = context
        .eval_str("(list (cons (position-symbol 'n 0) 7) (position-symbol 'n 1))")
        .expect("lexical environment");
    let symbol = intern("n");
    for enabled in [false, true, false, true] {
        context.set_variable("symbols-with-pos-enabled", Value::bool_val(enabled));
        assert_eq!(
            context.lexenv_lookup_cached_in(context.lexenv, symbol),
            enabled.then_some(Value::fixnum(7))
        );
        assert_eq!(
            context.lexenv_declares_special_cached_in(context.lexenv, symbol),
            enabled
        );
    }
}

#[test]
fn positioned_lexenv_saved_specpdl_lookup_honours_flag() {
    let mut context = Context::new();
    let env = context
        .eval_str("(list (cons (position-symbol 'n 0) 7))")
        .expect("lexical environment");
    context.push_specpdl_with(|| SpecBinding::LexicalEnv { old_lexenv: env });
    let symbol = intern("n");
    for enabled in [false, true, false] {
        context.set_variable("symbols-with-pos-enabled", Value::bool_val(enabled));
        assert_eq!(context.lexbound_p_in_specpdl(symbol), enabled);
    }
}
