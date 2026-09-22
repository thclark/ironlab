// IronLAB's own pipelines: triangles in figure points and stroked polylines in item space, each with a depth, drawn
// into egui's frame or into the offscreen renderer's texture with the mapping, blending and sampling of egui's own
// meshes, plus a depth test.

struct Mapping {
    // The size of the render target in points (pixels over pixels per point), as egui's own uniform holds it, in
    // the first two components.
    size_in_points: vec4<f32>,
    // The screen position of the figure's top-left corner in the first two components and the screen points per
    // figure point in the third.
    origin_scale: vec4<f32>,
};

@group(0) @binding(0) var<uniform> mapping: Mapping;

// Figure points to clip space, as egui maps its own vertices.
fn clip_of(figure: vec2<f32>, z: f32) -> vec4<f32> {
    let screen = mapping.origin_scale.xy + mapping.origin_scale.z * figure;
    return vec4<f32>(
        2.0 * screen.x / mapping.size_in_points.x - 1.0,
        1.0 - 2.0 * screen.y / mapping.size_in_points.y,
        z,
        1.0,
    );
}

// ---- Triangles ----

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
    out.clip_position = clip_of(in.position, in.z);
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

// ---- Strokes ----

struct StrokeParams {
    // The item-to-figure transform: [a, b, c, d] and [e, f, 0, 0].
    linear: vec4<f32>,
    offset: vec4<f32>,
    // Premultiplied sRGB colour.
    color: vec4<f32>,
    // The width in item units, or 0 for one screen point.
    width: f32,
    // 0 butt, 1 round, 2 square.
    cap: u32,
    // 0 miter, 1 round, 2 bevel.
    join: u32,
    // The number of dash entries in use, 0 for a solid stroke.
    dash_count: u32,
    dash_offset: f32,
    period: f32,
    // The depth of the stroke outside a depth group; inside one (vertex_z set) the segments carry it.
    z: f32,
    vertex_z: u32,
    dashes: array<vec4<f32>, 4>,
};

@group(1) @binding(0) var<uniform> stroke: StrokeParams;

struct SegmentInput {
    @location(0) prev: vec2<f32>,
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
    @location(3) next: vec2<f32>,
    @location(4) z: vec2<f32>,
    @location(5) arc: vec2<f32>,
    @location(6) grad: vec2<f32>,
    @location(7) flags: u32,
    @location(8) prev_arc: f32,
};

struct StrokeOutput {
    @builtin(position) clip_position: vec4<f32>,
    // The arc length along the subpath and the signed distance across the stroke, in item units, at the fragment.
    @location(0) st: vec2<f32>,
    // For the features at a joint: the arc length of the joint on the segment after it (x) and on the segment
    // before it (z, which differs at the seam of a closed subpath), and how the dashes decide the fragment (y):
    // 0 by its own place on the path (bodies and caps); 3 the join proper, drawn where the joint lies in a dash on
    // both sides; 1 the fan around a miter or bevel joint of a dashed stroke with caps, covered by the caps of the
    // dashes ending or starting beside the joint; 2 the fan of a round join, covered as 3 within half the width of
    // the joint, or by those caps.
    @location(1) @interpolate(flat) joint: vec4<f32>,
    // For a fan: the fragment's arc length and distance across in the frame of the segment before the joint, in
    // which the end caps of that segment's dashes lie. Equal to `st` elsewhere.
    @location(2) st_prev: vec2<f32>,
    // The arc lengths at which the subpath starts and ends, where the segment is its first or last, else far
    // away: no dash lies beyond them.
    @location(3) @interpolate(flat) bounds: vec2<f32>,
};

const JOIN_AT_START: u32 = 1u;
const JOIN_AT_END: u32 = 2u;
// Triangles per end feature (a join or a cap); see STROKE_VERTICES in gpu.rs.
const FEATURE_TRIANGLES: u32 = 8u;
const PI: f32 = 3.14159265358979;
// A miter join is drawn when the miter length is at most four times the width (the PDF limit), which holds when
// the cosine of the turn angle is at least 2 / 16 - 1.
const MITER_COS_LIMIT: f32 = -0.875;

fn item_to_figure(p: vec2<f32>) -> vec2<f32> {
    let m = stroke.linear;
    return vec2<f32>(m.x * p.x + m.z * p.y + stroke.offset.x, m.y * p.x + m.w * p.y + stroke.offset.y);
}

