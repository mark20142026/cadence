// Gradient triggers (CCLayerGradient): flat triangles with a color per corner. Same instance
// layout as sprite.wgsl, read differently: m0/m1.xy place the triangle (p0, p0 + (a, b),
// p0 + (c, d)), uv0 / uv1 / color are the three corners' colors.

struct Camera {
    origin: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vertex: u32,
    @location(0) m0: vec4<f32>,
    @location(1) m1: vec4<f32>,
    @location(2) c0: vec4<f32>,
    @location(3) c1: vec4<f32>,
    @location(4) c2: vec4<f32>,
) -> Out {
    // vertices 3..5 collapse onto the third corner (the sprite pipeline draws 6 per instance)
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(0.0, 1.0), vec2(0.0, 1.0),
    );
    var colors = array<vec4<f32>, 3>(c0, c1, c2);
    let c = corners[vertex];
    let world = vec2(m0.x * c.x + m0.z * c.y + m1.x, m0.y * c.x + m0.w * c.y + m1.y);
    var out: Out;
    out.position = vec4((world - camera.origin) * camera.scale - vec2(1.0, 1.0), 0.0, 1.0);
    out.color = colors[min(vertex, 2u)];
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return in.color;
}
