use super::RenderApp;
#[cfg(feature = "neo-term")]
use super::frame_windows::GuiFrameRenderState;
#[cfg(feature = "neo-term")]
use crate::core::face::{BoxType, Face, FaceAttributes, UnderlineStyle};
#[cfg(feature = "neo-term")]
use crate::core::frame_glyphs::{DisplaySlotId, FrameGlyph, FrameGlyphBuffer, GlyphRowRole};
#[cfg(feature = "neo-term")]
use crate::core::types::DisplayWindowId;
#[cfg(feature = "neo-term")]
use crate::core::types::{Color, FaceId, Px};
#[cfg(any(
    feature = "neo-term",
    all(feature = "wpe-webkit", wpe_platform_available)
))]
use crate::thread_comm::InputEvent;
#[cfg(feature = "neo-term")]
use std::collections::HashMap;

#[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
use crate::backend::wpe::sys::platform as plat;
#[cfg(feature = "wpe-webkit")]
use crate::render_thread::state::WebKitImportPolicy;
#[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
use neomacs_renderer_wgpu::WgpuRenderer;

impl RenderApp {
    #[cfg(feature = "neo-term")]
    fn expanded_terminal_glyphs_for_frame(
        frame: &FrameGlyphBuffer,
        terminal_contents: &HashMap<crate::terminal::TerminalId, crate::terminal::TerminalContent>,
    ) -> (Vec<FrameGlyph>, HashMap<FaceId, Face>) {
        let cell_w = frame.char_width;
        let cell_h = frame.char_height;
        let font_size = frame.font_pixel_size;
        let ascent = cell_h * 0.8;
        let mut extra_glyphs = Vec::new();
        let mut extra_faces = HashMap::new();

        for glyph in &frame.glyphs {
            let FrameGlyph::Terminal {
                terminal_id,
                x,
                y,
                width,
                height,
            } = glyph
            else {
                continue;
            };
            let Some(content) = terminal_contents.get(terminal_id) else {
                continue;
            };

            extra_glyphs.push(FrameGlyph::Stretch {
                window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                row_role: GlyphRowRole::Text,
                clip_rect: None,
                slot_id: DisplaySlotId::from_pixels(
                    DisplayWindowId::new(0),
                    Px(*x),
                    Px(*y),
                    Px(cell_w),
                    Px(cell_h),
                ),
                bidi_level: 0,
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                bg: content.default_bg,
                face_id: FaceId::new(0),
            });

            Self::expand_terminal_cells(
                content,
                *x,
                *y,
                cell_w,
                cell_h,
                ascent,
                font_size,
                false,
                1.0,
                &mut extra_glyphs,
                &mut extra_faces,
            );
        }

        (extra_glyphs, extra_faces)
    }

    #[cfg(feature = "neo-term")]
    fn expand_terminal_glyphs_for_render_state(
        render: &mut GuiFrameRenderState,
        terminal_contents: &HashMap<crate::terminal::TerminalId, crate::terminal::TerminalContent>,
    ) {
        let Some(frame) = render.compositor.current_frame.as_ref() else {
            return;
        };
        let (extra_glyphs, extra_faces) =
            Self::expanded_terminal_glyphs_for_frame(frame, terminal_contents);
        render.extend_current_frame_glyphs_and_faces(extra_glyphs, extra_faces);
    }

