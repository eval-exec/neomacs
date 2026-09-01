//! Media methods for WgpuRenderer.

use super::super::image_cache::ImageCache;
use super::WgpuRenderer;
use neomacs_display_protocol::{
    ImageColorContext, ImageFrameIndex, ImageId, ImageLoadToken, ImageMaskPolicy, ImageRealization,
    ImageRotation, ImageSequenceId, ImageSequenceRetirement, ImageSizeSpec,
};

impl WgpuRenderer {
    /// Exact image texture and decoded animation-sequence bytes currently
    /// retained by the renderer.
    pub fn image_cache_usage(&self) -> neomacs_display_protocol::ImageCacheUsage {
        self.caches.image.memory_usage()
    }

    /// Load image from file path (async - returns immediately)
    /// Returns image ID, actual texture loads in background
    pub fn load_image_file(
        &mut self,
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
    ) -> ImageId {
        let raster_scale = self.scale_factor;
        self.caches
            .image
            .load_file(path, size, rotation, colors, raster_scale)
    }

    /// Load image from file path with a pre-allocated ID (for threaded mode)
    pub fn load_image_file_with_id(
        &mut self,
        load: ImageLoadToken,
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
    ) {
        self.caches.image.load_file_with_id(
            load,
            path,
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
            sequence,
        )
    }

    /// Load image from data (async - returns immediately)
    pub fn load_image_data(
        &mut self,
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
    ) -> ImageId {
        let raster_scale = self.scale_factor;
        self.caches
            .image
            .load_data(data, size, rotation, colors, raster_scale)
    }

