// Image shader for rendering inline images
// Samples RGBA texture and applies alpha blending

struct Uniforms {
    screen_size: vec2<f32>,
    time: f32,
    content_alpha: f32,
    content_scale: f32,
    // Keeps content_pivot at an 8-byte-aligned offset, matching the CPU side.
    _pivot_padding: f32,
    content_pivot: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Child-frame picture transform: scale positions away from the anchor. The
// CPU side normalizes the identity to (1.0, (0,0)), which makes this a
// no-op for every settled frame.
fn scale_position(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        uniforms.content_pivot.x + (p.x - uniforms.content_pivot.x) * uniforms.content_scale,
        uniforms.content_pivot.y + (p.y - uniforms.content_pivot.y) * uniforms.content_scale,
    );
}


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
    let scaled = scale_position(in.position);
    let x = (scaled.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (scaled.y / uniforms.screen_size.y) * 2.0;
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
    return vec4<f32>(tex_color.rgb * in.color.rgb, tex_color.a * in.color.a * uniforms.content_alpha);
}

@fragment
fn fs_main_opaque(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample from texture, force alpha=1.0 (for XRGB/BGRX DMA-BUF textures
    // where the alpha channel is unused and may be 0x00)
    let tex_color = textureSample(t_image, s_image, in.tex_coords);
    return vec4<f32>(tex_color.rgb * in.color.rgb, in.color.a * uniforms.content_alpha);
}
