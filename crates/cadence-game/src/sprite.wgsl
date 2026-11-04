struct Camera {
    origin: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var sheet: texture_2d<f32>;
@group(1) @binding(1) var sheet_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vertex: u32,
    @location(0) m0: vec4<f32>,   // a b c d
    @location(1) m1: vec4<f32>,   // tx ty triangle depth (the occlusion pass's, else 0)
    @location(2) uv0: vec4<f32>,  // bottom-left, bottom-right
    @location(3) uv1: vec4<f32>,  // top-left, top-right
    @location(4) color: vec4<f32>,
) -> Out {
    // two triangles: (0,0) (1,0) (0,1) / (0,1) (1,0) (1,1)
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 4>(uv0.xy, uv0.zw, uv1.xy, uv1.zw);
    var corner_index = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    var c = corners[vertex];
    // Triangle mode: the fourth corner collapses onto the third, leaving the triangle
    // (tx, ty), (tx, ty) + (a, b), (tx, ty) + (c, d) (filled circles are fans of these).
    if (m1.z > 0.5 && c.x > 0.5 && c.y > 0.5) {
        c = vec2(0.0, 1.0);
    }
    let world = vec2(m0.x * c.x + m0.z * c.y + m1.x, m0.y * c.x + m0.w * c.y + m1.y);
    var out: Out;
    out.position = vec4((world - camera.origin) * camera.scale - vec2(1.0, 1.0), m1.w, 1.0);
    out.uv = uvs[corner_index[vertex]];
    out.color = color;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(sheet, sheet_sampler, in.uv);
    // A negative alpha marks a grayscale sprite (`CCSpriteGrayscale`): the texture's luminance,
    // tinted, at alpha −a.
    if (in.color.a < 0.0) {
        let g = dot(t.rgb, vec3(0.299, 0.587, 0.114));
        return vec4(g * in.color.rgb, t.a * -in.color.a);
    }
    return t * in.color;
}

// The occlusion pre-pass (render.rs): only the fully opaque texels of normal sprites, which hide
// everything drawn before them; their depth marks where earlier sprites needn't be drawn.
@fragment
fn fs_opaque(in: Out) -> @location(0) vec4<f32> {
    let t = textureSample(sheet, sheet_sampler, in.uv);
    let a = select(t.a * in.color.a, t.a * -in.color.a, in.color.a < 0.0);
    // exactly 1: then the normal blend (src + dst × (1 − a)) keeps nothing of what's beneath
    // (a tint above 1 would subtract from it instead, so it doesn't hide it)
    if (a != 1.0) {
        discard;
    }
    return vec4(0.0);
}
