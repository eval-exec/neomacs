// Background-aware glyph rendering for grayscale and LCD masks.
//
// The glyph texture stores per-channel coverage in RGB. We composite the glyph
// against the per-vertex background color directly in the shader so black text
// can preserve colored subpixel fringes instead of collapsing back to grayscale.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) fg_color: vec4<f32>,
    @location(3) bg_color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) fg_color: vec4<f32>,
    @location(2) bg_color: vec4<f32>,
}

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(1) @binding(1)
var glyph_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.fg_color = in.fg_color;
    out.bg_color = in.bg_color;
    return out;
}

// Cairo/FreeType text coverage is composited in encoded RGB. The surface
// attachment is sRGB, so its fragment input/output must remain linear RGB.
fn linear_to_srgb(rgb: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(rgb, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055,
                  12.92 * rgb, rgb <= vec3(0.0031308));
}

fn srgb_to_linear(rgb: vec3<f32>) -> vec3<f32> {
    return select(pow((rgb + 0.055) / 1.055, vec3(2.4)),
                  rgb / 12.92, rgb <= vec3(0.04045));
}

fn composite_coverage(in: VertexOutput, coverage: vec3<f32>) -> vec4<f32> {
    if max(coverage.r, max(coverage.g, coverage.b)) <= 0.0 {
        discard;
    }
    let rgb = mix(linear_to_srgb(in.bg_color.rgb),
                  linear_to_srgb(in.fg_color.rgb), coverage * in.fg_color.a);
    return vec4<f32>(srgb_to_linear(rgb), 1.0);
}

@fragment
fn fs_subpixel(in: VertexOutput) -> @location(0) vec4<f32> {
    return composite_coverage(in, textureSample(glyph_texture, glyph_sampler, in.tex_coords).rgb);
}

@fragment
fn fs_grayscale(in: VertexOutput) -> @location(0) vec4<f32> {
    return composite_coverage(in, vec3(textureSample(glyph_texture, glyph_sampler, in.tex_coords).r));
}
