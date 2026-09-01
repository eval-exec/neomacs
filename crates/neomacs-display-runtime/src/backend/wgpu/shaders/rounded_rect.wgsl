// SDF-based rounded rectangle border shader with fancy border styles.
//
// Renders anti-aliased rounded rectangle outlines using a signed distance
// field computed per-fragment.  Supports 11 border styles selected by
// style_params.x (style_id):
//
//   0 = Solid (single color)
//   1 = Rainbow (angular hue)
//   2 = Animated Rainbow (rotating hue)
//   3 = Gradient (two-color interpolation)
//   4 = Pulsing Glow (soft glow with breathing intensity)
//   5 = Neon Double-Stroke (two concentric rings)
//   6 = Dashed (arc-length modulated)
//   7 = Comet Trail (traveling bright spot)
//   8 = Iridescent (position + time chromatic shift)
//   9 = Fire / Plasma (procedural noise)
//  10 = Heartbeat (pulsing border width, two-color blend)

// ─── Constants ───────────────────────────────────────────────────────

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

// ─── Vertex / Fragment IO ────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec2<f32>,       // quad corner (logical pixels)
    @location(1) color: vec4<f32>,          // primary border color
    @location(2) rect_min: vec2<f32>,       // box top-left
    @location(3) rect_max: vec2<f32>,       // box bottom-right
    @location(4) params: vec2<f32>,         // [border_width, corner_radius]
    @location(5) style_params: vec4<f32>,   // [style_id, speed, reserved, reserved]
    @location(6) color2: vec4<f32>,         // secondary color (for gradient/neon)
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) rect_min: vec2<f32>,
    @location(2) rect_max: vec2<f32>,
    @location(3) params: vec2<f32>,
    @location(4) frag_pos: vec2<f32>,
    @location(5) style_params: vec4<f32>,
    @location(6) color2: vec4<f32>,
}

struct Uniforms {
    screen_size: vec2<f32>,
    time: f32,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// ─── Helper Functions ────────────────────────────────────────────────

/// Signed distance to a rounded rectangle centered at the origin.
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

/// Convert HSV (all 0-1) to RGB.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let c = v * s;
    let h6 = h * 6.0;
    let x = c * (1.0 - abs(h6 % 2.0 - 1.0));
    let m = v - c;
    var rgb: vec3<f32>;
    if (h6 < 1.0) {
        rgb = vec3<f32>(c, x, 0.0);
    } else if (h6 < 2.0) {
        rgb = vec3<f32>(x, c, 0.0);
    } else if (h6 < 3.0) {
        rgb = vec3<f32>(0.0, c, x);
    } else if (h6 < 4.0) {
        rgb = vec3<f32>(0.0, x, c);
    } else if (h6 < 5.0) {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    return rgb + vec3<f32>(m);
}

/// Simple 2D hash for noise (returns 0-1).
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + vec3<f32>(dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33)));
    return fract((p3.x + p3.y) * p3.z);
}

/// 2D value noise (smooth, returns 0-1).
fn noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f); // smoothstep
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/// Fractional Brownian Motion (2 octaves for fire/plasma).
fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var pp = p;
    for (var i = 0; i < 3; i = i + 1) {
        value = value + amplitude * noise2d(pp);
        pp = pp * 2.0;
        amplitude = amplitude * 0.5;
    }
    return value;
}

/// Compute angular position around a rounded rectangle (0-1 perimeter).
/// Uses atan2 relative to rect center.
fn rect_angle(pos: vec2<f32>, center: vec2<f32>) -> f32 {
    let rel = pos - center;
    return (atan2(rel.y, rel.x) / TAU) + 0.5; // normalize to 0-1
}

// ─── Vertex Shader ───────────────────────────────────────────────────

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
    out.style_params = in.style_params;
    out.color2 = in.color2;
    return out;
}

// ─── Style Functions ─────────────────────────────────────────────────
// Each returns (rgb, alpha) for the border fragment.

/// Style 0: Solid — single color, standard behavior.
fn style_solid(
    color: vec4<f32>,
    border_alpha: f32,
) -> vec4<f32> {
    return vec4<f32>(color.rgb, color.a * border_alpha);
}

/// Style 1: Rainbow — static angular hue mapping.
fn style_rainbow(
    pos: vec2<f32>, center: vec2<f32>,
    border_alpha: f32,
) -> vec4<f32> {
    let hue = rect_angle(pos, center);
    let rgb = hsv_to_rgb(hue, 0.9, 1.0);
    return vec4<f32>(rgb, border_alpha);
}

/// Style 2: Animated Rainbow — rotating hue.
fn style_animated_rainbow(
    pos: vec2<f32>, center: vec2<f32>,
    speed: f32, border_alpha: f32,
) -> vec4<f32> {
    let hue = fract(rect_angle(pos, center) + uniforms.time * speed * 0.3);
    let rgb = hsv_to_rgb(hue, 0.9, 1.0);
    return vec4<f32>(rgb, border_alpha);
}

