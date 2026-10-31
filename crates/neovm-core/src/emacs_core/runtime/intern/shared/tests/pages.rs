use super::*;

#[test]
fn pages_keep_name_and_symbol_coordinates_separate() {
    let symbols = PagedRows::<_, SymId>::from_rows(vec![17, 23]).unwrap();
    let names = PagedRows::<_, NameId>::from_rows(vec![31]).unwrap();
    assert_eq!(symbols.probe(SymId(1)), CacheProbe::Hit(&23));
    assert_eq!(symbols.probe(SymId(2)), CacheProbe::Miss);
    assert_eq!(names.probe(NameId(0)), CacheProbe::Hit(&31));
    assert_eq!(names.probe(NameId(1)), CacheProbe::Miss);
    assert_eq!(
        CanonicalSlot::from_symbol(SymId(0)).probe(),
        CacheProbe::Miss
    );
    assert_eq!(
        CanonicalSlot::from_symbol(SymId(4)).probe(),
        CacheProbe::Hit(SymId(4))
    );
}

#[test]
fn copy_on_write_preserves_tail_and_epoch_predecessor() {
    let old = PagedRows::<_, SymId>::from_rows((0..257).collect::<Vec<u32>>()).unwrap();
    let appended = old.appending(257).unwrap();
    assert!(Arc::ptr_eq(&old.pages[0], &appended.pages[0]));
    assert!(!Arc::ptr_eq(&old.pages[1], &appended.pages[1]));
    assert_eq!(old.probe(SymId(257)), CacheProbe::Miss);
    assert_eq!(appended.probe(SymId(257)), CacheProbe::Hit(&257));
    let replaced = appended.replacing(SymId(256), 999).unwrap();
    assert!(Arc::ptr_eq(&appended.pages[0], &replaced.pages[0]));
    assert_eq!(old.probe(SymId(256)), CacheProbe::Hit(&256));
    assert_eq!(appended.probe(SymId(256)), CacheProbe::Hit(&256));
    assert_eq!(replaced.probe(SymId(256)), CacheProbe::Hit(&999));
    assert!(matches!(
        replaced.replacing(SymId(258), 0),
        Err(SnapshotError::OutsideSpan)
    ));
}

#[test]
fn empty_and_full_page_append_use_exact_coverage() {
    let empty = PagedRows::<u32, NameId>::from_rows(vec![]).unwrap();
    assert_eq!(empty.probe(NameId(0)), CacheProbe::Miss);
    let first = empty.appending(7).unwrap();
    assert_eq!(first.probe(NameId(0)), CacheProbe::Hit(&7));
    let full = PagedRows::<_, NameId>::from_rows(vec![0; PAGE_ROWS]).unwrap();
    let next = full.appending(9).unwrap();
    assert!(Arc::ptr_eq(&full.pages[0], &next.pages[0]));
    assert_eq!(next.probe(NameId(256)), CacheProbe::Hit(&9));
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        TableSpan::<SymId>::try_from_len(u32::MAX as usize + 1),
        Err(SnapshotError::SpanOverflow)
    );
}
