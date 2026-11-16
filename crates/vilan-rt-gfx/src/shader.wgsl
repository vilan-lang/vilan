// vilan-rt-gfx: two pipelines, every primitive one instanced quad.
//
// Every coordinate is in DEVICE pixels, already scaled and snapped on the CPU
// (`encode.rs`). Colours arrive in sRGB with straight alpha; the shader
// converts to linear light and outputs PREMULTIPLIED alpha, which the
// pipelines blend with `PREMULTIPLIED_ALPHA_BLENDING` into an sRGB target.
//
// Kept inside WebGL2's limits on purpose (native-ui.md section 7): vertex
// attributes only, no storage buffers, no compute, at most 10 attributes.

struct Globals {
    viewport: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

fn to_clip_space(pixel: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        pixel.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
}

// The quad's corner for vertex 0..3 of a triangle strip.
fn corner_of(vertex: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex & 1u), f32((vertex >> 1u) & 1u));
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let low = color / 12.92;
    let high = pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, color <= vec3<f32>(0.04045));
}

// sRGB straight alpha -> linear premultiplied.
fn premultiplied_linear(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(srgb_to_linear(color.rgb) * color.a, color.a);
}

// Signed distance from `point` (relative to the box's centre) to a rounded box
// with a radius per corner: x top-left, y top-right, z bottom-right, w
// bottom-left. Negative inside.
fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    var radius: f32;
    if (point.x < 0.0) {
        radius = select(radii.w, radii.x, point.y < 0.0);
    } else {
        radius = select(radii.z, radii.y, point.y < 0.0);
    }
    radius = min(radius, min(half_size.x, half_size.y));
    let offset = abs(point) - half_size + vec2<f32>(radius);
    return min(max(offset.x, offset.y), 0.0) + length(max(offset, vec2<f32>(0.0))) - radius;
}

// Coverage (0..1) of the pixel centred at `pixel` by a rounded rectangle given
// as left, top, right, bottom.
fn rounded_rect_coverage(pixel: vec2<f32>, bounds: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half_size = (bounds.zw - bounds.xy) * 0.5;
    let center = bounds.xy + half_size;
    return clamp(0.5 - rounded_box_distance(pixel - center, half_size, radii), 0.0, 1.0);
}

// ─── SDF quads: rectangles and shadows ─────────────────────────────────────

struct QuadInstance {
    @location(0) bounds: vec4<f32>,
    @location(1) corner_radii: vec4<f32>,
    @location(2) border_widths: vec4<f32>,
    @location(3) fill_start: vec4<f32>,
    @location(4) fill_end: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) gradient: vec4<f32>,
    @location(7) clip_bounds: vec4<f32>,
    @location(8) clip_radii: vec4<f32>,
    @location(9) parameters: vec4<f32>,
};

struct QuadVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel: vec2<f32>,
    @location(1) @interpolate(flat) bounds: vec4<f32>,
    @location(2) @interpolate(flat) corner_radii: vec4<f32>,
    @location(3) @interpolate(flat) border_widths: vec4<f32>,
    @location(4) @interpolate(flat) fill_start: vec4<f32>,
    @location(5) @interpolate(flat) fill_end: vec4<f32>,
    @location(6) @interpolate(flat) border_color: vec4<f32>,
    @location(7) @interpolate(flat) gradient: vec4<f32>,
    @location(8) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(9) @interpolate(flat) clip_radii: vec4<f32>,
    @location(10) @interpolate(flat) parameters: vec4<f32>,
};

const QUAD_KIND_SHADOW: f32 = 1.0;
const FILL_LINEAR_GRADIENT: f32 = 1.0;

