//! `cons` while a reader lease is live. The lease must borrow the run,
//! so a later `&mut` use (`cons`) cannot overlap it. Passing `&mut run`
//! is the shared reborrow; two readers from one run stay a separate case.

use neovm_core::tagged::gc::{Mutator, TableRef};
use neovm_core::tagged::transport::SharedRoot;
use neovm_core::tagged::value::TaggedValue;

fn cons_while_reader_live(
    mut owner: Mutator<'static>,
    table: TaggedValue,
    car: &SharedRoot,
    cdr: &SharedRoot,
) {
    let mut run = owner.run_access().unwrap();
    let _lease = TableRef::enter(&mut run, table).unwrap();
    let _ = run.cons(car, cdr);
}

fn main() {
    let _ = cons_while_reader_live;
}
