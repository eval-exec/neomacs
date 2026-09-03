//! WebView texture cache for wgpu rendering.

use neomacs_host_runtime::time::Instant;
use std::collections::HashMap;

use neomacs_display_protocol::types::WebViewId;

use crate::external_buffer::DmaBufBuffer;
use crate::submission_retirement::SubmissionRetirementQueue;

#[derive(Debug)]
struct CacheOwnedTexture;

/// Cached WebView texture.
pub struct CachedWebView {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
    pub last_updated: Instant,
    // This marker is private so a browser-owned external texture cannot be
    // represented as a `CachedWebView` outside this module.
    _ownership: CacheOwnedTexture,
}

/// Cache of WebView textures for wgpu rendering.
///
/// Entries correspond 1:1 to live WebViews, so the cache is intentionally
/// uncapped: evicting a live view would blank its quad until the view's next
/// damage frame (DMA-BUF views may not push one for a long time). Lifetime is
/// instead bounded by guaranteed removal — display-runtime's
/// the WebView close handler calls `remove` for every destroyed view, and
/// dropping the renderer drops the cache wholesale.
pub struct WgpuWebViewCache {
    /// Budget accounting events since the last drain (texture create/free).
    accounting: Vec<crate::media_budget::MediaAccounting>,
    views: HashMap<WebViewId, CachedWebView>,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    retirement: SubmissionRetirementQueue<wgpu::SubmissionIndex>,
}

impl WgpuWebViewCache {
    /// Create a new WebView cache.
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("WebView Bind Group Layout"),
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("WebView Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            accounting: Vec::new(),
            views: HashMap::new(),
            bind_group_layout,
            sampler,
            retirement: SubmissionRetirementQueue::for_device(device.clone()),
        }
    }

    /// Get the bind group layout for texture rendering.
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Update or create a cached view from DmaBufBuffer.
    pub fn update_view<R: Send + 'static>(
        &mut self,
        view_id: WebViewId,
        buffer: DmaBufBuffer,
        retained_frame: R,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        let source = match buffer.to_external_wgpu_texture(device, queue) {
            Some(t) => t,
            None => {
                tracing::warn!("Failed to import DMA-BUF for view {}", view_id);
                return false;
            }
        };

        // WPE may recycle the DMA-BUF after the next platform frame. Cache an
        // ordinary wgpu allocation instead of retaining a texture that aliases
        // browser-owned memory.
        let (width, height) = buffer.dimensions();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("WebView Cache-Owned Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: source.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("WebView Browser Frame Copy"),
        });
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            texture.as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit(Some(encoder.finish()));
        // `source`, its descriptor-owned file handles, and WPE's native frame
        // lease all remain alive until this exact copy retires. Waiting happens
        // on the cache's FIFO worker, never on the render or WPE reactor thread.
        self.retirement
            .retire_after(submission, (source, buffer, retained_frame));

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WebView Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.accounting
            .push(crate::media_budget::MediaAccounting::Registered {
                media_type: crate::media_budget::MediaType::WebKit,
                id: view_id.get(),
                size_bytes: (width as usize) * (height as usize) * 4,
            });
        self.views.insert(
            view_id,
            CachedWebView {
                texture,
                view,
                bind_group,
                width,
                height,
                last_updated: Instant::now(),
                _ownership: CacheOwnedTexture,
            },
        );

        true
    }

    /// Update or create a cached view from raw pixel data (fallback path).
    /// Used when DMA-BUF import fails (e.g., incompatible modifier).
    pub fn update_view_from_pixels(
        &mut self,
        view_id: WebViewId,
        width: u32,
        height: u32,
        pixels: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        use wgpu::util::DeviceExt;

        // Validate pixel data size (BGRA = 4 bytes per pixel)
        let expected_size = (width * height * 4) as usize;
        if pixels.len() < expected_size {
            tracing::warn!(
                "update_view_from_pixels: pixel data too small ({} < {})",
                pixels.len(),
                expected_size
            );
            return false;
        }

        // Create texture
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("WebView Pixel Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &pixels[..expected_size],
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WebView Pixel Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.accounting
            .push(crate::media_budget::MediaAccounting::Registered {
                media_type: crate::media_budget::MediaType::WebKit,
                id: view_id.get(),
                size_bytes: (width as usize) * (height as usize) * 4,
            });
        self.views.insert(
            view_id,
            CachedWebView {
                texture,
                view,
                bind_group,
                width,
                height,
                last_updated: Instant::now(),
                _ownership: CacheOwnedTexture,
            },
        );

        tracing::info!(
            "update_view_from_pixels: successfully uploaded {}x{} texture for view {}",
            width,
            height,
            view_id
        );
        true
    }

    /// Get a cached view.
    pub fn get(&self, view_id: WebViewId) -> Option<&CachedWebView> {
        self.views.get(&view_id)
    }

    /// Get bind group for a view.
    pub fn get_bind_group(&self, view_id: WebViewId) -> Option<&wgpu::BindGroup> {
        self.views.get(&view_id).map(|v| &v.bind_group)
    }

    /// Remove a view.
    pub fn remove(&mut self, view_id: WebViewId) {
        if self.views.remove(&view_id).is_some() {
            self.accounting
                .push(crate::media_budget::MediaAccounting::Freed {
                    media_type: crate::media_budget::MediaType::WebKit,
                    id: view_id.get(),
                });
        }
    }

    /// Drain budget accounting events accumulated since the last call.
    pub fn drain_accounting(&mut self) -> Vec<crate::media_budget::MediaAccounting> {
        std::mem::take(&mut self.accounting)
    }

    /// Clear all cached views.
    pub fn clear(&mut self) {
        self.views.clear();
    }
}
