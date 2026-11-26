#[path = "lease_brand.rs"]
mod lease_brand;

use lease_brand::{Mutator, TableMut};

fn finish_while_leased(mut owner: Mutator<'static>, table: u8) {
    let lease = {
        let mut run = owner.run_access();
        TableMut::enter(&mut run, table)
    };
    owner.finish(1);
    drop(lease);
}

fn main() {
    let _ = finish_while_leased;
}
