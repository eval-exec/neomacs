use super::{Fixnum, TaggedValue};

#[test]
fn validated_fixnum_rejects_payload_bit_loss() {
    for n in [
        TaggedValue::MOST_NEGATIVE_FIXNUM,
        -1,
        0,
        1,
        TaggedValue::MOST_POSITIVE_FIXNUM,
    ] {
        assert_eq!(
            TaggedValue::from_fixnum(Fixnum::try_from(n).unwrap()).as_fixnum(),
            Some(n)
        );
    }
    assert!(Fixnum::try_from(TaggedValue::MOST_NEGATIVE_FIXNUM - 1).is_err());
    assert!(Fixnum::try_from(TaggedValue::MOST_POSITIVE_FIXNUM + 1).is_err());
    assert_eq!(
        i64::from(Fixnum::saturating(i64::MAX)),
        TaggedValue::MOST_POSITIVE_FIXNUM
    );
    assert_eq!(
        i64::from(Fixnum::saturating(i64::MIN)),
        TaggedValue::MOST_NEGATIVE_FIXNUM
    );
}

#[test]
fn explicit_gnu_payload_bits_are_signed_immediates() {
    for raw in [0, u64::MAX, 1u64 << 61, 1u64 << 62, i64::MAX as u64] {
        let expected = (raw.wrapping_shl(2) as i64) >> 2;
        let value = TaggedValue::from_fixnum(Fixnum::from_payload_bits(raw));
        assert_eq!(value.as_fixnum(), Some(expected));
    }
}

#[test]
fn checked_fixnum_division_preserves_the_payload_domain() {
    let operands = [
        TaggedValue::MOST_NEGATIVE_FIXNUM,
        TaggedValue::MOST_NEGATIVE_FIXNUM + 1,
        -7,
        -3,
        -1,
        0,
        1,
        3,
        7,
        TaggedValue::MOST_POSITIVE_FIXNUM - 1,
        TaggedValue::MOST_POSITIVE_FIXNUM,
    ];
    for lhs in operands {
        for rhs in operands {
            let got = Fixnum::try_from(lhs)
                .unwrap()
                .checked_div(Fixnum::try_from(rhs).unwrap());
            if rhs == 0 || (lhs == TaggedValue::MOST_NEGATIVE_FIXNUM && rhs == -1) {
                assert_eq!(got, None, "{lhs} / {rhs}");
            } else {
                let want = lhs / rhs;
                assert!(
                    (TaggedValue::MOST_NEGATIVE_FIXNUM..=TaggedValue::MOST_POSITIVE_FIXNUM)
                        .contains(&want)
                );
                assert_eq!(got.map(i64::from), Some(want), "{lhs} / {rhs}");
            }
        }
    }
}
