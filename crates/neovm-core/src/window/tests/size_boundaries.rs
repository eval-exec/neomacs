use super::{HorizontalScroll, SplitSizeError, SplitSizes, WindowPixels, WindowTotal};

#[test]
fn tsb_window_boundary_types_reject_invalid_extents_and_clip_scroll() {
    assert!(WindowPixels::try_from(-1).is_err());
    assert!(WindowPixels::try_from(i64::MAX).is_err());
    assert_eq!(i64::from(WindowPixels::try_from(0).unwrap()), 0);
    assert_eq!(
        i64::from(WindowPixels::try_from(i64::from(i32::MAX)).unwrap()),
        i64::from(i32::MAX)
    );
    assert_eq!(SplitSizes::new(0.0, None), Err(SplitSizeError::NewTooSmall));
    assert_eq!(
        SplitSizes::new(1.0, Some(1)),
        Err(SplitSizeError::OldTooSmall)
    );
    assert_eq!(
        SplitSizes::new(80.0, Some(i64::MIN)),
        Err(SplitSizeError::NewTooSmall)
    );
    assert_eq!(
        SplitSizes::new(80.0, Some(i64::MAX)),
        Err(SplitSizeError::OldTooSmall)
    );
    let sizes = SplitSizes::new(80.0, Some(-30)).unwrap();
    assert_eq!((sizes.old(), sizes.new_size()), (30.0, 50.0));
    let max = crate::emacs_core::value::Value::MOST_POSITIVE_FIXNUM;
    let total = WindowTotal::try_from(max).unwrap();
    let added = total.gnu_added(total);
    assert_eq!(i64::from(added), -2);
    assert_eq!(i64::from(added.gnu_added(total)), max - 2);
    assert_eq!(i64::from(HorizontalScroll::saturating(i128::MIN)), 0);
    assert_eq!(
        i64::from(HorizontalScroll::saturating(i128::MAX)),
        crate::emacs_core::value::Value::MOST_POSITIVE_FIXNUM
    );
}

#[test]
fn tsb_window_pixel_staging_keeps_range_validation_ahead_of_c_int_narrowing() {
    use super::{WindowPixelOperation, WindowPixelStage};
    use crate::emacs_core::value::Value;
    use crate::tagged::value::Fixnum;

    assert!(WindowPixelStage::try_from((-1, WindowPixelOperation::Set)).is_err());
    assert_eq!(
        i64::from(WindowPixelStage::try_from((0, WindowPixelOperation::Set)).unwrap()),
        0,
    );
    assert!(
        WindowPixelStage::try_from((i64::from(i32::MAX) + 1, WindowPixelOperation::Set)).is_err()
    );
    let ordinary_add = WindowPixelOperation::Add(Fixnum::try_from(12).unwrap());
    assert!(WindowPixelStage::try_from((-13, ordinary_add)).is_err());
    assert_eq!(
        i64::from(WindowPixelStage::try_from((-12, ordinary_add)).unwrap()),
        0
    );
    assert_eq!(
        i64::from(WindowPixelStage::try_from((i64::from(i32::MAX) - 12, ordinary_add)).unwrap()),
        i64::from(i32::MAX),
    );
    assert!(WindowPixelStage::try_from((i64::from(i32::MAX) - 11, ordinary_add)).is_err());

    // A failed sibling plan can leave MINF in the staging slot. The accepted
    // large integer delta is deliberately narrowed only after range checking.
    let legacy_add =
        WindowPixelOperation::Add(Fixnum::try_from(Value::MOST_NEGATIVE_FIXNUM).unwrap());
    let (minimum, maximum) = legacy_add.bounds();
    assert!(WindowPixelStage::try_from((minimum - 1, legacy_add)).is_err());
    assert!(WindowPixelStage::try_from((maximum + 1, legacy_add)).is_err());
    assert_eq!(
        i64::from(WindowPixelStage::try_from((minimum, legacy_add)).unwrap()),
        Value::MOST_NEGATIVE_FIXNUM,
    );
    assert_eq!(
        i64::from(WindowPixelStage::try_from((maximum, legacy_add)).unwrap()),
        Value::MOST_NEGATIVE_FIXNUM + i64::from(i32::MAX),
    );
}