    #[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
    pub(super) fn pump_glib(&mut self) {
        unsafe {
            // WPEViewHeadless attaches to thread-default context.
            // Do NOT fall back to g_main_context_default() — the Emacs main
            // thread dispatches that via xg_select(), and iterating it here
            // races with pselect() causing EBADF crashes.
            let thread_ctx = plat::g_main_context_get_thread_default();
            if !thread_ctx.is_null() {
                while plat::g_main_context_iteration(thread_ctx, 0) != 0 {}
            }
        }

        // Update all webkit views and send state change events
        for (id, view) in self.webkit_views.iter_mut() {
            let old_title = view.title.clone();
            let old_url = view.url.clone();
            let old_progress = view.progress;

            view.update();

            // Send state change events
            if view.title != old_title
                && let Some(ref title) = view.title
            {
                self.comms.send_input(InputEvent::WebKitTitleChanged {
                    id: *id,
                    title: title.clone(),
                });
            }
            if view.url != old_url {
                self.comms.send_input(InputEvent::WebKitUrlChanged {
                    id: *id,
                    url: view.url.clone(),
                });
            }
            if (view.progress - old_progress).abs() > 0.01 {
                self.comms.send_input(InputEvent::WebKitProgressChanged {
                    id: *id,
                    progress: view.progress,
                });
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", wpe_platform_available)))]
    pub(super) fn pump_glib(&mut self) {}

    /// Process webkit frames and import to wgpu textures
    #[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
    pub(super) fn process_webkit_frames(&mut self) {
        use crate::backend::wpe::DmaBufData;
        use neomacs_renderer_wgpu::DmaBufBuffer;
        use std::os::fd::{FromRawFd, OwnedFd};

        // Get mutable reference to renderer - we need to update its internal webkit cache
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => {
                tracing::trace!("process_webkit_frames: no renderer available");
                return;
            }
        };

        if self.webkit_views.is_empty() {
            tracing::trace!("process_webkit_frames: no webkit views");
            return;
        }

        let policy = self.webkit_import_policy.effective();

        let try_upload_dmabuf =
            |renderer: &mut WgpuRenderer, view_id: u32, dmabuf: DmaBufData| -> bool {
                let num_planes = dmabuf.fds.len().min(4) as u32;
                let mut fds: [Option<OwnedFd>; 4] = [None, None, None, None];
                let mut strides = [0u32; 4];
                let mut offsets = [0u32; 4];

                let n = num_planes as usize;
                // `DmaBufData` carries fds already dup'd for our ownership by
                // `take_latest_dmabuf`, and has no Drop of its own. Adopt each
                // into an OwnedFd so the DmaBufBuffer closes them on drop —
                // previously these descriptors leaked once copied out as raw ints.
                for (slot, &raw) in fds[..n].iter_mut().zip(&dmabuf.fds[..n]) {
                    if raw >= 0 {
                        *slot = Some(unsafe { OwnedFd::from_raw_fd(raw) });
                    }
                }
                strides[..n].copy_from_slice(&dmabuf.strides[..n]);
                offsets[..n].copy_from_slice(&dmabuf.offsets[..n]);

                let buffer = DmaBufBuffer::new(
                    fds,
                    strides,
                    offsets,
                    num_planes,
                    dmabuf.width,
                    dmabuf.height,
                    dmabuf.fourcc,
                    dmabuf.modifier,
                );

                renderer.update_webkit_view_dmabuf(view_id, buffer)
            };

        for (view_id, view) in &self.webkit_views {
            match policy {
                WebKitImportPolicy::DmaBufFirst => {
                    if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            // Discard pending pixel fallback when DMA-BUF succeeds.
                            let _ = view.take_latest_pixels();
                            tracing::debug!(
                                "Imported DMA-BUF for webkit view {} (dmabuf-first)",
                                view_id
                            );
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(
                                *view_id,
                                raw_pixels.width,
                                raw_pixels.height,
                                &raw_pixels.pixels,
                            ) {
                                tracing::debug!(
                                    "Uploaded pixels for webkit view {} (dmabuf-first fallback)",
                                    view_id
                                );
                            } else {
                                tracing::warn!(
                                    "Both DMA-BUF and pixel upload failed for webkit view {}",
                                    view_id
                                );
                            }
                        } else {
                            tracing::warn!(
                                "Both DMA-BUF import and pixel fallback unavailable for webkit view {}",
                                view_id
                            );
                        }
                    } else if let Some(raw_pixels) = view.take_latest_pixels()
                        && renderer.update_webkit_view_pixels(
                            *view_id,
                            raw_pixels.width,
                            raw_pixels.height,
                            &raw_pixels.pixels,
                        )
                    {
                        tracing::debug!(
                            "Uploaded pixels for webkit view {} (dmabuf-first: no dmabuf frame)",
                            view_id
                        );
                    }
                }
                WebKitImportPolicy::PixelsFirst | WebKitImportPolicy::Auto => {
                    // Prefer pixel upload over DMA-BUF zero-copy.
                    //
                    // wgpu's create_texture_from_hal() always inserts textures with
                    // UNINITIALIZED tracking state, causing a second UNDEFINED layout
                    // transition that discards DMA-BUF content on AMD RADV (and
                    // potentially other drivers with compressed modifiers like DCC/CCS).
                    // Until wgpu supports pre-initialized HAL textures, pixel upload
                    // via wpe_buffer_import_to_pixels() is the reliable path.
                    if let Some(raw_pixels) = view.take_latest_pixels() {
                        // Drain any pending DMA-BUF so it doesn't accumulate
                        let _ = view.take_latest_dmabuf();
                        if renderer.update_webkit_view_pixels(
                            *view_id,
                            raw_pixels.width,
                            raw_pixels.height,
                            &raw_pixels.pixels,
                        ) {
                            tracing::debug!("Uploaded pixels for webkit view {}", view_id);
                        }
                    }
                    // DMA-BUF zero-copy fallback (only if no pixel data available)
                    else if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            tracing::debug!(
                                "Imported DMA-BUF for webkit view {} (pixels-first fallback)",
                                view_id
                            );
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(
                                *view_id,
                                raw_pixels.width,
                                raw_pixels.height,
                                &raw_pixels.pixels,
                            ) {
                                tracing::debug!(
                                    "Uploaded pixels for webkit view {} (pixels-first second fallback)",
                                    view_id
                                );
                            } else {
                                tracing::warn!(
                                    "Both pixel and DMA-BUF import failed for webkit view {}",
                                    view_id
                                );
                            }
                        } else {
                            tracing::warn!(
                                "Both pixel and DMA-BUF import failed for webkit view {}",
                                view_id
                            );
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", target_os = "linux")))]
    pub(super) fn process_webkit_frames(&mut self) {}

    /// Process pending video frames
    #[cfg(feature = "video")]
    pub(super) fn process_video_frames(&mut self) {
        tracing::trace!("process_video_frames called");
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_pending_videos();
        }
    }

    #[cfg(not(feature = "video"))]
    pub(super) fn process_video_frames(&mut self) {}

    /// Check if any video is currently playing (needs continuous rendering)
    #[cfg(feature = "video")]
    pub(super) fn has_playing_videos(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.has_playing_videos())
    }

    #[cfg(not(feature = "video"))]
    pub(super) fn has_playing_videos(&self) -> bool {
        false
    }

    /// Render pending shader-surface passes (call each frame before the main
    /// pass samples the surface textures).
    pub(super) fn process_shader_surfaces(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_shader_surfaces();
        }
    }

    /// Check if any animated shader surface was composited recently (needs
    /// continuous rendering while visible).
    pub(super) fn has_active_shader_surfaces(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.has_active_shader_surfaces())
    }