/// Style 3: Gradient — interpolate between color and color2 by position.
fn style_gradient(
    pos: vec2<f32>, rect_min: vec2<f32>, rect_max: vec2<f32>,
    color: vec4<f32>, color2: vec4<f32>,
    border_alpha: f32,
) -> vec4<f32> {
    // Diagonal gradient (top-left to bottom-right)
    let t_x = (pos.x - rect_min.x) / max(rect_max.x - rect_min.x, 1.0);
    let t_y = (pos.y - rect_min.y) / max(rect_max.y - rect_min.y, 1.0);
    let t = clamp((t_x + t_y) * 0.5, 0.0, 1.0);
    let rgb = mix(color.rgb, color2.rgb, t);
    return vec4<f32>(rgb, max(color.a, color2.a) * border_alpha);
}

/// Style 4: Pulsing Glow — soft glow with breathing intensity.
fn style_pulsing_glow(
    d_outer: f32, border_alpha: f32,
    color: vec4<f32>, speed: f32,
) -> vec4<f32> {
    // Core border
    let core = border_alpha;
    // Outer glow: exponential falloff beyond border edge
    let glow_dist = max(d_outer, 0.0);
    let pulse = (1.0 + sin(uniforms.time * speed * 3.0)) * 0.5;
    let glow = exp(-glow_dist * 0.3) * pulse * 0.7;
    let total = max(core, glow);
    // Brighten color for glow
    let glow_color = mix(color.rgb, vec3<f32>(1.0), glow * 0.3);
    return vec4<f32>(glow_color, color.a * total);
}

/// Style 5: Neon Double-Stroke — two concentric rings.
fn style_neon(
    pos: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>,
    border_width: f32, radius: f32, d_outer: f32,
    color: vec4<f32>, color2: vec4<f32>,
) -> vec4<f32> {
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d_outer);
    // Outer ring (full border width)
    let inner_r1 = max(radius - border_width, 0.0);
    let d_inner1 = sd_rounded_box(pos - center, half_size - vec2<f32>(border_width), inner_r1);
    let inner_alpha1 = 1.0 - smoothstep(-0.5, 0.5, d_inner1);
    let ring1 = outer_alpha - inner_alpha1;

    // Inner ring (thinner, inset by 2px gap)
    let gap = border_width * 0.6;
    let inner_bw = border_width * 0.4;
    let ring2_outer_d = sd_rounded_box(pos - center, half_size - vec2<f32>(border_width + gap), max(radius - border_width - gap, 0.0));
    let ring2_inner_d = sd_rounded_box(pos - center, half_size - vec2<f32>(border_width + gap + inner_bw), max(radius - border_width - gap - inner_bw, 0.0));
    let ring2 = (1.0 - smoothstep(-0.5, 0.5, ring2_outer_d)) - (1.0 - smoothstep(-0.5, 0.5, ring2_inner_d));

    // Soft glow around outer ring
    let glow_dist = max(d_outer, 0.0);
    let glow = exp(-glow_dist * 0.4) * 0.4;

    let total = max(max(ring1, ring2), glow);
    let is_inner = step(0.5, ring2 / max(total, 0.001));
    let rgb = mix(color.rgb, color2.rgb, is_inner);
    return vec4<f32>(rgb, max(color.a, color2.a) * total);
}

/// Style 6: Dashed — arc-length modulated alpha.
fn style_dashed(
    pos: vec2<f32>, center: vec2<f32>,
    speed: f32, border_alpha: f32, color: vec4<f32>,
) -> vec4<f32> {
    let angle = rect_angle(pos, center);
    let dash = smoothstep(0.45, 0.5, abs(fract(angle * 12.0 + uniforms.time * speed * 0.5) - 0.5));
    return vec4<f32>(color.rgb, color.a * border_alpha * dash);
}

/// Style 7: Comet Trail — traveling bright spot on perimeter.
fn style_comet(
    pos: vec2<f32>, center: vec2<f32>,
    speed: f32, border_alpha: f32, color: vec4<f32>,
) -> vec4<f32> {
    let angle = rect_angle(pos, center);
    let comet_pos = fract(uniforms.time * speed * 0.25);
    // Distance along perimeter (wrapping)
    let dist = abs(angle - comet_pos);
    let wrap_dist = min(dist, 1.0 - dist);
    // Bright head with longer fading trail
    let brightness = exp(-wrap_dist * 15.0);
    let trail_brightness = exp(-wrap_dist * 5.0) * 0.4;
    let total_brightness = max(brightness, trail_brightness);
    let rgb = mix(color.rgb, vec3<f32>(1.0), total_brightness * 0.8);
    // Minimum visibility: dim border + bright comet
    let alpha = max(border_alpha * 0.15, border_alpha * total_brightness);
    return vec4<f32>(rgb, color.a * max(alpha, border_alpha * 0.15));
}

