use super::super::super::{LispString, NameAtomStorage};
use super::*;

#[test]
fn visible_names_keep_heap_identity_and_never_convert_to_values() {
    let a = HeapIdentity::from_legacy_word(21).unwrap();
    let b = HeapIdentity::from_legacy_word(22).unwrap();
    let other = HeapIdentity::from_legacy_word(23).unwrap();
    let span = TableSpan::try_from_len(2).unwrap();
    let facet = HeapVisibleNames::from_entries(
        span,
        [
            (a, SymId(1), HeapNameWord(11)),
            (b, SymId(1), HeapNameWord(12)),
        ],
    )
    .unwrap();
    match facet.probe(a, SymId(1)) {
        CacheProbe::Hit(name) => {
            assert_eq!(name.word, HeapNameWord(11));
        }
        CacheProbe::Miss => panic!("first heap record"),
    }
    match facet.probe(b, SymId(1)) {
        CacheProbe::Hit(name) => {
            assert_eq!(name.word, HeapNameWord(12));
        }
        CacheProbe::Miss => panic!("second heap record"),
    }
    assert!(matches!(facet.probe(other, SymId(1)), CacheProbe::Miss));
    assert!(matches!(facet.probe(a, SymId(0)), CacheProbe::Miss));
    assert_eq!(facet.heap_count(), 2);
    assert!(matches!(
        HeapVisibleNames::from_entries(
            span,
            [
                (a, SymId(1), HeapNameWord(11)),
                (a, SymId(1), HeapNameWord(12)),
            ]
        ),
        Err(SnapshotError::DuplicateHeapName)
    ));
}

#[test]
fn reader_templates_remain_distinct_from_identity_atoms() {
    let mut storage = NameAtomStorage::new();
    let template = storage.push(LispString::from_utf8("reader-template"));
    let span = TableSpan::try_from_len(2).unwrap();
    let facet = ReaderTemplates::from_entries(span, [(SymId(1), template)]).unwrap();
    assert!(matches!(facet.probe(SymId(0)), CacheProbe::Miss));
    match facet.probe(SymId(1)) {
        CacheProbe::Hit(atom) => assert!(std::ptr::eq(atom.0, template.0)),
        CacheProbe::Miss => panic!("template record"),
    }
    assert!(matches!(
        ReaderTemplates::from_entries(span, [(SymId(2), template)]),
        Err(SnapshotError::OutsideSpan)
    ));
    assert!(matches!(
        ReaderTemplates::from_entries(span, [(SymId(1), template), (SymId(1), template)]),
        Err(SnapshotError::DuplicateTemplate)
    ));
}
