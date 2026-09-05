//! CPU-only tests for shader-surface WGSL/GLSL composition and naga
//! validation (no GPU device needed).

use neomacs_renderer_wgpu::shader_surface::{
    ShaderValidationError, SurfaceContract, SurfaceShaderLanguage, compose_surface_glsl,
    compose_surface_wgsl, uniform_accessor_name, validate_surface_glsl, validate_surface_wgsl,
};

const PLASMA: &str = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
    let uv = fragCoord / u.iResolution.xy;
    return vec4<f32>(0.5 + 0.5 * cos(u.iTime + uv.xyx + vec3<f32>(0.0, 2.0, 4.0)), 1.0);
}";

#[test]
fn valid_shader_composes_and_validates() {
    let uniforms = vec![("speed".to_owned(), 1u8), ("tint".to_owned(), 3u8)];
    let composed =
        validate_surface_wgsl(PLASMA, &uniforms, SurfaceContract::V1).expect("valid shader");
    assert!(composed.contains("struct NeoUniforms"));
    assert!(composed.contains("fn u_speed() -> f32 { return u.custom[0].x; }"));
    assert!(composed.contains("fn u_tint() -> vec3<f32> { return u.custom[1].xyz; }"));
    assert!(composed.contains("neo_fs_main"));
    assert!(composed.ends_with("}\n"));
}

#[test]
fn shader_using_uniform_accessors_validates() {
    let source = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
        return vec4<f32>(u_tint() * u_speed(), 1.0);
    }";
    let uniforms = vec![("speed".to_owned(), 1u8), ("tint".to_owned(), 3u8)];
    validate_surface_wgsl(source, &uniforms, SurfaceContract::V1).expect("accessors resolve");
}

#[test]
fn syntax_error_reports_span() {
    let err =
        validate_surface_wgsl("fn mainImage(", &[], SurfaceContract::V1).expect_err("parse error");
    assert!(
        err.to_string().contains("error"),
        "diagnostic missing: {err}"
    );
}

#[test]
fn missing_main_image_is_rejected() {
    let err = validate_surface_wgsl(
        "fn not_main() -> f32 { return 0.0; }",
        &[],
        SurfaceContract::V1,
    )
    .expect_err("no entry");
    assert!(
        err.to_string().contains("mainImage"),
        "should mention mainImage: {err}"
    );
}

#[test]
fn wrong_main_image_signature_is_rejected() {
    let source = "fn mainImage(fragCoord: vec2<f32>) -> f32 { return 0.0; }";
    validate_surface_wgsl(source, &[], SurfaceContract::V1).expect_err("wrong return type");
}

#[test]
fn a_ninth_uniform_is_refused_as_an_arity_error_rather_than_a_naga_diagnostic() {
    // The two refusals mean different things: the arity one is about the
    // request, and a caller that wants to say so (or drop the ninth uniform)
    // cannot tell them apart if both arrive as prose.
    let uniforms: Vec<(String, u8)> = (0..9).map(|i| (format!("u{i}"), 1u8)).collect();
    let err =
        validate_surface_wgsl(PLASMA, &uniforms, SurfaceContract::V1).expect_err("9 uniforms");
    assert!(matches!(
        err,
        ShaderValidationError::TooManyUniforms { given: 9 }
    ));
    let rejected =
        validate_surface_wgsl("fn mainImage(", &[], SurfaceContract::V1).expect_err("parse error");
    assert!(matches!(rejected, ShaderValidationError::Rejected(_)));
}

#[test]
fn accessor_names_are_sanitized() {
    assert_eq!(uniform_accessor_name("speed"), "u_speed");
    assert_eq!(uniform_accessor_name("my-color"), "u_my_color");
    assert_eq!(uniform_accessor_name("weird name!"), "u_weird_name_");
}

#[test]
fn lisp_style_uniform_names_compose_into_valid_wgsl() {
    let source = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
        return vec4<f32>(vec3<f32>(u_glow_strength()), 1.0);
    }";
    let uniforms = vec![("glow-strength".to_owned(), 1u8)];
    validate_surface_wgsl(source, &uniforms, SurfaceContract::V1)
        .expect("kebab-case name sanitized");
}

#[test]
fn channel0_sampling_validates() {
    let source = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
        let uv = fragCoord / u.iResolution.xy;
        return textureSample(iChannel0, iChannel0Sampler, uv);
    }";
    let composed = validate_surface_wgsl(source, &[], SurfaceContract::V1)
        .expect("channel sampling validates");
    assert!(composed.contains("var iChannel0: texture_2d<f32>"));
    assert!(composed.contains("var iChannel0Sampler: sampler"));
}

#[test]
fn compose_is_deterministic() {
    let uniforms = vec![("a".to_owned(), 2u8)];
    assert_eq!(
        compose_surface_wgsl(PLASMA, &uniforms, SurfaceContract::V1),
        compose_surface_wgsl(PLASMA, &uniforms, SurfaceContract::V1)
    );
}

// ---- GLSL (Shadertoy dialect) ----

/// Shadertoy's default new-shader template, pasted verbatim (int literals in
/// vector constructors and all).
const GLSL_PLASMA: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec3 col = 0.5 + 0.5 * cos(iTime + uv.xyx + vec3(0, 2, 4));
    fragColor = vec4(col, 1.0);
}";

