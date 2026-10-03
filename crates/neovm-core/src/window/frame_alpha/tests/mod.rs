use super::*;
#[test]
fn gnu_frame_alpha_components() {
    assert_eq!(component(Value::fixnum(1), 1.0).unwrap(), 0.01);
    assert_eq!(component(Value::make_float(1.0), 1.0).unwrap(), 1.0);
    assert_eq!(component(Value::NIL, 1.0).unwrap(), 1.0);
    for v in [
        Value::fixnum(-1),
        Value::fixnum(101),
        Value::make_float(f64::NAN),
        Value::make_float(f64::INFINITY),
        Value::symbol("bad"),
    ] {
        assert!(component(v, 1.0).is_err());
    }
    assert!(component(Value::list(vec![Value::fixnum(50)]), 1.0).is_err());
}
#[test]
fn gnu_frame_alpha_raw_and_accepted_state_are_distinct() {
    use crate::buffer::BufferId;
    use crate::window::{Frame, FrameId, FrameParam, Rect, Window, WindowId};
    let root = Window::new_leaf(WindowId(1), BufferId(0), Rect::new(0.0, 0.0, 80.0, 60.0));
    let mut frame = Frame::new(
        FrameId(1),
        Value::string("alpha"),
        1,
        80,
        60,
        root,
        WindowId(2),
    );
    frame.set_known_parameter(FrameParam::AlphaBackground, Value::fixnum(50));
    frame.set_known_parameter(FrameParam::AlphaBackground, Value::fixnum(101));
    assert_eq!(frame.background_alpha, 0.5);
    assert_eq!(
        frame.known_parameter(FrameParam::AlphaBackground),
        Some(Value::fixnum(101))
    );
    frame.set_known_parameter(FrameParam::Alpha, Value::fixnum(80));
    frame.set_known_parameter(
        FrameParam::Alpha,
        Value::list(vec![Value::fixnum(30), Value::fixnum(101)]),
    );
    assert_eq!(frame.frame_alpha, [0.8, 0.8]);
    frame.set_known_parameter(FrameParam::AlphaBackground, Value::NIL);
    assert_eq!(frame.background_alpha, 1.0);
    frame.set_known_parameter(FrameParam::Alpha, Value::NIL);
    assert_eq!(frame.frame_alpha, [-1.0; 2]);
}

#[test]
fn gnu_frame_alpha_pair() {
    assert_eq!(pair(Value::fixnum(50)).unwrap(), [0.5, 0.5]);
    assert_eq!(
        pair(Value::cons(Value::fixnum(80), Value::fixnum(60))).unwrap(),
        [0.8, 0.6]
    );
    assert_eq!(
        pair(Value::list(vec![Value::fixnum(80)])).unwrap(),
        [0.8, -1.0]
    );
    assert_eq!(
        pair(Value::list(vec![
            Value::fixnum(80),
            Value::fixnum(60),
            Value::symbol("ignored")
        ]))
        .unwrap(),
        [0.8, 0.6]
    );
    assert!(pair(Value::list(vec![Value::fixnum(80), Value::fixnum(101)])).is_err());
}
