//! Frame ingestion and cursor target extraction.

use super::RenderApp;
use super::frame_windows::{
    ActivePresentationTransition, GuiFrameRenderState, GuiFrameWindowState,
};
use crate::core::types::DisplayFrameId;
use crate::render_thread::cursor::{CursorConfigSnapshot, CursorTarget};
use neomacs_display_protocol::frame_chrome::{FrameChrome, FrameChromeContent};

#[cfg(feature = "webview")]
fn intersect_webview_rect(
    left: neomacs_display_protocol::RootSurfaceRect,
    right: neomacs_display_protocol::RootSurfaceRect,
) -> Option<neomacs_display_protocol::RootSurfaceRect> {
    let x = left.x().max(right.x());
    let y = left.y().max(right.y());
    let far_x = (left.x() + left.width()).min(right.x() + right.width());
    let far_y = (left.y() + left.height()).min(right.y() + right.height());
    (far_x > x && far_y > y)
        .then(|| neomacs_display_protocol::RootSurfaceRect::new(x, y, far_x - x, far_y - y).ok())
        .flatten()
}

#[cfg(feature = "webview")]
fn collect_frame_webviews(
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    offset_x: f32,
    offset_y: f32,
    frame_clip: neomacs_display_protocol::RootSurfaceRect,
    scale: neomacs_display_protocol::DeviceScale,
    next_occurrence: &mut u64,
    placements: &mut std::collections::HashMap<
        neomacs_display_protocol::WebViewId,
        neomacs_webview::ResolvedWebViewPlacement,
    >,
) {
    for glyph in &frame.glyphs {
        let crate::core::frame_glyphs::FrameGlyph::Xwidget {
            window_id,
            webview_id,
            x,
            y,
            content,
            clip_rect,
            ..
        } = glyph
        else {
            continue;
        };
        // The native view is sized from the widget's own extent, GNU
        // `xww->width`/`xww->height`, never from the glyph slot: layout may
        // have cropped the slot at the right edge (`produce_xwidget_glyph`,
        // src/xdisp.c:32577-32579, emacs-31.0.90) and GNU answers that by
        // clipping the view, not resizing it (`x_draw_xwidget_glyph_string`,
        // src/xwidget.c:2841-2847).  The clip below is that text-area clip.
        let Ok(content) = neomacs_display_protocol::RootSurfaceRect::new(
            offset_x + *x,
            offset_y + *y,
            content.width_px(),
            content.height_px(),
        ) else {
            continue;
        };
        let mut visible = intersect_webview_rect(content, frame_clip);
        if let Some(clip) = clip_rect {
            let Ok(clip) = neomacs_display_protocol::RootSurfaceRect::new(
                offset_x + clip.x,
                offset_y + clip.y,
                clip.width,
                clip.height,
            ) else {
                continue;
            };
            visible = visible.and_then(|visible| intersect_webview_rect(visible, clip));
        }
        let Some(visible) = visible else {
            continue;
        };
        *next_occurrence = next_occurrence.saturating_add(1);
        let Ok(placement) = neomacs_webview::ResolvedWebViewPlacement::new(
            *webview_id,
            neomacs_webview::WebViewOccurrenceId::new(*next_occurrence),
            *window_id,
            content,
            visible,
            scale,
        ) else {
            continue;
        };
        // Portable native backends support one active occurrence. Frames are
        // visited in renderer z-order, so a later/topmost occurrence wins.
        placements.insert(*webview_id, placement);
    }
}

/// Monotonic revision source for one top-level host's derived WebView scene.
///
/// A host scene combines one root frame and any number of child-frame
/// presentations. Their evaluator presentation IDs are independent clocks,
/// so a `max()` of those IDs can move backwards when a child disappears.
/// This clock advances only when the resolved placement snapshot changes.
/// Everything the resolved placements of one top-level host depend on,
/// cheap enough to rebuild on every event-loop pass.
///
/// The glyph walk in `collect_frame_webviews` is a function of the presented
/// root frame, the presented child frames with their placement and clip, and
/// the device scale -- nothing else.  Redisplay replaces a presentation
/// wholesale (a new `PresentationId`), so equal inputs mean equal glyphs and
/// the walk can be skipped; that is what makes an idle session with one web
/// view stop paying O(glyphs) per pass.
#[cfg(feature = "webview")]
#[derive(Clone, Debug, PartialEq)]
pub(super) struct WebViewPlacementInputs {
    /// Root presentation, width and height; `None` before the first frame.
    root: Option<(
        neomacs_display_protocol::frame_chrome::PresentationId,
        f32,
        f32,
    )>,
    scale: f32,
    /// Child frames in renderer z-order: id, presentation, absolute offset,
    /// clip in root.
    children: Vec<(
        u64,
        neomacs_display_protocol::frame_chrome::PresentationId,
        f32,
        f32,
        neomacs_display_protocol::PresentedClip,
    )>,
}

