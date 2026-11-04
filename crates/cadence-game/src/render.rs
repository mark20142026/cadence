//! wgpu sprite renderer: Metal on macOS, WebGPU/WebGL2 on the web.
//!
//! Every sprite is one instance: a 2D affine transform taking the unit square to the sprite's
//! world-space quad, the four corner UVs (so rotated/flipped atlas frames are exact), and a tint.
//! Sprites are drawn in the order given, in runs of the same sheet and blend mode.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::assets::{Frame, Image};

/// Use only the GL backend (WebGL2 on the web), to test the path older devices get.
pub static FORCE_GL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Instance {
    /// Affine transform: world = [a c tx; b d ty] * (u, v, 1) for the unit quad corner (u, v).
    pub m: [f32; 6],
    /// [0]: 1 draws only the triangle of the first three corners (see `triangle_instance`).
    pub _pad: [f32; 2],
    /// UVs of the bottom-left, bottom-right, top-left, top-right corners.
    pub uv: [f32; 8],
    pub color: [f32; 4],
}

/// The game's blend modes (textures are premultiplied, like cocos2d's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// ONE, ONE_MINUS_SRC_ALPHA: ordinary sprites.
    Normal,
    /// SRC_ALPHA, ONE: glows, the ground line, additive objects.
    Additive,
    /// DST_COLOR, ONE_MINUS_SRC_ALPHA: the ground's edge shadows.
    Shade,
    /// ONE, ZERO: the background.
    Opaque,
    /// Gradient triggers (`gradient_instances`, per-corner colors, not premultiplied):
    /// SRC_ALPHA, ONE_MINUS_SRC_ALPHA (blending 0).
    GradientNormal,
    /// SRC_ALPHA, ONE (blending 1).
    GradientAdditive,
    /// DST_COLOR, ONE_MINUS_SRC_ALPHA with premultiplied colors (blending 2).
    GradientMultiply,
    /// ONE_MINUS_DST_COLOR, ONE_MINUS_SRC_COLOR with premultiplied colors (blending 3).
    GradientInvert,
}

impl Blend {
    fn is_gradient(self) -> bool {
        matches!(self, Blend::GradientNormal | Blend::GradientAdditive | Blend::GradientMultiply | Blend::GradientInvert)
    }

    /// The gradient trigger's blending (key 174) as a blend mode.
    pub fn gradient(blending: i32) -> Blend {
        match blending {
            1 => Blend::GradientAdditive,
            2 => Blend::GradientMultiply,
            3 => Blend::GradientInvert,
            _ => Blend::GradientNormal,
        }
    }
}

/// A quad with a color per corner (bottom-left, bottom-right, top-left, top-right; GL's
/// triangle strip order, like `CCLayerColor`), as two triangles for the gradient pipelines.
pub fn gradient_instances(corners: [(f32, f32); 4], colors: [[f32; 4]; 4]) -> [Instance; 2] {
    let tri = |a: usize, b: usize, c: usize| {
        let (p0, p1, p2) = (corners[a], corners[b], corners[c]);
        let (c0, c1) = (colors[a], colors[b]);
        Instance {
            m: [p1.0 - p0.0, p1.1 - p0.1, p2.0 - p0.0, p2.1 - p0.1, p0.0, p0.1],
            _pad: [1.0, 0.0],
            uv: [c0[0], c0[1], c0[2], c0[3], c1[0], c1[1], c1[2], c1[3]],
            color: colors[c],
        }
    };
    [tri(0, 1, 2), tri(1, 3, 2)]
}

/// The shader layer for one frame (`ShaderLayer::visit`): `draws[range]` are drawn into a
/// render texture, which the post shader (postfx.wgsl) then draws over the frame.
pub struct PostFx {
    pub range: (usize, usize),
    /// `cadence_core::shaders::Uniforms`.
    pub uniforms: [[f32; 4]; 34],
}

