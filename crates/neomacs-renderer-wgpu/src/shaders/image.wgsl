// Image shader for rendering inline images
// Samples RGBA texture and applies alpha blending

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert from pixel coordinates to clip space
    let x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

@group(1) @binding(0)
var t_image: texture_2d<f32>;
@group(1) @binding(1)
var s_image: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample RGBA from image texture and multiply by vertex color (for tinting)
    let tex_color = textureSample(t_image, s_image, in.tex_coords);
    return tex_color * in.color;
}

@fragment
fn fs_main_opaque(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample from texture, force alpha=1.0 (for XRGB/BGRX DMA-BUF textures
    // where the alpha channel is unused and may be 0x00)
    let tex_color = textureSample(t_image, s_image, in.tex_coords);
    return vec4<f32>(tex_color.rgb * in.color.rgb, in.color.a);
}
