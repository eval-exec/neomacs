//! value-gc B14 regression guard: `CollectorTablesSlot`'s inner `Cell` must
//! stay private so sibling modules in `tagged::gc` cannot install arbitrary
//! raw pointers into the `AllStopped` bridge.
#![allow(dead_code)]

mod mutator_registration {
    use std::marker::PhantomData;
    pub struct AllStopped<'era>(PhantomData<&'era ()>);
    #[derive(Debug)]
    pub enum RendezvousError {
        Busy,
    }
}

#[path = "../../../neovm-value/src/tagged/gc/collector_tables_slot.rs"]
mod collector_tables_slot;

fn main() {
    let slot = collector_tables_slot::CollectorTablesSlot::new();
    slot.cell.set(std::ptr::null());
}
