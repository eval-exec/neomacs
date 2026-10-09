use super::*;

fn evaluator() -> crate::emacs_core::Context {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(r#"(fset 'substitute-command-keys (lambda (help) (concat "sub:" help)))"#)
        .unwrap();
    eval
}

fn help_with_inhibit(
    eval: &mut crate::emacs_core::Context,
    text: &str,
    pos: i64,
    value: Value,
) -> Value {
    let help = Value::string(text);
    crate::emacs_core::textprop::builtin_put_text_property_5(
        eval,
        Value::fixnum(pos),
        Value::fixnum(pos + 1),
        Value::symbol("help-echo-inhibit-substitution"),
        value,
        help,
    )
    .unwrap();
    help
}

#[test]
fn help_echo_inhibit_reads_first_character() {
    let mut eval = evaluator();
    let help = help_with_inhibit(&mut eval, "жz", 0, Value::T);
    assert_eq!(eval.substitute_help_echo_command_keys(help).unwrap(), help);
}

#[test]
fn help_echo_second_character_does_not_inhibit() {
    let mut eval = evaluator();
    let help = help_with_inhibit(&mut eval, "xy", 1, Value::T);
    let substituted = eval.substitute_help_echo_command_keys(help).unwrap();
    assert_eq!(substituted.as_str_owned().as_deref(), Some("sub:xy"));
}

#[test]
fn empty_help_echo_skips_property_lookup_and_still_substitutes() {
    let mut eval = evaluator();
    eval.eval_str("(setq default-text-properties '(help-echo-inhibit-substitution t))")
        .unwrap();
    let substituted = eval
        .substitute_help_echo_command_keys(Value::string(""))
        .unwrap();
    assert_eq!(substituted.as_str_owned().as_deref(), Some("sub:"));
}

#[test]
fn help_echo_first_character_nil_overrides_default_inhibit() {
    let mut eval = evaluator();
    eval.eval_str("(setq default-text-properties '(help-echo-inhibit-substitution t))")
        .unwrap();
    let help = help_with_inhibit(&mut eval, "x", 0, Value::NIL);
    let substituted = eval.substitute_help_echo_command_keys(help).unwrap();
    assert_eq!(substituted.as_str_owned().as_deref(), Some("sub:x"));
}