// The stroke's half width in item units: half the width, or half a screen point for a hairline.
fn half_width() -> f32 {
    if stroke.width > 0.0 {
        return 0.5 * stroke.width;
    }
    let m = stroke.linear;
    let stretch = sqrt(abs(m.x * m.w - m.y * m.z));
    return 0.5 / (mapping.origin_scale.z * max(stretch, 1e-12));
}

// Rotates a unit vector by an angle.
fn rotate(v: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

// The signed angle from unit vector a to unit vector b.
fn angle_between(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return atan2(a.x * b.y - a.y * b.x, dot(a, b));
}

@vertex
fn vs_stroke(in: SegmentInput, @builtin(vertex_index) index: u32) -> StrokeOutput {
    var out: StrokeOutput;
    let h = half_width();
    let delta = in.p1 - in.p0;
    let len = length(delta);
    // A degenerate segment emits nothing visible.
    var item = in.p0;
    var st = vec2<f32>(in.arc.x, 0.0);
    var joint = vec4<f32>(in.arc.x, 0.0, in.prev_arc, 0.0);
    var st_prev = vec2<f32>(0.0, 0.0);
    var fan_vertex = false;
    let start_join_flag = (in.flags & JOIN_AT_START) != 0u;
    let end_join_flag = (in.flags & JOIN_AT_END) != 0u;
    let bounds = vec2<f32>(
        select(in.arc.x, -1.0e30, start_join_flag),
        select(in.arc.y, 1.0e30, end_join_flag),
    );
    // The polyline vertex the emitted vertex belongs to, and the depth there; the depth of the emitted vertex
    // follows the plane's gradient away from it.
    var anchor = in.p0;
    var z = in.z.x;
    if len > 0.0 {
        let d = delta / len;
        let n = vec2<f32>(-d.y, d.x);
        // The item-space offset of the body's left edge from the centreline: n * h, or for a hairline the vector
        // that is half a screen point across the line on screen, whatever the transform stretches.
        var edge = n * h;
        if stroke.width <= 0.0 {
            let m = stroke.linear;
            let d_fig = vec2<f32>(m.x * d.x + m.z * d.y, m.y * d.x + m.w * d.y);
            let n_fig = normalize(vec2<f32>(-d_fig.y, d_fig.x)) * (0.5 / mapping.origin_scale.z);
            let det = m.x * m.w - m.y * m.z;
            edge = vec2<f32>(m.w * n_fig.x - m.z * n_fig.y, -m.y * n_fig.x + m.x * n_fig.y) / det;
        }
        let start_join = (in.flags & JOIN_AT_START) != 0u;
        let end_join = (in.flags & JOIN_AT_END) != 0u;
        if index < 6u {
            // The body: a quad of half width h, extended by h at a free end for square caps.
            let at_end = index == 2u || index == 3u || index == 5u;
            let left = index == 0u || index == 2u || index == 5u;
            var along = 0.0;
            if !at_end && !start_join && stroke.cap == 2u {
                along = -h;
            }
            if at_end && !end_join && stroke.cap == 2u {
                along = h;
            }
            let base = select(in.p0, in.p1, at_end);
            let side = select(-h, h, left);
            item = base + d * along + edge * select(-1.0, 1.0, left);
            st = vec2<f32>(select(in.arc.x, in.arc.y, at_end) + along, side);
            anchor = base;
            z = select(in.z.x, in.z.y, at_end);
        } else {
            let feature = (index - 6u) / (3u * FEATURE_TRIANGLES);
            let local = (index - 6u) % (3u * FEATURE_TRIANGLES);
            let triangle = local / 3u;
            let corner = local % 3u;
            let at_end = feature == 1u;
            let base = select(in.p0, in.p1, at_end);
            let arc = select(in.arc.x, in.arc.y, at_end);
            anchor = base;
            z = select(in.z.x, in.z.y, at_end);
            item = base;
            st = vec2<f32>(arc, 0.0);
            joint = vec4<f32>(arc, 0.0, in.prev_arc, 0.0);
            let has_join = select(start_join, end_join, at_end);
            if !at_end && has_join {
                // The join at p0, on the outer side of the turn from prev to p0.
                let before = in.p0 - in.prev;
                let before_len = length(before);
                if before_len > 0.0 {
                    let d_prev = before / before_len;
                    let n_prev = vec2<f32>(-d_prev.y, d_prev.x);
                    let cross = d_prev.x * d.y - d_prev.y * d.x;
                    let outer = select(1.0, -1.0, cross > 0.0);
                    let o_prev = n_prev * outer;
                    let o_next = n * outer;
                    let cos_turn = dot(d_prev, d);
                    if abs(cross) > 1e-6 {
                        // The join proper: a miter within the limit (two triangles), else a bevel (one), or
                        // nothing for a round join, which is all fan.
                        var kind = stroke.join;
                        if kind == 0u && cos_turn < MITER_COS_LIMIT {
                            kind = 2u;
                        }
                        var used = 0u;
                        if kind == 0u {
                            used = 2u;
                        } else if kind == 2u {
                            used = 1u;
                        }
                        // A dashed stroke with round or square caps caps every dash end, so around a joint the
                        // caps of the dashes ending or starting near it cover what a round join would, and more:
                        // the remaining triangles draw a fan over that region, wide enough for a square cap's
                        // corners, and the dashes decide its coverage by the fragment's own place along the path.
                        // The mode is flat-interpolated from a triangle's first vertex, so every corner carries it.
                        let dashed_caps = stroke.cap != 0u && stroke.dash_count > 0u;
                        let fan_triangle = triangle >= used && (kind == 1u || dashed_caps);
                        if fan_triangle {
                            joint = vec4<f32>(arc, select(1.0, 2.0, kind == 1u), in.prev_arc, 0.0);
                            fan_vertex = true;
                            st_prev = vec2<f32>(in.prev_arc, 0.0);
                        } else if triangle < used {
                            joint = vec4<f32>(arc, 3.0, in.prev_arc, 0.0);
                        }
                        if corner != 0u {
                            if triangle < used {
                                if kind == 2u {
                                    item = base + select(o_prev, o_next, corner == 2u) * h;
                                } else {
                                    let bisector = o_prev + o_next;
                                    let tip = base + bisector * (h / (1.0 + dot(o_prev, o_next)));
                                    if triangle == 0u {
                                        item = select(tip, base + o_prev * h, corner == 1u);
                                    } else {
                                        item = select(base + o_next * h, tip, corner == 1u);
                                    }
                                }
                                st = vec2<f32>(arc, outer * h);
                            } else if fan_triangle {
                                let fan = FEATURE_TRIANGLES - used;
                                let sweep = angle_between(o_prev, o_next);
                                let k = f32(triangle - used + corner - 1u);
                                let v = rotate(o_prev, sweep * k / f32(fan));
                                // The square cap of a dash reaches the corner of its square, h * sqrt(2) from
                                // the joint; a solid stroke's round join is the disc of radius h.
                                let reach = select(h, h * 1.41421356, stroke.cap == 2u && stroke.dash_count > 0u);
                                item = base + v * reach;
                                st = vec2<f32>(arc + dot(v, d) * reach, dot(v, n) * reach);
                                st_prev = vec2<f32>(in.prev_arc + dot(v, d_prev) * reach, dot(v, n_prev) * reach);
                            }
                        }
                    }
                }
            } else if !has_join && stroke.cap == 1u && corner != 0u {
                // A round cap: a semicircle from the left offset through the free end to the right offset.
                let away = select(-d, d, at_end);
                let k = f32(triangle + corner - 1u);
                let angle = PI * k / f32(FEATURE_TRIANGLES);
                // From n (left) sweeping through `away` to -n (right).
                let v = n * cos(angle) + away * sin(angle);
                item = base + v * h;
                st = vec2<f32>(arc + dot(v, d) * h, dot(v, n) * h);
            }
        }
    }
    if stroke.vertex_z == 0u {
        z = stroke.z;
    } else {
        z = z + dot(in.grad, item - anchor);
    }
    out.clip_position = clip_of(item_to_figure(item), z);
    out.st = st;
    out.joint = joint;
    out.st_prev = select(st, st_prev, fan_vertex);
    out.bounds = bounds;
    return out;
}

fn dash_entry(i: u32) -> f32 {
    let v = stroke.dashes[i / 4u];
    switch i % 4u {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

// The position of an arc length within the dash pattern, in [0, period).
fn in_pattern(s: f32) -> f32 {
    let shifted = s + stroke.dash_offset;
    return shifted - floor(shifted / stroke.period) * stroke.period;
}

// The coverage of an arc length by the "on" dashes, each extended by `extend` at both ends, falling off over about
// a pixel (`px`, the change of arc length per pixel) at a dash end.
fn along_coverage(s: f32, extend: f32, px: f32, bounds: vec2<f32>) -> f32 {
    let u = in_pattern(s);
    // The subpath's extent in the same window of the pattern as `u`.
    let lo = u + (bounds.x - s);
    let hi = u + (bounds.y - s);
    var best = 0.0;
    var start = 0.0;
    for (var i = 0u; i < stroke.dash_count; i = i + 1u) {
        let entry = dash_entry(i);
        let end = start + entry;
        // A dash of no length paints nothing with butt or square caps (PDF 32000, 8.5.3.2); round caps make a dot.
        if i % 2u == 0u && entry > 0.0 {
            for (var wrap = -1; wrap <= 1; wrap = wrap + 1) {
                let a = max(start + f32(wrap) * stroke.period, lo);
                let b = min(end + f32(wrap) * stroke.period, hi);
                if a <= b {
                    best = max(best, clamp(min(u - (a - extend), (b + extend) - u) / px + 0.5, 0.0, 1.0));
                }
            }
        }
        start = end;
    }
    return best;
}

// Whether an arc length lies within an "on" dash, taking each dash as half-open: a joint at a dash's start is
// joined whole and one at its end is not, as PDF has it. Dashes of no length hold nothing.
fn dash_contains(s: f32, bounds: vec2<f32>) -> bool {
    let u = in_pattern(s);
    let lo = u + (bounds.x - s);
    let hi = u + (bounds.y - s);
    var start = 0.0;
    for (var i = 0u; i < stroke.dash_count; i = i + 1u) {
        let entry = dash_entry(i);
        let end = start + entry;
        if i % 2u == 0u && entry > 0.0 {
            for (var wrap = -1; wrap <= 1; wrap = wrap + 1) {
                let a = max(start + f32(wrap) * stroke.period, lo);
                let b = min(end + f32(wrap) * stroke.period, hi);
                if u >= a && u < b {
                    return true;
                }
            }
        }
        start = end;
    }
    return false;
}

// The coverage of a fragment by the cap of a dash end at distance `along` beyond the dash (positive away from it)
// and `across` from the centreline: a half disc for a round cap, a half square for a square cap, each of half
// width h, with the edges falling off over a pixel.
fn cap_coverage(along: f32, across: f32, px: f32) -> f32 {
    let h = half_width();
    if along < 0.0 {
        return 0.0;
    }
    if stroke.cap == 1u {
        let e = sqrt(along * along + across * across);
        return clamp((h - e) / px + 0.5, 0.0, 1.0);
    }
    return min(
        clamp((h - along) / px + 0.5, 0.0, 1.0),
        clamp((h - abs(across)) / px + 0.5, 0.0, 1.0),
    );
}

// The coverage of a fragment near a joint by the start caps of the dashes that begin on the segment after the
// joint, in that segment's frame. The fragment's place along the path is taken relative to the joint, so that a
// dash and the joint are compared in one window of the pattern.
fn start_cap_coverage(st: vec2<f32>, px: f32, joint_s: f32, bounds: vec2<f32>) -> f32 {
    let uj = in_pattern(joint_s);
    let u = uj + (st.x - joint_s);
    let hi = u + (bounds.y - st.x);
    var best = 0.0;
    var start = 0.0;
    for (var i = 0u; i < stroke.dash_count; i = i + 1u) {
        let entry = dash_entry(i);
        if i % 2u == 0u && (entry > 0.0 || stroke.cap == 1u) {
            for (var wrap = -1; wrap <= 1; wrap = wrap + 1) {
                let a = start + f32(wrap) * stroke.period;
                if a >= uj - 1e-5 && a <= hi {
                    best = max(best, cap_coverage(a - u, st.y, px));
                }
            }
        }
        start = start + entry;
    }
    return best;
}

// The coverage of a fragment near a joint by the end caps of the dashes that end on the segment before the joint,
// in that segment's frame.
fn end_cap_coverage(st: vec2<f32>, px: f32, joint_s: f32, bounds: vec2<f32>) -> f32 {
    let uj = in_pattern(joint_s);
    let u = uj + (st.x - joint_s);
    let lo = u + (bounds.x - st.x);
    var best = 0.0;
    var start = 0.0;
    for (var i = 0u; i < stroke.dash_count; i = i + 1u) {
        let entry = dash_entry(i);
        let end = start + entry;
        if i % 2u == 0u && (entry > 0.0 || stroke.cap == 1u) {
            for (var wrap = -1; wrap <= 1; wrap = wrap + 1) {
                let b = end + f32(wrap) * stroke.period;
                if b <= uj + 1e-5 && b >= lo {
                    best = max(best, cap_coverage(u - b, st.y, px));
                }
            }
        }
        start = end;
    }
    return best;
}

// The coverage of a fragment at (s, t) by the dashes: 1 inside an "on" dash, 0 in a gap, falling off over about a
// pixel at a dash end; square caps extend each dash by half the width and round caps round its ends.
fn dash_coverage(st: vec2<f32>, joint: vec4<f32>, st_prev: vec2<f32>, bounds: vec2<f32>) -> f32 {
    if stroke.dash_count == 0u || stroke.period <= 0.0 {
        return 1.0;
    }
    let h = half_width();
    // The change of s per pixel, for anti-aliasing the dash ends.
    let px = max(fwidth(st.x), 1e-6);
    let mode = joint.y;
    if mode > 2.5 {
        // The join proper: drawn where the joint lies in a dash on both of its sides, which differ only at the
        // seam of a closed subpath.
        return select(0.0, 1.0, dash_contains(joint.x, bounds) && dash_contains(joint.z, bounds));
    }
    if mode > 0.5 {
        // A fan: the caps of the dashes starting on this segment and ending on the one before, and for a round
        // join the join itself wherever the joint lies inside a dash.
        var coverage = 0.0;
        if mode > 1.5 {
            // The round join itself: the disc of half the width about the joint, where the joint lies in a dash.
            let from_joint = sqrt((st.x - joint.x) * (st.x - joint.x) + st.y * st.y);
            let disc = clamp((h - from_joint) / px + 0.5, 0.0, 1.0);
            let in_dash = select(0.0, 1.0, dash_contains(joint.x, bounds) && dash_contains(joint.z, bounds));
            coverage = min(in_dash, disc);
        }
        if stroke.cap != 0u {
            coverage = max(coverage, start_cap_coverage(st, px, joint.x, bounds));
            coverage = max(coverage, end_cap_coverage(st_prev, px, joint.z, bounds));
        }
        return coverage;
    }
    if stroke.cap == 1u {
        // Round: inside a dash the body draws whole; beyond its end the fragment lies within half the width of
        // the dash's centreline, which rounds the end.
        let u = in_pattern(st.x);
        let lo = u + (bounds.x - st.x);
        let hi = u + (bounds.y - st.x);
        var best = 0.0;
        var start = 0.0;
        for (var i = 0u; i < stroke.dash_count; i = i + 1u) {
            let entry = dash_entry(i);
            let end = start + entry;
            if i % 2u == 0u {
                for (var wrap = -1; wrap <= 1; wrap = wrap + 1) {
                    let a = max(start + f32(wrap) * stroke.period, lo);
                    let b = min(end + f32(wrap) * stroke.period, hi);
                    if a <= b {
                        let beyond = max(max(a - u, u - b), 0.0);
                        var coverage = 1.0;
                        if beyond > 0.0 {
                            let e = sqrt(beyond * beyond + st.y * st.y);
                            coverage = clamp((h - e) / px + 0.5, 0.0, 1.0);
                        }
                        best = max(best, coverage);
                    }
                }
            }
            start = end;
        }
        return best;
    }
    return along_coverage(st.x, select(0.0, h, stroke.cap == 2u), px, bounds);
}

@fragment
fn fs_stroke(in: StrokeOutput) -> @location(0) vec4<f32> {
    let coverage = dash_coverage(in.st, in.joint, in.st_prev, in.bounds);
    if coverage <= 0.0 {
        discard;
    }
    return stroke.color * coverage;
}
