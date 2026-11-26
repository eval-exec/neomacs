//! The member-producing funnel and GNU `make-symbol` stay distinct.
//!
//! PEER 5786 step 2: a bare name-registry `intern` creates a canonical
//! SymId but no obarray membership; only `Obarray::intern_member` may turn
//! a fresh canonical name into a Lisp-visible symbol, and
//! `make_uninterned_symbol` produces GNU's uninterned object.

use super::{Obarray, intern};
use crate::intern::{is_canonical_id, make_uninterned_symbol};

#[test]
fn name_registry_intern_alone_creates_no_membership() {
    let mut obarray = Obarray::new();
    let id = intern("p5786-name-registry-only");
    assert!(!obarray.is_global_member(id));
    obarray.ensure_symbol_id(id);
    assert!(!obarray.is_global_member(id));
    assert!(obarray.intern_soft("p5786-name-registry-only").is_none());

    let member_id = obarray.intern_member("p5786-name-registry-only");
    assert_eq!(member_id, id, "the canonical id is reused");
    assert!(obarray.is_global_member(member_id));
    assert!(obarray.intern_soft("p5786-name-registry-only").is_some());
}

#[test]
fn make_uninterned_symbol_is_never_a_member() {
    let mut obarray = Obarray::new();
    let uninterned = make_uninterned_symbol("p5786-uninterned");
    assert!(!is_canonical_id(uninterned));
    assert!(!obarray.is_global_member(uninterned));

    let canonical = obarray.intern_member("p5786-uninterned");
    assert_ne!(
        canonical, uninterned,
        "GNU make-symbol and intern produce different objects"
    );
    assert!(obarray.is_global_member(canonical));
    assert!(!obarray.is_global_member(uninterned));
}