/// The render texture the shader layer draws into, kept while the size holds.
struct LayerTarget {
    size: (u32, u32),
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

/// CADENCE_NO_OCCLUSION: draw every sprite (to compare with the occlusion pass).
fn no_occlusion() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *OFF.get_or_init(|| std::env::var_os("CADENCE_NO_OCCLUSION").is_some())
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::app::dev_flag("noocclusion")
    }
}

/// The occlusion pass's depth buffer format.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A sprite to draw: which sheet, how to blend, and its instance data.
#[derive(Clone, Copy)]
pub struct Draw {
    pub sheet: usize,
    pub blend: Blend,
    pub instance: Instance,
}

/// Tint for a premultiplied texture: color scaled by opacity, alpha = opacity.
pub fn tint(rgb: [f32; 3], opacity: f32) -> [f32; 4] {
    [rgb[0] * opacity, rgb[1] * opacity, rgb[2] * opacity, opacity]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Camera {
    /// world -> clip: clip = (world - origin) * scale - 1
    origin: [f32; 2],
    scale: [f32; 2],
}

fn screenshot_size() -> Option<(u32, u32)> {
    let v = std::env::var("CADENCE_SIZE").ok()?;
    let (w, h) = v.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// The quad for one frame drawn in a node's local content space (cocos2d `CCSprite` rules):
/// the trimmed image sits at ((source - size) / 2 + offset) inside the content box.
pub fn sprite_instance(frame: &Frame, atlas: (f32, f32), transform: [f32; 6], flip_x: bool, flip_y: bool, color: [f32; 4]) -> Instance {
    let (w, h) = frame.size;
    let (sw, sh) = frame.source_size;
    let ox = if flip_x { -frame.offset.0 } else { frame.offset.0 };
    let oy = if flip_y { -frame.offset.1 } else { frame.offset.1 };
    let x0 = (sw - w) / 2.0 + ox;
    let y0 = (sh - h) / 2.0 + oy;
    // local quad (x0, y0)-(x0+w, y0+h) composed into the node transform
    let [a, b, c, d, tx, ty] = transform;
    let m = [a * w, b * w, c * h, d * h, a * x0 + c * y0 + tx, b * x0 + d * y0 + ty];

    let (aw, ah) = atlas;
    let uv = if frame.rotated {
        let (mut left, mut right) = (frame.tex_x / aw, (frame.tex_x + frame.tex_h) / aw);
        let (mut top, mut bottom) = (frame.tex_y / ah, (frame.tex_y + frame.tex_w) / ah);
        if flip_x {
            std::mem::swap(&mut top, &mut bottom);
        }
        if flip_y {
            std::mem::swap(&mut left, &mut right);
        }
        [left, top, left, bottom, right, top, right, bottom]
    } else {
        let (mut left, mut right) = (frame.tex_x / aw, (frame.tex_x + frame.tex_w) / aw);
        let (mut top, mut bottom) = (frame.tex_y / ah, (frame.tex_y + frame.tex_h) / ah);
        if flip_x {
            std::mem::swap(&mut left, &mut right);
        }
        if flip_y {
            std::mem::swap(&mut top, &mut bottom);
        }
        [left, bottom, right, bottom, left, top, right, top]
    };
    Instance { m, _pad: [0.0; 2], uv, color }
}

/// A solid triangle (p0, p1, p2), sampling one texel at `uv` (a white texel gives flat color).
pub fn triangle_instance(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), uv: (f32, f32), color: [f32; 4]) -> Instance {
    let m = [p1.0 - p0.0, p1.1 - p0.1, p2.0 - p0.0, p2.1 - p0.1, p0.0, p0.1];
    Instance { m, _pad: [1.0, 0.0], uv: [uv.0, uv.1, uv.0, uv.1, uv.0, uv.1, uv.0, uv.1], color }
}

/// A particle quad (`CCParticleSystemQuad::updateQuadWithParticle`): `size` square centered on
/// `center`, rotated by `rotation` degrees clockwise, over the whole texture.
pub fn particle_instance(center: (f32, f32), size: f32, rotation: f32, color: [f32; 4]) -> Instance {
    quad_instance(center, (size, size), rotation, [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], color)
}

/// A `w` × `h` quad centered on `center`, rotated `rotation` degrees clockwise, with corner UVs
/// (bottom-left, bottom-right, top-left, top-right).
pub fn quad_instance(center: (f32, f32), (w, h): (f32, f32), rotation: f32, uv: [f32; 8], color: [f32; 4]) -> Instance {
    // (unturned quads, most particles: sin_cos(±0) is exactly (±0, 1), without the call)
    let r = -rotation;
    let (sin, cos) = if r == 0.0 { (r, 1.0) } else { r.to_radians().sin_cos() };
    let (a, b, c, d) = (w * cos, w * sin, -h * sin, h * cos);
    let m = [a, b, c, d, center.0 - 0.5 * (a + c), center.1 - 0.5 * (b + d)];
    Instance { m, _pad: [0.0; 2], uv, color }
}

struct SheetTexture {
    bind_group: wgpu::BindGroup,
    size: (f32, f32),
}

pub struct Renderer {
    pub window: Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipelines: [wgpu::RenderPipeline; 8],
    /// The occlusion pass's pipelines (same order) and its opaque pre-pass, and the depth
    /// texture they share (by size).
    occluded: [wgpu::RenderPipeline; 8],
    prepass: wgpu::RenderPipeline,
    depth: Option<((u32, u32), wgpu::TextureView)>,
    /// The instance upload's list, reused every frame.
    scratch: Vec<Instance>,
    post_pipeline: wgpu::RenderPipeline,
    post_layout: wgpu::BindGroupLayout,
    post_uniforms: wgpu::Buffer,
    layer_target: Option<LayerTarget>,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    sheets: Vec<SheetTexture>,
    /// The sheet `capture` renders into, once it has been used.
    capture_slot: Option<usize>,
    pub backend: String,
}

impl Renderer {
    pub async fn new(window: Arc<winit::window::Window>) -> Result<Renderer, String> {
        // On the web, check that WebGPU really yields an adapter before using it; otherwise use
        // WebGL2 (older GPUs and many Chromebooks expose navigator.gpu but can't provide one).
        #[cfg(target_arch = "wasm32")]
        let instance = {
            let mut desc = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone()));
            if FORCE_GL.load(std::sync::atomic::Ordering::Relaxed) {
                desc.backends = wgpu::Backends::GL;
                wgpu::Instance::new(desc)
            } else {
                wgpu::util::new_instance_with_webgpu_detection(desc).await
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let backend = format!("{:?} ({})", adapter.get_info().backend, adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("cadence"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported")?;
        // The game blends in gamma space (cocos2d on OpenGL, no sRGB); use a non-sRGB target so
        // alpha edges, glows and tints blend exactly like it.
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = format;
        }
        // CADENCE_NOVSYNC (benchmarks): present as fast as frames are made, to measure headroom.
        config.present_mode = if cfg!(not(target_arch = "wasm32")) && std::env::var_os("CADENCE_NOVSYNC").is_some() { wgpu::PresentMode::AutoNoVsync } else { wgpu::PresentMode::AutoVsync };
        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC; // screenshots
        }
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sheet"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            ..Default::default()
        });
        let make_sprite = |blend: wgpu::BlendState, depth: Option<wgpu::DepthStencilState>, fs: &str, write_mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sprite"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(blend),
                        write_mask,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let make_pipeline = |blend: wgpu::BlendState| make_sprite(blend, None, "fs", wgpu::ColorWrites::ALL);
        // the occlusion pass: tested against the pre-pass's depth, never writing it
        let tested = |compare| Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let gradient_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gradient"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gradient.wgsl").into()),
        });
        let gradient_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gradient"),
            bind_group_layouts: &[Some(&camera_layout)],
            ..Default::default()
        });
        let make_gradient_with = |blend: wgpu::BlendComponent, depth: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gradient"),
                layout: Some(&gradient_layout),
                vertex: wgpu::VertexState {
                    module: &gradient_shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gradient_shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState { color: blend, alpha: blend }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let make_gradient = |blend: wgpu::BlendComponent| make_gradient_with(blend, None);
        let component = |src, dst| wgpu::BlendComponent { src_factor: src, dst_factor: dst, operation: wgpu::BlendOperation::Add };
        use wgpu::BlendFactor as F;
        let blends = [
            wgpu::BlendState { color: component(F::One, F::OneMinusSrcAlpha), alpha: component(F::One, F::OneMinusSrcAlpha) },
            wgpu::BlendState { color: component(F::SrcAlpha, F::One), alpha: component(F::SrcAlpha, F::One) },
            wgpu::BlendState { color: component(F::Dst, F::OneMinusSrcAlpha), alpha: component(F::Dst, F::OneMinusSrcAlpha) },
            wgpu::BlendState::REPLACE,
        ];
        let gradient_blends = [
            component(F::SrcAlpha, F::OneMinusSrcAlpha),
            component(F::SrcAlpha, F::One),
            component(F::Dst, F::OneMinusSrcAlpha),
            component(F::OneMinusDst, F::OneMinusSrc),
        ];
        // The same eight for the occlusion pass: sprites behind the pre-pass's opaque texels are
        // skipped (LessEqual: a sprite's own opaque texels pass); gradients are never skipped.
        let occluded: [wgpu::RenderPipeline; 8] = [
            make_sprite(blends[0], tested(wgpu::CompareFunction::LessEqual), "fs", wgpu::ColorWrites::ALL),
            make_sprite(blends[1], tested(wgpu::CompareFunction::LessEqual), "fs", wgpu::ColorWrites::ALL),
            make_sprite(blends[2], tested(wgpu::CompareFunction::LessEqual), "fs", wgpu::ColorWrites::ALL),
            make_sprite(blends[3], tested(wgpu::CompareFunction::LessEqual), "fs", wgpu::ColorWrites::ALL),
            make_gradient_with(gradient_blends[0], tested(wgpu::CompareFunction::Always)),
            make_gradient_with(gradient_blends[1], tested(wgpu::CompareFunction::Always)),
            make_gradient_with(gradient_blends[2], tested(wgpu::CompareFunction::Always)),
            make_gradient_with(gradient_blends[3], tested(wgpu::CompareFunction::Always)),
        ];
        let prepass = make_sprite(
            wgpu::BlendState::REPLACE,
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            "fs_opaque",
            wgpu::ColorWrites::empty(),
        );
        let pipelines = [
            make_pipeline(wgpu::BlendState { color: component(F::One, F::OneMinusSrcAlpha), alpha: component(F::One, F::OneMinusSrcAlpha) }),
            make_pipeline(wgpu::BlendState { color: component(F::SrcAlpha, F::One), alpha: component(F::SrcAlpha, F::One) }),
            make_pipeline(wgpu::BlendState { color: component(F::Dst, F::OneMinusSrcAlpha), alpha: component(F::Dst, F::OneMinusSrcAlpha) }),
            make_pipeline(wgpu::BlendState::REPLACE),
            make_gradient(component(F::SrcAlpha, F::OneMinusSrcAlpha)),
            make_gradient(component(F::SrcAlpha, F::One)),
            make_gradient(component(F::Dst, F::OneMinusSrcAlpha)),
            make_gradient(component(F::OneMinusDst, F::OneMinusSrc)),
        ];

        // The shader layer's post pass: the layer texture through postfx.wgsl, drawn over the
        // frame like the game's render-texture sprite (ONE, ONE_MINUS_SRC_ALPHA).
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("postfx"),
            source: wgpu::ShaderSource::Wgsl(include_str!("postfx.wgsl").into()),
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("postfx"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("postfx"),
            bind_group_layouts: &[Some(&post_layout)],
            ..Default::default()
        });
        let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("postfx"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState { module: &post_shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState { color: component(F::One, F::OneMinusSrcAlpha), alpha: component(F::One, F::OneMinusSrcAlpha) }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let post_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("postfx"),
            size: (34 * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::bytes_of(&Camera { origin: [0.0; 2], scale: [1.0; 2] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let instance_capacity = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (instance_capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Renderer {
            window,
            surface,
            device,
            queue,
            config,
            pipelines,
            occluded,
            prepass,
            depth: None,
            scratch: Vec::new(),
            post_pipeline,
            post_layout,
            post_uniforms,
            layer_target: None,
            texture_layout,
            sampler,
            camera_buffer,
            camera_group,
            instance_buffer,
            instance_capacity,
            sheets: Vec::new(),
            capture_slot: None,
            backend,
        })
    }

    /// Upload a sheet image; returns its index (matching the asset sheet index if called in order).
    pub fn add_sheet(&mut self, image: &Image) -> usize {
        let size = wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sheet"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &image.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * image.width), rows_per_image: Some(image.height) },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sheet"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.sheets.push(SheetTexture { bind_group, size: (image.width as f32, image.height as f32) });
        self.sheets.len() - 1
    }

    /// Render `draws` (view: left, bottom, width, height in points) into a `px` × `px` transparent
    /// texture and make it a sheet (cocos2d `CCRenderTexture`; the death shatter captures the icon
    /// this way). Captures reuse one sheet slot. Returns the sheet index.
    pub fn capture(&mut self, draws: &[Draw], view: (f32, f32, f32, f32), px: u32) -> usize {
        let px = px.clamp(1, 2048);
        let size = wgpu::Extent3d { width: px, height: px, depth_or_array_layers: 1 };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let (left, bottom, width, height) = view;
        let camera = Camera { origin: [left, bottom], scale: [2.0 / width, 2.0 / height] };
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        self.ensure_instances(draws);
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("capture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            self.encode_draws(&mut pass, draws);
        }
        self.queue.submit([encoder.finish()]);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&target) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let sheet = SheetTexture { bind_group, size: (px as f32, px as f32) };
        match self.capture_slot {
            Some(i) => {
                self.sheets[i] = sheet;
                i
            }
            None => {
                self.sheets.push(sheet);
                self.capture_slot = Some(self.sheets.len() - 1);
                self.sheets.len() - 1
            }
        }
    }

    /// Grow the instance buffer if needed and upload the draws' instances. With `depths`, each
    /// instance carries its depth for the occlusion pass: later draws nearer (smaller).
    fn ensure_instances(&mut self, draws: &[Draw]) {
        self.upload_instances(draws, false);
    }

    fn upload_instances(&mut self, draws: &[Draw], depths: bool) {
        if draws.len() > self.instance_capacity {
            self.instance_capacity = draws.len().next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instances"),
                size: (self.instance_capacity * std::mem::size_of::<Instance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if draws.is_empty() {
            return;
        }
        // into a scratch list kept between frames (no allocation per frame)
        let n = draws.len() as f32 + 2.0;
        let mut out = std::mem::take(&mut self.scratch);
        out.clear();
        out.extend(draws.iter().enumerate().map(|(i, d)| {
            let mut inst = d.instance;
            if depths {
                inst._pad[1] = 1.0 - (i as f32 + 1.0) / n;
            }
            inst
        }));
        self.queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&out));
        self.scratch = out;
    }

    /// The depth texture for a `size` target.
    fn depth_view(&mut self, size: (u32, u32)) -> wgpu::TextureView {
        if self.depth.as_ref().is_none_or(|d| d.0 != size) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("occlusion depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((size, texture.create_view(&wgpu::TextureViewDescriptor::default())));
        }
        self.depth.as_ref().map(|d| d.1.clone()).expect("depth")
    }

    /// The occlusion pre-pass over `draws[from..to]`: the normal and opaque sprites, nearest
    /// runs first, writing depth where their texels are fully opaque.
    fn encode_prepass(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[Draw], from: usize, to: usize) {
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.set_pipeline(&self.prepass);
        let mut runs = Vec::new();
        let mut start = from;
        while start < to {
            let (sheet, blend) = (draws[start].sheet, draws[start].blend);
            let mut end = start + 1;
            while end < to && draws[end].sheet == sheet && draws[end].blend == blend {
                end += 1;
            }
            if matches!(blend, Blend::Normal | Blend::Opaque) {
                runs.push((sheet, start, end));
            }
            start = end;
        }
        for &(sheet, start, end) in runs.iter().rev() {
            pass.set_bind_group(1, &self.sheets[sheet].bind_group, &[]);
            pass.draw(0..6, start as u32..end as u32);
        }
    }

    /// Draw in runs of the same sheet and blend mode.
    fn encode_draws(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[Draw]) {
        self.encode_range(pass, draws, 0, draws.len());
    }

    /// Draw `draws[from..to]` (instances already uploaded for all of `draws`).
    fn encode_range(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[Draw], from: usize, to: usize) {
        self.encode_with(pass, draws, from, to, &self.pipelines);
    }

    fn encode_with(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[Draw], from: usize, to: usize, pipelines: &[wgpu::RenderPipeline; 8]) {
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        let mut start = from;
        while start < to {
            let (sheet, blend) = (draws[start].sheet, draws[start].blend);
            let mut end = start + 1;
            while end < to && (draws[end].sheet == sheet || blend.is_gradient()) && draws[end].blend == blend {
                end += 1;
            }
            pass.set_pipeline(&pipelines[blend as usize]);
            if !blend.is_gradient() {
                pass.set_bind_group(1, &self.sheets[sheet].bind_group, &[]);
            }
            pass.draw(0..6, start as u32..end as u32);
            start = end;
        }
    }

    /// The shader layer's texture for a `size` frame.
    fn layer_target(&mut self, size: (u32, u32)) {
        if self.layer_target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shader layer"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader layer"),
            layout: &self.post_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.post_uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.layer_target = Some(LayerTarget { size, view, bind_group });
    }

    pub fn atlas_size(&self, sheet: usize) -> (f32, f32) {
        self.sheets[sheet].size
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Drawing size in pixels. Screenshot mode can override it with $CADENCE_SIZE=WxH (to match a
    /// real-game capture).
    pub fn size(&self) -> (u32, u32) {
        screenshot_size().unwrap_or((self.config.width, self.config.height))
    }

    /// Draw one frame: clear to `clear` (0..1, gamma space like the game), then the sprites in order. The view shows
    /// world x in [left, left + width) and y in [bottom, bottom + height).
    /// Optionally saves the frame as a PNG (RGBA) at `screenshot`.
    pub fn render_to(&mut self, clear: [f64; 3], view: (f32, f32, f32, f32), draws: &[Draw], screenshot: Option<&std::path::Path>) {
        self.render_frame(clear, view, draws, None, screenshot);
    }

    /// `render_to` with an optional shader layer (`PostFx`): `draws[..start]` go to the frame,
    /// `draws[start..end]` into the layer texture, the layer through the post shader onto the
    /// frame, then `draws[end..]`.
    pub fn render_frame(
        &mut self,
        clear: [f64; 3],
        view: (f32, f32, f32, f32),
        draws: &[Draw],
        post: Option<&PostFx>,
        screenshot: Option<&std::path::Path>,
    ) {
        let (left, bottom, width, height) = view;
        let camera = Camera { origin: [left, bottom], scale: [2.0 / width, 2.0 / height] };
        // Occlusion (frames without a shader layer): a pre-pass marks the fully opaque texels
        // of normal sprites; the frame then skips whatever lies behind them, which would be
        // overdrawn completely anyway (CADENCE_NO_OCCLUSION turns it off, for comparisons).
        let occlusion = post.is_none() && !no_occlusion();
        // (measurement) ?nodraw on the web: clear only, to see the cost of everything but our draws
        #[cfg(target_arch = "wasm32")]
        let draws: &[Draw] = if crate::app::dev_flag("nodraw") { &[] } else { draws };

        // Screenshots render offscreen so they don't depend on the window being visible.
        let (shot_w, shot_h) = self.size();
        let offscreen = screenshot.map(|_| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen"),
                size: wgpu::Extent3d { width: shot_w, height: shot_h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let frame = if offscreen.is_some() {
            None
        } else {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
                wgpu::CurrentSurfaceTexture::Occluded => {
                    // Hidden window: no vsync to wait on, so don't spin.
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    let _ = self.device.poll(wgpu::PollType::Poll);
                    return;
                }
                _ => {
                    self.surface.configure(&self.device, &self.config);
                    let _ = self.device.poll(wgpu::PollType::Poll);
                    return;
                }
            }
        };
        // Uploads only for a frame that is drawn: written but never submitted (a covered window
        // returns above), each frame's staging memory stayed allocated.
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        self.upload_instances(draws, occlusion);
        let target_texture = offscreen.as_ref().unwrap_or_else(|| &frame.as_ref().unwrap().texture);
        let target = target_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let clear_color = wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: 1.0 };
        let post = post.map(|p| (p.range.0.min(draws.len()), p.range.1.min(draws.len()).max(p.range.0.min(draws.len())), p.uniforms));
        if let Some((_, _, uniforms)) = &post {
            let size = (target_texture.width(), target_texture.height());
            self.layer_target(size);
            self.queue.write_buffer(&self.post_uniforms, 0, bytemuck::cast_slice(uniforms.as_slice()));
        }
        let (first_end, rest_start) = post.map_or((draws.len(), draws.len()), |(s, e, _)| (s, e));
        let depth = occlusion.then(|| self.depth_view((target_texture.width(), target_texture.height())));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear_color), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: depth.as_ref().map(|view| wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if occlusion {
                self.encode_prepass(&mut pass, draws, 0, first_end);
                self.encode_with(&mut pass, draws, 0, first_end, &self.occluded);
            } else {
                self.encode_range(&mut pass, draws, 0, first_end);
            }
        }
        if let (Some((start, end, _)), Some(layer)) = (post, self.layer_target.as_ref()) {
            {
                // The game clears its render texture to transparent; a layer starting at the
                // background is opaque anyway, so clear it like the frame.
                let load = if start == 0 { clear_color } else { wgpu::Color::TRANSPARENT };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("shader layer"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &layer.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(load), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
                self.encode_range(&mut pass, draws, start, end);
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.post_pipeline);
            pass.set_bind_group(0, &layer.bind_group, &[]);
            pass.draw(0..3, 0..1);
            drop(pass);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("over the shader layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            self.encode_range(&mut pass, draws, rest_start, draws.len());
        }
        let readback = screenshot.map(|_| {
            let (w, h) = (shot_w, shot_h);
            let row = (4 * w).div_ceil(256) * 256;
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screenshot"),
                size: (row * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                target_texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            (buffer, row, w, h)
        });
        self.queue.submit([encoder.finish()]);
        if let (Some((buffer, row, w, h)), Some(path)) = (readback, screenshot) {
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let data = match slice.get_mapped_range() {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("screenshot readback failed: {e:?}");
                    return;
                }
            };
            let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                let line = &data[(y * row) as usize..(y * row + 4 * w) as usize];
                for px in line.chunks(4) {
                    if bgra { rgba.extend([px[2], px[1], px[0], 255]) } else { rgba.extend([px[0], px[1], px[2], 255]) }
                }
            }
            let saved = std::fs::File::create(path).map_err(|e| e.to_string()).and_then(|file| {
                let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
                writer.write_image_data(&rgba).map_err(|e| e.to_string())
            });
            match saved {
                Ok(()) => eprintln!("screenshot: {} ({w}x{h})", path.display()),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
        }
        if let Some(frame) = frame {
            self.window.pre_present_notify();
            self.queue.present(frame);
        }
        // Free what finished frames used (write_buffer's staging memory, command buffers):
        // wgpu only releases them when the device is polled, and without this a visible window
        // grew ~2 MB/s of GPU memory until macOS ran out (2026-11-04 freeze).
        let _ = self.device.poll(wgpu::PollType::Poll);
    }
}
