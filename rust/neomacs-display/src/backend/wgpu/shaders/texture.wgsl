// Texture rendering shader for images, video, webkit

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var t_texture: texture_2d<f32>;
@group(1) @binding(1)
var t_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_texture, t_sampler, in.tex_coords);
}
