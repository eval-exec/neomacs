#[path = "../../support/lifetime_contract.rs"]
mod lifetime_contract;

use lifetime_contract::{ModelWord, ModelWorld, with_world};

fn main() {
    let mut world = ModelWorld::new(ModelWord::INITIAL);
    let result = with_world(&mut world, |context, _witness| {
        context.view().word()
    });
    assert_eq!(result, ModelWord::INITIAL);
    // The view ended with the guard; a new exclusive borrow is available.
    let again = with_world(&mut world, |context, _witness| {
        context.view().word()
    });
    assert_eq!(again, result);
}