/// Style 8: Iridescent — position + time chromatic shift.
fn style_iridescent(
    pos: vec2<f32>, rect_min: vec2<f32>, rect_max: vec2<f32>,
    speed: f32, border_alpha: f32, color: vec4<f32>,
) -> vec4<f32> {
    let rel = (pos - rect_min) / max(rect_max - rect_min, vec2<f32>(1.0));
    let hue = fract(rel.x * 0.4 + rel.y * 0.3 + uniforms.time * speed * 0.15);
    let iridescent_rgb = hsv_to_rgb(hue, 0.6, 1.0);
    let rgb = mix(color.rgb, iridescent_rgb, 0.65);
    return vec4<f32>(rgb, color.a * border_alpha);
}

/// Style 9: Fire / Plasma — procedural noise coloring.
fn style_fire(
    pos: vec2<f32>, speed: f32,
    border_alpha: f32, color: vec4<f32>,
) -> vec4<f32> {
    let scaled = pos * 0.08;
    let t = uniforms.time * speed;
    let n = fbm(scaled + vec2<f32>(t * 0.4, t * 0.2));
    // Warm fire palette: red → orange → yellow
    let hue = 0.0 + n * 0.12; // hue range: 0.0 (red) to 0.12 (orange-yellow)
    let brightness = 0.6 + n * 0.4;
    let fire_rgb = hsv_to_rgb(hue, 0.9, brightness);
    let rgb = mix(color.rgb, fire_rgb, 0.7);
    return vec4<f32>(rgb, color.a * border_alpha);
}

/// Style 10: Heartbeat — pulsing border width with two-color blend.
/// Uses :color (resting) and :color2 (beat flash).
fn style_heartbeat(
    pos: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>,
    border_width: f32, radius: f32,
    outer_alpha: f32, speed: f32,
    color: vec4<f32>, color2: vec4<f32>,
) -> vec4<f32> {
    // Double-beat pattern: two quick pulses then a pause (like a real heartbeat)
    let t = fract(uniforms.time * speed * 0.5);
    let beat1 = exp(-pow(t * 4.0 - 0.5, 2.0) * 20.0);
    let beat2 = exp(-pow(t * 4.0 - 1.2, 2.0) * 30.0);
    let pulse = 1.0 + 0.6 * (beat1 + beat2 * 0.7);

    // Color blend: resting color → beat color during pulses
    let beat_intensity = beat1 + beat2 * 0.7;
    let rgb = mix(color.rgb, color2.rgb, clamp(beat_intensity, 0.0, 1.0));

    let effective_bw = border_width * pulse;
    let inner_r = max(radius - effective_bw, 0.0);
    let d_inner = sd_rounded_box(pos - center, half_size - vec2<f32>(effective_bw), inner_r);
    let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, d_inner);
    let border_alpha = outer_alpha - inner_alpha;
    return vec4<f32>(rgb, color.a * border_alpha);
}

// ─── Fragment Shader ─────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let border_width = in.params.x;
    let radius = in.params.y;
    let style_id = u32(in.style_params.x);
    let speed = in.style_params.y;

    let pos = in.frag_pos;
    let center = (in.rect_min + in.rect_max) * 0.5;
    let half_size = (in.rect_max - in.rect_min) * 0.5;

    // Outer SDF
    let d_outer = sd_rounded_box(pos - center, half_size, radius);
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d_outer);

    // Filled mode (no inner cutout)
    if (border_width <= 0.0) {
        return vec4<f32>(in.color.rgb, in.color.a * outer_alpha);
    }

    // Standard inner SDF for border mask
    let inner_radius = max(radius - border_width, 0.0);
    let d_inner = sd_rounded_box(pos - center, half_size - vec2<f32>(border_width), inner_radius);
    let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, d_inner);
    let border_alpha = outer_alpha - inner_alpha;

    // Dispatch to style
    switch style_id {
        case 1u: {
            return style_rainbow(pos, center, border_alpha);
        }
        case 2u: {
            return style_animated_rainbow(pos, center, speed, border_alpha);
        }
        case 3u: {
            return style_gradient(pos, in.rect_min, in.rect_max, in.color, in.color2, border_alpha);
        }
        case 4u: {
            return style_pulsing_glow(d_outer, border_alpha, in.color, speed);
        }
        case 5u: {
            return style_neon(pos, center, half_size, border_width, radius, d_outer, in.color, in.color2);
        }
        case 6u: {
            return style_dashed(pos, center, speed, border_alpha, in.color);
        }
        case 7u: {
            return style_comet(pos, center, speed, border_alpha, in.color);
        }
        case 8u: {
            return style_iridescent(pos, in.rect_min, in.rect_max, speed, border_alpha, in.color);
        }
        case 9u: {
            return style_fire(pos, speed, border_alpha, in.color);
        }
        case 10u: {
            return style_heartbeat(pos, center, half_size, border_width, radius, outer_alpha, speed, in.color, in.color2);
        }
        default: {
            // Style 0: Solid
            return style_solid(in.color, border_alpha);
        }
    }
}
