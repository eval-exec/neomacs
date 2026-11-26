#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::{BatonWitness, WorldBatonGuard};

fn wrong(guard: &mut WorldBatonGuard<'static, 'static>) -> BatonWitness<'static, 'static, 'static> {
    guard.with_scope(|_context, witness| witness).unwrap()
}

fn main() {}
