#[path = "../support/lifetime_contract.rs"]
mod lifetime_contract;

use lifetime_contract::{ModelWorld, View, with_world};

#[allow(dead_code)] // The compiler checks the invalid escape without running it.
fn escape<'world>(world: &'world mut ModelWorld) -> View<'world> {
    with_world(world, |context, _witness| context.view())
}

fn main() {}
