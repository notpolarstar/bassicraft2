// weighted blended order-independent transparency
// McGuire & Bavoil 2013

struct CameraUniform {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) packed: u32,
}

struct VertexOutput {
    @builtin(position)              clip_position: vec4<f32>,
    @location(0)                    tex_coords:    vec2<f32>,
}

const UV_MAX: f32 = 1023.0;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let u = f32(model.packed & 0x3FFu) / UV_MAX;
    let v = f32((model.packed >> 10u) & 0x3FFu) / UV_MAX;
    out.tex_coords    = vec2<f32>(u, v);
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

fn wboit_weight(z: f32, alpha: f32) -> f32 {
    let a = min(1.0, alpha) * 8.0 + 0.01;
    let b = 1.0 - z * 0.9;
    return clamp(a * a * a * 1e8 * b * b * b, 1e-2, 3e3);
}

struct FragOutput {
    @location(0) accum:  vec4<f32>,
    @location(1) reveal: vec4<f32>,
}

@fragment
fn fs_main(in: VertexOutput) -> FragOutput {
    var color = textureSample(t_diffuse, s_diffuse, in.tex_coords);

    let alpha = 0.65;

    let w = wboit_weight(in.clip_position.z, alpha);

    var out: FragOutput;
    out.accum  = vec4<f32>(color.rgb * alpha * w, alpha * w);
    out.reveal = vec4<f32>(0.0, 0.0, 0.0, alpha);

    return out;
}
