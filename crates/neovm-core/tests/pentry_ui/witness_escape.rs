#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong(
    guard: &mut WorldBatonGuard<'static, 'static>,
) -> BatonWitness<'static, 'static, 'static> {
    guard.with_witness(|witness| witness)
}
fn main() {}