    /// The cadence the shader-surface demand should run at (max of active
    /// `:fps` caps, else the display rate); see
    /// `WgpuRenderer::shader_surface_demand_rate`.
    pub(super) fn shader_surface_demand_rate(&self, display_rate: u32) -> u32 {
        self.renderer
            .as_ref()
            .map_or(display_rate, |r| r.shader_surface_demand_rate(display_rate))
    }

    /// Check if any WebKit view needs redraw
    #[cfg(feature = "wpe-webkit")]
    pub(super) fn has_webkit_needing_redraw(&self) -> bool {
        self.webkit_views.values().any(|v| v.needs_redraw())
    }

    #[cfg(not(feature = "wpe-webkit"))]
    pub(super) fn has_webkit_needing_redraw(&self) -> bool {
        false
    }

    /// Check if any terminal has pending content from PTY reader threads.
    #[cfg(feature = "neo-term")]
    pub(super) fn has_terminal_activity(&self) -> bool {
        for view in self.terminal_manager.terminals.values() {
            if view.event_proxy.peek_wakeup() || view.dirty {
                return true;
            }
        }
        false
    }

    #[cfg(not(feature = "neo-term"))]
    pub(super) fn has_terminal_activity(&self) -> bool {
        false
    }

    /// Process pending image uploads (decode → GPU texture)
    pub(super) fn process_pending_images(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            for outcome in renderer.process_pending_images() {
                let (id, terminal) = match outcome {
                    neomacs_renderer_wgpu::ImageDecodeOutcome::Ready { id, metadata } => {
                        let metadata =
                            neovm_core::emacs_core::image_catalog::ResolvedImageMetadata {
                                width: metadata.width,
                                height: metadata.height,
                                background: metadata.background,
                                background_transparent: metadata.background_transparent,
                            };
                        (id, super::ImageDecodeTerminal::Ready(metadata))
                    }
                    neomacs_renderer_wgpu::ImageDecodeOutcome::Failed { id, error } => {
                        (id, super::ImageDecodeTerminal::Failed(error))
                    }
                };
                let (lock, cvar) = &*self.image_metadata;
                match lock.lock() {
                    Ok(mut images) => {
                        images.insert(id, terminal.clone());
                    }
                    Err(poisoned) => {
                        poisoned.into_inner().insert(id, terminal);
                    }
                }
                cvar.notify_all();
                self.comms
                    .send_input(crate::thread_comm::InputEvent::ImageStateChanged { id });
            }
        }
    }

    pub(super) fn has_pending_images(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|renderer| renderer.has_pending_images())
    }

    /// Update terminal content and expand Terminal glyphs into renderable cells.
    #[cfg(feature = "neo-term")]
    pub(super) fn update_terminals(&mut self) {
        use crate::terminal::TerminalMode;

        // Get frame font metrics for terminal cell sizing.
        // These come from FRAME_COLUMN_WIDTH / FRAME_LINE_HEIGHT / FRAME_FONT->pixel_size.
        let (cell_w, cell_h, font_size, frame_w, frame_h) = if let Some(frame) = self
            .frame_windows
            .primary_window()
            .and_then(|ws| ws.render.compositor.current_frame.as_ref())
        {
            (
                frame.char_width,
                frame.char_height,
                frame.font_pixel_size,
                frame.width,
                frame.height,
            )
        } else {
            let (frame_w, frame_h) = self
                .frame_windows
                .primary_window()
                .map_or((0.0, 0.0), |ws| {
                    let (w, h) = ws.native_size();
                    let s = ws.scale_factor() as f32;
                    (w as f32 / s, h as f32 / s)
                });
            (8.0, 16.0, 14.0, frame_w, frame_h)
        };
        let ascent = cell_h * 0.8;

        // Auto-resize Window-mode terminals to fit the frame area.
        // Reserve space for mode-line (~1 row) and echo area (~1 row).
        let term_area_height = (frame_h - cell_h * 2.0).max(cell_h);
        let target_cols = (frame_w / cell_w).floor() as u16;
        let target_rows = (term_area_height / cell_h).floor() as u16;

        if target_cols > 0 && target_rows > 0 {
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get_mut(id) {
                    if view.mode != TerminalMode::Window {
                        continue;
                    }
                    // Resize if grid dimensions changed
                    if let Some(content) = view.content()
                        && (content.cols as u16 != target_cols
                            || content.rows as u16 != target_rows)
                    {
                        view.resize(target_cols, target_rows);
                    }
                }
            }
        }

        // Update all terminal content (check for PTY data)
        self.terminal_manager.update_all();

        // Check for exited terminals and notify Emacs
        for id in self.terminal_manager.ids() {
            if let Some(view) = self.terminal_manager.get_mut(id)
                && view.event_proxy.is_exited()
                && !view.exit_notified
            {
                view.exit_notified = true;
                self.comms.send_input(InputEvent::TerminalExited { id });
            }
        }

        let terminal_contents: HashMap<_, _> = self
            .terminal_manager
            .ids()
            .into_iter()
            .filter_map(|id| {
                self.terminal_manager
                    .get(id)
                    .and_then(|view| view.content().map(|content| (id, content.clone())))
            })
            .collect();

        self.frame_windows
            .for_each_top_level_window_mut(|window_state| {
                Self::expand_terminal_glyphs_for_render_state(
                    &mut window_state.render,
                    &terminal_contents,
                );
            });

        if let Some(primary_frame) = self
            .frame_windows
            .primary_window_mut()
            .map(|ws| &mut ws.render)
        {
            Self::expand_terminal_glyphs_for_render_state(primary_frame, &terminal_contents);
        }

        // Render Window-mode terminals as overlays covering the frame body.
        let mut win_glyphs = Vec::new();
        let mut win_faces = HashMap::new();
        for id in self.terminal_manager.ids() {
            if let Some(view) = self.terminal_manager.get(id) {
                if view.mode != TerminalMode::Window {
                    continue;
                }
                if let Some(content) = view.content() {
                    let x = 0.0_f32;
                    let y = 0.0_f32;
                    let width = content.cols as f32 * cell_w;
                    let height = content.rows as f32 * cell_h;

                    // Terminal background
                    win_glyphs.push(FrameGlyph::Stretch {
                        window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                        row_role: GlyphRowRole::ModeLine,
                        clip_rect: None,
                        slot_id: DisplaySlotId::from_pixels(
                            DisplayWindowId::new(0),
                            Px(x),
                            Px(y),
                            Px(cell_w),
                            Px(cell_h),
                        ),
                        bidi_level: 0,
                        x,
                        y,
                        width,
                        height,
                        bg: content.default_bg,
                        face_id: FaceId::new(0),
                    });

                    Self::expand_terminal_cells(
                        content,
                        x,
                        y,
                        cell_w,
                        cell_h,
                        ascent,
                        font_size,
                        true,
                        1.0,
                        &mut win_glyphs,
                        &mut win_faces,
                    );
                }
            }
        }

        if let Some(primary_frame) = self
            .frame_windows
            .primary_window_mut()
            .map(|ws| &mut ws.render)
        {
            primary_frame.extend_current_frame_glyphs_and_faces(win_glyphs, win_faces);
        }

        // Render floating terminals
        let mut float_glyphs = Vec::new();
        let mut float_faces = HashMap::new();
        for id in self.terminal_manager.ids() {
            if let Some(view) = self.terminal_manager.get(id) {
                if view.mode != TerminalMode::Floating {
                    continue;
                }
                if let Some(content) = view.content() {
                    let x = view.float_x;
                    let y = view.float_y;
                    let width = content.cols as f32 * cell_w;
                    let height = content.rows as f32 * cell_h;

                    let mut bg = content.default_bg;
                    bg.a = view.float_opacity;
                    float_glyphs.push(FrameGlyph::Stretch {
                        window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                        row_role: GlyphRowRole::ModeLine,
                        clip_rect: None,
                        slot_id: DisplaySlotId::from_pixels(
                            DisplayWindowId::new(0),
                            Px(x),
                            Px(y),
                            Px(cell_w),
                            Px(cell_h),
                        ),
                        bidi_level: 0,
                        x,
                        y,
                        width,
                        height,
                        bg,
                        face_id: FaceId::new(0),
                    });

                    Self::expand_terminal_cells(
                        content,
                        x,
                        y,
                        cell_w,
                        cell_h,
                        ascent,
                        font_size,
                        true,
                        view.float_opacity,
                        &mut float_glyphs,
                        &mut float_faces,
                    );
                }
            }
        }

        if let Some(primary_frame) = self
            .frame_windows
            .primary_window_mut()
            .map(|ws| &mut ws.render)
        {
            primary_frame.extend_current_frame_glyphs_and_faces(float_glyphs, float_faces);
        }
    }

    /// Expand terminal content cells into FrameGlyph entries.
    ///
    /// Terminal cells carry their own per-cell colors and SGR flags rather than
    /// a GNU face. Since `FrameGlyph::Char` resolves its visual attributes from
    /// the frame face table by `face_id`, each distinct (fg, bold, italic,
    /// underline, strikeout) combination is interned as a synthesized `Face` in
    /// `faces`, and the glyph references it. The synthesized face uses a
    /// transparent background so no per-character background is painted (the
    /// per-cell stretch above and the terminal's default-background stretch
    /// supply the background, exactly as when `Char.bg` was `None`).
    #[cfg(feature = "neo-term")]
    fn expand_terminal_cells(
        content: &crate::terminal::content::TerminalContent,
        origin_x: f32,
        origin_y: f32,
        cell_w: f32,
        cell_h: f32,
        ascent: f32,
        font_size: f32,
        is_overlay: bool,
        opacity: f32,
        out: &mut Vec<FrameGlyph>,
        faces: &mut HashMap<FaceId, Face>,
    ) {
        use rio_vt::crosswords::style::StyleFlags as CellFlags;
        let row_role = if is_overlay {
            GlyphRowRole::ModeLine
        } else {
            GlyphRowRole::Text
        };

        for cell in &content.cells {
            let cx = origin_x + cell.col as f32 * cell_w;
            let cy = origin_y + cell.row as f32 * cell_h;

            if cell.bg != content.default_bg {
                let mut bg = cell.bg;
                bg.a *= opacity;
                out.push(FrameGlyph::Stretch {
                    window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                    row_role,
                    clip_rect: None,
                    slot_id: DisplaySlotId::from_pixels(
                        DisplayWindowId::new(0),
                        Px(cx),
                        Px(cy),
                        Px(cell_w),
                        Px(cell_h),
                    ),
                    bidi_level: 0,
                    x: cx,
                    y: cy,
                    width: cell_w,
                    height: cell_h,
                    bg,
                    face_id: FaceId::new(0),
                });
            }

            if cell.c != ' ' && cell.c != '\0' {
                let mut fg = cell.fg;
                fg.a *= opacity;
                let bold = cell.flags.contains(CellFlags::BOLD);
                let italic = cell.flags.contains(CellFlags::ITALIC);
                let underline = cell.flags.contains(CellFlags::UNDERLINE);
                let strikeout = cell.flags.contains(CellFlags::STRIKEOUT);
                let face_id = terminal_cell_face_id(fg, bold, italic, underline, strikeout);
                faces.entry(face_id).or_insert_with(|| {
                    terminal_cell_face(face_id, fg, bold, italic, underline, strikeout, font_size)
                });
                out.push(FrameGlyph::Char {
                    window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                    row_role,
                    clip_rect: None,
                    slot_id: DisplaySlotId::from_pixels(
                        DisplayWindowId::new(0),
                        Px(cx),
                        Px(cy),
                        Px(cell_w),
                        Px(cell_h),
                    ),
                    bidi_level: 0,
                    char: cell.c,
                    composed: None,
                    x: cx,
                    y: cy,
                    baseline: cy + ascent,
                    width: cell_w,
                    height: cell_h,
                    ascent,
                    face_id,
                });
            }
        }

        // Terminal cursor
        if content.cursor.visible {
            let cx = origin_x + content.cursor.col as f32 * cell_w;
            let cy = origin_y + content.cursor.row as f32 * cell_h;
            let mut fg = content.default_fg;
            fg.a *= opacity;
            out.push(FrameGlyph::Border {
                window_id: neomacs_display_protocol::types::DisplayWindowId::new(0),
                row_role,
                clip_rect: None,
                x: cx,
                y: cy,
                width: cell_w,
                height: cell_h,
                color: fg,
            });
        }
    }
}