#[cfg(feature = "webview")]
#[derive(Default)]
pub(super) struct WebViewSceneClock {
    revision: u64,
    placements: Vec<neomacs_webview::ResolvedWebViewPlacement>,
    /// The inputs `placements` were resolved from, so an unchanged
    /// presentation reuses them instead of walking the glyph buffer again.
    inputs: Option<WebViewPlacementInputs>,
}

#[cfg(feature = "webview")]
impl WebViewSceneClock {
    /// The host's scene for this pass.  `compute` -- the glyph walk -- runs
    /// only when `inputs` differ from the inputs the cached placements came
    /// from, and the revision advances only when the placements changed.
    fn resolve_cached(
        &mut self,
        host: neomacs_webview::HostWindowId,
        inputs: WebViewPlacementInputs,
        compute: impl FnOnce() -> Vec<neomacs_webview::ResolvedWebViewPlacement>,
    ) -> Result<neomacs_webview::ResolvedWebViewScene, neomacs_webview::WebViewSceneError> {
        if self.inputs.as_ref() != Some(&inputs) {
            let placements = compute();
            if self.revision == 0 || self.placements != placements {
                self.revision = self.revision.saturating_add(1);
                self.placements = placements;
            }
            self.inputs = Some(inputs);
        }
        neomacs_webview::ResolvedWebViewScene::try_new(
            host,
            neomacs_webview::WebViewSceneRevision::new(self.revision),
            self.placements.clone(),
        )
    }
}

#[cfg(all(test, feature = "webview"))]
#[path = "frame_ingest_test.rs"]
mod tests;

struct CursorSyncOutcome {
    target: CursorTarget,
    had_target: bool,
    target_moved: bool,
    old_cursor_rect: (f32, f32, f32, f32),
}

struct FrameIngestOutcome {
    cursor: Option<CursorSyncOutcome>,
    presentation: Option<ActivePresentationTransition>,
}

impl RenderApp {
    #[cfg(feature = "webview")]
    fn resolved_webview_placements(
        window_state: &GuiFrameWindowState,
    ) -> Vec<neomacs_webview::ResolvedWebViewPlacement> {
        let render = &window_state.render;
        let Some(root) = render.compositor.current_frame.as_ref() else {
            return Vec::new();
        };
        let Ok(scale) =
            neomacs_display_protocol::DeviceScale::new(window_state.scale_factor() as f32)
        else {
            return Vec::new();
        };
        let Ok(root_clip) =
            neomacs_display_protocol::RootSurfaceRect::new(0.0, 0.0, root.width, root.height)
        else {
            return Vec::new();
        };
        let mut occurrence = 0;
        let mut placements = std::collections::HashMap::new();
        collect_frame_webviews(
            root,
            0.0,
            0.0,
            root_clip,
            scale,
            &mut occurrence,
            &mut placements,
        );
        for frame_id in render.compositor.child_frames.sorted_for_rendering() {
            let Some(entry) = render.compositor.child_frames.frames.get(frame_id) else {
                continue;
            };
            let clip = match entry.clip_in_root {
                neomacs_display_protocol::PresentedClip::Empty => continue,
                neomacs_display_protocol::PresentedClip::Rect(clip) => clip,
            };
            collect_frame_webviews(
                &entry.frame,
                entry.abs_x,
                entry.abs_y,
                clip,
                scale,
                &mut occurrence,
                &mut placements,
            );
        }
        let mut placements = placements.into_values().collect::<Vec<_>>();
        placements.sort_by_key(|placement| placement.occurrence());
        placements
    }

    /// The inputs the host's resolved placements depend on; see
    /// [`WebViewPlacementInputs`].
    #[cfg(feature = "webview")]
    fn webview_placement_inputs(window_state: &GuiFrameWindowState) -> WebViewPlacementInputs {
        let render = &window_state.render;
        let root = render
            .compositor
            .current_frame
            .as_ref()
            .map(|root| (root.presentation_id, root.width, root.height));
        let children = render
            .compositor
            .child_frames
            .sorted_for_rendering()
            .iter()
            .filter_map(|frame_id| {
                let entry = render.compositor.child_frames.frames.get(frame_id)?;
                Some((
                    *frame_id,
                    entry.frame.presentation_id,
                    entry.abs_x,
                    entry.abs_y,
                    entry.clip_in_root,
                ))
            })
            .collect();
        WebViewPlacementInputs {
            root,
            scale: window_state.scale_factor() as f32,
            children,
        }
    }