#[test]
fn glsl_shadertoy_procedural_validates() {
    let composed = validate_surface_glsl(GLSL_PLASMA, &[], SurfaceContract::V1)
        .expect("shadertoy template validates");
    assert!(composed.starts_with("#version 450\n"));
    assert!(composed.contains("layout(set = 0, binding = 0, std140) uniform NeoUniforms"));
    // Shadertoy y-up fragCoord in the generated footer.
    assert!(composed.contains("iResolutionV.y - gl_FragCoord.y"));
}

#[test]
fn glsl_channel0_texture_sampling_validates() {
    // `texture(iChannel0, uv)` works through the Vulkan-GLSL combined
    // constructor: `#define iChannel0 sampler2D(iChannel0Tex, iChannel0Sampler)`
    // over separate texture2D/sampler bindings — naga's glsl-in registers the
    // image/sampler pairing (MacroCall::Sampler) and texture() accepts it.
    let source = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec2 uv = fragCoord / iResolution.xy;
        fragColor = texture(iChannel0, uv);
    }";
    let composed = validate_surface_glsl(source, &[], SurfaceContract::V1)
        .expect("channel sampling validates");
    assert!(composed.contains("layout(set = 0, binding = 1) uniform texture2D iChannel0Tex;"));
    assert!(composed.contains("layout(set = 0, binding = 2) uniform sampler iChannel0Sampler;"));
    assert!(composed.contains("#define iChannel0 sampler2D(iChannel0Tex, iChannel0Sampler)"));
}

#[test]
fn glsl_channel0_lod_and_size_validate() {
    // The Shadertoy corpus also leans on textureLod/textureSize; both accept
    // the combined-constructor define.
    let source = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec2 uv = fragCoord / iResolution.xy;
        vec2 sz = vec2(textureSize(iChannel0, 0));
        fragColor = textureLod(iChannel0, uv, 0.0) + vec4(sz * 0.0, 0.0, 0.0);
    }";
    validate_surface_glsl(source, &[], SurfaceContract::V1)
        .expect("textureLod/textureSize validate");
}

#[test]
fn glsl_uniform_accessors_validate() {
    let source = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        fragColor = vec4(u_tint() * u_speed() * u_glow_strength(), 1.0);
    }";
    let uniforms = vec![
        ("speed".to_owned(), 1u8),
        ("tint".to_owned(), 3u8),
        ("glow-strength".to_owned(), 1u8),
    ];
    let composed =
        validate_surface_glsl(source, &uniforms, SurfaceContract::V1).expect("accessors resolve");
    assert!(composed.contains("float u_speed() { return neo_custom[0].x; }"));
    assert!(composed.contains("vec3 u_tint() { return neo_custom[1].xyz; }"));
    assert!(composed.contains("float u_glow_strength() { return neo_custom[2].x; }"));
}

#[test]
fn glsl_mouse_frame_timedelta_validate() {
    let source = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        float f = float(iFrame) * iTimeDelta;
        vec2 m = iMouse.xy / iResolution.xy;
        fragColor = vec4(m, fract(f), 1.0);
    }";
    validate_surface_glsl(source, &[], SurfaceContract::V1)
        .expect("iMouse/iFrame/iTimeDelta resolve");
}

#[test]
fn glsl_malformed_returns_diagnostic() {
    let err = validate_surface_glsl("void mainImage(", &[], SurfaceContract::V1)
        .expect_err("parse error");
    assert!(
        err.to_string().contains("error"),
        "diagnostic missing: {err}"
    );
}

#[test]
fn glsl_missing_main_image_is_rejected() {
    // Guards the no-prototype design: with a `void mainImage(...);` prototype
    // in the prelude, naga validates a call to the never-defined function
    // (silently blank surface); without it this is a parse error at the
    // footer call naming mainImage.
    let err = validate_surface_glsl("float helper() { return 1.0; }", &[], SurfaceContract::V1)
        .expect_err("no entry");
    assert!(
        err.to_string().contains("mainImage"),
        "should mention mainImage: {err}"
    );
}

#[test]
fn the_glsl_dialect_refuses_a_ninth_uniform_with_the_same_arity_error_as_wgsl() {
    // The slot count is a property of the shared uniform block, not of a
    // dialect; if the two ever disagree, the same spec is accepted or refused
    // depending only on which language the user wrote it in.
    let uniforms: Vec<(String, u8)> = (0..9).map(|i| (format!("u{i}"), 1u8)).collect();
    let err =
        validate_surface_glsl(GLSL_PLASMA, &uniforms, SurfaceContract::V1).expect_err("9 uniforms");
    assert!(matches!(
        err,
        ShaderValidationError::TooManyUniforms { given: 9 }
    ));
}

#[test]
fn glsl_compose_is_deterministic() {
    let uniforms = vec![("a".to_owned(), 2u8)];
    assert_eq!(
        compose_surface_glsl(GLSL_PLASMA, &uniforms, SurfaceContract::V1),
        compose_surface_glsl(GLSL_PLASMA, &uniforms, SurfaceContract::V1)
    );
}

#[test]
fn surface_shader_language_distinguishes_dialects() {
    assert_ne!(SurfaceShaderLanguage::Wgsl, SurfaceShaderLanguage::Glsl);
    let copy = SurfaceShaderLanguage::Glsl;
    assert_eq!(copy, SurfaceShaderLanguage::Glsl);
}
