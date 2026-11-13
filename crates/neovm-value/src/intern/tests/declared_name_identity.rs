use super::StringInterner;
use crate::heap_types::{GnuDeclaredCharacterCount, LispString};

fn supplied(bytes: &[u8], chars: i128) -> LispString {
    LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(chars).expect("test metadata"),
    )
}

#[test]
fn name_identity_ignores_multibyte_flag_for_matching_chars_and_bytes() {
    let mut names = StringInterner::new();
    let ub = LispString::from_unibyte_slice(&[233, 233, 233]);
    let mb = supplied(&[233, 233, 233], 3);
    assert_eq!(names.intern_lisp_string(&ub), names.intern_lisp_string(&mb));
    assert_eq!(names.lookup_lisp_string(&ub), names.lookup_lisp_string(&mb));
}

#[test]
fn name_identity_distinguishes_supplied_counts_for_identical_bytes() {
    let mut names = StringInterner::new();
    let unicode = LispString::from_utf8("λ");
    let supplied = supplied("λ".as_bytes(), 2);
    let ub = LispString::from_unibyte_slice("λ".as_bytes());
    let unicode_id = names.intern_lisp_string(&unicode);
    let supplied_id = names.intern_lisp_string(&supplied);
    assert_ne!(unicode_id, supplied_id);
    assert_eq!(names.intern_lisp_string(&ub), supplied_id);
    assert_eq!(names.lookup_lisp_string(&unicode), Some(unicode_id));
    assert_eq!(names.lookup_lisp_string(&supplied), Some(supplied_id));
}

#[test]
fn ascii_atom_normalization_cannot_erase_declared_character_count() {
    let mut names = StringInterner::new();
    let name = supplied(b"ab", 1);
    let id = names.intern_lisp_string(&name);
    assert_eq!(names.resolve_lisp_string(id).schars(), 1);
    assert_ne!(id, names.intern("ab"));
}

#[test]
fn utf8_lookup_and_supplied_name_use_the_same_identity() {
    let mut names = StringInterner::new();
    let id = names.intern("λé");
    let name = supplied("λé".as_bytes(), 2);
    assert_eq!(names.intern_lisp_string(&name), id);
    assert_eq!(names.lookup("λé"), Some(id));
    assert_eq!(names.resolve_lisp_string(id).as_bytes(), "λé".as_bytes());
    assert_eq!(names.resolve_lisp_string(id).schars(), 2);
}

#[test]
fn zero_character_nonempty_name_is_distinct_from_ascii_spelling() {
    let mut names = StringInterner::new();
    let zero = supplied(b"a", 0);
    assert_ne!(names.intern_lisp_string(&zero), names.intern("a"));
}

#[test]
fn empty_query_preserves_positive_count_before_string_construction() {
    let mut names = StringInterner::new();
    let empty = names.intern("");
    let count = GnuDeclaredCharacterCount::try_from(1).expect("test metadata");
    let query = super::SymbolNameQuery::new(b"", count);
    assert_eq!(names.lookup_name_query(query), None);
    let name = supplied(b"", 1);
    assert_eq!(name.schars(), 0);
    assert_eq!(names.lookup_lisp_string(&name), Some(empty));
}