    #[cfg(feature = "webview")]
    pub(super) fn synchronize_webview_presentations(&mut self) {
        let mut scenes = Vec::new();
        let mut live_hosts = std::collections::HashSet::new();
        let clocks = &mut self.webview_scene_clocks;
        self.frame_windows
            .for_each_top_level_window(|window_state| {
                let host_id =
                    neomacs_webview::HostWindowId::new(window_state.render.emacs_frame_id);
                live_hosts.insert(host_id);
                let host = window_state
                    .window()
                    .cloned()
                    .map(neomacs_webview::WebViewHost::new);
                // The glyph walk runs only when the presented frames changed.
                let inputs = Self::webview_placement_inputs(window_state);
                match clocks
                    .entry(host_id)
                    .or_default()
                    .resolve_cached(host_id, inputs, || {
                        Self::resolved_webview_placements(window_state)
                    }) {
                    Ok(scene) => scenes.push((scene, host)),
                    Err(error) => {
                        tracing::warn!(?host_id, %error, "invalid resolved WebView scene");
                    }
                }
            });
        clocks.retain(|host, _clock| live_hosts.contains(host));
        let Some(system) = self.webview_system.as_mut() else {
            return;
        };
        for stale in system
            .presented_host_ids()
            .into_iter()
            .filter(|host| !live_hosts.contains(host))
            .collect::<Vec<_>>()
        {
            system.unregister_host(stale);
        }
        for (scene, host) in scenes {
            if let Some(host) = host {
                system.register_host(scene.host(), host);
            }
            if let Err(error) = system.synchronize_presentation(scene) {
                tracing::warn!(%error, "failed to synchronize WebView presentation");
            }
        }
    }

    fn ingest_frame_window_root_frame(
        window_state: &mut GuiFrameWindowState,
        frame: crate::core::frame_glyphs::FrameGlyphBuffer,
        row_damage: neomacs_renderer_wgpu::FrameRowDamage,
        cursor_config: CursorConfigSnapshot,
    ) -> FrameIngestOutcome {
        Self::ingest_top_level_render_frame(
            &mut window_state.render,
            frame,
            row_damage,
            cursor_config,
        )
    }

    fn ingest_top_level_render_frame(
        render: &mut GuiFrameRenderState,
        frame: crate::core::frame_glyphs::FrameGlyphBuffer,
        row_damage: neomacs_renderer_wgpu::FrameRowDamage,
        cursor_config: CursorConfigSnapshot,
    ) -> FrameIngestOutcome {
        use neomacs_display_protocol::frame_chrome::FrameChromeKind;
        if frame.frame_chrome.band(FrameChromeKind::MenuBar).is_none() {
            render.chrome.interaction.clear_menu_bar();
        }
        if frame.frame_chrome.band(FrameChromeKind::ToolBar).is_none() {
            render.chrome.interaction.clear_toolbar();
        }
        if frame
            .frame_chrome
            .band(FrameChromeKind::CompactBar)
            .is_none()
        {
            render.chrome.interaction.clear_compact_bar();
        }
        if frame.frame_chrome.band(FrameChromeKind::TabBar).is_none() {
            render.chrome.interaction.clear_tab_bar();
        }
        render.cursor.reset_blink();
        let presentation = render.set_current_frame(Some(frame), Some(row_damage));
        let cursor_sync = Self::sync_render_cursor(render, cursor_config);
        render.sync_visual_cursors_from_current_frame(|cursor| cursor.apply_config(cursor_config));
        render.mark_dirty();
        FrameIngestOutcome {
            cursor: cursor_sync,
            presentation,
        }
    }

    fn sync_render_cursor(
        render: &mut GuiFrameRenderState,
        cursor_config: CursorConfigSnapshot,
    ) -> Option<CursorSyncOutcome> {
        let mut active_cursor = render.compositor.current_frame.as_ref().and_then(|frame| {
            crate::render_thread::frame_windows::GuiFrameWindowManager::cursor_target_for_frame(
                render.emacs_frame_id,
                frame,
            )
        });

        if active_cursor.is_none() {
            for entry in render.compositor.child_frames.frames.values() {
                if let Some(cursor) = entry.frame.active_cursor() {
                    // Resolve through the shared cursor_draw_rect (where the
                    // static cursor draws), not the grid-approximate cursor
                    // geometry; see cursor_target_for_frame.
                    let (x, y, width, height) = entry.frame.cursor_draw_rect(
                        cursor.slot_id,
                        cursor.style,
                        cursor.ascent,
                        (cursor.x, cursor.y, cursor.width, cursor.height),
                    );
                    active_cursor = Some(CursorTarget {
                        window_id: cursor.window_id.get(),
                        x,
                        y,
                        width,
                        height,
                        style: cursor.style,
                        frame_id: entry.frame_id,
                    });
                    break;
                }
            }
        }

        render.cursor.apply_config(cursor_config);
        if let Some(new_target) = active_cursor {
            let old_cursor_rect = (
                render.cursor.current_x,
                render.cursor.current_y,
                render.cursor.current_w,
                render.cursor.current_h,
            );
            let (had_target, target_moved) = render.cursor.set_target(new_target.clone());
            if target_moved {
                render.mark_dirty();
            }
            Some(CursorSyncOutcome {
                target: new_target,
                had_target,
                target_moved,
                old_cursor_rect,
            })
        } else {
            render.cursor.clear_target();
            render.clear_ime_preedit();
            None
        }
    }

    fn sync_frame_window_cursor(
        window_state: &mut GuiFrameWindowState,
        cursor_config: CursorConfigSnapshot,
    ) -> Option<CursorSyncOutcome> {
        let cursor_sync = Self::sync_render_cursor(&mut window_state.render, cursor_config);
        if window_state.render.cursor.target_cloned().is_none() {
            window_state.reset_ime_cursor_area();
        }
        cursor_sync
    }

