// The game's ShaderLayer fragment shader (embedded in the binary at 0x91ab52), ported to WGSL.
// Coordinates follow the game's: v_texCoord (0, 0) is the bottom-left of the render texture.
// The uniform block is `cadence_core::shaders::Uniforms` (34 vec4s, slot names there).
// textureSampleLevel everywhere: several samples sit in non-uniform control flow.

struct Fx {
    base: vec4<f32>,     // textureScale.xy, screenAspect (h/w), screenAspectInv
    sw0: vec4<f32>,      // shockWaveTime, time1, time2, time3
    sw1: vec4<f32>,      // time4, strength, waves, invert
    sw2: vec4<f32>,      // center.xy, minSize, maxSize
    sw3: vec4<f32>,      // maxDistVal
    sl0: vec4<f32>,      // shockLineTime, time1, time2, time3
    sl1: vec4<f32>,      // time4, axis, direction, dual
    sl2: vec4<f32>,      // waves, strength.xy, center
    sl3: vec4<f32>,      // maxDistVal
    glitch0: vec4<f32>,  // bot, top, xOffset, rnd
    glitch1: vec4<f32>,  // colOffset.xy, chromaticXOff, chromaticYOff
    lens0: vec4<f32>,    // origin.xy, start, end
    lens1: vec4<f32>,    // strength, additive
    lens2: vec4<f32>,    // tint.rgb
    bulge0: vec4<f32>,   // origin.xy, value, value2
    bulge1: vec4<f32>,   // radius
    pinch0: vec4<f32>,   // centerPos.xy, value.xy
    pinch1: vec4<f32>,   // calc1.xy, radius
    blur0: vec4<f32>,    // useRef, intensity, fade, onlyEmpty
    blur1: vec4<f32>,    // refColor.rgb
    radial: vec4<f32>,   // center.xy, value
    motion: vec4<f32>,   // value.xy, mult, dual
    cg0: vec4<f32>,      // time, rgbOffset, yOffset, strength
    cg1: vec4<f32>,      // height, lineStrength, lineThick
    color0: vec4<f32>,   // sepia, grayscale, useLum
    gray_tint: vec4<f32>,
    invert: vec4<f32>,
    hue: vec4<f32>,      // cosA, sinA
    cc_c: vec4<f32>,
    cc_b: vec4<f32>,
    split0: vec4<f32>,   // rowmod, colmod, rowmodCalc, colmodCalc
    split1: vec4<f32>,   // xStart, xRange, xRangeMult, yStart
    split2: vec4<f32>,   // yRange, yRangeMult
    pixelate: vec4<f32>, // pixel size in UV (Cadence's stand-in for the game's smaller texture)
};

@group(0) @binding(0) var<uniform> fx: Fx;
@group(0) @binding(1) var layer: texture_2d<f32>;
@group(0) @binding(2) var layer_sampler: sampler;

struct VOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    // one triangle covering the screen
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VOut;
    out.position = vec4(p[i], 0.0, 1.0);
    return out;
}

// GL texture coordinates (y up) → the texture (row 0 at the top).
fn tex(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(layer, layer_sampler, vec2(uv.x, 1.0 - uv.y), 0.0);
}

// GLSL smoothstep, also for edge0 > edge1 (the shader relies on it).
fn sstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn inside_range(v: f32, bottom: f32, top: f32) -> f32 {
    return step(bottom, v) - step(top, v);
}

fn rand(n: vec2<f32>) -> f32 {
    return fract(sin(dot(n, vec2(12.9898, 4.1414))) * 437.5854);
}

