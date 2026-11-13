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

struct CountedRow {
    value: u32,
    clones: std::rc::Rc<std::cell::Cell<usize>>,
}
impl Clone for CountedRow {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            value: self.value,
            clones: std::rc::Rc::clone(&self.clones),
        }
    }
}

#[test]
fn batch_extension_copies_the_existing_partial_tail_only_once() {
    let copies = std::rc::Rc::new(std::cell::Cell::new(0));
    let prefix_len = 3 * PAGE_ROWS + 17;
    let make = |value| CountedRow {
        value: value as u32,
        clones: std::rc::Rc::clone(&copies),
    };
    let old = PagedRows::<_, SymId>::from_rows((0..prefix_len).map(make).collect()).unwrap();
    let count = 3 * PAGE_ROWS + 5;
    let new = old
        .extending((prefix_len..prefix_len + count).map(make))
        .unwrap();
    assert_eq!(copies.get(), 17, "only the old partial page is row-cloned");
    for page in 0..3 {
        assert!(Arc::ptr_eq(&old.pages[page], &new.pages[page]));
    }
    assert!(!Arc::ptr_eq(&old.pages[3], &new.pages[3]));
    assert_eq!(old.span().len(), prefix_len);
    assert_eq!(new.span().len(), prefix_len + count);
    match new.probe(SymId((prefix_len + count - 1) as u32)) {
        CacheProbe::Hit(row) => assert_eq!(row.value, (prefix_len + count - 1) as u32),
        CacheProbe::Miss => panic!("last batch row missing"),
    }
    assert!(matches!(
        old.probe(SymId(prefix_len as u32)),
        CacheProbe::Miss
    ));
}

#[test]
fn aligned_and_empty_batches_do_not_copy_any_existing_rows() {
    let copies = std::rc::Rc::new(std::cell::Cell::new(0));
    let make = |value| CountedRow {
        value: value as u32,
        clones: std::rc::Rc::clone(&copies),
    };
    let old = PagedRows::<_, NameId>::from_rows((0..512).map(make).collect()).unwrap();
    let same = old.extending([]).unwrap();
    assert!(
        Arc::ptr_eq(&old.pages, &same.pages),
        "empty batch shares its whole directory"
    );
    let new = old.extending((512..1024).map(make)).unwrap();
    assert_eq!(copies.get(), 0);
    assert!(Arc::ptr_eq(&old.pages[0], &new.pages[0]));
    assert!(Arc::ptr_eq(&old.pages[1], &new.pages[1]));
    assert_eq!(new.span().len(), 1024);
}
