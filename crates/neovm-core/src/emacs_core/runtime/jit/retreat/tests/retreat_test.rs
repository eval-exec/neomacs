//! The per-pc retreat word and its lazily allocated table.

use super::*;

/// The bits are distinct, fit the low byte, and leave the count byte alone.
#[test]
fn retreat_bits_are_distinct_and_fit_the_low_byte() {
    let mut seen = 0u16;
    for bit in RetreatBit::ALL {
        let b = bit as u16;
        assert_eq!(b.count_ones(), 1, "{bit:?}");
        assert_eq!(
            b & !SiteRetreat::BITS_MASK,
            0,
            "{bit:?} leaves the low byte"
        );
        assert_eq!(seen & b, 0, "{bit:?} repeats a bit");
        seen |= b;
    }
    let site = SiteRetreat::default();
    for bit in RetreatBit::ALL {
        site.set(bit);
    }
    assert_eq!(site.count(), 0);
    assert_eq!(site.bits().collect::<Vec<_>>(), RetreatBit::ALL);
}

/// Setting one bit sets only that bit, and the word renders it by name.
#[test]
fn a_set_bit_is_the_only_one_on() {
    for bit in RetreatBit::ALL {
        let site = SiteRetreat::default();
        site.set(bit);
        for other in RetreatBit::ALL {
            assert_eq!(site.has(other), other == bit, "{bit:?} vs {other:?}");
        }
        assert!(format!("{site:?}").contains(bit.name()));
    }
}

/// A read never allocates; the first mark sizes the table to the body.
#[test]
fn the_table_allocates_on_the_first_mark_only() {
    let table = SiteRetreatTable::new();
    assert!(!table.has(3, RetreatBit::NoInline));
    assert!(table.get(3).is_none(), "a read allocated the table");
    table
        .site(3, 8)
        .expect("pc 3 of an 8-op body")
        .set(RetreatBit::NoInline);
    assert!(table.has(3, RetreatBit::NoInline));
    assert!(!table.has(2, RetreatBit::NoInline));
    assert!(!table.has(3, RetreatBit::NoHoist));
    assert!(table.site(8, 8).is_none(), "pc past the body");
    assert!(!table.has(100, RetreatBit::NoInline));
}

/// P0.4's no-inline marks keep their meaning through the table.
#[test]
fn no_inline_marks_live_in_the_retreat_table() {
    let rt = crate::emacs_core::jit::RuntimeState::new();
    assert!(!rt.call_site_no_inline(5));
    rt.mark_call_site_no_inline(5, 10);
    assert!(rt.call_site_no_inline(5));
    assert!(!rt.call_site_no_inline(4));
    assert!(!rt.call_site_no_inline(6));
    // A pc past the body is ignored, as the bitset did.
    rt.mark_call_site_no_inline(64, 10);
    assert!(!rt.call_site_no_inline(64));
}
