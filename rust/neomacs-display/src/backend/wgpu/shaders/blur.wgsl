// Two-pass separable Gaussian blur shader.
//
// Pass 1 (horizontal): direction = vec2(1.0, 0.0)
// Pass 2 (vertical):   direction = vec2(0.0, 1.0)
//
// The blur uses a 9-tap Gaussian kernel with weights precomputed for sigma ~2.0.
// For larger radii, multiple passes can be applied.

struct Uniforms {
    screen_size: vec2<f32>,
}

struct BlurUniforms {
    // Texel size: vec2(1.0/width, 1.0/height) in texture coordinates
    texel_size: vec2<f32>,
    // Blur direction: (1,0) for horizontal, (0,1) for vertical
    direction: vec2<f32>,
    // Blur radius multiplier (scales the offset distances)
    radius: f32,
    _pad: vec3<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var t_source: texture_2d<f32>;
@group(1) @binding(1)
var s_source: sampler;

@group(2) @binding(0)
var<uniform> blur_uniforms: BlurUniforms;

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
    let x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let step = blur_uniforms.texel_size * blur_uniforms.direction * blur_uniforms.radius;

    // 9-tap Gaussian kernel (sigma ~2.0, normalized)
    // Offsets: 0, +-1.385, +-3.231 (linear sampling optimization)
    // Weights: center=0.2270, inner=0.3162, outer=0.0702
    var color = textureSample(t_source, s_source, in.tex_coords) * 0.2270270270;

    color += textureSample(t_source, s_source, in.tex_coords + step * 1.3846153846) * 0.3162162162;
    color += textureSample(t_source, s_source, in.tex_coords - step * 1.3846153846) * 0.3162162162;

    color += textureSample(t_source, s_source, in.tex_coords + step * 3.2307692308) * 0.0702702703;
    color += textureSample(t_source, s_source, in.tex_coords - step * 3.2307692308) * 0.0702702703;

    return color;
}
