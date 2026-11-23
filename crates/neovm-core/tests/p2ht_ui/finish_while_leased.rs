//! Finish while a writer lease is live. The lease must borrow the run, so
//! it cannot outlive the run access that `finish` consumes with the owner.

use neovm_core::tagged::gc::{Mutator, TableMut};
use neovm_core::tagged::value::TaggedValue;
use std::num::NonZeroU64;

fn finish_while_leased(mut owner: Mutator<'static>, table: TaggedValue) {
    let lease = {
        let mut run = owner.run_access().unwrap();
        TableMut::enter(&mut run, table).unwrap()
    };
    let _ = owner.finish(NonZeroU64::new(1).unwrap());
    drop(lease);
}

fn main() {
    let _ = finish_while_leased;
}
