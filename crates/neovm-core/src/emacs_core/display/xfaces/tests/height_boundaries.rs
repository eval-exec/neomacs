#[test]
fn tsb_face_height_types_keep_lisp_and_backend_ranges_distinct() {
    use super::{FaceHeightError, NumericFaceHeight, gnu_height_merge_fixnum};
    use crate::emacs_core::value::Value;
    use crate::face::FaceHeight;

    let height = NumericFaceHeight::try_from(Value::fixnum(2_147_483_648)).unwrap();
    assert_eq!(
        FaceHeight::try_from(height),
        Err(FaceHeightError::BackendRange)
    );
    assert_eq!(i64::from(gnu_height_merge_fixnum(f64::INFINITY)), 0);
    assert_eq!(i64::from(gnu_height_merge_fixnum(f64::NAN)), 0);
    assert_eq!(i64::from(gnu_height_merge_fixnum(1.0e301)), 0);
    // GNU's internal test merge deliberately narrows the signed payload.
    assert!(i64::from(gnu_height_merge_fixnum(4.7e18)) > 0);
    assert!(i64::from(gnu_height_merge_fixnum(2.4e18)) < 0);
}

#[test]
fn tsb_backend_height_accepts_small_merged_scales_but_rejects_nonfinite() {
    use super::{BackendFaceHeight, FaceHeightError, PositiveRelativeHeight};
    use crate::emacs_core::value::Value;
    use crate::face::FaceHeight;

    // GNU merges two accepted 0.1 relative heights into approximately 0.01.
    // This backend scale need not pass the setter's test merge against 10.
    assert!(PositiveRelativeHeight::try_from(0.01).is_ok());
    assert!(PositiveRelativeHeight::try_from(f64::INFINITY).is_err());
    assert!(PositiveRelativeHeight::try_from(f64::NAN).is_err());
    assert!(PositiveRelativeHeight::try_from(0.0).is_err());
    assert_eq!(
        BackendFaceHeight::try_from(Value::fixnum(2_147_483_648)).map(FaceHeight::from),
        Err(FaceHeightError::BackendRange)
    );
    assert_eq!(
        BackendFaceHeight::try_from(Value::fixnum(120)).map(FaceHeight::from),
        Ok(FaceHeight::Absolute(120))
    );
}
