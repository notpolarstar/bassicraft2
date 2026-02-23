struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       tex_coords:    vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );

    var out: VertexOutput;
    out.clip_position = vec4<f32>(positions[vid], 0.0, 1.0);
    out.tex_coords    = uvs[vid];
    return out;
}

@group(0) @binding(0) var s_wboit:  sampler;
@group(0) @binding(1) var t_accum:  texture_2d<f32>;
@group(0) @binding(2) var t_reveal: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let accum_sample  = textureSample(t_accum,  s_wboit, in.tex_coords);
    let reveal_sample = textureSample(t_reveal, s_wboit, in.tex_coords);

    let transmittance = reveal_sample.r;

    if transmittance > 0.9999 {
        discard;
    }

    let total_weight = max(accum_sample.a, 1e-5);
    let avg_color    = accum_sample.rgb / total_weight;

    let alpha_out = 1.0 - transmittance;
    return vec4<f32>(avg_color, alpha_out);
}
