use super::*;
use crate::tagged::gc::symbol_table::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, SymbolChunkCount,
    SymbolChunkIndex, SymbolChunkSlots, SymbolSlotCount,
};
use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use std::rc::Rc;

struct AbiView<'table> {
    slots: SymbolSlotCount,
    chunks: SymbolChunkCount,
    chunk_slots: SymbolChunkSlots,
    addresses: NonNull<SymbolChunkAddress>,
    _owner: PhantomData<&'table OwnedSymbolTable>,
}
impl AbiView<'_> {
    fn chunk(&self, index: SymbolChunkIndex) -> Option<SymbolChunkAddress> {
        if index.0 >= self.chunks.get() {
            return None;
        }
        // SAFETY: read_abi borrowed the descriptor retaining this initialized
        // immutable array, and the ABI count bounds this pointer addition.
        Some(unsafe { self.addresses.as_ptr().add(index.0).read() })
    }
}
fn read_abi(owner: &OwnedSymbolTable) -> AbiView<'_> {
    let base = owner.as_ptr().cast::<u8>();
    // SAFETY: these explicit repr(C)/transparent offsets name the initialized
    // descriptor fields with their real types and alignment. Pointer loads
    // preserve provenance, and the view borrows the allocation's unique owner.
    unsafe {
        AbiView {
            slots: base
                .add(TABLE_SLOT_COUNT_OFFSET.get())
                .cast::<SymbolSlotCount>()
                .read(),
            chunks: base
                .add(TABLE_CHUNK_COUNT_OFFSET.get())
                .cast::<SymbolChunkCount>()
                .read(),
            chunk_slots: base
                .add(TABLE_CHUNK_SLOTS_OFFSET.get())
                .cast::<SymbolChunkSlots>()
                .read(),
            addresses: base
                .add(TABLE_ADDRESSES_OFFSET.get())
                .cast::<NonNull<SymbolChunkAddress>>()
                .read(),
            _owner: PhantomData,
        }
    }
}
fn table(
    addresses: impl IntoIterator<Item = SymbolChunkAddress>,
    chunks: usize,
) -> OwnedSymbolTable {
    let shape = SymbolTableShape::new(
        SymbolSlotCount::from(chunks.checked_mul(2).unwrap()),
        SymbolChunkCount::from(chunks),
        SymbolChunkSlots::from(NonZeroUsize::new(2).unwrap()),
    )
    .unwrap();
    PreparedSymbolTable::new(shape, PointerTableGeneration::INITIAL)
        .unwrap()
        .initialize(addresses)
        .unwrap()
}
fn address<T>(backing: &mut T) -> SymbolChunkAddress {
    // SAFETY: every test retains its separately allocated backing until all
    // tables/views have dropped; this address never owns that allocation.
    unsafe { SymbolChunkAddress::from_allocation(NonNull::from(backing).cast()) }
}

#[test]
fn nmjit_n4_table_abi_reads_coherent_fields_and_original_addresses() {
    let mut first = Box::new([11u64, 12]);
    let mut second = Box::new([21u64, 22]);
    let addresses = [address(&mut *first), address(&mut *second)];
    let table = table(addresses, 2);
    let view = read_abi(&table);
    assert_eq!(view.slots, table.table().slots());
    assert_eq!(view.slots.get(), 4);
    assert_eq!(view.chunks, table.table().chunk_count());
    assert_eq!(view.chunk_slots, table.table().chunk_slots());
    for (i, expected) in addresses.into_iter().enumerate() {
        assert_eq!(view.chunk(i.into()), Some(expected));
        assert_eq!(view.chunk(i.into()), table.table().chunk(i.into()));
        // SAFETY: the fixture still retains its initialized u64 backing and
        // the nonowning ABI address is the same original allocation address.
        assert_eq!(
            unsafe { *expected.as_ptr().cast::<u64>() },
            if i == 0 { 11 } else { 21 }
        );
    }
    assert_eq!(TABLE_ADDRESS_STRIDE.get(), size_of::<SymbolChunkAddress>());
    assert_eq!(view.chunk(2.into()), None);
}

#[test]
fn nmjit_n4_table_abi_empty_and_out_of_range_do_not_read_the_array() {
    let table = table([], 0);
    let view = read_abi(&table);
    assert_eq!(view.slots.get(), 0);
    assert_eq!(view.chunks.get(), 0);
    assert_eq!(view.chunk_slots.get(), 2);
    assert_eq!(view.chunk(0.into()), None);
    assert_eq!(view.chunk(usize::MAX.into()), None);
    assert_eq!(table.table().chunk(usize::MAX.into()), None);
}

#[test]
fn nmjit_n4_table_abi_array_root_survives_descriptor_owner_moves() {
    let mut backing = Box::new([31u64, 32]);
    let address = address(&mut *backing);
    let table = table([address], 1);
    // Obtain the original pointer before moving any containing owner handle.
    let original = read_abi(&table).addresses;
    let mut owners = vec![table];
    owners.reserve(64);
    let view = read_abi(&owners[0]);
    assert_eq!(view.addresses, original);
    assert_eq!(view.chunk(0.into()), Some(address));
    // SAFETY: the moved unique owner still retains the same original array;
    // there has been no array mutation, ownership reconstruction or retag.
    assert_eq!(unsafe { original.as_ptr().read() }, address);
    drop(view);
    drop(owners);
    assert_eq!(*backing, [31, 32]);
}

#[test]
fn nmjit_n4_table_abi_drop_preserves_unique_backing_ownership() {
    struct Backing(Rc<Cell<usize>>);
    impl Drop for Backing {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut first = Box::new(Backing(Rc::clone(&drops)));
    let mut second = Box::new(Backing(Rc::clone(&drops)));
    let table = table([address(&mut *first), address(&mut *second)], 2);
    assert!(read_abi(&table).chunk(1.into()).is_some());
    drop(table);
    assert_eq!(drops.get(), 0, "only address-array ownership was released");
    drop(first);
    assert_eq!(drops.get(), 1);
    drop(second);
    assert_eq!(
        drops.get(),
        2,
        "each original backing owner drops exactly once"
    );
}

#[test]
fn nmjit_n4_publication_view_keeps_old_descriptor_across_replacement() {
    use crate::tagged::gc::symbol_table::publication::{PublishedSymbolTables, TableLimits};
    let mut first = Box::new([41u64, 42]);
    let mut second = Box::new([51u64, 52]);
    let first_address = address(&mut *first);
    let second_address = address(&mut *second);
    let tables =
        PublishedSymbolTables::new(table([first_address], 1), TableLimits::default()).unwrap();
    let publication = tables.publication();
    let old = publication.load();
    let shape =
        SymbolTableShape::new(4.into(), 2.into(), NonZeroUsize::new(2).unwrap().into()).unwrap();
    tables
        .try_prepare(shape)
        .result
        .unwrap()
        .publish([first_address, second_address])
        .unwrap();
    assert_eq!(old.table().slots().get(), 2);
    assert_eq!(old.table().chunk(0.into()), Some(first_address));
    let new = publication.load();
    assert_eq!(new.table().slots().get(), 4);
    assert_eq!(new.table().chunk(1.into()), Some(second_address));
    assert_eq!(tables.stats().unwrap().retired.get(), 1);
}
