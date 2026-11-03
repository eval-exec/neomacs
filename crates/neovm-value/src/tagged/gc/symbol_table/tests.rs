use super::*;

fn shape(chunks: usize) -> SymbolTableShape {
    SymbolTableShape::new(
        SymbolSlotCount::from(chunks * 2),
        SymbolChunkCount::from(chunks),
        SymbolChunkSlots::from(NonZeroUsize::new(2).unwrap()),
    )
    .unwrap()
}

fn address<T>(chunk: &mut T) -> SymbolChunkAddress {
    // SAFETY: each fixture retains its backing allocation until tables drop.
    unsafe { SymbolChunkAddress::from_allocation(NonNull::from(chunk).cast()) }
}

#[test]
fn nmjit_n3_shape_rejects_mixed_bound_and_chunk_count() {
    assert_eq!(
        SymbolTableShape::new(
            SymbolSlotCount::from(3),
            SymbolChunkCount::from(1),
            SymbolChunkSlots::from(NonZeroUsize::new(2).unwrap())
        ),
        Err(TableError::Shape)
    );
    assert_eq!(
        SymbolTableShape::new(
            SymbolSlotCount::from(0),
            SymbolChunkCount::from(usize::MAX),
            SymbolChunkSlots::from(NonZeroUsize::new(2).unwrap())
        ),
        Err(TableError::Capacity)
    );
}

#[test]
fn nmjit_n3_initial_table_owns_addresses_not_chunks() {
    let mut first = Box::new([11u64, 12]);
    let mut second = Box::new([21u64, 22]);
    let addresses = [address(&mut *first), address(&mut *second)];
    let table = PreparedSymbolTable::new(shape(2), PointerTableGeneration::INITIAL)
        .unwrap()
        .initialize(addresses)
        .unwrap();
    assert_eq!(table.table().slots().get(), 4);
    assert_eq!(table.table().chunk_count().get(), 2);
    assert_eq!(table.table().generation(), PointerTableGeneration::INITIAL);
    assert_eq!(
        table.table().chunk(SymbolChunkIndex::from(0)),
        Some(addresses[0])
    );
    assert_eq!(table.table().chunk(SymbolChunkIndex::from(2)), None);
    drop(table);
    // Table destruction must not destroy, reconstruct or assume chunk ownership.
    assert_eq!(*first, [11, 12]);
    assert_eq!(*second, [21, 22]);
}

#[test]
fn nmjit_n3_descriptor_root_survives_owner_handle_moves() {
    let mut chunk = Box::new([11u64, 12]);
    let address = address(&mut *chunk);
    let table = PreparedSymbolTable::new(shape(1), PointerTableGeneration::INITIAL)
        .unwrap()
        .initialize([address])
        .unwrap();
    let raw = table.as_ptr();
    let mut owners = vec![table];
    owners.reserve(64);
    // SAFETY: ownership remains in owners; descriptor is immutable and this
    // copied original root was obtained after consuming its Box ownership.
    let view = unsafe { &*raw };
    assert_eq!(view.chunk(SymbolChunkIndex::from(0)), Some(address));
    assert_eq!(owners[0].as_ptr(), raw);
    // SAFETY: fixture backing remains live; only an initialized scalar is read.
    assert_eq!(unsafe { *address.as_ptr().cast::<u64>() }, 11);
    drop(owners);
    assert_eq!(*chunk, [11, 12]);
}

#[test]
fn nmjit_n3_initializer_rejects_short_and_long_address_streams() {
    let mut chunk = Box::new([11u64, 12]);
    let address = address(&mut *chunk);
    assert!(matches!(
        PreparedSymbolTable::new(shape(1), PointerTableGeneration::INITIAL)
            .unwrap()
            .initialize([]),
        Err(TableError::Shape)
    ));
    assert!(matches!(
        PreparedSymbolTable::new(shape(1), PointerTableGeneration::INITIAL)
            .unwrap()
            .initialize([address, address]),
        Err(TableError::Shape)
    ));
    assert_eq!(*chunk, [11, 12]);
}

#[test]
fn nmjit_n3_unpublished_preparation_drops_without_touching_backing() {
    let chunk = Box::new([11u64, 12]);
    drop(PreparedSymbolTable::new(shape(1), PointerTableGeneration::INITIAL).unwrap());
    assert_eq!(*chunk, [11, 12]);
}

#[test]
fn nmjit_n3_generation_successor_is_checked_and_independent() {
    assert_ne!(
        PointerTableGeneration::INITIAL.checked_successor().unwrap(),
        PointerTableGeneration::INITIAL
    );
    assert_eq!(
        PointerTableGeneration(NonZeroU64::new(u64::MAX).unwrap()).checked_successor(),
        Err(TableError::GenerationExhausted)
    );
}
