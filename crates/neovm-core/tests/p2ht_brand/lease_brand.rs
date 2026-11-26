//! Source-only lifetime brand for the A7 mutation controls.
//!
//! The product signatures in `table_authority.rs` are asserted to contain the
//! same parameter types. Removing only that brand (the era lifetime, below)
//! makes the unchanged negative bodies compile.
use std::marker::PhantomData;

pub struct Mutator<'era>(PhantomData<&'era ()>);
pub struct RunAccess<'owner, 'era>(PhantomData<&'owner mut Mutator<'era>>);
pub struct TableMut<'a>(PhantomData<&'a mut ()>);
pub struct TableRef<'a>(PhantomData<&'a ()>);
pub struct SharedRoot;

impl<'era> Mutator<'era> {
    pub fn run_access(&mut self) -> RunAccess<'_, 'era> {
        RunAccess(PhantomData)
    }

    pub fn finish(self, _epoch: u64) {}
}

impl RunAccess<'_, '_> {
    pub fn cons(&mut self, _car: &SharedRoot, _cdr: &SharedRoot) {}
}

impl TableMut<'_> {
    pub fn enter<'a>(access: &'a mut RunAccess<'_, '_>, _table: u8) -> TableMut<'a> {
        let _brand = access;
        TableMut(PhantomData)
    }
}

impl TableRef<'_> {
    pub fn enter<'a>(access: &'a RunAccess<'_, '_>, _table: u8) -> TableRef<'a> {
        let _brand = access;
        TableRef(PhantomData)
    }
}

// The product leases run `Drop`. Without it, an unused binding would not
// keep the run borrow alive, and the cons bodies would type-check.
impl Drop for TableMut<'_> {
    fn drop(&mut self) {}
}

impl Drop for TableRef<'_> {
    fn drop(&mut self) {}
}
