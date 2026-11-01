//! Mutation fixture for the closed-world dispatch mapping.
//!
//! The checked-in `effect_inventory.inc` carries the same shape as
//! `opcode_dispatch` below: a wildcard-free match that enumerates every
//! variant of the real closed enum. Adding one opcode variant without
//! regenerating the inventory must therefore fail with E0004 (non-exhaustive
//! patterns), never silently take a default World arm.

#![allow(dead_code)]

#[derive(Clone, Copy)]
enum Op {
    Goto,
    Return,
    AddedAfterTheGeneratedMapping,
}

#[derive(Clone, Copy)]
enum Arm {
    Scalar,
    World,
}

fn opcode_dispatch(op: Op) -> Arm {
    match op {
        Op::Goto => Arm::Scalar,
        Op::Return => Arm::World,
    }
}

fn main() {
    let _ = opcode_dispatch(Op::Goto);
}
