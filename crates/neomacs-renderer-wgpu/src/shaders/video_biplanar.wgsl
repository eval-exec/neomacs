struct Uniforms {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
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
    out.clip_position = vec4<f32>(
        (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (in.position.y / uniforms.screen_size.y) * 2.0,
        0.0,
        1.0,
    );
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

// Full-target variant used only when a legacy shader-surface channel needs a
// single RGB texture. Ordinary inline video uses vs_main and performs this
// conversion in its final compositor draw without an intermediate texture.
@vertex
fn vs_copy(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, 3.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
    );
    let coordinates = array<vec2<f32>, 3>(
        vec2<f32>(0.0, -1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
    );
    var out: VertexOutput;
    out.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.tex_coords = coordinates[vertex_index];
    out.color = vec4<f32>(1.0);
    return out;
}

struct ColorTransform {
    yuv_row_0: vec4<f32>,
    yuv_row_1: vec4<f32>,
    yuv_row_2: vec4<f32>,
    gamut_row_0: vec4<f32>,
    gamut_row_1: vec4<f32>,
    gamut_row_2: vec4<f32>,
    chroma: vec4<f32>,
    params: vec4<u32>,
}

@group(1) @binding(0)
var t_luma: texture_2d<f32>;
@group(1) @binding(1)
var t_chroma: texture_2d<f32>;
@group(1) @binding(2)
var video_sampler: sampler;
@group(1) @binding(3)
var<uniform> transform: ColorTransform;

fn inverse_srgb(value: vec3<f32>) -> vec3<f32> {
    let low = value / 12.92;
    let high = pow((value + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, value <= vec3<f32>(0.04045));
}

fn inverse_bt709(value: vec3<f32>) -> vec3<f32> {
    let low = value / 4.5;
    let high = pow((value + vec3<f32>(0.099)) / 1.099, vec3<f32>(1.0 / 0.45));
    return select(high, low, value < vec3<f32>(0.081));
}

fn inverse_pq(value: vec3<f32>) -> vec3<f32> {
    let m1 = 2610.0 / 16384.0;
    let m2 = 2523.0 / 32.0;
    let c1 = 3424.0 / 4096.0;
    let c2 = 2413.0 / 128.0;
    let c3 = 2392.0 / 128.0;
    let powered = pow(value, vec3<f32>(1.0 / m2));
    let normalized_nits = pow(
        max(powered - vec3<f32>(c1), vec3<f32>(0.0)) /
            max(vec3<f32>(c2) - vec3<f32>(c3) * powered, vec3<f32>(0.000001)),
        vec3<f32>(1.0 / m1),
    );
    return normalized_nits * (10000.0 / 203.0);
}

fn inverse_hlg(value: vec3<f32>) -> vec3<f32> {
    let a = 0.17883277;
    let b = 0.28466892;
    let c = 0.55991073;
    let low = value * value / 3.0;
    let high = (exp((value - vec3<f32>(c)) / a) + vec3<f32>(b)) / 12.0;
    return select(high, low, value <= vec3<f32>(0.5)) * (1000.0 / 203.0);
}

fn decode_transfer(encoded: vec3<f32>, transfer: u32) -> vec3<f32> {
    let value = clamp(encoded, vec3<f32>(0.0), vec3<f32>(1.0));
    switch transfer {
        case 0u: { return inverse_srgb(value); }
        case 1u: { return inverse_bt709(value); }
        case 2u: { return inverse_pq(value); }
        default: { return inverse_hlg(value); }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(t_luma, video_sampler, in.tex_coords).r;
    let uv = textureSample(
        t_chroma,
        video_sampler,
        in.tex_coords + transform.chroma.xy,
    ).rg;
    let yuv = vec3<f32>(y, uv);
    let encoded = vec3<f32>(
        dot(transform.yuv_row_0.xyz, yuv) + transform.yuv_row_0.w,
        dot(transform.yuv_row_1.xyz, yuv) + transform.yuv_row_1.w,
        dot(transform.yuv_row_2.xyz, yuv) + transform.yuv_row_2.w,
    );
    let source_linear = decode_transfer(encoded, transform.params.x);
    var display_linear = vec3<f32>(
        dot(transform.gamut_row_0.xyz, source_linear),
        dot(transform.gamut_row_1.xyz, source_linear),
        dot(transform.gamut_row_2.xyz, source_linear),
    );
    if transform.params.x >= 2u {
        display_linear /= 1.0 + max(max(display_linear.r, display_linear.g), display_linear.b);
    }
    return vec4<f32>(max(display_linear, vec3<f32>(0.0)), 1.0) * in.color;
}