/// Base for synthesized terminal-cell face ids. Kept far above any real GNU
/// face id so terminal faces never collide with faces published by layout.
#[cfg(feature = "neo-term")]
const TERMINAL_FACE_ID_BASE: u32 = 0xF000_0000;

/// Deterministic face id for a terminal cell's visual style.
///
/// Encodes the 8-bit-per-channel foreground plus the four SGR flags into the
/// low 28 bits below [`TERMINAL_FACE_ID_BASE`]. Identical styles map to the
/// same id, so equally styled cells share one synthesized face and one glyph
/// atlas cache entry; distinct colors/flags never collide.
#[cfg(feature = "neo-term")]
fn terminal_cell_face_id(
    fg: Color,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
) -> FaceId {
    let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
    let rgb = (to_u8(fg.r) << 16) | (to_u8(fg.g) << 8) | to_u8(fg.b);
    let flags =
        (bold as u32) | ((italic as u32) << 1) | ((underline as u32) << 2) | ((strike as u32) << 3);
    FaceId::new(TERMINAL_FACE_ID_BASE | ((rgb << 4) | flags))
}

/// Synthesize the `Face` for a terminal cell so that
/// [`FrameGlyphBuffer::resolved_face`] returns exactly the colors and
/// decorations the cell glyph used to inline: foreground from the cell,
/// transparent background (no per-character fill), bold via font weight 700,
/// italic/underline/strike-through via attributes.
#[cfg(feature = "neo-term")]
fn terminal_cell_face(
    face_id: FaceId,
    fg: Color,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    font_size: f32,
) -> Face {
    let mut attrs = FaceAttributes::empty();
    if bold {
        attrs |= FaceAttributes::BOLD;
    }
    if italic {
        attrs |= FaceAttributes::ITALIC;
    }
    if underline {
        attrs |= FaceAttributes::UNDERLINE;
    }
    if strike {
        attrs |= FaceAttributes::STRIKE_THROUGH;
    }
    let underline_style = if underline {
        UnderlineStyle::from_gnu_code(1).unwrap_or_default()
    } else {
        UnderlineStyle::None
    };
    Face {
        id: face_id,
        foreground: fg,
        background: Color::TRANSPARENT,
        use_default_foreground: false,
        use_default_background: false,
        underline_color: None,
        overline_color: None,
        strike_through_color: None,
        box_color: None,
        font_family: "monospace".to_string(),
        font_size,
        font_weight: if bold { 700 } else { 400 },
        attributes: attrs,
        underline_style,
        box_type: BoxType::None,
        box_line_width: Default::default(),
        box_corner_radius: 0,
        box_border_style: neomacs_display_protocol::face::BoxBorderStyle::Solid,
        box_border_speed: 1.0,
        box_color2: None,
        font_file_path: None,
        font_ascent: 0,
        font_descent: 0,
        underline_position: 1,
        underline_thickness: 1,
        background_gradient: None,
        lisp_name: None,
        default_resolved_font_id: None,
        stipple: None,
    }
}

