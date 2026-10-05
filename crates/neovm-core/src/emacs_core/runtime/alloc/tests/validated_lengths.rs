use super::{
    AllocLen, BufferByteLen, CharTableExtras, HashTableSize, ObarrayBits, RecordLen, RepeatCount,
};
use crate::emacs_core::value::Value;

#[test]
fn validated_allocation_counts_reject_before_reservation() {
    assert_eq!(
        RecordLen::try_from(Value::fixnum(4094)).unwrap().capacity(),
        4095
    );
    assert!(RecordLen::try_from(Value::fixnum(4095)).is_err());
    assert!(RecordLen::try_from(Value::fixnum(Value::MOST_POSITIVE_FIXNUM)).is_err());
    assert_eq!(ObarrayBits::try_from(Value::NIL).unwrap().capacity(), 8);
    assert_eq!(
        ObarrayBits::try_from(Value::fixnum(0)).unwrap().capacity(),
        1
    );
    assert_eq!(
        ObarrayBits::try_from(Value::fixnum(8)).unwrap().capacity(),
        16
    );
    assert!(ObarrayBits::try_from(Value::fixnum(1 << 31)).is_err());
    assert!(HashTableSize::try_from(Value::fixnum(Value::MOST_POSITIVE_FIXNUM)).is_err());
    assert_eq!(
        CharTableExtras::try_from(Value::fixnum(10))
            .unwrap()
            .capacity(),
        10
    );
    assert_eq!(
        CharTableExtras::try_from(Value::fixnum(Value::MOST_POSITIVE_FIXNUM))
            .unwrap()
            .capacity(),
        0
    );
    let reserved = BufferByteLen::repeated(5, RepeatCount::try_from(50).unwrap(), 1)
        .unwrap()
        .reserved_bytes()
        .unwrap();
    assert!(reserved.is_empty() && reserved.capacity() > 250);
    assert!(
        BufferByteLen::repeated(
            1,
            RepeatCount::try_from(Value::MOST_POSITIVE_FIXNUM).unwrap(),
            0
        )
        .is_err()
    );
    assert!(
        BufferByteLen::repeated(
            5,
            RepeatCount::try_from(Value::MOST_POSITIVE_FIXNUM).unwrap(),
            0
        )
        .is_err()
    );
    assert!(
        BufferByteLen::repeated(
            1,
            RepeatCount::try_from(1).unwrap(),
            (Value::MOST_POSITIVE_FIXNUM - 1) as usize
        )
        .is_err()
    );
}
