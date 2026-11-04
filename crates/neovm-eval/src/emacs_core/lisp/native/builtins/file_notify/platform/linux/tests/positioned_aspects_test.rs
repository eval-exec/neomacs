use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn positioned_single_and_list_aspects_follow_eq_policy() {
    let mut ctx = Context::new();
    let aspect = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("modify"), Value::fixnum(0));
    for argument in [aspect, Value::list(vec![aspect])] {
        assert_eq!(
            parse_aspects(argument, SymbolMode::Positioned).unwrap(),
            vec!["modify"]
        );
        assert!(parse_aspects(argument, SymbolMode::Bare).is_err());
    }
}
