// Gradient rendering shader (WGSL)
//
// This shader is embedded in the main fragment shader during glyph rendering.
// For each fragment, we evaluate the gradient if the face has a gradient background,
// otherwise use the solid background color.

// Simplex noise for procedural noise gradients
fn simplex_noise_2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);

    // Smoothstep function for interpolation
    let u = f * f * (3.0 - 2.0 * f);

    // Pseudo-random function
    fn pseudo_rand(n: vec2<f32>) -> f32 {
        let x = sin(dot(n, vec2<f32>(12.9898, 78.233))) * 43758.5453;
        return fract(x);
    }

    let n00 = pseudo_rand(i);
    let n10 = pseudo_rand(i + vec2<f32>(1.0, 0.0));
    let n01 = pseudo_rand(i + vec2<f32>(0.0, 1.0));
    let n11 = pseudo_rand(i + vec2<f32>(1.0, 1.0));

    let nx0 = mix(n00, n10, u.x);
    let nx1 = mix(n01, n11, u.x);
    return mix(nx0, nx1, u.y);
}

// Linear gradient: evaluate at position in [0, 1]
fn linear_gradient(pos: vec2<f32>, angle_deg: f32, color_stops: array<vec4<f32>, 8>, num_stops: u32) -> vec4<f32> {
    // Convert angle to radians and normalize direction
    let angle_rad = angle_deg * 3.14159265 / 180.0;
    let dir = vec2<f32>(cos(angle_rad), sin(angle_rad));

    // Project position onto gradient direction
    let t = dot(pos, dir);

    // Find which color stops bracket this position
    var t_clamped = clamp(t, 0.0, 1.0);

    // Linear interpolation through color stops
    var color = color_stops[0];
    for (var i = 1u; i < num_stops; i = i + 1u) {
        // Assuming color_stops[i].w stores the position
        if (t_clamped >= color_stops[i - 1u].w) {
            let t1 = color_stops[i - 1u].w;
            let t2 = color_stops[i].w;
            if (t2 > t1) {
                let mix_factor = (t_clamped - t1) / (t2 - t1);
                color = mix(color_stops[i - 1u], color_stops[i], mix_factor);
            }
        }
    }
    return color;
}

// Radial gradient: evaluate at position in [0, 1]
fn radial_gradient(pos: vec2<f32>, center: vec2<f32>, radius: f32, color_stops: array<vec4<f32>, 8>, num_stops: u32) -> vec4<f32> {
    // Distance from center
    let dist = distance(pos, center) / radius;
    let t_clamped = clamp(dist, 0.0, 1.0);

    // Interpolate through color stops
    var color = color_stops[0];
    for (var i = 1u; i < num_stops; i = i + 1u) {
        if (t_clamped >= color_stops[i - 1u].w) {
            let t1 = color_stops[i - 1u].w;
            let t2 = color_stops[i].w;
            if (t2 > t1) {
                let mix_factor = (t_clamped - t1) / (t2 - t1);
                color = mix(color_stops[i - 1u], color_stops[i], mix_factor);
            }
        }
    }
    return color;
}

// Conic gradient: evaluate at position in [0, 1]
fn conic_gradient(pos: vec2<f32>, center: vec2<f32>, angle_offset_deg: f32, color_stops: array<vec4<f32>, 8>, num_stops: u32) -> vec4<f32> {
    // Angle from center to position
    let offset = pos - center;
    var angle = atan2(offset.y, offset.x);

    // Convert offset angle to [0, 2π]
    if (angle < 0.0) {
        angle = angle + 6.28318531;
    }

    // Apply offset and normalize to [0, 1]
    let angle_offset_rad = angle_offset_deg * 3.14159265 / 180.0;
    var t = (angle + angle_offset_rad) / 6.28318531;
    t = fract(t);  // Wrap around

    // Interpolate through color stops
    var color = color_stops[0];
    for (var i = 1u; i < num_stops; i = i + 1u) {
        if (t >= color_stops[i - 1u].w) {
            let t1 = color_stops[i - 1u].w;
            let t2 = color_stops[i].w;
            if (t2 > t1) {
                let mix_factor = (t - t1) / (t2 - t1);
                color = mix(color_stops[i - 1u], color_stops[i], mix_factor);
            }
        }
    }
    return color;
}

// Noise gradient: procedural pattern
fn noise_gradient(pos: vec2<f32>, scale: f32, octaves: u32, color1: vec4<f32>, color2: vec4<f32>) -> vec4<f32> {
    var noise = 0.0;
    var amplitude = 1.0;
    var frequency = 1.0;
    var max_value = 0.0;

    for (var i = 0u; i < octaves; i = i + 1u) {
        noise = noise + amplitude * simplex_noise_2d(pos * frequency * scale);
        max_value = max_value + amplitude;
        amplitude = amplitude * 0.5;
        frequency = frequency * 2.0;
    }

    noise = noise / max_value;
    noise = clamp(noise, 0.0, 1.0);

    return mix(color1, color2, noise);
}

// Main gradient evaluation function
// Gradient type encoding: 0=linear, 1=radial, 2=conic, 3=noise
fn evaluate_gradient(
    pos: vec2<f32>,  // Position in [0, 1] within the region
    gradient_type: u32,
    linear_angle: f32,
    radial_center: vec2<f32>,
    radial_radius: f32,
    conic_center: vec2<f32>,
    conic_angle_offset: f32,
    noise_scale: f32,
    noise_octaves: u32,
    color_stops: array<vec4<f32>, 8>,
    num_stops: u32,
    color1: vec4<f32>,
    color2: vec4<f32>,
) -> vec4<f32> {
    if (gradient_type == 0u) {
        // Linear
        return linear_gradient(pos, linear_angle, color_stops, num_stops);
    } else if (gradient_type == 1u) {
        // Radial
        return radial_gradient(pos, radial_center, radial_radius, color_stops, num_stops);
    } else if (gradient_type == 2u) {
        // Conic
        return conic_gradient(pos, conic_center, conic_angle_offset, color_stops, num_stops);
    } else {
        // Noise
        return noise_gradient(pos, noise_scale, noise_octaves, color1, color2);
    }
}
