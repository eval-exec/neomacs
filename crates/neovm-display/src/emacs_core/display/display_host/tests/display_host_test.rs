use super::{FrameFontRequest, FrameFontSize};
use crate::face::{Face, FaceHeight};

#[test]
fn frame_font_size_constructors_reject_invalid_lisp_numbers() {
    assert_eq!(FrameFontSize::pixels(0), None);
    assert_eq!(FrameFontSize::pixels(-1), None);
    assert_eq!(FrameFontSize::points(0.0), None);
    assert_eq!(FrameFontSize::points(f64::NAN), None);
    assert_eq!(FrameFontSize::points(f64::INFINITY), None);
    assert_eq!(FrameFontSize::relative(0.0), None);
    assert_eq!(FrameFontSize::relative(f64::NAN), None);
    assert_eq!(FrameFontSize::relative(f64::INFINITY), None);
}

#[test]
fn frame_font_request_has_one_sizing_authority() {
    let mut face = Face::new("default");
    face.height = Some(FaceHeight::Absolute(150));

    let request = FrameFontRequest::from_face(face);

    assert_eq!(
        request.size(),
        FrameFontSize::points(15.0).expect("positive point size")
    );
    assert_eq!(request.face().height, None);

    let mut face = Face::new("default");
    face.height = Some(FaceHeight::Relative(2.0));
    let request = FrameFontRequest::with_size(
        face,
        FrameFontSize::pixels(15).expect("positive pixel size"),
    );

    assert_eq!(request.face().height, None);
    assert_eq!(
        request.size(),
        FrameFontSize::pixels(15).expect("positive pixel size")
    );
}
