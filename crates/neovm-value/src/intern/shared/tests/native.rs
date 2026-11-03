use super::super::super::{LispString, NameAtomStorage, NameId, SymbolNameOrigin};
use super::*;
use crate::heap_types::GnuDeclaredCharacterCount;

fn row(atoms: &mut NameAtomStorage, name: &str) -> SymbolRow {
    SymbolRow::new(
        atoms.push(LispString::from_utf8(name)),
        NameId(0),
        CanonicalClass::Canonical,
        SymbolNameOrigin::AtomOnly,
        NativeQueryClass::NativeUtf8,
    )
}
fn supplied(atoms: &mut NameAtomStorage, name: &str, chars: usize) -> NameAtomRef {
    atoms.push(LispString::from_emacs_bytes_with_declared_chars(
        name.as_bytes().to_vec(),
        GnuDeclaredCharacterCount::try_from(chars as i128).unwrap(),
    ))
}

#[test]
fn admission_validates_utf8_and_reuses_the_existing_character_count_domain() {
    let mut atoms = NameAtomStorage::new();
    for text in ["", "ascii", "λ界", "nul\0tail"] {
        let source = row(&mut atoms, text).atom();
        let native = NativeUtf8Atom::try_from_atom(source).unwrap();
        assert_eq!(native.as_str(), text);
        assert_eq!(native.as_str().as_ptr(), source.0.as_bytes().as_ptr());
        let equivalent = supplied(&mut atoms, text, text.chars().count());
        assert_eq!(
            NativeUtf8Atom::try_from_atom(equivalent).unwrap().as_str(),
            text
        );
    }
    for (text, count) in [("ascii", 0), ("λ", "λ".len())] {
        assert!(matches!(
            NativeUtf8Atom::try_from_atom(supplied(&mut atoms, text, count)),
            Err(NativeAtomRefusal::CharacterCountMismatch)
        ));
    }
    for bytes in [&[0xff][..], &[0xc0, 0x80][..]] {
        let byte8 = atoms.push(LispString::from_unibyte_slice(bytes));
        assert!(matches!(
            NativeUtf8Atom::try_from_atom(byte8),
            Err(NativeAtomRefusal::InvalidUtf8)
        ));
    }
}

#[test]
fn builder_requires_canonical_eligible_rows_and_conflicts_leave_it_unchanged() {
    let mut atoms = NameAtomStorage::new();
    let atom = row(&mut atoms, ":entry").atom();
    let mut builder = NativeIndexBuilder::new();
    let uninterned = SymbolRow::new(
        atom,
        NameId(0),
        CanonicalClass::Uninterned,
        SymbolNameOrigin::LispObject,
        NativeQueryClass::NativeUtf8,
    );
    assert_eq!(
        builder.insert(SymId(1), &uninterned),
        Err(NativeIndexError::Uninterned)
    );
    let exact = SymbolRow::new(
        atom,
        NameId(0),
        CanonicalClass::Canonical,
        SymbolNameOrigin::LispObject,
        NativeQueryClass::ExactOnly,
    );
    assert_eq!(
        builder.insert(SymId(1), &exact),
        Err(NativeIndexError::ExactOnly)
    );
    let canonical = row(&mut atoms, ":entry");
    builder.insert(SymId(1), &canonical).unwrap();
    builder.insert(SymId(1), &canonical).unwrap();
    assert_eq!(
        builder.insert(SymId(2), &canonical),
        Err(NativeIndexError::ConflictingCanonical)
    );
    builder.insert(SymId(0), &row(&mut atoms, "nil")).unwrap();
    let index = builder.build();
    assert_eq!(index.entry_count().len(), 2);
    assert_eq!(index.probe(":entry"), CacheProbe::Hit(SymId(1)));
    assert_eq!(index.probe("nil"), CacheProbe::Hit(SymId(0)));
    // Native positivity supplies no live-name/obarray proof for Lisp keywordp.
    assert_eq!(
        canonical.atom_keyword(),
        super::super::snapshot::AtomKeyword::Colon
    );
}

#[test]
fn supplied_counts_cannot_alias_native_and_temporary_queries_are_never_retained() {
    let mut atoms = NameAtomStorage::new();
    let unusual = SymbolRow::new(
        supplied(&mut atoms, "native-λ", 0),
        NameId(0),
        CanonicalClass::Canonical,
        SymbolNameOrigin::LispObject,
        NativeQueryClass::NativeUtf8,
    );
    let mut builder = NativeIndexBuilder::new();
    assert_eq!(
        builder.insert(SymId(1), &unusual),
        Err(NativeIndexError::Atom(
            NativeAtomRefusal::CharacterCountMismatch
        ))
    );
    assert_eq!(builder.index.probe("native-λ"), CacheProbe::Miss);
    builder
        .insert(SymId(2), &row(&mut atoms, "native-λ"))
        .unwrap();
    let index = builder.build();
    let query = String::from("native-λ");
    assert_eq!(index.probe(&query), CacheProbe::Hit(SymId(2)));
    drop(query);
    assert_eq!(index.probe("native-λ"), CacheProbe::Hit(SymId(2)));
    assert_eq!(index.probe("missing"), CacheProbe::Miss);
}

