use super::*;

#[test]
fn linear_gradient_validation() {
    let grad = Gradient::Linear {
        angle: 45.0,
        stops: vec![
            ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),
            ColorStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)),
        ],
    };
    assert!(grad.validate().is_ok());

    // Invalid: out of order
    let bad = Gradient::Linear {
        angle: 45.0,
        stops: vec![
            ColorStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)),
            ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),
        ],
    };
    assert!(bad.validate().is_err());
}

#[test]
fn radial_gradient_validation() {
    let grad = Gradient::Radial {
        center_x: 0.5,
        center_y: 0.5,
        radius: 0.7,
        stops: vec![ColorStop::new(0.0, Color::rgb(1.0, 1.0, 1.0))],
    };
    assert!(grad.validate().is_ok());

    // Invalid: center out of bounds
    let bad = Gradient::Radial {
        center_x: 1.5,
        center_y: 0.5,
        radius: 0.7,
        stops: vec![ColorStop::new(0.0, Color::rgb(1.0, 1.0, 1.0))],
    };
    assert!(bad.validate().is_err());
}