    fn update_top_level_cursor_effects(
        renderer: Option<&neomacs_renderer_wgpu::WgpuRenderer>,
        render: &mut GuiFrameRenderState,
        new_target: &CursorTarget,
        had_target: bool,
        target_moved: bool,
        old_cursor_rect: (f32, f32, f32, f32),
        typing_ripple_enabled: bool,
        cursor_trail_fade_enabled: bool,
    ) {
        if target_moved
            && had_target
            && typing_ripple_enabled
            && let Some(renderer) = renderer
        {
            let cx = new_target.x + new_target.width / 2.0;
            let cy = new_target.y + new_target.height / 2.0;
            renderer.spawn_transient_ripple(&mut render.compositor.renderer_effects, cx, cy);
        }

        if target_moved
            && had_target
            && cursor_trail_fade_enabled
            && let Some(renderer) = renderer
        {
            renderer.record_transient_cursor_trail(
                &mut render.compositor.renderer_effects,
                old_cursor_rect.0,
                old_cursor_rect.1,
                old_cursor_rect.2,
                old_cursor_rect.3,
            );
        }
    }

    fn update_frame_window_cursor_side_effects(
        renderer: Option<&neomacs_renderer_wgpu::WgpuRenderer>,
        window_state: &mut GuiFrameWindowState,
        cursor_sync: CursorSyncOutcome,
        typing_ripple_enabled: bool,
        cursor_trail_fade_enabled: bool,
        update_transient_effects: bool,
    ) {
        if update_transient_effects {
            Self::update_top_level_cursor_effects(
                renderer,
                &mut window_state.render,
                &cursor_sync.target,
                cursor_sync.had_target,
                cursor_sync.target_moved,
                cursor_sync.old_cursor_rect,
                typing_ripple_enabled,
                cursor_trail_fade_enabled,
            );
        }
        Self::update_frame_window_ime_cursor_area_if_needed(window_state, &cursor_sync.target);
    }

    pub(super) fn sync_frame_chrome_assets(&mut self, frame_chrome: &FrameChrome) {
        for band in frame_chrome.bands() {
            let (icon_size, items) = match band.content() {
                FrameChromeContent::ToolBar(content) => (
                    content.icon_size(),
                    content
                        .items()
                        .iter()
                        .map(|item| item.item().clone())
                        .collect::<Vec<_>>(),
                ),
                FrameChromeContent::CompactBar(content) => (
                    content.icon_size(),
                    content
                        .tool_items()
                        .iter()
                        .map(|item| item.item().clone())
                        .collect::<Vec<_>>(),
                ),
                _ => continue,
            };
            self.ensure_toolbar_icon_textures(&items, icon_size);
        }
    }

