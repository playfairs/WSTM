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

@vertex fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> { var positions = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0), vec2<f32>(3.0,-1.0), vec2<f32>(-1.0,3.0)); return vec4<f32>(positions[vertex_index],0.0,1.0); }

fn hash(p: vec2<f32>) -> f32 { return fract(sin(dot(p, vec2<f32>(127.1,311.7))) * 43758.5453123); }
fn noise(p: vec2<f32>) -> f32 { let i = floor(p); let f = fract(p); let u = f*f*(vec2<f32>(3.0)-2.0*f); return mix(mix(hash(i+vec2<f32>(0.0,0.0)), hash(i+vec2<f32>(1.0,0.0)), u.x), mix(hash(i+vec2<f32>(0.0,1.0)), hash(i+vec2<f32>(1.0,1.0)), u.x), u.y); }
fn fbm(p: vec2<f32>) -> f32 { var value = 0.0; var amplitude = 0.5; var frequency = 1.0; for (var i = 0; i < 5; i = i + 1) { value += amplitude*noise(p*frequency); frequency*=2.0; amplitude*=0.55; } return value; }
fn palette(t: f32) -> vec3<f32> { let a = vec3<f32>(0.62,0.14,0.18); let b = vec3<f32>(0.18,0.6,0.85); let c = vec3<f32>(0.95,0.82,0.4); let d = vec3<f32>(0.22,0.15,0.7); return a + b * cos(6.28318*(c*t+d)); }

@fragment fn sim_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let p = uv * 2.0 - vec2<f32>(1.0, 1.0);
    let t = uniforms.time * (0.4 + abs(uniforms.twist_seed)) + uniforms.frame * 0.0009;
    let energy = clamp(uniforms.bass * 3.4 + uniforms.peak * 2.8 + uniforms.beat * 1.6, 0.0, 1.0);
    let a = atan2(p.y, p.x);
    let r = length(p);
    let twist = sin(a * 9.0 + t * 2.0 + r * 5.0) * 0.5 + 0.5;
    let spokes = smoothstep(0.06, 0.0, abs(r - (0.18 + twist * 0.06)));
    let color = palette(uniforms.hue_shift + twist * 0.22 + uniforms.pattern_seed) * (0.35 + energy * 1.1) * (0.45 + spokes * 1.4) + palette(0.7 + uniforms.hue_shift) * twist * 0.2;
    return vec4<f32>(color, uniforms.blend_alpha);
}

@fragment fn display_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(uniforms.width, uniforms.height);
    let previous = textureSample(feedback_texture, feedback_sampler, uv).rgb;
    let color = previous * 0.76 + palette(uniforms.hue_shift + uniforms.pattern_seed * 0.28) * 0.17;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