@vertex
fn quad_vertex(@builtin(vertex_index) vertex: u32, instance: QuadInstance) -> QuadVarying {
    // A shadow's quad reaches ceil(3 sigma) past its shape, the same reach the
    // CPU used for its bounds in the overlap test.
    var reach = 0.0;
    if (instance.parameters.x == QUAD_KIND_SHADOW) {
        reach = ceil(instance.parameters.z * 3.0);
    }
    let origin = instance.bounds.xy - vec2<f32>(reach);
    let size = instance.bounds.zw + vec2<f32>(reach * 2.0);
    let pixel = origin + corner_of(vertex) * size;
    var out: QuadVarying;
    out.position = to_clip_space(pixel);
    out.pixel = pixel;
    out.bounds = instance.bounds;
    out.corner_radii = instance.corner_radii;
    out.border_widths = instance.border_widths;
    out.fill_start = instance.fill_start;
    out.fill_end = instance.fill_end;
    out.border_color = instance.border_color;
    out.gradient = instance.gradient;
    out.clip_bounds = instance.clip_bounds;
    out.clip_radii = instance.clip_radii;
    out.parameters = instance.parameters;
    return out;
}

// The fill at `pixel`: solid, or a two-stop linear gradient interpolated in
// sRGB with premultiplied alpha (CSS's default). Answers sRGB straight alpha.
fn fill_color(pixel: vec2<f32>, in: QuadVarying) -> vec4<f32> {
    if (in.parameters.y != FILL_LINEAR_GRADIENT) {
        return in.fill_start;
    }
    let direction = in.gradient.zw - in.gradient.xy;
    let length_squared = dot(direction, direction);
    var position = 0.0;
    if (length_squared > 0.0) {
        position = clamp(dot(pixel - in.gradient.xy, direction) / length_squared, 0.0, 1.0);
    }
    let start = vec4<f32>(in.fill_start.rgb * in.fill_start.a, in.fill_start.a);
    let end = vec4<f32>(in.fill_end.rgb * in.fill_end.a, in.fill_end.a);
    let mixed = mix(start, end, position);
    if (mixed.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(mixed.rgb / mixed.a, mixed.a);
}

// Evan Wallace's closed-form blur of a rounded rectangle (the one GPUI uses).
fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * 3.14159265) * sigma);
}

fn error_function(x: vec2<f32>) -> vec2<f32> {
    let sign_of = sign(x);
    let magnitude = abs(x);
    var y = 1.0 + (0.278393 + (0.230389 + 0.078108 * (magnitude * magnitude)) * magnitude) * magnitude;
    y = y * y;
    return sign_of - sign_of / (y * y);
}

