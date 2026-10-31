use super::super::super::{LispString, NameAtomStorage};
use super::super::pages::TableSpan;
use super::*;

struct EmptyNativeIndex;
impl NativeIndexShape for EmptyNativeIndex {
    fn entry_count(&self) -> NativeSlotCount {
        NativeSlotCount::try_from_len(0).unwrap()
    }
}

fn snapshot(name: NameId) -> Result<InternSnapshot<EmptyNativeIndex>, SnapshotError> {
    let mut atoms = NameAtomStorage::new();
    let ordinary = atoms.push(LispString::from_utf8("nil"));
    let colon = atoms.push(LispString::from_utf8(":name"));
    let symbols = PagedRows::from_rows(vec![
        SymbolRow::new(
            ordinary,
            NameId(0),
            CanonicalClass::Canonical,
            SymbolNameOrigin::AtomOnly,
            NativeQueryClass::NativeUtf8,
        ),
        SymbolRow::new(
            colon,
            name,
            CanonicalClass::Canonical,
            SymbolNameOrigin::ReaderAtom,
            NativeQueryClass::NativeUtf8,
        ),
        SymbolRow::new(
            colon,
            name,
            CanonicalClass::Uninterned,
            SymbolNameOrigin::LispObject,
            NativeQueryClass::ExactOnly,
        ),
    ])?;
    let names = PagedRows::from_rows(vec![
        CanonicalSlot::from_symbol(SymId(0)),
        CanonicalSlot::from_symbol(SymId(1)),
    ])?;
    let span = TableSpan::try_from_len(3)?;
    InternSnapshot::try_new(
        symbols,
        names,
        EmptyNativeIndex,
        ReaderTemplates::from_entries(span, [])?,
        HeapVisibleNames::from_entries(span, [])?,
        RegistryEpoch::from_published(7),
        MaterializationEpoch::from_published(8),
        DescriptorGeneration::from_published(9),
    )
}

#[test]
fn snapshot_covers_rows_and_keeps_canonical_and_atom_keyword_distinct() {
    let snapshot = snapshot(NameId(1)).unwrap();
    assert_eq!(snapshot.canonical(NameId(0)), CacheProbe::Miss);
    assert_eq!(snapshot.canonical(NameId(1)), CacheProbe::Hit(SymId(1)));
    assert_eq!(snapshot.canonical(NameId(2)), CacheProbe::Miss);
    match snapshot.symbol(SymId(2)) {
        CacheProbe::Hit(row) => {
            assert_eq!(row.canonical(), CanonicalClass::Uninterned);
            assert_eq!(row.atom_keyword(), AtomKeyword::Colon);
            assert_eq!(row.origin(), SymbolNameOrigin::LispObject);
            assert_eq!(row.native_class(), NativeQueryClass::ExactOnly);
        }
        CacheProbe::Miss => panic!("covered row"),
    }
    assert!(matches!(snapshot.symbol(SymId(3)), CacheProbe::Miss));
    assert_eq!(std::mem::size_of_val(&snapshot), 80);
}

#[test]
fn malformed_cross_column_reference_is_refused() {
    assert!(matches!(
        snapshot(NameId(2)),
        Err(SnapshotError::NameOutsideSpan)
    ));
}

#[test]
fn clocks_are_separate_checked_domains_and_bypass_has_no_ready_payload() {
    assert_eq!(
        RegistryEpoch::from_published(7).checked_successor(),
        Some(RegistryEpoch::from_published(8))
    );
    assert_eq!(
        DescriptorGeneration::from_published(u64::MAX).checked_successor(),
        None
    );
    assert_eq!(
        MaterializationEpoch::from_published(u64::MAX).checked_successor(),
        None
    );
    let deferred: PreparedSnapshot<EmptyNativeIndex> =
        PreparedSnapshot::DeferredPublication(DeferredPublication::NeedGrace);
    match deferred {
        PreparedSnapshot::ReadySnapshot(_) => panic!("bypass cannot carry a ready table"),
        PreparedSnapshot::DeferredPublication(reason) => {
            assert_eq!(reason, DeferredPublication::NeedGrace)
        }
    }
}
