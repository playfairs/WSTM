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
    pattern_a: i32,
    pattern_b: i32,
    transition: f32,
    blend_alpha: f32,
    pattern_seed: f32,
    flow_seed: f32,
    chaos_seed: f32,
    twist_seed: f32,
    drift_seed: f32,
    pulse_seed: f32,
    motion_speed: f32,
    warp_scale: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var feedback_texture: texture_2d<f32>;
@group(1) @binding(1) var feedback_sampler: sampler;
@group(2) @binding(0) var<storage, read> samples: array<f32>;

@vertex fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 { return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123); }
fn noise(p: vec2<f32>) -> f32 { let i = floor(p); let f = fract(p); let u = f * f * (vec2<f32>(3.0) - 2.0 * f); return mix(mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y); }
fn fbm(p: vec2<f32>) -> f32 { var value = 0.0; var amplitude = 0.5; var frequency = 1.0; for (var i = 0; i < 5; i = i + 1) { value += amplitude * noise(p * frequency); frequency *= 2.0; amplitude *= 0.55; } return value; }
fn palette(t: f32) -> vec3<f32> { let a = vec3<f32>(0.278, 0.119, 0.358); let b = vec3<f32>(0.502, 0.416, 0.769); let c = vec3<f32>(0.899, 0.509, 0.368); let d = vec3<f32>(0.179, 0.700, 0.628); return a + b * cos(6.28318 * (c * t + d)); }

@fragment fn sim_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let p = uv * 2.0 - vec2<f32>(1.0, 1.0);
    let t = uniforms.time * (0.4 + abs(uniforms.drift_seed)) + uniforms.frame * 0.0011;
    let energy = clamp(uniforms.bass * 3.0 + uniforms.rms * 2.1 + uniforms.beat * 1.5, 0.0, 1.0);
    let a = atan2(p.y, p.x);
    let r = length(p);
    let fuzz = fbm(p * (3.0 + abs(uniforms.flow_seed) * 2.0) + vec2<f32>(t, -t * 0.35));
    let halo = smoothstep(0.14, 0.0, abs(r - (0.16 + fuzz * 0.08)));
    let color = palette(uniforms.hue_shift + fuzz * 0.34 + sin(a * (5.0 + abs(uniforms.twist_seed) * 2.0) + t) * 0.15) * (0.35 + energy * 1.15) * (0.4 + halo * 1.6);
    return vec4<f32>(color, uniforms.blend_alpha);
}

@fragment fn display_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let previous = textureSample(feedback_texture, feedback_sampler, uv).rgb;
    let color = previous * 0.76 + palette(uniforms.hue_shift + uniforms.pattern_seed * 0.26 + 0.980) * 0.18;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