fn shadow_row(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * error_function((vec2<f32>(x) + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

fn shadow_alpha(point: vec2<f32>, half_size: vec2<f32>, sigma: f32, corner: f32) -> f32 {
    let low = point.y - half_size.y;
    let high = point.y + half_size.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step_size = (end - start) / 4.0;
    var y = start + step_size * 0.5;
    var value = 0.0;
    for (var index = 0; index < 4; index = index + 1) {
        value = value + shadow_row(point.x, point.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step_size;
        y = y + step_size;
    }
    return value;
}

@fragment
fn quad_fragment(in: QuadVarying) -> @location(0) vec4<f32> {
    let clip = rounded_rect_coverage(in.pixel, in.clip_bounds, in.clip_radii);
    let half_size = in.bounds.zw * 0.5;
    let local = in.pixel - (in.bounds.xy + half_size);

    if (in.parameters.x == QUAD_KIND_SHADOW) {
        let corner = min(in.corner_radii.x, min(half_size.x, half_size.y));
        let coverage = shadow_alpha(local, half_size, in.parameters.z, corner);
        return premultiplied_linear(in.fill_start) * coverage * clip;
    }

    let outer = clamp(0.5 - rounded_box_distance(local, half_size, in.corner_radii), 0.0, 1.0);
    let fill = premultiplied_linear(fill_color(in.pixel, in));
    // widths: x top, y right, z bottom, w left
    let widths = in.border_widths;
    if (max(max(widths.x, widths.y), max(widths.z, widths.w)) <= 0.0) {
        return fill * outer * clip;
    }
    // The inner edge of the border: the bounds inset by each side's width,
    // each corner's radius shrunk by the wider of its two sides.
    let inner_min = in.bounds.xy + vec2<f32>(widths.w, widths.x);
    let inner_max = in.bounds.xy + in.bounds.zw - vec2<f32>(widths.y, widths.z);
    let inner_half = (inner_max - inner_min) * 0.5;
    var border_amount = 1.0;
    if (inner_half.x > 0.0 && inner_half.y > 0.0) {
        let inner_radii = max(
            in.corner_radii - vec4<f32>(
                max(widths.w, widths.x),
                max(widths.x, widths.y),
                max(widths.y, widths.z),
                max(widths.z, widths.w),
            ),
            vec4<f32>(0.0),
        );
        let inner = rounded_box_distance(in.pixel - (inner_min + inner_half), inner_half, inner_radii);
        border_amount = clamp(0.5 + inner, 0.0, 1.0);
    }
    // The fill stops where the border starts (CSS's `background-clip:
    // padding-box`): a translucent border shows what is under the rectangle,
    // not the rectangle's own fill.
    let border = premultiplied_linear(in.border_color);
    return mix(fill, border, border_amount) * outer * clip;
}

// ─── Textured quads: glyphs and images ─────────────────────────────────────

@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct SpriteInstance {
    @location(0) bounds: vec4<f32>,
    @location(1) texture_bounds: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) corner_radii: vec4<f32>,
    @location(4) clip_bounds: vec4<f32>,
    @location(5) clip_radii: vec4<f32>,
    @location(6) parameters: vec4<f32>,
};

struct SpriteVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel: vec2<f32>,
    @location(1) texture_position: vec2<f32>,
    @location(2) @interpolate(flat) bounds: vec4<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) corner_radii: vec4<f32>,
    @location(5) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(6) @interpolate(flat) clip_radii: vec4<f32>,
    @location(7) @interpolate(flat) parameters: vec4<f32>,
};

const SPRITE_KIND_MASK: f32 = 0.0;
const SPRITE_KIND_IMAGE: f32 = 2.0;

@vertex
fn sprite_vertex(@builtin(vertex_index) vertex: u32, instance: SpriteInstance) -> SpriteVarying {
    let corner = corner_of(vertex);
    let pixel = instance.bounds.xy + corner * instance.bounds.zw;
    var out: SpriteVarying;
    out.position = to_clip_space(pixel);
    out.pixel = pixel;
    out.texture_position = mix(instance.texture_bounds.xy, instance.texture_bounds.zw, corner);
    out.bounds = instance.bounds;
    out.color = instance.color;
    out.corner_radii = instance.corner_radii;
    out.clip_bounds = instance.clip_bounds;
    out.clip_radii = instance.clip_radii;
    out.parameters = instance.parameters;
    return out;
}

@fragment
fn sprite_fragment(in: SpriteVarying) -> @location(0) vec4<f32> {
    let clip = rounded_rect_coverage(in.pixel, in.clip_bounds, in.clip_radii);
    let texel = textureSampleLevel(sprite_texture, sprite_sampler, in.texture_position, 0.0);
    if (in.parameters.x == SPRITE_KIND_MASK) {
        // A mask page is single-channel: coverage in red.
        return premultiplied_linear(in.color) * texel.r * clip;
    }
    // A colour glyph or an image: an sRGB texture, sampled as linear light;
    // `color.a` carries the layer opacity.
    let coverage = clip * in.color.a;
    if (in.parameters.x == SPRITE_KIND_IMAGE) {
        // Images are uploaded premultiplied (`renderer.rs`, `premultiplied`)
        // because they are filtered.
        let bounds = vec4<f32>(in.bounds.xy, in.bounds.xy + in.bounds.zw);
        return texel * coverage * rounded_rect_coverage(in.pixel, bounds, in.corner_radii);
    }
    // Colour glyphs are straight alpha, sampled texel for texel.
    return vec4<f32>(texel.rgb * texel.a, texel.a) * coverage;
}
