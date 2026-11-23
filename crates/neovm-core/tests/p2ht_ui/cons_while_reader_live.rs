//! `cons` while a reader lease is live. The lease must borrow the run,
//! so a later `&mut` use (`cons`) cannot overlap it. `&mut run` is a
//! mutable reborrow (E0499). `&run` is the shared borrow (E0502).

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

fn cons_while_shared_reader_live(
    mut owner: Mutator<'static>,
    table: TaggedValue,
    car: &SharedRoot,
    cdr: &SharedRoot,
) {
    let mut run = owner.run_access().unwrap();
    let _lease = TableRef::enter(&run, table).unwrap();
    let _ = run.cons(car, cdr);
}

fn main() {
    let _ = cons_while_reader_live;
    let _ = cons_while_shared_reader_live;
}