    /// Load image from data with pre-allocated ID (for threaded mode)
    pub fn load_image_data_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        resources: crate::SvgResourceContext,
    ) {
        self.caches.image.load_data_with_id(
            load,
            data,
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
            sequence,
            resources,
        )
    }

    /// Invalidate CPU-side animation decoder/compositor state independently
    /// from frame textures.
    pub fn retire_image_sequence(&mut self, retirement: ImageSequenceRetirement) {
        self.caches.image.retire_sequence(retirement);
    }

    /// Load image from raw ARGB32 pixel data
    pub fn load_image_argb32(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> ImageId {
        self.caches.image.load_raw_argb32(
            data,
            width,
            height,
            stride,
            ImageSizeSpec::default(),
            ImageRotation::None,
        )
    }

    /// Load image from raw RGB24 pixel data
    pub fn load_image_rgb24(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> ImageId {
        self.caches.image.load_raw_rgb24(
            data,
            width,
            height,
            stride,
            ImageSizeSpec::default(),
            ImageRotation::None,
        )
    }

    /// Load image from raw ARGB32 pixel data with pre-allocated ID (for threaded mode)
    pub fn load_image_argb32_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) {
        self.caches
            .image
            .load_raw_argb32_with_id(load, data, width, height, stride)
    }

    /// Load image from raw RGB24 pixel data with pre-allocated ID (for threaded mode)
    pub fn load_image_rgb24_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) {
        self.caches
            .image
            .load_raw_rgb24_with_id(load, data, width, height, stride)
    }

    /// Query image file dimensions for a pending-image placeholder.
    pub fn query_image_file_size(path: &str) -> Option<(u32, u32)> {
        ImageCache::query_file_dimensions(path).map(|extent| extent.dimensions())
    }

    /// Query image data dimensions for a pending-image placeholder.
    pub fn query_image_data_size(data: &[u8]) -> Option<(u32, u32)> {
        ImageCache::query_data_dimensions(data).map(|extent| extent.dimensions())
    }

    /// Get image dimensions (works for pending and loaded images)
    pub fn get_image_size(&self, image: ImageId) -> Option<(u32, u32)> {
        self.caches
            .image
            .get_dimensions(image)
            .map(|extent| extent.dimensions())
    }

    /// Check if image is ready for rendering
    pub fn is_image_ready(&self, image: ImageId) -> bool {
        self.caches.image.is_ready(image)
    }

    pub fn has_pending_images(&self) -> bool {
        self.caches.image.has_pending()
    }

    /// Begin presentation-safe retirement of an image texture.
    pub fn retire_image(&mut self, image: ImageId) {
        self.caches.image.retire(image)
    }

    /// Replace the complete set of image identities retained by render
    /// presentations, releasing any retirement whose last reference vanished.
    pub fn synchronize_retained_images(
        &mut self,
        retained: neomacs_display_protocol::RetainedImageSet,
    ) {
        self.caches.image.synchronize_retained_images(retained);
    }

    /// Process pending decoded images (call each frame before rendering)
    pub fn process_pending_images(&mut self) -> Vec<crate::ImageCacheEvent> {
        self.caches.image.process_pending(&self.device, &self.queue)
    }

    /// Load video from file path (async - returns immediately)
    /// Returns video ID, frames decode in background
    #[cfg(feature = "video")]
    pub fn load_video_file(&mut self, path: &str) -> u32 {
        self.caches.video.load_file(path)
    }

    /// Load video from file path with a pre-allocated ID.
    #[cfg(feature = "video")]
    pub fn load_video_file_with_id(
        &mut self,
        id: u32,
        path: &str,
        loop_count: i32,
        autoplay: bool,
    ) {
        self.caches
            .video
            .load_file_with_id(id, path, loop_count, autoplay);
    }

    /// Load video from URI with a pre-allocated ID.
    #[cfg(feature = "video")]
    pub fn load_video_uri_with_id(&mut self, id: u32, uri: &str, loop_count: i32, autoplay: bool) {
        self.caches
            .video
            .load_uri_with_id(id, uri, loop_count, autoplay);
    }

    /// Get video dimensions
    #[cfg(feature = "video")]
    pub fn get_video_size(&self, id: u32) -> Option<(u32, u32)> {
        self.caches.video.get_dimensions(id)
    }

    /// Get video state
    #[cfg(feature = "video")]
    pub fn get_video_state(&self, id: u32) -> Option<super::super::video_cache::VideoState> {
        self.caches.video.get_state(id)
    }

    /// Play video
    #[cfg(feature = "video")]
    pub fn video_play(&mut self, id: u32) {
        self.caches.video.play(id)
    }

    /// Pause video
    #[cfg(feature = "video")]
    pub fn video_pause(&mut self, id: u32) {
        self.caches.video.pause(id)
    }

    /// Stop video
    #[cfg(feature = "video")]
    pub fn video_stop(&mut self, id: u32) {
        self.caches.video.stop(id)
    }

    /// Set video loop count (-1 for infinite)
    #[cfg(feature = "video")]
    pub fn video_set_loop(&mut self, id: u32, count: i32) {
        self.caches.video.set_loop(id, count)
    }

    /// Free a video from cache
    #[cfg(feature = "video")]
    pub fn free_video(&mut self, id: u32) {
        self.caches.video.remove(id)
    }

    #[cfg(feature = "video")]
    pub fn process_pending_videos_at(
        &mut self,
        now: std::time::Instant,
        presented: &std::collections::HashSet<neomacs_display_protocol::types::VideoId>,
    ) -> &neomacs_video::VideoServiceResult {
        self.caches.video.process_pending(now, presented)
    }

    #[cfg(feature = "video")]
    pub fn video_recovery_manifests(
        &self,
    ) -> Vec<super::super::video_cache::VideoRecoveryManifest> {
        self.caches.video.recovery_manifests()
    }

    #[cfg(feature = "video")]
    pub fn restore_videos_after_device_loss(
        &mut self,
        manifests: Vec<super::super::video_cache::VideoRecoveryManifest>,
    ) {
        self.caches.video.restore_after_device_loss(manifests);
    }

    /// Get cached video for rendering
    #[cfg(feature = "video")]
    pub fn get_video(&self, id: u32) -> Option<&super::super::video_cache::CachedVideo> {
        self.caches.video.get(id)
    }

    // =========== Shader surfaces (doc/display-engine/SHADER_SURFACES.md) ===========

    /// Create a shader surface from user WGSL. The composite bind group uses
    /// the image cache's layout/sampler so the inline-media phase can draw it
    /// with the shared image pipeline. Texture is allocated at physical
    /// resolution (logical size x current scale factor).
    #[allow(clippy::too_many_arguments)]
    pub fn create_shader_surface(
        &mut self,
        id: u32,
        language: crate::shader_surface::SurfaceShaderLanguage,
        user_source: &str,
        uniforms: &[crate::shader_surface::SurfaceUniformInit],
        width: u32,
        height: u32,
        animate: bool,
        fps: Option<u32>,
        channel0: Option<crate::shader_surface::SurfaceChannelSource>,
        recreatable: bool,
    ) -> Result<(), String> {
        let layout = self.caches.image.bind_group_layout();
        let sampler = self.caches.image.sampler();
        let (width_px, height_px) = self.caches.surface.create_shader(
            &self.device,
            layout,
            sampler,
            self.surface_format,
            id,
            language,
            user_source,
            uniforms,
            width,
            height,
            self.scale_factor,
            animate,
            fps,
            channel0,
        )?;
        self.register_surface_bytes(id, width_px, height_px, recreatable);
        Ok(())
    }

    /// Create a static surface from raw RGBA8 pixel data. Pixel surfaces are
    /// only created through the imperative API, so they are never
    /// budget-evictable.
    pub fn create_pixel_surface(
        &mut self,
        id: u32,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let layout = self.caches.image.bind_group_layout();
        let sampler = self.caches.image.sampler();
        let (width_px, height_px) = self.caches.surface.create_pixels(
            &self.device,
            &self.queue,
            layout,
            sampler,
            id,
            data,
            width,
            height,
        )?;
        self.register_surface_bytes(id, width_px, height_px, false);
        Ok(())
    }

    /// Register a created surface's physical bytes and run the eviction
    /// driver: over budget, free least-recently-used *recreatable* surfaces
    /// (declarative specs re-resolve on the next redisplay walk). One victim
    /// per iteration so the shrinking shortfall requeries the candidate
    /// prefix; never the surface just created.
    pub(super) fn register_surface_bytes(
        &mut self,
        id: u32,
        width_px: u32,
        height_px: u32,
        recreatable: bool,
    ) {
        use crate::media_budget::MediaType;
        let bytes = width_px as usize * height_px as usize * 4;
        self.media_budget.register(MediaType::Surface, id, bytes);
        self.surface_recreatable.insert(id, recreatable);
        while self.media_budget.is_over_budget() {
            let victim = self
                .media_budget
                .get_eviction_candidates(0)
                .into_iter()
                .find(|(kind, victim_id)| {
                    *kind == MediaType::Surface
                        && *victim_id != id
                        && self.surface_recreatable.get(victim_id) == Some(&true)
                });
            let Some((_, victim_id)) = victim else {
                tracing::debug!(
                    "media budget over limit ({} / {} bytes) with no \
                     recreatable shader surface to evict",
                    self.media_budget.current_usage(),
                    self.media_budget.max_limit()
                );
                break;
            };
            self.caches.surface.free(victim_id);
            self.media_budget.unregister(MediaType::Surface, victim_id);
            self.surface_recreatable.remove(&victim_id);
            tracing::info!("evicting shader surface {victim_id} (over media budget)");
        }
    }

    /// Set the unified media budget limit (bytes).
    pub fn set_media_budget_limit(&mut self, max_bytes: usize) {
        self.media_budget = crate::media_budget::MediaBudget::with_max_bytes(max_bytes);
    }

    /// Drain the caches' accounting events into the budget. Runs once per
    /// frame from `process_shader_surfaces`.
    fn reconcile_media_budget(&mut self) {
        for event in self.caches.image.drain_accounting() {
            self.media_budget.apply(event);
        }
        #[cfg(feature = "video")]
        for event in self.caches.video.drain_accounting() {
            self.media_budget.apply(event);
        }
        #[cfg(all(feature = "webview", target_os = "linux"))]
        for event in self.caches.webview.drain_accounting() {
            self.media_budget.apply(event);
        }
    }

    /// Update one named uniform on a shader surface.
    pub fn set_surface_uniform(&mut self, id: u32, name: &str, value: [f32; 4]) {
        self.caches.surface.set_uniform(id, name, value);
    }

    /// Route a button press over a surface glyph into its `iMouse.zw`
    /// (Shadertoy click state). `u`/`v` are the press position normalized
    /// inside the composited quad (top-left origin), mapped like hover's
    /// `set_mouse_uv`; zw stay positive until `surface_mouse_release`.
    pub fn surface_mouse_press(&mut self, id: u32, u: f32, v: f32) {
        self.caches.surface.set_mouse_press_uv(id, u, v);
    }

    /// End the click on whichever surface is pressed (negates its
    /// `iMouse.zw`). Safe to call on every button release.
    pub fn surface_mouse_release(&mut self) {
        self.caches.surface.set_mouse_release();
    }

    /// Free a shader surface's GPU objects.
    pub fn free_surface(&mut self, id: u32) {
        self.caches.surface.free(id);
        self.media_budget
            .unregister(crate::media_budget::MediaType::Surface, id);
        self.surface_recreatable.remove(&id);
    }

    /// Render pending shader-surface passes (call each frame before the main
    /// pass samples the surface textures). Image/video channels resolve here,
    /// where every cache is visible; missing/not-ready sources fall back to
    /// transparent black inside the surface cache.
    pub fn process_shader_surfaces(&mut self) {
        use crate::shader_surface::SurfaceChannelSource;
        self.reconcile_media_budget();
        let sources = self.caches.surface.external_channel_sources();
        #[cfg(feature = "video")]
        let video_views = self.caches.video.prepare_channel_views(
            sources.iter().filter_map(|source| match source {
                SurfaceChannelSource::Video(id) => {
                    Some(neomacs_display_protocol::types::VideoId::new(*id))
                }
                SurfaceChannelSource::Surface(_) | SurfaceChannelSource::Image(_) => None,
            }),
            &self.device,
            &self.queue,
            &self.pipelines.bi_planar_video_copy,
            &self.uniform_bind_group,
        );
        let mut external = std::collections::HashMap::new();
        for source in sources {
            let view = match source {
                SurfaceChannelSource::Surface(_) => None,
                SurfaceChannelSource::Image(id) => {
                    self.caches.image.get(id).map(|cached| cached.view.clone())
                }
                #[cfg(feature = "video")]
                SurfaceChannelSource::Video(id) => video_views
                    .get(&neomacs_display_protocol::types::VideoId::new(id))
                    .cloned(),
                #[cfg(not(feature = "video"))]
                SurfaceChannelSource::Video(_) => None,
            };
            if let Some(view) = view {
                external.insert(source, view);
            }
        }
        self.caches
            .surface
            .render_pending(&self.device, &self.queue, &external);
    }

    /// Whether any animated shader surface was composited recently — drives
    /// `DemandReason::ShaderSurface` pacing. A frame post shader is always
    /// animated while installed.
    pub fn has_active_shader_surfaces(&self) -> bool {
        self.caches.surface.has_active_surfaces() || self.frame_post.is_some()
    }

    /// The cadence the `DemandReason::ShaderSurface` demand should run at,
    /// given the window's display refresh as the ceiling. The frame post
    /// shader re-shades the whole composited frame (cursor blink, transitions)
    /// so it forces the full rate; otherwise the rate is the max of the active
    /// surfaces' `:fps` caps (uncapped active surfaces also force full rate).
    /// Result is clamped to `[1, display_rate]`.
    pub fn shader_surface_demand_rate(&self, display_rate: u32) -> u32 {
        let ceiling = display_rate.max(1);
        if self.frame_post.is_some() {
            return ceiling;
        }
        match self.caches.surface.active_animation_max_fps() {
            Some(cap) => cap.clamp(1, ceiling),
            None => ceiling,
        }
    }

    // =========== Frame post shader (doc/display-engine/SHADER_SURFACES.md) ===========

    /// Install (or replace) the full-frame post shader from an
    /// already-composed WGSL module (the host validates + composes on the
    /// Lisp thread, uniform accessors included; `uniforms` carries the
    /// name -> slot table and initial values in slot order).
    pub fn set_frame_post(
        &mut self,
        language: crate::shader_surface::SurfaceShaderLanguage,
        composed_source: &str,
        uniforms: &[crate::shader_surface::SurfaceUniformInit],
    ) -> Result<(), String> {
        let post = crate::frame_post::FramePost::new(
            &self.device,
            self.surface_format,
            language,
            composed_source,
            uniforms,
        )?;
        self.frame_post = Some(post);
        tracing::info!("frame post shader installed");
        Ok(())
    }

    /// Update one named uniform on the installed frame post shader (cheap;
    /// no recompile). Unknown names warn inside [`crate::frame_post::FramePost::set_uniform`];
    /// no shader installed warns here — the host errors to Lisp before this
    /// point, so hitting it means install/remove raced ahead in the queue.
    pub fn set_frame_post_uniform(&mut self, name: &str, value: [f32; 4]) {
        match self.frame_post.as_mut() {
            Some(post) => post.set_uniform(name, value),
            None => tracing::warn!("set_frame_post_uniform: no frame post shader installed"),
        }
    }

    /// Remove the full-frame post shader.
    pub fn clear_frame_post(&mut self) {
        if self.frame_post.take().is_some() {
            tracing::info!("frame post shader removed");
        }
    }

    /// Whether a frame post shader is installed.
    pub fn has_frame_post(&self) -> bool {
        self.frame_post.is_some()
    }

    /// Run the post pass: shade `src_view` (the rendered frame) into
    /// `dst_view`. `mouse` is the pointer in logical px (y-down, window
    /// coords); converted here to the physical y-up contract.
    pub fn frame_post_to_view(
        &mut self,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        width_px: u32,
        height_px: u32,
        mouse: (f32, f32),
    ) {
        let scale = self.scale_factor;
        let mouse_px = (
            (mouse.0 * scale).clamp(0.0, width_px as f32),
            (height_px as f32 - mouse.1 * scale).clamp(0.0, height_px as f32),
        );
        if let Some(post) = self.frame_post.as_mut() {
            post.run(
                &self.device,
                &self.queue,
                src_view,
                dst_view,
                width_px,
                height_px,
                scale,
                mouse_px,
            );
        }
    }

    /// Update a WebView in the cache from a DMA-BUF buffer.
    /// Returns true if successful.
    #[cfg(all(feature = "webview", target_os = "linux"))]
    pub fn update_webview_dmabuf<R: Send + 'static>(
        &mut self,
        view_id: neomacs_display_protocol::WebViewId,
        buffer: super::super::external_buffer::DmaBufBuffer,
        retained_frame: R,
    ) -> bool {
        self.caches
            .webview
            .update_view(view_id, buffer, retained_frame, &self.device, &self.queue)
    }

    /// Update a WebView in the cache from pixel data.
    /// Returns true if successful.
    #[cfg(all(feature = "webview", target_os = "linux"))]
    pub fn update_webview_pixels(
        &mut self,
        view_id: neomacs_display_protocol::WebViewId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        self.caches.webview.update_view_from_pixels(
            view_id,
            width,
            height,
            pixels,
            &self.device,
            &self.queue,
        )
    }

    /// Remove a WebView from the cache.
    #[cfg(all(feature = "webview", target_os = "linux"))]
    pub fn remove_webview(&mut self, view_id: neomacs_display_protocol::WebViewId) {
        self.caches.webview.remove(view_id);
    }
}
