// Vertex shader

struct CameraUniform {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: CameraUniform;

struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_position: vec3<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    let particle_center = vec3<f32>(model_matrix[3].x, model_matrix[3].y, model_matrix[3].z);
    
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.world_position = particle_center;

    let to_camera = normalize(camera.view_pos.xyz - particle_center);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), to_camera));
    let up = cross(to_camera, right);

    let billboard_pos = particle_center 
        + right * model.position.x * 0.15
        + up * model.position.y * 0.15;
    
    out.clip_position = camera.view_proj * vec4<f32>(billboard_pos, 1.0);
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
