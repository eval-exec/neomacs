//! GPU cache for shader surfaces (`docs/display-engine/SHADER_SURFACES.md`).
//!
//! Mirrors `VideoCache`'s shape — a `HashMap<u32, CachedShaderSurface>` whose
//! entries own a texture plus the bind group the inline-media composite phase
//! samples — but where video *uploads* frames, this cache *renders* them: each
//! animated (or dirtied) surface gets one offscreen fullscreen-triangle pass
//! per frame with the user's compiled WGSL pipeline.
//!
//! Battery policy: a surface only re-renders while it was actually composited
//! recently (`mark_drawn` from the draw phase stamps `active_until`). Scrolled
//! offscreen, its demand lapses and `iTime` freezes; scrolling it back into
//! view resumes the clock. This is deliberately stricter than video's
//! process-wide demand.

use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use bytemuck::Zeroable;

use crate::shader_surface::{
    SURFACE_UNIFORM_BYTES, SURFACE_USER_UNIFORM_SLOTS, SurfaceChannelSource, SurfaceContract,
    SurfaceShaderLanguage, SurfaceUniformInit, SurfaceUniforms, compose_surface_glsl,
    compose_surface_wgsl, uniform_accessor_name,
};

/// Build the render pipeline for a composed surface shader in either dialect.
/// GLSL modules carry only the fragment stage (entry `main`), so the vertex
/// stage comes from a minimal WGSL fullscreen-triangle module. Wrapped in a
/// validation error scope: the host already naga-validated the same source,
/// so a failure here is device-specific and reported, not fatal.
pub(crate) fn build_surface_pipeline(
    device: &wgpu::Device,
    uniform_layout: &wgpu::BindGroupLayout,
    target_format: wgpu::TextureFormat,
    language: SurfaceShaderLanguage,
    composed_source: &str,
    label: &str,
) -> Result<wgpu::RenderPipeline, String> {
    const VS_WGSL: &str = "@vertex\n\
        fn neo_vs_main(@builtin(vertex_index) neo_vi: u32) -> @builtin(position) vec4<f32> {\n\
            var neo_pos = array<vec2<f32>, 3>(\n\
                vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));\n\
            return vec4<f32>(neo_pos[neo_vi], 0.0, 1.0);\n\
        }\n";
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let (vs_module, fs_module, fs_entry) = match language {
        SurfaceShaderLanguage::Wgsl => {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(composed_source.into()),
            });
            (module.clone(), module, "neo_fs_main")
        }
        SurfaceShaderLanguage::Glsl => {
            let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Surface Vertex (GLSL companion)"),
                source: wgpu::ShaderSource::Wgsl(VS_WGSL.into()),
            });
            let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Glsl {
                    shader: composed_source.into(),
                    stage: naga::ShaderStage::Fragment,
                    defines: Default::default(),
                },
            });
            (vs, fs, "main")
        }
    };
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(uniform_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("neo_vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: Some(fs_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    if let Some(error) = pollster::block_on(error_scope.pop()) {
        return Err(format!("{label}: pipeline rejected: {error}"));
    }
    Ok(pipeline)
}

/// Largest allowed surface edge in physical pixels (matches
/// `ImageCache::MAX_TEXTURE_SIZE`).
pub const MAX_SURFACE_SIZE: u32 = 4096;

/// How long after its last composite a surface still counts as visible for
/// animation demand.
const ACTIVE_GRACE: Duration = Duration::from_millis(500);