fn noise(n: vec2<f32>) -> f32 {
    let d = vec2(0.0, 1.0);
    let b = floor(n);
    let f = smoothstep(vec2(0.0), vec2(1.0), fract(n));
    return mix(mix(rand(b), rand(b + d.yx), f.x), mix(rand(b + d.xy), rand(b + d.yy), f.x), f.y);
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(layer));
    let v_tex = vec2(in.position.x / size.x, 1.0 - in.position.y / size.y);
    let scale = fx.base.xy;
    let scale_inv = 1.0 / scale;
    let aspect = fx.base.z;
    let aspect_inv = fx.base.w;
    var tv = v_tex;

    // Pixelate (Cadence: snap to the pixel grid of the game's smaller texture).
    if (fx.pixelate.x > 0.0 || fx.pixelate.y > 0.0) {
        let px = max(fx.pixelate.xy, vec2(1.0 / size.x, 1.0 / size.y));
        tv = (floor(tv / px) + 0.5) * px;
    }

    // SHOCKWAVE
    if (fx.sw0.x > 0.0) {
        let p = (tv * scale_inv - fx.sw2.xy) * vec2(1.0, aspect);
        var dis = max(length(p), 0.0001);
        var k = 1.0;
        if (fx.sw3.x != 0.0) {
            k = clamp(1.0 - dis * fx.sw3.x, 0.0, 1.0);
        }
        if (fx.sw1.w > 0.5) {
            dis = fx.sw2.w - max(dis, fx.sw2.z);
        }
        k *= clamp(sstep(fx.sw0.y, fx.sw0.z, dis) - sstep(fx.sw0.w, fx.sw1.x, dis), 0.0, 1.0);
        tv += k * ((p / dis) * sin(dis * fx.sw1.z - fx.sw0.x) * fx.sw1.y) * vec2(1.0, aspect_inv);
    }

    // SHOCKLINE
    if (fx.sl0.x != 0.0) {
        var dis: f32;
        if (fx.sl1.y > 0.5) {
            dis = (tv.y * scale_inv.y - fx.sl2.w) * aspect;
        } else {
            dis = tv.x * scale_inv.x - fx.sl2.w;
        }
        var k = 1.0;
        if (fx.sl3.x != 0.0) {
            let sgn = select(-1.0, 1.0, dis < 0.0);
            k = clamp(1.0 + sgn * dis * fx.sl3.x, 0.0, 1.0);
        }
        if (fx.sl1.w > 0.5) {
            dis = abs(dis);
        }
        if (fx.sl1.z > 0.5) {
            dis = 1.0 - dis;
        }
        k *= clamp(sstep(fx.sl0.y, fx.sl0.z, dis) - sstep(fx.sl0.w, fx.sl1.x, dis), 0.0, 1.0);
        tv += k * sin(dis * fx.sl2.x - fx.sl0.x) * fx.sl2.yz * vec2(1.0, aspect_inv);
    }

    // BULGE
    if (fx.bulge0.z > 0.0) {
        let d = (tv * scale_inv - fx.bulge0.xy) * vec2(1.0, aspect);
        let di = length(d);
        if (di < fx.bulge1.x) {
            let fade = pow(di / fx.bulge1.x, 3.0);
            let bulged = (fx.bulge0.xy * vec2(1.0, aspect) + normalize(d) * tan(sqrt(dot(d, d)) * fx.bulge0.z) * fx.bulge0.w) * scale * vec2(1.0, aspect_inv);
            tv = tv * fade + bulged * (1.0 - fade);
        }
    }

    // PINCH
    if (fx.pinch0.z != 0.0 || fx.pinch0.w != 0.0) {
        let d = (tv * scale_inv - fx.pinch0.xy) * vec2(1.0, aspect);
        let di = length(d);
        if (di < fx.pinch1.z) {
            let fade = pow(di / fx.pinch1.z, 2.0);
            let pinched = (fx.pinch0.xy * vec2(1.0, aspect) + normalize(d) * atan(sqrt(dot(d, d)) * -fx.pinch0.zw * 20.0) * fx.pinch1.xy) * scale * vec2(1.0, aspect_inv);
            tv = tv * fade + pinched * (1.0 - fade);
        }
    }

    // GLITCH (slices)
    if (fx.glitch0.y > 0.0) {
        tv.x += fx.glitch0.z * inside_range(tv.y, fx.glitch0.x, fx.glitch0.y);
    }

    // SPLITSCREEN
    if (fx.split0.y != 1.0 || fx.split0.x != 1.0) {
        var nx = (tv.x - fx.split1.x) * fx.split1.z;
        if (nx < 0.5 * scale.x) {
            nx = nx * fx.split0.y;
        } else {
            nx = nx * fx.split0.y + fx.split0.w;
        }
        tv.x = fx.split1.x + fract(nx) * fx.split1.y;
        var ny = (tv.y - fx.split1.w) * fx.split2.y;
        if (ny < 0.5 * scale.y) {
            ny = ny * fx.split0.x;
        } else {
            ny = ny * fx.split0.x + fx.split0.z;
        }
        tv.y = fx.split1.w + fract(ny) * fx.split2.x;
    }

    var color = tex(tv);

    // CHROMATICGLITCH
    if (fx.cg0.y != 0.0) {
        let c_val = (v_tex.y + fx.cg0.z) * 50.0 * fx.cg1.x;
        tv.x += (noise(vec2(fx.cg0.x, c_val)) - noise(vec2(0.0, c_val))) * fx.cg0.w;
        let r = tex(vec2(tv.x - fx.cg0.y, tv.y));
        let g = tex(tv);
        let b = tex(vec2(tv.x + fx.cg0.y, tv.y));
        var rgb = vec3(r.r, g.g, b.b);
        if (fx.cg1.z > 0.0) {
            rgb -= vec3(smoothstep(fx.cg1.z - 0.01, fx.cg1.z + 0.01, sin((v_tex.y + fx.cg0.z) * 300.0 * fx.cg1.x)) * fx.cg1.y);
        }
        color = vec4(rgb, (r.a + g.a + b.a) * 0.3333);
    }

    // GLITCH (one channel)
    if (fx.glitch0.y > 0.0) {
        let col2 = tex(tv + fx.glitch1.xy);
        if (fx.glitch0.w < 0.33) {
            color.r = col2.r;
        } else if (fx.glitch0.w < 0.66) {
            color.g = col2.g;
        } else {
            color.b = col2.b;
        }
        color.a = max(color.a, col2.a);
    }

    // CHROMATIC
    if (fx.glitch1.z != 0.0 || fx.glitch1.w != 0.0) {
        let r = tex(tv + fx.glitch1.zw);
        let b = tex(tv - fx.glitch1.zw);
        color = vec4(r.r, color.g, b.b, max(r.a, max(color.a, b.a)));
    }

    // RADIALBLUR / MOTIONBLUR
    let use_ref = fx.blur0.x > 0.5;
    let only_empty = fx.blur0.w > 0.5;
    if (fx.radial.z != 0.0) {
        var result = color;
        if (!only_empty || result.a < 0.1) {
            var uv = tv;
            let blur_vector = (fx.radial.xy - uv) * fx.radial.z;
            var mod_val = 1.0 + 4.5 * fx.blur0.z;
            result *= mod_val;
            for (var i = 1; i < 10; i++) {
                mod_val -= fx.blur0.z;
                uv += blur_vector;
                result += tex(uv) * mod_val;
            }
            result *= 0.1;
            if (use_ref) {
                color = vec4(result.rgb + fx.blur1.rgb * (1.0 - result.a), result.a * fx.blur0.y);
            } else {
                color = result;
            }
        }
    } else if (fx.motion.x != 0.0 || fx.motion.y != 0.0) {
        var result = color;
        if (!only_empty || result.a < 0.1) {
            if (fx.motion.w > 0.5) {
                for (var i = 1.0; i < 5.0; i += 1.0) {
                    let m = 1.0 - i * fx.blur0.z;
                    let offset = fx.motion.xy * (i / 9.0);
                    result += tex(tv + offset) * m;
                    result += tex(tv - offset) * m;
                }
            } else {
                for (var i = 1.0; i < 9.0; i += 1.0) {
                    let m = 1.0 - i * fx.blur0.z;
                    let offset = fx.motion.xy * (i / 9.0);
                    result += tex(tv + offset) * m;
                }
            }
            result *= fx.motion.z;
            if (use_ref) {
                color = vec4(result.rgb + fx.blur1.rgb * (1.0 - result.a), result.a * fx.blur0.y);
            } else {
                color = result;
            }
        }
    }

    // The game's color effects work on the texture's colors as they are, then its layer sprite
    // is blended so that a pixel's alpha hides its color. Cadence's layer is premultiplied and
    // composited with (one, one − src alpha), where an inverted transparent pixel (1, 1, 1, 0)
    // would add white: do the color effects on straight color and premultiply again.
    let alpha_in = color.a;
    if (alpha_in > 0.0) {
        color = vec4(color.rgb / alpha_in, alpha_in);
    }

    // GRAYSCALE
    if (fx.color0.y > 0.0) {
        var gray: f32;
        if (fx.color0.z > 0.5) {
            gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
        } else {
            gray = (color.r + color.g + color.b) * 0.333;
        }
        let rgb = color.rgb * (1.0 - fx.color0.y) + vec3(gray) * fx.color0.y * fx.gray_tint.rgb;
        color = vec4(rgb, color.a);
    }

    // SEPIA (GLSL vec4 * mat4: each output is the dot of the color with a matrix column)
    if (fx.color0.x > 0.0) {
        let c = color;
        let s = vec3(
            dot(c, vec4(0.393, 0.769, 0.189, 0.0)),
            dot(c, vec4(0.349, 0.686, 0.168, 0.0)),
            dot(c, vec4(0.272, 0.534, 0.131, 0.0)),
        );
        color = vec4(color.rgb * (1.0 - fx.color0.x) + s * fx.color0.x, color.a);
    }

    // INVERTCOLOR
    if (fx.invert.a > 0.0) {
        let inv = 1.0 - color;
        color = vec4(color.rgb * (1.0 - fx.invert.rgb) + inv.rgb * fx.invert.rgb, color.a);
    }

    // HUESHIFT
    if (fx.hue.x != 0.0) {
        let k = vec3(0.57735, 0.57735, 0.57735);
        let c = color.rgb;
        color = vec4(c * fx.hue.x + cross(k, c) * fx.hue.y + k * dot(k, c) * (1.0 - fx.hue.x), color.a);
    }

    // COLORCHANGE
    if (fx.cc_c.r > 0.0) {
        color = vec4(color.rgb * fx.cc_c.rgb + fx.cc_b.rgb, color.a);
    }
    color = vec4(clamp(color.rgb, vec3(0.0), vec3(1.0)) * color.a, color.a);

    // LENSCIRCLE
    if (fx.lens1.x > 0.0) {
        let dist = distance(v_tex * scale_inv * vec2(1.0, aspect), fx.lens0.xy);
        var k: f32;
        if (fx.lens0.z == fx.lens0.w) {
            k = select(0.0, fx.lens1.x, dist >= fx.lens0.w);
        } else {
            k = fx.lens1.x * (1.0 - sstep(fx.lens0.w, fx.lens0.z, dist));
        }
        if (fx.lens1.y > 0.5) {
            color = vec4(color.rgb + fx.lens2.rgb * k, color.a);
        } else {
            color = vec4(color.rgb * (1.0 - k) + fx.lens2.rgb * k, color.a);
        }
    }

    return color;
}
