//! `cons` while a writer lease is live. The lease must hold the run
//! exclusively, so the run cannot allocate until the lease drops.

use neovm_core::tagged::gc::{Mutator, TableMut};
use neovm_core::tagged::transport::SharedRoot;
use neovm_core::tagged::value::TaggedValue;

fn cons_while_writer_live(
    mut owner: Mutator<'static>,
    table: TaggedValue,
    car: &SharedRoot,
    cdr: &SharedRoot,
) {
    let mut run = owner.run_access().unwrap();
    let _lease = TableMut::enter(&mut run, table).unwrap();
    let _ = run.cons(car, cdr);
}

fn main() {
    let _ = cons_while_writer_live;
}