    /// Get latest frame from Emacs (non-blocking).
    pub(super) fn poll_frame(&mut self) {
        self.frame_windows.tick_top_level_child_frames();
        let mut queued = std::collections::VecDeque::new();
        queued.extend(std::mem::take(&mut self.pending_child_frames).into_values());
        queued.extend(self.comms.frame_rx.try_iter());
        loop {
            let mut deferred = std::collections::HashMap::new();
            let mut made_progress = false;
            while let Some(display_state) = queued.pop_front() {
                super::frame_stats::note_scene_commit(std::time::Instant::now());
                let frame_id = display_state.frame_placement.frame();
                let parent_id = display_state
                    .frame_placement
                    .parent()
                    .unwrap_or(neomacs_display_protocol::DisplayFrameId::new(0));
                if parent_id != DisplayFrameId::new(0)
                    && !self.frame_windows.has_presented_frame(parent_id.get())
                {
                    if let Some(superseded) = deferred.insert(frame_id.get(), display_state) {
                        let placement = superseded.frame_placement;
                        self.comms.send_input(
                            crate::thread_comm::InputEvent::PresentationDiscarded {
                                presentation: placement.presentation().get(),
                                emacs_frame_id: placement.frame().get(),
                            },
                        );
                    }
                    continue;
                }
                made_progress = true;
                self.sync_frame_chrome_assets(&display_state.frame_chrome);

                // Materialize FrameDisplayState → FrameGlyphBuffer for the
                // existing rendering code.  The layout engine populates
                // the grid and non-grid items; materialize() converts the
                // grid into pixel-positioned glyphs and appends non-grid items.
                let frame = display_state.materialize();
                // Row-damage summary for the renderer's vertex reuse. Built from
                // exactly this display_state (the one `frame` was materialized
                // from) so damage and glyphs can never describe different frames.
                let row_damage =
                    neomacs_renderer_wgpu::FrameRowDamage::from_display_state(&display_state);

                // ── Observation point: inspect what will be rendered ──
                // `NEOMACS_DUMP_FRAME_GLYPHS=1`   → counts + role/text + raw glyph Debug.
                // `NEOMACS_DUMP_FRAME_GLYPHS=full` → the above PLUS a diff-stable,
                //   face-RESOLVED per-glyph view (fg/bg as hex + stipple size) sorted
                //   by (window, y, x), plus cursors. This is the reusable frontend
                //   debugging view: any color/face/stipple/position bug is a text diff
                //   (neomacs-before vs -after, or vs GNU) with no rebuild or screenshot.
                let dump_frame_glyphs_mode = std::env::var("NEOMACS_DUMP_FRAME_GLYPHS").ok();
                if matches!(dump_frame_glyphs_mode.as_deref(), Some("1") | Some("full")) {
                    let mut char_count = 0usize;
                    let mut bg_count = 0usize;
                    let mut border_count = 0usize;
                    let mut scrollbar_count = 0usize;
                    let mut image_count = 0usize;
                    let mut stretch_count = 0usize;
                    let mut video_count = 0usize;
                    let mut webkit_count = 0usize;
                    let mut other_count = 0usize;
                    for g in &frame.glyphs {
                        match g {
                            crate::core::frame_glyphs::FrameGlyph::Char { .. } => char_count += 1,
                            crate::core::frame_glyphs::FrameGlyph::Background { .. } => {
                                bg_count += 1
                            }
                            crate::core::frame_glyphs::FrameGlyph::Border { .. } => {
                                border_count += 1
                            }
                            crate::core::frame_glyphs::FrameGlyph::ScrollBar { .. } => {
                                scrollbar_count += 1
                            }
                            crate::core::frame_glyphs::FrameGlyph::Image { .. } => image_count += 1,
                            crate::core::frame_glyphs::FrameGlyph::Stretch { .. } => {
                                stretch_count += 1
                            }
                            crate::core::frame_glyphs::FrameGlyph::Video { .. } => video_count += 1,
                            crate::core::frame_glyphs::FrameGlyph::Xwidget { .. } => {
                                webkit_count += 1
                            }
                            _ => other_count += 1,
                        }
                    }
                    // Role-aware breakdown: reconstruct chrome-row text + Y so we
                    // can tell whether tab-line/header-line glyphs are emitted at
                    // all (and where) vs. silently dropped.
                    {
                        use crate::core::frame_glyphs::FrameGlyph;
                        let mut per_role: std::collections::HashMap<String, (usize, f32, String)> =
                            std::collections::HashMap::new();
                        for g in &frame.glyphs {
                            if let FrameGlyph::Char {
                                row_role,
                                char: ch,
                                y,
                                window_id,
                                ..
                            } = g
                            {
                                let key = format!("{:?}/win{}", row_role, window_id);
                                let e = per_role.entry(key).or_insert((0, *y, String::new()));
                                e.0 += 1;
                                e.1 = *y;
                                if e.2.len() < 60 {
                                    e.2.push(*ch);
                                }
                            }
                        }
                        let mut tabline_total = 0usize;
                        for (role, (n, _, _)) in &per_role {
                            if role.starts_with("TabLine") {
                                tabline_total += n;
                            }
                        }
                        let mut keys: Vec<_> = per_role.keys().cloned().collect();
                        keys.sort();
                        for k in keys {
                            let (n, y, text) = &per_role[&k];
                            tracing::info!("DUMP_ROLE {k}: {n} chars y={y:.1} text=[{text}]");
                        }
                        tracing::info!("DUMP_ROLE tabline_char_total={tabline_total}");
                    }
                    let cursor_count = frame.window_cursors.len();
                    let active_cursor_count =
                        frame.window_cursors.iter().filter(|c| c.active).count();
                    tracing::info!(
                        "poll_frame: frame_id={} parent_id={} size={:.0}x{:.0} char={:.1}x{:.1} \
                     glyphs={} (char={} bg={} border={} stretch={} scrollbar={} image={} video={} webkit={} other={}) \
                     windows={} window_cursors={} active_cursors={} faces={}",
                        frame_id,
                        parent_id,
                        frame.width,
                        frame.height,
                        frame.char_width,
                        frame.char_height,
                        frame.glyphs.len(),
                        char_count,
                        bg_count,
                        border_count,
                        stretch_count,
                        scrollbar_count,
                        image_count,
                        video_count,
                        webkit_count,
                        other_count,
                        frame.window_infos.len(),
                        cursor_count,
                        active_cursor_count,
                        frame.faces.len(),
                    );
                    if let Some(cursor) = frame.active_cursor() {
                        tracing::info!(
                            "active_cursor: window_id={} slot=(window_id={},row={},col={}) \
                         rect=({:.2},{:.2}) {:.2}x{:.2} ascent={:.2} style={:?} color={:?} cursor_fg={:?}",
                            cursor.window_id.get(),
                            cursor.slot_id.window_id,
                            cursor.slot_id.row,
                            cursor.slot_id.col,
                            cursor.x,
                            cursor.y,
                            cursor.width,
                            cursor.height,
                            cursor.ascent,
                            cursor.style,
                            cursor.color,
                            cursor.cursor_fg,
                        );
                        match frame.slot_glyph(cursor.slot_id) {
                            Some(slot_glyph) => {
                                tracing::info!("active_cursor_slot_glyph: {:?}", slot_glyph)
                            }
                            None => tracing::warn!(
                                "active_cursor_slot_glyph: missing slot=(window_id={},row={},col={})",
                                cursor.slot_id.window_id,
                                cursor.slot_id.row,
                                cursor.slot_id.col,
                            ),
                        }
                        if let Some(effects) = frame.phys_cursor_effects() {
                            tracing::info!("active_cursor_effects: {:?}", effects);
                        }
                    } else {
                        tracing::info!("active_cursor: none");
                    }
                    if !frame.window_cursors.is_empty() {
                        let all_window_cursors: String = frame
                        .window_cursors
                        .iter()
                        .enumerate()
                        .fold(String::new(), |acc, (i, cursor)| {
                            acc + &format!(
                                "  window_cursor[{}]: window_id={} slot=(window_id={},row={},col={}) \
                                 rect=({:.2},{:.2}) {:.2}x{:.2} style={:?} color={:?}\n",
                                i,
                                cursor.window_id.get(),
                                cursor.slot_id.window_id,
                                cursor.slot_id.row,
                                cursor.slot_id.col,
                                cursor.x,
                                cursor.y,
                                cursor.width,
                                cursor.height,
                                cursor.style,
                                cursor.color,
                            )
                        });
                        tracing::info!("window_cursors:\n{}", all_window_cursors);
                    }
                    let all_glyphs: String =
                        frame
                            .glyphs
                            .iter()
                            .enumerate()
                            .fold(String::new(), |acc, (i, g)| {
                                let slot = g.slot_id();
                                acc + &format!(
                                    "  glyph[{}][r={},c={}]: {:?}\n",
                                    i,
                                    slot.map_or(0, |s| s.row),
                                    slot.map_or(0, |s| s.col),
                                    g
                                )
                            });
                    tracing::info!("all_glyphs:\n{}", all_glyphs);
                    if dump_frame_glyphs_mode.as_deref() == Some("full") {
                        tracing::info!(
                            "GLYPH_DUMP_FULL frame={}:\n{}",
                            frame_id,
                            dump_frame_glyphs_resolved(&frame)
                        );
                    }
                }

                if parent_id == DisplayFrameId::new(0) {
                    let routed_to_primary_fallback =
                        self.frame_windows.is_primary_frame_id(frame_id.get());
                    let update_transient_effects = routed_to_primary_fallback;
                    let typing_ripple_enabled = self.effects.typing_ripple.enabled;
                    let cursor_trail_fade_enabled = self.effects.cursor_trail_fade.enabled;
                    let renderer = self.renderer.as_ref();
                    if let Some(window_state) = self.frame_windows.get_mut(frame_id.get()) {
                        let cursor_config = self.cursor_defaults.config_snapshot();
                        let outcome = Self::ingest_frame_window_root_frame(
                            window_state,
                            frame,
                            row_damage,
                            cursor_config,
                        );
                        if let Some(cursor_sync) = outcome.cursor {
                            Self::update_frame_window_cursor_side_effects(
                                renderer,
                                window_state,
                                cursor_sync,
                                typing_ripple_enabled,
                                cursor_trail_fade_enabled,
                                update_transient_effects,
                            );
                        } else {
                            window_state.reset_ime_cursor_area();
                        }
                        if let Some(transition) = outcome.presentation {
                            self.comms.send_input(
                                crate::thread_comm::InputEvent::PresentationActivated {
                                    presentation: transition.activated.get(),
                                    emacs_frame_id: frame_id.get(),
                                },
                            );
                            if let Some(presentation) = transition.replaced.and_then(|replaced| {
                                window_state
                                    .render
                                    .route_presentation_retirement(replaced.get())
                            }) {
                                self.comms.send_input(
                                    crate::thread_comm::InputEvent::PresentationRetired {
                                        presentation,
                                    },
                                );
                            }
                        }
                        continue;
                    }
                }

                if parent_id != DisplayFrameId::new(0) {
                    tracing::debug!(
                        frame_id = frame_id.get(),
                        parent_frame_id = parent_id.get(),
                        width = frame.width,
                        height = frame.height,
                        parent_x = frame.frame_placement.outer_in_parent().x(),
                        parent_y = frame.frame_placement.outer_in_parent().y(),
                        glyphs = frame.glyphs.len(),
                        "child_frame_lifecycle: render_thread_child_frame_state_received"
                    );
                    let update_transient_effects =
                        self.frame_windows.is_primary_frame_id(parent_id.get());
                    let typing_ripple_enabled = self.effects.typing_ripple.enabled;
                    let cursor_trail_fade_enabled = self.effects.cursor_trail_fade.enabled;
                    let renderer = self.renderer.as_ref();
                    if let Some(window_state) = self
                        .frame_windows
                        .get_mut_by_presented_frame(parent_id.get())
                    {
                        let old_presentation = window_state
                            .render
                            .compositor
                            .child_frames
                            .frames
                            .get(&frame_id.get())
                            .map(|entry| entry.frame.presentation_id);
                        let new_presentation = frame.presentation_id;
                        let cursor_config = self.cursor_defaults.config_snapshot();
                        if window_state.render.update_child_frame(frame) {
                            let transition = ActivePresentationTransition::between(
                                old_presentation,
                                new_presentation,
                            );
                            let cursor_sync =
                                Self::sync_frame_window_cursor(window_state, cursor_config);
                            if let Some(cursor_sync) = cursor_sync {
                                Self::update_frame_window_cursor_side_effects(
                                    renderer,
                                    window_state,
                                    cursor_sync,
                                    typing_ripple_enabled,
                                    cursor_trail_fade_enabled,
                                    update_transient_effects,
                                );
                            }
                            if let Some(transition) = transition {
                                self.comms.send_input(
                                    crate::thread_comm::InputEvent::PresentationActivated {
                                        presentation: transition.activated.get(),
                                        emacs_frame_id: frame_id.get(),
                                    },
                                );
                                if let Some(presentation) =
                                    transition.replaced.and_then(|replaced| {
                                        window_state
                                            .render
                                            .route_presentation_retirement(replaced.get())
                                    })
                                {
                                    self.comms.send_input(
                                        crate::thread_comm::InputEvent::PresentationRetired {
                                            presentation,
                                        },
                                    );
                                }
                            }
                        } else if old_presentation != Some(new_presentation) {
                            self.comms.send_input(
                                crate::thread_comm::InputEvent::PresentationDiscarded {
                                    presentation: new_presentation.get(),
                                    emacs_frame_id: frame_id.get(),
                                },
                            );
                        }
                        continue;
                    }
                }

                if parent_id != DisplayFrameId::new(0)
                    && self.frame_windows.is_primary_frame_id(parent_id.get())
                {
                    if let Some(ws) = self.frame_windows.primary_window_mut() {
                        let old_presentation = ws
                            .render
                            .compositor
                            .child_frames
                            .frames
                            .get(&frame_id.get())
                            .map(|entry| entry.frame.presentation_id);
                        let new_presentation = frame.presentation_id;
                        if ws.render.update_child_frame(frame) {
                            if let Some(transition) = ActivePresentationTransition::between(
                                old_presentation,
                                new_presentation,
                            ) {
                                self.comms.send_input(
                                    crate::thread_comm::InputEvent::PresentationActivated {
                                        presentation: transition.activated.get(),
                                        emacs_frame_id: frame_id.get(),
                                    },
                                );
                                if let Some(presentation) =
                                    transition.replaced.and_then(|replaced| {
                                        ws.render.route_presentation_retirement(replaced.get())
                                    })
                                {
                                    self.comms.send_input(
                                        crate::thread_comm::InputEvent::PresentationRetired {
                                            presentation,
                                        },
                                    );
                                }
                            }
                        } else if old_presentation != Some(new_presentation) {
                            self.comms.send_input(
                                crate::thread_comm::InputEvent::PresentationDiscarded {
                                    presentation: new_presentation.get(),
                                    emacs_frame_id: frame_id.get(),
                                },
                            );
                        }
                    } else {
                        self.comms.send_input(
                            crate::thread_comm::InputEvent::PresentationDiscarded {
                                presentation: frame.presentation_id.get(),
                                emacs_frame_id: frame_id.get(),
                            },
                        );
                    };
                } else if parent_id == DisplayFrameId::new(0)
                    && self.frame_windows.is_primary_frame_id(frame_id.get())
                {
                    let cursor_config = self.cursor_defaults.config_snapshot();
                    if let Some(primary_frame) = self
                        .frame_windows
                        .primary_window_mut()
                        .map(|ws| &mut ws.render)
                    {
                        let outcome = Self::ingest_top_level_render_frame(
                            primary_frame,
                            frame,
                            row_damage,
                            cursor_config,
                        );
                        if let Some(transition) = outcome.presentation {
                            self.comms.send_input(
                                crate::thread_comm::InputEvent::PresentationActivated {
                                    presentation: transition.activated.get(),
                                    emacs_frame_id: frame_id.get(),
                                },
                            );
                            if let Some(presentation) = transition.replaced.and_then(|replaced| {
                                primary_frame.route_presentation_retirement(replaced.get())
                            }) {
                                self.comms.send_input(
                                    crate::thread_comm::InputEvent::PresentationRetired {
                                        presentation,
                                    },
                                );
                            }
                        }
                    } else {
                        self.comms.send_input(
                            crate::thread_comm::InputEvent::PresentationDiscarded {
                                presentation: frame.presentation_id.get(),
                                emacs_frame_id: frame_id.get(),
                            },
                        );
                    }
                } else {
                    self.comms
                        .send_input(crate::thread_comm::InputEvent::PresentationDiscarded {
                            presentation: frame.presentation_id.get(),
                            emacs_frame_id: frame_id.get(),
                        });
                }
            }
            if deferred.is_empty() {
                break;
            }
            if !made_progress {
                self.pending_child_frames = deferred;
                break;
            }
            queued.extend(deferred.into_values());
        }

        let cursor_config = self.cursor_defaults.config_snapshot();
        if let Some(primary_frame) = self
            .frame_windows
            .primary_window_mut()
            .map(|ws| &mut ws.render)
        {
            let cursor_sync = Self::sync_render_cursor(primary_frame, cursor_config);
            primary_frame.sync_visual_cursors_from_current_frame(|cursor| {
                cursor.apply_config(cursor_config)
            });
            if let Some(cursor_sync) = cursor_sync {
                self.update_ime_cursor_area_if_needed(&cursor_sync.target);
            } else {
                if let Some(window_state) = self.frame_windows.primary_window_mut() {
                    window_state.reset_ime_cursor_area()
                };
                if let Some(ws) = self.frame_windows.primary_window_mut() {
                    ws.render.clear_ime_preedit()
                };
            }
        }
    }
}