#[test]
fn splits_copy_only_the_changed_bounded_path_and_keep_predecessors_readable() {
    let mut atoms = NameAtomStorage::new();
    let mut builder = NativeIndexBuilder::new();
    let names: Vec<_> = (0..300).map(|i| format!("shared-native-{i}")).collect();
    for (i, name) in names.iter().enumerate() {
        builder
            .insert(SymId(i as u32), &row(&mut atoms, name))
            .unwrap();
    }
    let old = builder.build();
    let extra = "shared-native-extra";
    let hash = NativeHash::of(&NativeCacheQuery::from(extra));
    let (changed, _) = HashDepth::ROOT.descend(hash).unwrap();
    let mut builder = old.builder();
    builder.insert(SymId(500), &row(&mut atoms, extra)).unwrap();
    let new = builder.build();
    let (Some(Node::Directory(before)), Some(Node::Directory(after))) =
        (old.root.as_deref(), new.root.as_deref())
    else {
        panic!("split roots");
    };
    let mut shared = 0;
    for i in 0..FANOUT {
        if i == changed.offset() {
            continue;
        }
        match (&before[i], &after[i]) {
            (Some(before), Some(after)) => {
                assert!(Arc::ptr_eq(before, after));
                shared += 1;
            }
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => panic!("unrelated branch changed"),
        }
    }
    assert!(shared > 0);
    assert_eq!(old.entry_count().len(), 300);
    assert_eq!(new.entry_count().len(), 301);
    assert_eq!(old.probe(extra), CacheProbe::Miss);
    assert_eq!(new.probe(extra), CacheProbe::Hit(SymId(500)));
    for (i, name) in names.iter().enumerate() {
        assert_eq!(old.probe(name), CacheProbe::Hit(SymId(i as u32)));
        assert_eq!(new.probe(name), CacheProbe::Hit(SymId(i as u32)));
    }
}

#[test]
fn invalidation_preserves_old_views_and_stale_alias_removal_keeps_the_new_winner() {
    let mut atoms = NameAtomStorage::new();
    let row = row(&mut atoms, "reintern");
    let atom = NativeUtf8Atom::try_from_atom(row.atom()).unwrap();
    let mut builder = NativeIndexBuilder::new();
    builder.insert(SymId(4), &row).unwrap();
    let old = builder.build();
    let removed = old.without_symbol(atom, SymId(4)).unwrap();
    assert_eq!(old.probe("reintern"), CacheProbe::Hit(SymId(4)));
    assert_eq!(removed.probe("reintern"), CacheProbe::Miss);
    assert_eq!(removed.entry_count().len(), 0);
    let mut builder = removed.builder();
    builder.insert(SymId(5), &row).unwrap();
    let new = builder.build();
    let unchanged = new.without_symbol(atom, SymId(4)).unwrap();
    assert!(Arc::ptr_eq(
        new.root.as_ref().unwrap(),
        unchanged.root.as_ref().unwrap()
    ));
    assert_eq!(unchanged.probe("reintern"), CacheProbe::Hit(SymId(5)));
}

#[test]
fn equal_hash_fragments_still_compare_the_validated_spelling() {
    let mut atoms = NameAtomStorage::new();
    let query = NativeCacheQuery::from("absent-λ");
    let mut entry = NativeEntry::from_row(SymId(7), &row(&mut atoms, "present-λ")).unwrap();
    // Mechanism-level collision injection: the production constructor always
    // derives this fragment from the common native hash rule.
    entry.hash = NativeHash::of(&query).fragment();
    assert!(!entry.matches(&query));
    let exact = NativeCacheQuery::from(entry.atom.as_str());
    entry.hash = NativeHash::of(&exact).fragment();
    assert!(entry.matches(&exact));
}

#[test]
fn exhausted_hash_depth_refuses_a_full_bucket_without_modifying_it() {
    let mut atoms = NameAtomStorage::new();
    let entries: Vec<_> = (0..BUCKET_ENTRIES)
        .map(|i| {
            NativeEntry::from_row(SymId(i as u32), &row(&mut atoms, &format!("collision-{i}")))
                .unwrap()
        })
        .collect();
    let bucket = Arc::new(Node::Bucket(Arc::from(entries)));
    let extra = NativeEntry::from_row(SymId(20), &row(&mut atoms, "extra-collision")).unwrap();
    let hash = NativeHash::of(&NativeCacheQuery::from(extra.atom.as_str()));
    assert!(matches!(
        inserting(Some(&bucket), extra, hash, HashDepth(16)),
        Err(NativeIndexError::Capacity)
    ));
    match &*bucket {
        Node::Bucket(entries) => assert_eq!(entries.len(), BUCKET_ENTRIES),
        Node::Directory(_) => panic!("predecessor mutated"),
    }
}