pub struct CachedShaderSurface {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Bind group for the inline-media composite phase (image pipeline
    /// layout: texture + sampler).
    pub composite_bind_group: wgpu::BindGroup,
    /// User render pipeline; `None` for pixel-upload surfaces.
    pipeline: Option<wgpu::RenderPipeline>,
    uniform_buffer: Option<wgpu::Buffer>,
    /// Media sampled as `iChannel0` (resolved per pass so late creation,
    /// decode completion, and per-frame video uploads are picked up).
    channel0: Option<SurfaceChannelSource>,
    /// name -> (slot, components) for `set_uniform` by Lisp name.
    uniform_slots: HashMap<String, (usize, u8)>,
    custom: [[f32; 4]; SURFACE_USER_UNIFORM_SLOTS],
    /// `iMouse` uniform: xy = last hover position in physical pixels
    /// (Shadertoy convention: origin bottom-left, y-up), updated by
    /// `set_mouse_uv` while the pointer is over the composited quad and
    /// persisting when it leaves. zw = click state (`set_mouse_press_uv` /
    /// `set_mouse_release`): the press position, positive while a button is
    /// held, negated after release, 0 until the first click ever.
    mouse: [f32; 4],
    elapsed: f32,
    frame_index: u32,
    animate: bool,
    /// Per-surface animation rate cap (`:fps`). `None` re-renders every frame
    /// (display refresh); `Some(n)` re-renders at most n times/sec.
    fps: Option<u32>,
    /// When this surface last actually re-rendered — the `:fps` cap is
    /// measured from here, and `iTime` advances by the time since it, so a
    /// capped cadence still plays at correct speed (just fewer frames).
    /// `None` until the first render.
    ///
    /// An `EventTime` taken from the frame sample, not a clock read: `iTime`
    /// is a visual phase, so it has to be right when the pixels appear.
    last_render: Option<EventTime>,
    /// Needs one render even if not animating (created / uniform changed).
    dirty: bool,
    /// Last time the composite phase drew this surface (plus grace).
    ///
    /// Deliberately a wall-clock `Instant`, unlike `last_render`: this answers
    /// a resource-lifetime question ("is this surface still live enough to
    /// keep animating?"), and it is read by scheduler queries that run outside
    /// any frame, where no frame sample exists.
    active_until: Option<Instant>,
    /// Logical (scale-independent) size the surface was created with. Physical
    /// `width_px`/`height_px` are derived from it via `clamp_size`; kept so a
    /// DPI change can recompute the physical size EXACTLY as create would,
    /// with no accumulating round-trip drift across repeated rescales.
    width: u32,
    height: u32,
    width_px: u32,
    height_px: u32,
    scale: f32,
}

pub struct ShaderSurfaceCache {
    surfaces: HashMap<u32, CachedShaderSurface>,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    /// Filtering sampler for `iChannel0`.
    channel_sampler: wgpu::Sampler,
    /// 1x1 texture bound when a channel is unbound or missing. Never written:
    /// wgpu zero-initializes textures, so it samples transparent black
    /// (Shadertoy's unbound-channel behavior).
    fallback_channel_view: wgpu::TextureView,
    /// Surface whose `iMouse.zw` is currently held positive by a button
    /// press; `set_mouse_release` negates it and clears this.
    pressed: Option<u32>,
}

impl ShaderSurfaceCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Shader Surface Uniforms"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
        let channel_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shader Surface Channel Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (_, fallback_channel_view) =
            Self::make_texture(device, 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb, false);
        Self {
            surfaces: HashMap::new(),
            uniform_bind_group_layout,
            channel_sampler,
            fallback_channel_view,
            pressed: None,
        }
    }

    pub fn get(&self, id: u32) -> Option<&CachedShaderSurface> {
        self.surfaces.get(&id)
    }

    /// Whether any shader surface exists at all.
    ///
    /// Every routing entry point here no-ops on an id it does not hold, so an
    /// empty cache means no pointer position can reach a uniform. Sessions
    /// without a single shader surface are the common case, and this lets the
    /// pointer path skip searching a frame's glyphs for one.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    fn clamp_size(width: u32, height: u32, scale: f32) -> (u32, u32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let px = |v: u32| ((v as f32 * scale).round() as u32).clamp(1, MAX_SURFACE_SIZE);
        (px(width), px(height))
    }

    fn make_texture(
        device: &wgpu::Device,
        width_px: u32,
        height_px: u32,
        format: wgpu::TextureFormat,
        render_target: bool,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        if render_target {
            usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shader Surface"),
            size: wgpu::Extent3d {
                width: width_px,
                height: height_px,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn composite_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader Surface Composite"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Create a surface driven by a user WGSL shader. The source is composed
    /// with the generated prelude and compiled inside a validation error
    /// scope; the Lisp thread already naga-validated the same composition, so
    /// a failure here (device-specific rejection) is reported, not fatal.
    #[allow(clippy::too_many_arguments)]
    pub fn create_shader(
        &mut self,
        device: &wgpu::Device,
        composite_layout: &wgpu::BindGroupLayout,
        composite_sampler: &wgpu::Sampler,
        target_format: wgpu::TextureFormat,
        id: u32,
        language: SurfaceShaderLanguage,
        user_source: &str,
        uniforms: &[SurfaceUniformInit],
        width: u32,
        height: u32,
        scale: f32,
        animate: bool,
        fps: Option<u32>,
        channel0: Option<SurfaceChannelSource>,
    ) -> Result<(u32, u32), String> {
        let (width_px, height_px) = Self::clamp_size(width, height, scale);
        let names: Vec<(String, u8)> = uniforms
            .iter()
            .map(|u| (u.name.clone(), u.components))
            .collect();
        let contract = SurfaceContract::default();
        let source = match language {
            SurfaceShaderLanguage::Wgsl => compose_surface_wgsl(user_source, &names, contract),
            SurfaceShaderLanguage::Glsl => compose_surface_glsl(user_source, &names, contract),
        };
        let pipeline = build_surface_pipeline(
            device,
            &self.uniform_bind_group_layout,
            target_format,
            language,
            &source,
            "Shader Surface Pipeline",
        )
        .map_err(|err| format!("shader surface {id}: {err}"))?;

        let (texture, view) = Self::make_texture(device, width_px, height_px, target_format, true);
        let composite_bind_group =
            Self::composite_bind_group(device, composite_layout, composite_sampler, &view);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shader Surface Uniform Buffer"),
            size: SURFACE_UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Sampling the texture a pass renders into is a wgpu validation error;
        // treat self-reference as unbound (transparent black).
        let channel0 = match channel0 {
            Some(SurfaceChannelSource::Surface(channel)) if channel == id => {
                tracing::warn!("shader surface {id}: :channel0 cannot reference itself; ignored");
                None
            }
            other => other,
        };

        let mut uniform_slots = HashMap::new();
        let mut custom = [[0.0f32; 4]; SURFACE_USER_UNIFORM_SLOTS];
        for (slot, init) in uniforms.iter().enumerate().take(SURFACE_USER_UNIFORM_SLOTS) {
            uniform_slots.insert(init.name.clone(), (slot, init.components));
            custom[slot] = init.value;
        }

        self.surfaces.insert(
            id,
            CachedShaderSurface {
                texture,
                view,
                composite_bind_group,
                pipeline: Some(pipeline),
                uniform_buffer: Some(uniform_buffer),
                channel0,
                uniform_slots,
                custom,
                mouse: [0.0; 4],
                elapsed: 0.0,
                frame_index: 0,
                animate,
                fps,
                last_render: None,
                dirty: true,
                active_until: None,
                width,
                height,
                width_px,
                height_px,
                scale: if scale.is_finite() && scale > 0.0 {
                    scale
                } else {
                    1.0
                },
            },
        );
        tracing::info!(
            "shader surface {id} created: {width_px}x{height_px}px animate={animate} channel0={channel0:?}"
        );
        Ok((width_px, height_px))
    }

    /// Create a static surface from raw RGBA8 pixels (stage 1: GPU texture
    /// from Lisp data, no shader).
    #[allow(clippy::too_many_arguments)]
    pub fn create_pixels(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composite_layout: &wgpu::BindGroupLayout,
        composite_sampler: &wgpu::Sampler,
        id: u32,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(u32, u32), String> {
        let width = width.clamp(1, MAX_SURFACE_SIZE);
        let height = height.clamp(1, MAX_SURFACE_SIZE);
        let expected = width as usize * height as usize * 4;
        if data.len() < expected {
            return Err(format!(
                "surface {id}: pixel data too short: {} bytes, need {expected}",
                data.len()
            ));
        }
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let (texture, view) = Self::make_texture(device, width, height, format, false);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data[..expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let composite_bind_group =
            Self::composite_bind_group(device, composite_layout, composite_sampler, &view);
        self.surfaces.insert(
            id,
            CachedShaderSurface {
                texture,
                view,
                composite_bind_group,
                pipeline: None,
                uniform_buffer: None,
                channel0: None,
                uniform_slots: HashMap::new(),
                custom: [[0.0; 4]; SURFACE_USER_UNIFORM_SLOTS],
                mouse: [0.0; 4],
                elapsed: 0.0,
                frame_index: 0,
                animate: false,
                fps: None,
                last_render: None,
                dirty: false,
                active_until: None,
                // Pixel surfaces are content-defined (the texture IS the data),
                // so logical == physical and DPI rescale leaves them untouched.
                width,
                height,
                width_px: width,
                height_px: height,
                scale: 1.0,
            },
        );
        tracing::info!("pixel surface {id} created: {width}x{height}px");
        Ok((width, height))
    }

    /// Resample every shader surface's render target to a new scale factor
    /// (DPI change / monitor move). Physical size is recomputed from the
    /// retained logical size EXACTLY as `create_shader` would at the new
    /// scale — no round-trip drift across repeated rescales — and `iTime`,
    /// uniforms, mouse, and animation state are preserved (the next
    /// `render_pending` rewrites `iResolution` from the updated fields, and
    /// re-renders because `dirty` is set). Pixel surfaces are content-defined
    /// and left untouched. Surface-to-surface `iChannel0` wiring self-heals:
    /// the internal channel lookup reads `surface.view`, which is replaced in
    /// place. Returns `(id, new_width_px, new_height_px)` for each surface
    /// whose physical size actually changed, so the caller can re-account
    /// `MediaBudget` for the new byte cost.
    pub fn rescale(
        &mut self,
        device: &wgpu::Device,
        composite_layout: &wgpu::BindGroupLayout,
        composite_sampler: &wgpu::Sampler,
        target_format: wgpu::TextureFormat,
        new_scale: f32,
    ) -> Vec<(u32, u32, u32)> {
        let new_scale = if new_scale.is_finite() && new_scale > 0.0 {
            new_scale
        } else {
            1.0
        };
        let mut rescaled = Vec::new();
        for (id, surface) in &mut self.surfaces {
            if surface.pipeline.is_none() {
                continue;
            }
            let (new_w, new_h) = Self::clamp_size(surface.width, surface.height, new_scale);
            surface.scale = new_scale;
            if new_w == surface.width_px && new_h == surface.height_px {
                continue;
            }
            let (texture, view) = Self::make_texture(device, new_w, new_h, target_format, true);
            surface.composite_bind_group =
                Self::composite_bind_group(device, composite_layout, composite_sampler, &view);
            surface.texture = texture;
            surface.view = view;
            surface.width_px = new_w;
            surface.height_px = new_h;
            surface.dirty = true;
            rescaled.push((*id, new_w, new_h));
        }
        rescaled
    }

    /// Update one named uniform; unknown names are ignored with a warning
    /// (the accessor set is fixed at create time).
    pub fn set_uniform(&mut self, id: u32, name: &str, value: [f32; 4]) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            tracing::warn!("set_uniform: no shader surface {id}");
            return;
        };
        match surface.uniform_slots.get(name) {
            Some((slot, _)) => {
                surface.custom[*slot] = value;
                surface.dirty = true;
            }
            None => tracing::warn!(
                "set_uniform: surface {id} has no uniform {name:?} (accessor {})",
                uniform_accessor_name(name)
            ),
        }
    }

    /// Route a hover position into `iMouse.xy`. `u`/`v` are the pointer's
    /// normalized position inside the composited quad (top-left origin, as
    /// drawn); they map to physical pixels in Shadertoy's bottom-left y-up
    /// convention. zw (click state) are left untouched. Sub-half-pixel moves
    /// neither rewrite nor dirty, so a static surface re-renders on real
    /// hover movement, not every frame the pointer rests on it.
    pub fn set_mouse_uv(&mut self, id: u32, u: f32, v: f32) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        if surface.pipeline.is_none() {
            // Pixel-upload surfaces have no uniforms to route.
            return;
        }
        let x = u * surface.width_px as f32;
        let y = (1.0 - v) * surface.height_px as f32;
        if (x - surface.mouse[0]).abs() <= 0.5 && (y - surface.mouse[1]).abs() <= 0.5 {
            return;
        }
        surface.mouse[0] = x;
        surface.mouse[1] = y;
        surface.dirty = true;
    }

    /// Route a button press into `iMouse.zw` (Shadertoy click state). `u`/`v`
    /// map exactly like `set_mouse_uv` — normalized position inside the
    /// composited quad (top-left origin) to physical pixels, bottom-left y-up.
    /// zw stay positive while the button is held; `set_mouse_release` negates
    /// them. No dirty threshold: even a press at the previous position must
    /// re-render, because the sign flip is the signal.
    pub fn set_mouse_press_uv(&mut self, id: u32, u: f32, v: f32) {
        if self.pressed.is_some_and(|p| p != id) {
            // A new press landed on a different surface while another was
            // still held (multi-button corner) — release the old one so its
            // zw doesn't stay positive forever.
            self.set_mouse_release();
        }
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        if surface.pipeline.is_none() {
            // Pixel-upload surfaces have no uniforms to route.
            return;
        }
        surface.mouse[2] = u * surface.width_px as f32;
        surface.mouse[3] = (1.0 - v) * surface.height_px as f32;
        surface.dirty = true;
        self.pressed = Some(id);
    }

    /// End the click on whichever surface is pressed: negate `iMouse.zw`
    /// (Shadertoy "not pressed; last click was here"). No-op when nothing is
    /// pressed, so callers may invoke it on every button release.
    pub fn set_mouse_release(&mut self) {
        let Some(id) = self.pressed.take() else {
            return;
        };
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        surface.mouse[2] = -surface.mouse[2].abs();
        surface.mouse[3] = -surface.mouse[3].abs();
        surface.dirty = true;
    }

    pub fn free(&mut self, id: u32) {
        if self.pressed == Some(id) {
            self.pressed = None;
        }
        if self.surfaces.remove(&id).is_some() {
            tracing::info!("shader surface {id} freed");
        }
    }

    /// Stamp a surface as composited this frame; animation demand and the
    /// iTime clock stay live for `ACTIVE_GRACE` past the last composite.
    pub fn mark_drawn(&mut self, id: u32) {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            // LIFETIME, NOT PHASE: a real clock read, because the grace window
            // is compared against real clock reads in `has_active_surfaces`
            // and `active_animation_max_fps`, which the frame scheduler calls
            // outside a frame where no sample exists.
            surface.active_until = Some(Instant::now() + ACTIVE_GRACE);
        }
    }

    /// Whether any animated surface was composited recently — the
    /// `DemandReason::ShaderSurface` signal.
    pub fn has_active_surfaces(&self) -> bool {
        // LIFETIME, NOT PHASE: called by the scheduler to decide whether to
        // ask for a frame at all, so there is no frame sample to date it to.
        let now = Instant::now();
        self.surfaces
            .values()
            .any(|s| s.pipeline.is_some() && (s.dirty || (s.animate && s.is_active(now))))
    }

    /// The frame-rate cap the currently-animating surfaces collectively need.
    /// `None` means "run at the full display rate" — either an uncapped
    /// (`:fps`-less) animated surface is active, or none is (the caller gates
    /// submit/retract on [`Self::has_active_surfaces`]). `Some(n)` means every
    /// active animated surface is `:fps`-capped and the highest cap is `n`.
    /// Only sustained animation counts; a one-shot `dirty` surface gets its
    /// single frame via the redraw request regardless of cadence.
    pub fn active_animation_max_fps(&self) -> Option<u32> {
        // LIFETIME, NOT PHASE: same scheduler query, same reasoning as
        // `has_active_surfaces`.
        let now = Instant::now();
        let mut max_cap: Option<u32> = None;
        for surface in self.surfaces.values() {
            if surface.pipeline.is_none() || !surface.animate || !surface.is_active(now) {
                continue;
            }
            match surface.fps {
                None => return None,
                Some(cap) => max_cap = Some(max_cap.map_or(cap, |m| m.max(cap))),
            }
        }
        max_cap
    }

    /// Cache ids of every image/video channel referenced by a shader surface
    /// (surface-to-surface channels resolve internally). The caller resolves
    /// these against the image/video caches and passes the views into
    /// [`Self::render_pending`].
    pub fn external_channel_sources(&self) -> Vec<SurfaceChannelSource> {
        let mut sources: Vec<SurfaceChannelSource> = self
            .surfaces
            .values()
            .filter_map(|surface| surface.channel0)
            .filter(|source| !matches!(source, SurfaceChannelSource::Surface(_)))
            .collect();
        sources.sort_unstable();
        sources.dedup();
        sources
    }

    /// Render every surface that needs a new frame (dirty, or animated and
    /// recently composited). One encoder for all passes, submitted before the
    /// main frame pass samples the textures. Returns how many passes ran.
    ///
    /// `external` maps image/video channel sources (from
    /// [`Self::external_channel_sources`]) to their current texture views;
    /// missing entries sample the transparent-black fallback.
    ///
    /// Two phases: advance clocks + write uniform buffers while collecting the
    /// render list (with each target's `iChannel0` view resolved — possibly
    /// another entry in the map, hence the split), then encode the passes. A
    /// chain A→B may therefore see B's previous frame (Shadertoy multipass
    /// buffers have the same one-frame semantics).
    pub fn render_pending(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        external: &std::collections::HashMap<SurfaceChannelSource, wgpu::TextureView>,
        sample: FrameSample,
    ) -> usize {
        // Two clocks, two questions. `iTime`, `iTimeDelta` and the `:fps`
        // cadence are a visual phase, so they are dated to when this frame
        // reaches the screen...
        let now = sample.presentation_time();
        // ...while the composite grace window is a resource-lifetime question,
        // stamped by `mark_drawn` off the wall clock and compared against it
        // by the scheduler. Keeping them separate is why neither has to be
        // converted into the other's domain.
        let live_now = Instant::now();

        let mut pending: Vec<u32> = Vec::new();
        for (id, surface) in &mut self.surfaces {
            let (Some(_), Some(buffer)) =
                (surface.pipeline.as_ref(), surface.uniform_buffer.as_ref())
            else {
                continue;
            };
            let animating = surface.animate && surface.is_active(live_now);
            if !surface.dirty && !animating {
                continue;
            }
            // `:fps` cap — skip an animated re-render arriving sooner than
            // 1/fps since the last one. A dirty surface always renders
            // (create / uniform change / DPI rescale), regardless of cap.
            if !surface.dirty
                && animating
                && let (Some(fps), Some(last)) = (surface.fps, surface.last_render)
            {
                let min_interval = Duration::from_secs_f32(1.0 / fps.max(1) as f32);
                if now.saturating_since(last) < min_interval {
                    continue;
                }
            }
            // Advance the clock by the time since THIS surface last rendered,
            // so a capped cadence plays at correct speed (fewer frames, not
            // slow motion). Uncapped surfaces render every tick, so this equals
            // the frame delta as before.
            let surface_dt = surface
                .last_render
                .map(|last| now.saturating_since(last).as_secs_f32().clamp(0.0, 0.1))
                .unwrap_or(0.0);
            if animating {
                surface.elapsed += surface_dt;
            }
            surface.last_render = Some(now);
            surface.frame_index = surface.frame_index.wrapping_add(1);
            surface.dirty = false;

            let mut values = SurfaceUniforms::zeroed();
            values.i_resolution = [
                surface.width_px as f32,
                surface.height_px as f32,
                surface.scale,
                0.0,
            ];
            values.i_mouse = surface.mouse;
            values.i_time = surface.elapsed;
            values.i_time_delta = if animating { surface_dt } else { 0.0 };
            values.i_frame = surface.frame_index as f32;
            values.custom = surface.custom;
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&values));
            pending.push(*id);
        }

        if pending.is_empty() {
            return 0;
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Shader Surface Passes"),
        });
        for id in &pending {
            let channel_view = self
                .surfaces
                .get(id)
                .and_then(|surface| surface.channel0)
                .and_then(|channel| match channel {
                    SurfaceChannelSource::Surface(source_id) => self
                        .surfaces
                        .get(&source_id)
                        .map(|source| source.view.clone()),
                    external_source => external.get(&external_source).cloned(),
                })
                .unwrap_or_else(|| self.fallback_channel_view.clone());
            let Some(surface) = self.surfaces.get(id) else {
                continue;
            };
            let (Some(pipeline), Some(buffer)) =
                (surface.pipeline.as_ref(), surface.uniform_buffer.as_ref())
            else {
                continue;
            };
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shader Surface Uniform Bind Group"),
                layout: &self.uniform_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&channel_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.channel_sampler),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Shader Surface Pass"),
                    multiview_mask: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            tracing::trace!("shader surface {id} rendered (t={:.3})", surface.elapsed);
        }
        queue.submit(std::iter::once(encoder.finish()));
        pending.len()
    }
}

impl CachedShaderSurface {
    /// Takes a wall-clock `Instant`, matching `active_until`: this is the
    /// lifetime question, not the phase one.
    fn is_active(&self, now: Instant) -> bool {
        self.active_until.is_some_and(|until| now < until)
    }
}

#[cfg(test)]
#[path = "shader_surface_cache_test.rs"]
mod tests;
