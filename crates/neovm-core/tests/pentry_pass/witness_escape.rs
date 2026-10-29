#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong(
    guard: &mut WorldBatonGuard<'static, 'static>,
) {
    guard.with_witness(|witness| drop(witness));
}
fn main() {}