/// Diff-stable, face-RESOLVED dump of every visible glyph + cursors, sorted by
/// (window, y, x). Powers `NEOMACS_DUMP_FRAME_GLYPHS=full`: the reusable
/// frontend-debugging view where any color/face/stipple/position bug becomes a
/// text diff (neomacs before-vs-after, or vs GNU) with no rebuild or screenshot.
/// The face is resolved to final fg/bg (hex) + stipple size, so e.g. a text
/// glyph and a stipple on the same cell reveal a face-id/color mismatch inline.
fn dump_frame_glyphs_resolved(frame: &crate::core::frame_glyphs::FrameGlyphBuffer) -> String {
    use crate::core::frame_glyphs::FrameGlyph;
    use crate::core::types::Color;
    let hx = |c: Color| -> String {
        let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", ch(c.r), ch(c.g), ch(c.b))
    };
    // Param type is inferred as `FaceId` from `resolved_face`.
    let face_desc = |face_id| {
        let rf = frame.resolved_face(face_id);
        let f = frame.faces.get(&face_id);
        let stipple = f
            .and_then(|f| f.stipple.as_deref())
            .map(|s| format!("{}x{}", s.width, s.height))
            .unwrap_or_else(|| "-".to_string());
        let (family, size) = f
            .map(|f| (f.font_family.as_str(), f.font_size))
            .unwrap_or(("-", 0.0));
        format!(
            "face={:?} fg={} bg={} stipple={} font='{}'@{:.1}",
            face_id,
            hx(rf.fg),
            hx(rf.bg),
            stipple,
            family,
            size
        )
    };
    let mut rows: Vec<(i64, i64, String)> = Vec::new();
    for g in &frame.glyphs {
        rows.push(match g {
            FrameGlyph::Char {
                window_id,
                row_role,
                char,
                x,
                y,
                face_id,
                ..
            } => (
                *y as i64,
                *x as i64,
                format!(
                    "[win{} {:?} y={:.0} x={:.0}] {:?}  {}",
                    window_id.get(),
                    row_role,
                    y,
                    x,
                    char,
                    face_desc(*face_id)
                ),
            ),
            FrameGlyph::Stretch {
                window_id,
                row_role,
                x,
                y,
                width,
                face_id,
                ..
            } => (
                *y as i64,
                *x as i64,
                format!(
                    "[win{} {:?} y={:.0} x={:.0}] <stretch w={:.0}>  {}",
                    window_id.get(),
                    row_role,
                    y,
                    x,
                    width,
                    face_desc(*face_id)
                ),
            ),
            FrameGlyph::FringeBitmap {
                window_id,
                x,
                y,
                bitmap_index,
                face_id,
                ..
            } => (
                *y as i64,
                *x as i64,
                format!(
                    "[win{} fringe y={:.0} x={:.0} bmp={}]  {}",
                    window_id.get(),
                    y,
                    x,
                    bitmap_index,
                    face_desc(*face_id)
                ),
            ),
            FrameGlyph::Image { x, y, width, .. } => (
                *y as i64,
                *x as i64,
                format!("[y={:.0} x={:.0}] <image w={:.0}>", y, x, width),
            ),
            FrameGlyph::Background { bounds, color } => (
                bounds.y as i64,
                bounds.x as i64,
                format!(
                    "[y={:.0} x={:.0}] <bg {:.0}x{:.0}> color={}",
                    bounds.y,
                    bounds.x,
                    bounds.width,
                    bounds.height,
                    hx(*color)
                ),
            ),
            other => (i64::MAX, 0, format!("<other> {other:?}")),
        });
    }
    rows.sort();
    let mut out = String::new();
    for (_, _, line) in &rows {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    for c in &frame.window_cursors {
        out.push_str(&format!(
            "  [cursor win{} y={:.0} x={:.0}] style={:?} color={} fg={}\n",
            c.window_id.get(),
            c.y,
            c.x,
            c.style,
            hx(c.color),
            hx(c.cursor_fg),
        ));
    }
    out
}

#[cfg(all(test, feature = "webview"))]
#[path = "frame_ingest_test.rs"]
mod tests;
