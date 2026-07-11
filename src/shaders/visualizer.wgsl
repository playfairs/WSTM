struct Uniforms {
    time: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    rms: f32,
    peak: f32,
    beat: f32,
    centroid: f32,
    rolloff: f32,
    loudness: f32,
    bloom: f32,
    motion_blur: f32,
    particle_count: f32,
    frame: f32,
    width: f32,
    height: f32,
    hue_shift: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var feedback_texture: texture_2d<f32>;

@group(1) @binding(1)
var feedback_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);
    let a = hash(i + vec2<f32>(0.0, 0.0));
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    for (var i = 0; i < 5; i = i + 1) {
        value = value + amplitude * noise(p * frequency);
        frequency = frequency * 2.0;
        amplitude = amplitude * 0.55;
    }
    return value;
}

fn palette(t: f32) -> vec3<f32> {
    let a = vec3<f32>(0.26, 0.08, 0.58);
    let b = vec3<f32>(0.64, 0.92, 0.36);
    let c = vec3<f32>(0.95, 0.88, 0.65);
    let d = vec3<f32>(0.16, 0.3, 0.82);
    return a + b * cos(6.28318 * (c * t + d));
}

fn swirl(p: vec2<f32>, force: f32) -> vec2<f32> {
    let angle = length(p) * 1.7;
    let s = sin(angle);
    let c = cos(angle);
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c) * force;
}

@fragment
fn sim_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let aspect = uniforms.width / uniforms.height;
    let coord = uv * 2.0 - vec2<f32>(1.0, 1.0);
    let scaled = vec2<f32>(coord.x * aspect, coord.y);
    let time = uniforms.time * 0.9 + uniforms.frame * 0.0008;

    let energy = clamp(uniforms.bass * 4.0 + uniforms.peak * 2.4 + uniforms.beat * 1.8, 0.0, 1.0);
    let swirl_strength = 0.14 + uniforms.mid * 0.12 + uniforms.treble * 0.05;
    let noise_strength = 0.18 + uniforms.loudness * 0.22;
    let drift = vec2<f32>(sin(time * 0.7 + scaled.y * 1.5), cos(time * 1.1 - scaled.x * 1.9)) * 0.22;
    let flow = swirl(scaled * 1.4 + drift, swirl_strength);
    let warp = vec2<f32>(noise(scaled * 3.4 + time * 0.7), noise(scaled * 3.8 - time * 0.6)) * noise_strength;
    let adv = (flow + warp) * (0.038 + energy * 0.03);

    let sample_uv = clamp(uv + adv * 0.035, vec2<f32>(0.0), vec2<f32>(1.0));
    let previous = textureSample(feedback_texture, feedback_sampler, sample_uv).rgb;
    let decay = 0.92 + energy * 0.03;
    var color = previous * decay;

    let seed = hash(vec2<f32>(scaled.x + time * 0.42, scaled.y - time * 0.37));
    let flicker = smoothstep(0.12, 0.0, abs(seed - fract(time * 0.14)));
    let attract = vec2<f32>(sin(time * 0.27), cos(time * 0.19)) * 0.35;
    let center_dist = length(scaled - attract);
    let pulse = exp(-center_dist * (4.8 - energy * 2.4)) * (0.24 + energy * 0.42 + flicker * 0.35);

    let hue = uniforms.hue_shift * 0.6 + 0.35 + fbm(scaled * 1.3 + time * 0.35) * 0.4;
    let base = palette(hue + center_dist * 0.22 + uniforms.peak * 0.18);
    let ribbon = palette(hue + 0.65 + sin(scaled.x * 5.6 + time * 1.9) * 0.2);
    let ribbon_mask = smoothstep(0.12, 0.03, abs(scaled.y + sin(scaled.x * 3.2 + time * 2.5) * 0.13));
    let accent = palette(hue + 1.1 + uniforms.treble * 0.8);

    color = color + base * pulse * 0.8;
    color = color + ribbon * ribbon_mask * 0.18 * (0.9 + uniforms.treble * 0.4);
    color = color + accent * (smoothstep(0.18, 0.0, center_dist - 0.02) * 0.24);
    color = color + vec3<f32>(0.16, 0.08, 0.04) * energy * 0.9 * fbm(scaled * 2.2 + time * 0.9);

    color = clamp(color, vec3<f32>(0.0), vec3<f32>(20.0));
    return vec4<f32>(color, 1.0);
}

@fragment
fn display_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let previous = textureSample(feedback_texture, feedback_sampler, uv).rgb;
    let contrast = previous * 1.2;
    let bloom = previous * 0.45;
    let gamma = pow(contrast + bloom, vec3<f32>(0.92));
    let color = mix(gamma, palette(uniforms.hue_shift * 0.6 + 0.32), 0.06);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