#[cfg(test)]
#[cfg(feature = "neo-term")]
mod tests {
    use super::*;
    use crate::core::frame_glyphs::FrameGlyphBuffer;
    use crate::core::types::Color;
    use crate::terminal::content::{RenderCell, RenderCursor, TerminalContent};
    use rio_vt::crosswords::style::StyleFlags as CellFlags;

    #[test]
    fn terminal_glyph_expansion_uses_frame_metrics() {
        let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        frame.font_pixel_size = 18.0;
        frame.glyphs.push(FrameGlyph::Terminal {
            terminal_id: 7,
            x: 30.0,
            y: 40.0,
            width: 50.0,
            height: 20.0,
        });
        let mut contents = HashMap::new();
        contents.insert(
            7,
            TerminalContent {
                cells: vec![RenderCell {
                    col: 1,
                    row: 0,
                    c: 'x',
                    fg: Color::WHITE,
                    bg: Color::BLACK,
                    flags: CellFlags::empty(),
                }],
                cols: 2,
                rows: 1,
                cursor: RenderCursor {
                    col: 0,
                    row: 0,
                    visible: false,
                },
                default_bg: Color::BLACK,
                default_fg: Color::WHITE,
            },
        );

        let (glyphs, faces) = RenderApp::expanded_terminal_glyphs_for_frame(&frame, &contents);

        assert!(matches!(
            glyphs.first(),
            Some(FrameGlyph::Stretch {
                x: 30.0,
                y: 40.0,
                width: 50.0,
                height: 20.0,
                ..
            })
        ));
        // Geometry stays on the glyph; the font size now lives on the
        // synthesized face referenced by the glyph's face_id.
        let Some(FrameGlyph::Char {
            char: ch,
            x,
            y,
            width,
            height,
            face_id,
            ..
        }) = glyphs.get(1)
        else {
            panic!("expected a Char glyph at index 1");
        };
        assert_eq!(*ch, 'x');
        assert_eq!(*x, 40.0);
        assert_eq!(*y, 40.0);
        assert_eq!(*width, 10.0);
        assert_eq!(*height, 20.0);
        assert_eq!(faces.get(face_id).expect("terminal face").font_size, 18.0);
    }

    #[test]
    fn terminal_glyph_expansion_ignores_missing_terminal_content() {
        let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
        frame.glyphs.push(FrameGlyph::Terminal {
            terminal_id: 7,
            x: 30.0,
            y: 40.0,
            width: 50.0,
            height: 20.0,
        });
        let contents = HashMap::new();

        let (glyphs, faces) = RenderApp::expanded_terminal_glyphs_for_frame(&frame, &contents);

        assert!(glyphs.is_empty());
        assert!(faces.is_empty());
    }
}
