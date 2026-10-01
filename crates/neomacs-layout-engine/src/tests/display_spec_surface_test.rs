//! Parse tests for the `(surface :id N :width W :height H)` display spec.

use super::*;

#[test]
fn parse_surface_layout_full_spec() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_layout(&Value::list(vec![
        Value::symbol("surface"),
        Value::symbol(":id"),
        Value::fixnum(7),
        Value::symbol(":width"),
        Value::fixnum(320),
        Value::symbol(":height"),
        Value::fixnum(120),
    ]))
    .expect("surface layout");
    assert_eq!(
        layout,
        DisplaySurfaceLayout {
            surface_id: 7,
            width: 320.0,
            height: 120.0,
        }
    );
}

#[test]
fn parse_surface_layout_accepts_gc_managed_handle() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_layout(&Value::list(vec![
        Value::symbol("surface"),
        Value::symbol(":id"),
        Value::make_surface_handle(42),
        Value::symbol(":width"),
        Value::fixnum(320),
        Value::symbol(":height"),
        Value::fixnum(120),
    ]))
    .expect("surface layout from handle");
    assert_eq!(
        layout,
        DisplaySurfaceLayout {
            surface_id: 42,
            width: 320.0,
            height: 120.0,
        }
    );
}

#[test]
fn parse_surface_source_layout_accepts_handle_channel0() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_source_layout(
        &Value::list(vec![
            Value::symbol("surface"),
            Value::symbol(":shader"),
            Value::string(
                "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
            ),
            Value::symbol(":channel0"),
            Value::make_surface_handle(7),
        ]),
        640.0,
        480.0,
    )
    .expect("declarative surface layout with handle channel0");
    // The parser keeps the raw value; the resolver (which has the display
    // host) interprets it — handles resolve to Surface channel ids there.
    assert_eq!(
        layout
            .channel0_value
            .and_then(|value| value.as_surface_handle()),
        Some(7)
    );
    assert_eq!(layout.request.channel0, None);
}

#[test]
fn parse_surface_layout_requires_id() {
    let _eval = neovm_core::emacs_core::Context::new();
    assert!(
        parse_display_surface_layout(&Value::list(vec![
            Value::symbol("surface"),
            Value::symbol(":width"),
            Value::fixnum(320),
        ]))
        .is_none()
    );
}

#[test]
fn parse_surface_layout_defaults_missing_dimensions() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_layout(&Value::list(vec![
        Value::symbol("surface"),
        Value::symbol(":id"),
        Value::fixnum(1),
    ]))
    .expect("surface layout");
    assert_eq!(layout.width, 64.0);
    assert_eq!(layout.height, 64.0);
}

#[test]
fn parse_surface_source_layout_full_spec() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_source_layout(
        &Value::list(vec![
            Value::symbol("surface"),
            Value::symbol(":shader"),
            Value::string(
                "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }",
            ),
            Value::symbol(":uniforms"),
            Value::list(vec![
                Value::cons(Value::symbol("speed"), Value::make_float(2.0)),
                Value::cons(
                    Value::symbol("tint"),
                    Value::vector(vec![
                        Value::make_float(1.0),
                        Value::make_float(0.5),
                        Value::make_float(0.25),
                    ]),
                ),
            ]),
            Value::symbol(":animate"),
            Value::NIL,
            Value::symbol(":fps"),
            Value::fixnum(30),
            Value::symbol(":width"),
            Value::fixnum(200),
            Value::symbol(":height"),
            Value::fixnum(80),
        ]),
        640.0,
        480.0,
    )
    .expect("declarative surface layout");
    assert!(layout.request.source.contains("mainImage"));
    assert!(!layout.request.animate);
    assert_eq!(layout.request.fps, Some(30));
    assert_eq!(layout.request.width, 200);
    assert_eq!(layout.request.height, 80);
    assert_eq!(layout.width, 200.0);
    assert_eq!(layout.height, 80.0);
    assert_eq!(
        layout.request.uniforms,
        vec![
            ("speed".to_owned(), [2.0f32.to_bits(), 0, 0, 0], 1u8),
            (
                "tint".to_owned(),
                [1.0f32.to_bits(), 0.5f32.to_bits(), 0.25f32.to_bits(), 0u32],
                3u8
            ),
        ]
    );
}

#[test]
fn parse_surface_source_layout_defaults_animate_and_dimensions() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_source_layout(
        &Value::list(vec![
            Value::symbol("surface"),
            Value::symbol(":shader"),
            Value::string(
                "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
            ),
        ]),
        640.0,
        480.0,
    )
    .expect("declarative surface layout");
    assert!(layout.request.animate);
    // No :fps -> uncapped (render at display refresh).
    assert_eq!(layout.request.fps, None);
    assert_eq!(layout.width, 640.0);
    assert_eq!(layout.height, 480.0);
}

#[test]
fn parse_surface_source_layout_rejects_nonpositive_fps() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_surface_source_layout(
        &Value::list(vec![
            Value::symbol("surface"),
            Value::symbol(":shader"),
            Value::string(
                "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
            ),
            Value::symbol(":fps"),
            Value::fixnum(0),
        ]),
        640.0,
        480.0,
    )
    .expect("declarative surface layout");
    // A non-positive :fps is treated as uncapped, not a 0 Hz freeze.
    assert_eq!(layout.request.fps, None);
}

#[test]
fn parse_surface_source_layout_requires_shader() {
    let _eval = neovm_core::emacs_core::Context::new();
    assert!(
        parse_display_surface_source_layout(
            &Value::list(vec![
                Value::symbol("surface"),
                Value::symbol(":id"),
                Value::fixnum(3),
            ]),
            640.0,
            480.0,
        )
        .is_none()
    );
}

#[test]
fn parse_surface_layout_rejects_other_heads() {
    let _eval = neovm_core::emacs_core::Context::new();
    assert!(
        parse_display_surface_layout(&Value::list(vec![
            Value::symbol("video"),
            Value::symbol(":id"),
            Value::fixnum(1),
        ]))
        .is_none()
    );
}
