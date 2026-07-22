// Instanced quad shader.
//
// - Draw with triangle-list topology and six vertices per instance.
// - `size_pixels`, `corner_radii_pixels`, and `border_width_pixels` are in
//   physical framebuffer pixels.
// - Corner radii are ordered: top-left, top-right, bottom-right, bottom-left.
// - The fragment output is premultiplied alpha. Use `One` /
//   `OneMinusSrcAlpha` color blending.

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,

    @location(0) @interpolate(flat) color: vec4<f32>,
    @location(1) @interpolate(flat) border_color: vec4<f32>,
    @location(2) @interpolate(flat) size_pixels: vec2<f32>,
    @location(3) @interpolate(flat) corner_radii_pixels: vec4<f32>,
    @location(4) @interpolate(flat) border_width_pixels: f32,

    @location(5) local_position: vec2<f32>,
};

// TODO: Move to CPU
fn normalize_corner_radii(
    size: vec2<f32>,
    corner_radii: vec4<f32>,
) -> vec4<f32> {
    let radii = max(corner_radii, vec4<f32>(0.0));
    let epsilon = 1e-6;

    let top = size.x / max(radii.x + radii.y, epsilon);
    let bottom = size.x / max(radii.w + radii.z, epsilon);
    let left = size.y / max(radii.x + radii.w, epsilon);
    let right = size.y / max(radii.y + radii.z, epsilon);

    let scale = min(1.0, min(min(top, bottom), min(left, right)));
    return radii * scale;
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,

    @location(0) color: vec4<f32>,
    @location(1) border_color: vec4<f32>,
    @location(2) center_ndc: vec2<f32>,
    @location(3) size_ndc: vec2<f32>,
    @location(4) size_pixels: vec2<f32>,
    @location(5) corner_radii_pixels: vec4<f32>,
    @location(6) border_width_pixels: f32,
) -> VertexOutput {
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(-0.5,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
    );

    let local_position = positions[vertex_index];
    let clip_position = local_position * size_ndc + center_ndc;
    let clamped_size_pixels = max(size_pixels, vec2<f32>(0.0));

    var out: VertexOutput;
    out.clip_position = vec4<f32>(clip_position, 0.0, 1.0);
    out.color = color;
    out.border_color = border_color;
    out.size_pixels = clamped_size_pixels;
    out.corner_radii_pixels = normalize_corner_radii(
        clamped_size_pixels,
        corner_radii_pixels,
    );
    out.border_width_pixels = max(border_width_pixels, 0.0);
    out.local_position = local_position;
    return out;
}

fn corner_radius(position: vec2<f32>, radii: vec4<f32>) -> f32 {
    let top_radius = select(radii.y, radii.x, position.x < 0.0);
    let bottom_radius = select(radii.z, radii.w, position.x < 0.0);
    return select(bottom_radius, top_radius, position.y > 0.0);
}

fn rounded_rectangle_distance(
    position: vec2<f32>,
    half_size: vec2<f32>,
    radii: vec4<f32>,
) -> f32 {
    let radius = corner_radius(position, radii);
    let q = abs(position) - half_size + vec2<f32>(radius);

    return length(max(q, vec2<f32>(0.0)))
        + min(max(q.x, q.y), 0.0)
        - radius;
}

fn coverage(signed_distance: f32) -> f32 {
    let antialias_width = max(fwidth(signed_distance), 1e-4);
    return clamp(0.5 - signed_distance / antialias_width, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.size_pixels * 0.5;
    let pixel_position = in.local_position * in.size_pixels;

    let outer_distance = rounded_rectangle_distance(
        pixel_position,
        half_size,
        in.corner_radii_pixels,
    );
    let outer_coverage = coverage(outer_distance);

    let border_width = min(
        in.border_width_pixels,
        min(half_size.x, half_size.y),
    );
    let inner_half_size = max(
        half_size - vec2<f32>(border_width),
        vec2<f32>(0.0),
    );
    let inner_radii = max(
        in.corner_radii_pixels - vec4<f32>(border_width),
        vec4<f32>(0.0),
    );
    let inner_distance = rounded_rectangle_distance(
        pixel_position,
        inner_half_size,
        inner_radii,
    );

    let has_interior = border_width < min(half_size.x, half_size.y);
    let inner_coverage = coverage(inner_distance)
        * select(0.0, 1.0, has_interior);
    let border_coverage = max(outer_coverage - inner_coverage, 0.0);

    let fill_alpha = in.color.a * inner_coverage;
    let border_alpha = in.border_color.a * border_coverage;

    let fill = vec4<f32>(in.color.rgb * fill_alpha, fill_alpha);
    let border = vec4<f32>(
        in.border_color.rgb * border_alpha,
        border_alpha,
    );

    return fill + border;
}
