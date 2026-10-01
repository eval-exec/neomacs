//! GNU's `max-image-size` (`src/image.c:13037`) as this port reads it.

use super::{ImageScalePolicy, ImageSizeLimit, image_scale_environment, image_size_limit};
use crate::emacs_core::Value;
use crate::emacs_core::eval::Context;

/// A frame of the given logical pixel size with its physical scale, plus the
/// obarray the variable lives in.
fn frame_environment(width: u32, height: u32, device_scale: f64) -> Context {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("max-image-size");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("max-image-size", width, height, buffer);
    let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
    frame.device_scale_factor = device_scale;
    eval
}

fn limit_of(eval: &Context, frame: crate::window::FrameId) -> ImageSizeLimit {
    let frame = eval.frame_manager().get(frame).expect("frame");
    image_size_limit(frame, eval.obarray())
}

fn frame_id(eval: &Context) -> crate::window::FrameId {
    eval.frame_manager()
        .selected_frame()
        .expect("the created frame is selected")
        .id
}

#[test]
fn a_float_is_a_fraction_of_this_frames_own_pixel_extent() {
    // GNU: `width <= XFLOAT_DATA (Vmax_image_size) * FRAME_PIXEL_WIDTH (f)`.
    let mut eval = frame_environment(800, 600, 1.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::make_float(0.5));

    let limit = limit_of(&eval, frame);

    assert_eq!(limit.maximum().dimensions(), (400, 300));
}

/// GNU compares against `FRAME_PIXEL_WIDTH`, which is in device pixels.
/// Neomacs frames carry logical pixel geometry and publish the physical scale
/// beside it, so the fraction has to be taken of the physical extent.
#[test]
fn a_float_is_taken_of_the_physical_frame_extent() {
    let mut eval = frame_environment(800, 600, 2.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::make_float(0.5));

    let limit = limit_of(&eval, frame);

    assert_eq!(limit.maximum().dimensions(), (800, 600));
}

#[test]
fn an_integer_is_an_absolute_bound_independent_of_the_frame() {
    let mut eval = frame_environment(800, 600, 2.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::fixnum(100));

    let limit = limit_of(&eval, frame);

    assert_eq!(limit.maximum().dimensions(), (100, 100));
}

/// `FIXNUMP` is tested first: an integer must not be read as a ratio.
#[test]
fn an_integer_is_not_interpreted_as_a_frame_ratio() {
    let mut eval = frame_environment(800, 600, 1.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::fixnum(2));

    assert_eq!(limit_of(&eval, frame).maximum().dimensions(), (2, 2));
}

/// GNU's `else return 1`: a non-numeric value is no limit at all, and so is a
/// symbol that is not bound at all (`(makunbound 'max-image-size)`).
#[test]
fn a_non_numeric_or_unbound_value_is_no_limit() {
    let mut eval = frame_environment(800, 600, 1.0);
    let frame = frame_id(&eval);

    for value in [Value::NIL, Value::string("big"), Value::symbol("huge")] {
        eval.obarray_mut()
            .set_symbol_value("max-image-size", value.clone());
        assert_eq!(
            limit_of(&eval, frame),
            ImageSizeLimit::UNLIMITED,
            "value = {value:?}"
        );
    }
}

/// GNU's own initializer is `make_float (MAX_IMAGE_SIZE)`, `MAX_IMAGE_SIZE`
/// 10.0 (`src/image.c:1740`, `:13047`), which on nothing but the frame is a
/// ten-frame-ratio bound. That it is what the bootstrap installs is asserted by
/// the GNU-surface suite (`gnu_surface/defvar_special.rs`); this pins what the
/// value *means* once it is there.
#[test]
fn the_bootstrapped_default_bounds_a_frame_at_ten_frame_ratios() {
    let mut eval = frame_environment(800, 600, 1.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::make_float(10.0));

    assert_eq!(limit_of(&eval, frame).maximum().dimensions(), (8000, 6000));
}

/// The limit and the scaling inputs are resolved together, so anything that
/// has the frame environment has the bound that goes with it.
#[test]
fn the_frame_environment_carries_the_same_limit() {
    let mut eval = frame_environment(800, 600, 1.0);
    let frame = frame_id(&eval);
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::fixnum(64));

    let environment = {
        let frame = eval.frame_manager().get(frame).expect("frame");
        image_scale_environment(frame, eval.obarray())
    };

    assert_eq!(environment.size_limit().maximum().dimensions(), (64, 64));
    // The scaling half of the environment is untouched by the limit.
    assert_eq!(
        environment
            .resolve(ImageScalePolicy::Unspecified)
            .device_scale(),
        1.0
    );
}
