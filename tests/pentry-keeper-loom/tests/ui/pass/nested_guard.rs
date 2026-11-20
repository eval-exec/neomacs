#[path = "../../support/lifetime_contract.rs"]
mod lifetime_contract;

use lifetime_contract::{ModelWord, ModelWorld, SerialContext, with_world};

fn callee<'g>(context: &mut SerialContext<'g>) -> ModelWord {
    context.with_nested(|nested| nested.view().word())
}

fn main() {
    let mut world = ModelWorld::new(ModelWord::INITIAL);
    let result = with_world(&mut world, |context, _witness| {
        let before = context.view().word();
        let nested = callee(context);
        let after = context.view().word();
        assert_eq!(before, nested);
        assert_eq!(nested, after);
        after
    });
    assert_eq!(result, ModelWord::INITIAL);
}
