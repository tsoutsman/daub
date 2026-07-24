@group(0) @binding(0)
var atlas: texture_2d<f32>;

@group(0) @binding(1)
var atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_position: vec2<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center_ndc: vec2<f32>,
    @location(1) size_ndc: vec2<f32>,
    @location(2) texture_origin: vec2<f32>,
    @location(3) texture_size: vec2<f32>,
) -> VertexOutput {
    let positions = array<vec2<f32>, 4>(
        vec2<f32>(-0.5,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>( 0.5, -0.5),
    );

    let local_position = positions[vertex_index];
    let unit_position = vec2<f32>(
        local_position.x + 0.5,
        0.5 - local_position.y,
    );

    var out: VertexOutput;
    out.clip_position = vec4<f32>(
        local_position * size_ndc + center_ndc,
        0.0,
        1.0,
    );
    out.texture_position = texture_origin + unit_position * texture_size;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas, atlas_sampler, in.texture_position);
    return vec4<f32>(color.rgb * color.a, color.a);
}
