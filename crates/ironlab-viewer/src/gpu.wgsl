// IronLAB's own pipelines: geometry in screen points with a depth, drawn into egui's frame or into the offscreen
// renderer's texture with the mapping, blending and sampling of egui's own meshes, plus a depth test.

struct Screen {
    // The size of the render target in points (pixels over pixels per point), as egui's own uniform holds it.
    size_in_points: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(1) var texture_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) z: f32,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(
        2.0 * in.position.x / screen.size_in_points.x - 1.0,
        1.0 - 2.0 * in.position.y / screen.size_in_points.y,
        in.z,
        1.0,
    );
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // The vertex colour and the texture both hold premultiplied colour in gamma space, as egui's do, so their
    // product is the premultiplied gamma-space output that the blend state expects.
    return in.color * textureSample(texture, texture_sampler, in.uv);
}

// One triangle covering the whole target at the far plane, drawn with depth writes and no colour writes, clears
// the depth buffer before the items of a depth group are drawn.
@vertex
fn vs_clear(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 1.0, 1.0);
}

@fragment
fn fs_clear() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
