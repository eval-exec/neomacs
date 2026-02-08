// SDF-based rounded rectangle border shader.
//
// Renders anti-aliased rounded rectangle outlines using a signed distance
// field computed per-fragment.  Each quad carries the logical box bounds,
// border width, and corner radius as vertex attributes so the fragment
// shader can evaluate the SDF without any extra textures or buffers.

struct VertexInput {
    @location(0) position: vec2<f32>,   // quad corner (logical pixels)
    @location(1) color: vec4<f32>,      // border color
    @location(2) rect_min: vec2<f32>,   // box top-left (logical pixels)
    @location(3) rect_max: vec2<f32>,   // box bottom-right (logical pixels)
    @location(4) params: vec2<f32>,     // [border_width, corner_radius]
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) rect_min: vec2<f32>,
    @location(2) rect_max: vec2<f32>,
    @location(3) params: vec2<f32>,
    @location(4) frag_pos: vec2<f32>,   // interpolated logical pixel position
}

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    out.rect_min = in.rect_min;
    out.rect_max = in.rect_max;
    out.params = in.params;
    out.frag_pos = in.position;
    return out;
}

// Signed distance to a rounded rectangle centered at the origin.
//   p — sample point
//   b — half-extents of the rectangle
//   r — corner radius
// Returns negative inside, zero on boundary, positive outside.
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let border_width = in.params.x;
    let radius = in.params.y;

    // Fragment position in logical pixels (interpolated from vertex shader).
    // Using frag_pos instead of clip_position.xy avoids HiDPI scale mismatch.
    let pos = in.frag_pos;

    // Center and half-size of the logical box.
    let center = (in.rect_min + in.rect_max) * 0.5;
    let half_size = (in.rect_max - in.rect_min) * 0.5;

    // Outer edge: distance from fragment to the rounded rect boundary.
    let d_outer = sd_rounded_box(pos - center, half_size, radius);

    // Inner edge: shrink the rect by border_width, reduce radius accordingly.
    let inner_radius = max(radius - border_width, 0.0);
    let d_inner = sd_rounded_box(pos - center, half_size - vec2<f32>(border_width), inner_radius);

    // Anti-aliased alpha: 1 inside outer, 0 outside; subtract inner hole.
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d_outer);

    // When border_width <= 0, render as filled rounded rect (no inner cutout).
    if (border_width <= 0.0) {
        return vec4<f32>(in.color.rgb, in.color.a * outer_alpha);
    }

    let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, d_inner);
    let border_alpha = outer_alpha - inner_alpha;

    return vec4<f32>(in.color.rgb, in.color.a * border_alpha);
}
