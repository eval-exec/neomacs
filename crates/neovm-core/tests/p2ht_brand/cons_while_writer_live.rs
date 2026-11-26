#[path = "lease_brand.rs"]
mod lease_brand;

use lease_brand::{Mutator, SharedRoot, TableMut};

fn cons_while_writer_live(mut owner: Mutator<'static>, table: u8, car: &SharedRoot, cdr: &SharedRoot) {
    let mut run = owner.run_access();
    let _lease = TableMut::enter(&mut run, table);
    let _ = run.cons(car, cdr);
}

fn main() {
    let _ = cons_while_writer_live;
}
