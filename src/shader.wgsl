// Vertex shader

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
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       tex_coords:    vec2<f32>,
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

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0)@binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var tex_sample: vec4<f32> = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    if (tex_sample.a < 0.1) {
        discard;
    }
    return tex_sample;
}
