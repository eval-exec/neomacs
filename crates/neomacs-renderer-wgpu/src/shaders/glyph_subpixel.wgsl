// Glyph rendering shader for LCD/subpixel masks.
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mask_sample = textureSample(glyph_texture, glyph_sampler, in.tex_coords);
    let coverage = max(mask_sample.r, max(mask_sample.g, mask_sample.b));
    if coverage <= 0.0 {
        discard;
    }

    let rgb = in.bg_color.rgb * (vec3<f32>(1.0, 1.0, 1.0) - mask_sample.rgb)
        + in.fg_color.rgb * mask_sample.rgb;
    return vec4<f32>(rgb, 1.0);
}
