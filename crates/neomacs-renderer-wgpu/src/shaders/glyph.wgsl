// Glyph rendering shader - alpha-masked text

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


@group(1) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(1) @binding(1)
var glyph_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let p = scale_position(in.position);
    let x = (p.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (p.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_texture, glyph_sampler, in.tex_coords).r * uniforms.content_alpha;
    // Gamma-correct compositing: swash rasterizes glyphs as linear coverage.
    // The GPU blends in sRGB space with pre-multiplied alpha, which darkens
    // mid-coverage anti-aliased edges.  Convert foreground to approximate
    // linear space, apply coverage, and convert back so the edge transition
    // matches perceptual brightness.
    let fg_srgb = in.color.rgb;
    let fg_linear = pow(fg_srgb, vec3(2.2));
    let result_linear = fg_linear * alpha;
    let result_srgb = pow(result_linear, vec3(1.0 / 2.2));
    return vec4<f32>(result_srgb, alpha);
}
