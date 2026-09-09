//! One menu session and its native popup chain.

use super::session::MenuSession;
use crate::presentation::{PopupCommit, PopupHost};
use neomacs_display_protocol::{
    Point, PopupConstraintPolicy, PopupPlacement, PopupPreferredSide, Rect, menu::MenuPanelPaint,
};
use neomacs_renderer_wgpu::WgpuRenderer;
use std::sync::Arc;
use winit::{
    event::{ElementState, MouseScrollDelta, WindowEvent},
    window::{Window, WindowId},
};

pub(crate) struct MenuRequest {
    pub tooltips: Option<neomacs_display_protocol::tooltip::MenuTooltips>,
    pub request_id: Option<neomacs_display_protocol::menu::MenuBarRequestId>,
    pub token: neomacs_display_protocol::menu::MenuToken,
    pub frame_id: u64,
    pub parent: Arc<dyn Window>,
    pub placement: PopupPlacement,
    pub session: MenuSession,
    /// Font bindings captured from the owner; no glyph rows are retained.
    pub fonts: neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer,
}

#[derive(Default)]
pub(crate) struct MenuPresentation {
    request: Option<MenuRequest>,
    host: PopupHost,
    panels: Vec<PanelState>,
    lifetime: super::session::MenuLifetime,
    menu_bar: super::menu_bar::MenuBarTracking,
    modifiers: winit::keyboard::ModifiersState,
    // A dismissing press still owns its matching release after the menu closes.
    release_owner: Option<(WindowId, winit::event::MouseButton)>,
    tooltip: crate::tooltips::Tooltips,
    help: super::help::HoverHelp,
}

struct PanelState {
    scroll: f32,
    items: Vec<usize>,
    atlas: neomacs_renderer_wgpu::WgpuGlyphAtlas,
}

impl MenuPresentation {
    pub fn tooltip_deadline(&self) -> Option<std::time::Instant> {
        self.help
            .deadline()
            .into_iter()
            .chain(self.tooltip.deadline())
            .min()
    }

    fn dismiss_help(&mut self) {
        self.help
            .cancel(self.tooltip.hide(), std::time::Instant::now());
    }

    fn select_help(&mut self, depth: usize) {
        let target = self.request.as_ref().and_then(|r| {
            let panel = r.session.panel(depth)?;
            let row = usize::try_from(panel.hover_index).ok()?;
            let item = *panel.item_indices.get(row)?;
            r.tooltips.as_ref()?;
            r.session.all_items.get(item)?.help.as_ref()?;
            Some(super::help::HelpTarget {
                panel: super::help::PanelId(depth),
                item: neomacs_display_protocol::menu::MenuItemId(u32::try_from(item).ok()?),
            })
        });
        if target == self.help.target() {
            return;
        }
        self.dismiss_help();
        if let Some(target) = target {
            let policy = self.request.as_ref().unwrap().tooltips.as_ref().unwrap();
            let now = std::time::Instant::now();
            self.help
                .select(target, now, policy.delay, policy.short_delay, policy.recent);
        }
    }

    fn sync_help(
        &mut self,
        commit: &PopupCommit<'_>,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        let now = std::time::Instant::now();
        if let Some(target) = self.help.take_due(now) {
            let depth = target.panel.0;
            let item = target.item.0 as usize;
            // Never insert a passive sibling beneath an already-open submenu.
            if depth + 1 == self.panels.len() {
                if let (Some(parent), Some(request)) =
                    (self.host.mapped_window(depth), self.request.as_ref())
                {
                    let policy = request.tooltips.as_ref().unwrap();
                    let panel = request.session.panel(depth).unwrap();
                    if let Some(row) = panel.item_indices.iter().position(|i| *i == item) {
                        let mut appearance = policy.appearance.clone();
                        appearance.text = request.session.all_items[item]
                            .help
                            .clone()
                            .unwrap_or_default();
                        let mut fonts =
                            neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer::with_size(
                                0.0, 0.0,
                            );
                        fonts.clone_font_bindings_from(&request.fonts);
                        let (fs, lh) = request.session.metrics();
                        self.tooltip.show(
                            crate::tooltips::TooltipOwner {
                                frame: request.frame_id,
                                parent,
                                anchor: Rect::new(
                                    0.0,
                                    panel.item_offsets[row] - self.panels[depth].scroll,
                                    panel.bounds.2,
                                    panel.item_height,
                                ),
                                metrics: (fs, lh, self.panels[depth].atlas.default_char_width()),
                                fonts,
                            },
                            appearance,
                            crate::tooltips::TooltipSource::NativeMenu,
                            now,
                        );
                    }
                }
            }
        }
        self.tooltip
            .sync(now, commit, instance, adapter, device, queue, format)
    }

    pub fn heading(&self) -> Option<&super::MenuHeading> {
        self.menu_bar.heading()
    }

    /// Horizontal navigation moves between headings only when the current
    /// submenu cannot consume it. Pending menus support the same navigation.
    pub fn heading_step(&self, id: WindowId, key: &winit::keyboard::Key) -> Option<i32> {
        self.heading()?;
        if !self.owns_parent(id) && self.host.depth(id).is_none() {
            return None;
        }
        use winit::keyboard::{Key, NamedKey};
        match key {
            Key::Named(NamedKey::ArrowLeft)
                if self
                    .request
                    .as_ref()
                    .is_none_or(|r| r.session.submenu_panels.is_empty()) =>
            {
                Some(-1)
            }
            Key::Named(NamedKey::ArrowRight) => {
                let has_submenu = self.request.as_ref().is_some_and(|r| {
                    let panel = r.session.active_panel();
                    usize::try_from(panel.hover_index)
                        .ok()
                        .and_then(|i| panel.item_indices.get(i))
                        .and_then(|i| r.session.all_items.get(*i))
                        .is_some_and(|item| item.enabled() && item.submenu())
                });
                (!has_submenu).then_some(1)
            }
            _ => None,
        }
    }

    pub fn select_heading(
        &mut self,
        heading: super::MenuHeading,
        pressed: bool,
    ) -> super::HeadingAction {
        if pressed {
            self.release_owner = Some((heading.parent, winit::event::MouseButton::Left));
        }
        let action = self.menu_bar.select(heading, pressed);
        if action != super::HeadingAction::Keep {
            // Cancel the old evaluator transaction, not the newly selected intent.
            self.lifetime.finish(-1);
            self.lifetime.close();
            self.truncate(0);
            self.request = None;
        }
        action
    }

    pub fn open(&mut self, request: MenuRequest) -> bool {
        if let Some(id) = request.request_id
            && !self.menu_bar.accepts(id)
        {
            self.lifetime.reject(request.token);
            return false;
        }
        if !self.lifetime.show(request.token) {
            return false;
        }
        if let Some(id) = request.request_id {
            self.menu_bar.shown(id, request.token);
        } else {
            self.menu_bar.clear();
        }
        self.truncate(0);
        self.request = Some(request);
        true
    }

    pub fn close(&mut self) {
        self.dismiss_help();
        self.menu_bar.clear();
        self.lifetime.close();
        self.truncate(0);
        self.request = None;
    }

    fn truncate(&mut self, len: usize) {
        if len < self.panels.len() {
            self.dismiss_help();
        }
        self.host.truncate(len);
        self.panels.truncate(len);
    }

    pub fn commit_retirements(&mut self, commit: &PopupCommit<'_>) {
        // Passive children must release their retained menu parents first.
        self.tooltip.commit_retirements(commit);
        self.host.commit_retirements(commit);
    }

    pub fn shutdown(&mut self) {
        self.close();
        self.tooltip.shutdown();
        self.host.shutdown();
    }

    pub fn owner(&self) -> Option<u64> {
        self.heading()
            .map(|h| h.frame)
            .or_else(|| self.request.as_ref().map(|r| r.frame_id))
    }

    pub fn owns_parent(&self, id: WindowId) -> bool {
        self.heading().is_some_and(|h| h.parent == id)
            || self.request.as_ref().is_some_and(|r| r.parent.id() == id)
    }

    pub fn take_result(&mut self) -> Option<neomacs_display_protocol::menu::MenuResult> {
        self.lifetime.take_result()
    }

    pub fn hide(&mut self, token: neomacs_display_protocol::menu::MenuToken) -> bool {
        if self.heading().is_some() && !self.menu_bar.owns_popup(token) {
            // Old evaluator teardown may arrive before the new ShowPopupMenu.
            return false;
        }
        if self.lifetime.hide(token) {
            self.close();
            return true;
        }
        false
    }

    pub fn cancel(&mut self) {
        self.finish(-1);
    }

    fn finish(&mut self, index: i32) {
        self.lifetime.finish(index);
        self.close();
    }

    pub fn sync(
        &mut self,
        commit: &PopupCommit<'_>,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        self.commit_retirements(commit);
        let Some(request) = self.request.as_ref() else {
            return Ok(());
        };
        let wanted = request.session.submenu_panels.len() + 1;
        let common = self
            .panels
            .iter()
            .zip(request.session.panels())
            .take_while(|(popup, panel)| popup.items == panel.item_indices)
            .count();
        self.truncate(common);
        self.commit_retirements(commit);
        for depth in common..wanted {
            self.dismiss_help();
            self.commit_retirements(commit);
            let request = self.request.as_ref().unwrap();
            let panel = request.session.panel(depth).unwrap();
            let placement = if depth == 0 {
                request.placement
            } else {
                let previous = request.session.panel(depth - 1).unwrap();
                let y = panel.y - previous.y - self.panels[depth - 1].scroll;
                PopupPlacement::new(
                    Rect::new(0.0, y, previous.bounds.2, previous.item_height),
                    PopupPreferredSide::Right,
                    Point::ZERO,
                    PopupConstraintPolicy::FlipAndShift { padding: 0.0 },
                )
            };
            let Some(geometry) = self.host.open(
                commit,
                request.parent.clone(),
                placement,
                (panel.bounds.2, panel.bounds.3),
                instance,
                adapter,
                device,
                format,
            )?
            else {
                break;
            };
            let mut atlas = neomacs_renderer_wgpu::WgpuGlyphAtlas::new_with_scale(
                device,
                geometry.device_scale().get(),
            );
            let metrics = request.session.metrics();
            atlas.set_metrics(metrics.0, metrics.1);
            atlas.set_current_frame_fonts(request.fonts.font_bindings());
            self.panels.push(PanelState {
                items: panel.item_indices.clone(),
                atlas,
                scroll: 0.0,
            });
        }
        self.sync_help(commit, instance, adapter, device, queue, format)?;
        Ok(())
    }

    /// Returns true for every event belonging to this menu, including late events.
    pub fn event(
        &mut self,
        id: WindowId,
        event: &WindowEvent,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) -> bool {
        if self.tooltip.event(id, event, device, queue, renderer) {
            return true;
        }
        if (self.owns_parent(id) || self.host.depth(id).is_some())
            && matches!(
                event,
                WindowEvent::PointerButton { .. }
                    | WindowEvent::KeyboardInput { .. }
                    | WindowEvent::PointerLeft { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            self.dismiss_help();
        }
        if let WindowEvent::PointerButton { state, button, .. } = event {
            if *state == ElementState::Pressed {
                // A new press supersedes capture whose release was lost.
                self.release_owner = None;
            } else if self
                .release_owner
                .is_some_and(|(_, pressed)| button.clone().mouse_button() == Some(pressed))
            {
                self.release_owner = None;
                // Drag-to-select releases must reach the native popup's item
                // activation, but still retire the original heading press.
                if self.host.depth(id).is_none() {
                    return true;
                }
            }
        }
        if matches!(event, WindowEvent::Destroyed)
            && self.release_owner.is_some_and(|(owner, _)| owner == id)
        {
            self.release_owner = None;
        }
        if let WindowEvent::ModifiersChanged(modifiers) = event {
            self.modifiers = modifiers.state();
        }
        let Some(depth) = self.host.depth(id) else {
            // Some platforms retain keyboard focus on the owner window.
            if self.owns_parent(id) {
                if matches!(
                    event,
                    WindowEvent::PointerMoved { .. } | WindowEvent::PointerEntered { .. }
                ) {
                    // Menu-bar hover switching is routed by frame chrome first.
                    // Other owner motion must not highlight underlying editor content.
                    return true;
                }
                if matches!(
                    event,
                    WindowEvent::PointerButton {
                        state: ElementState::Pressed,
                        ..
                    }
                ) {
                    if let WindowEvent::PointerButton { button, .. } = event {
                        self.release_owner =
                            button.clone().mouse_button().map(|button| (id, button));
                    }
                    self.cancel();
                    return true;
                }
                if matches!(event, WindowEvent::PointerButton { .. }) {
                    // The heading press belongs to the menu, including while
                    // the evaluator has not supplied its popup contents yet.
                    return true;
                }
                if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                    self.cancel();
                    return false;
                }
                if let WindowEvent::KeyboardInput { event, .. } = event {
                    if event.state == ElementState::Pressed {
                        self.navigate(&event.logical_key);
                        self.host.request_redraw();
                    }
                    return true;
                }
            }
            return false;
        };
        if self
            .request
            .as_ref()
            .and_then(|r| r.session.panel(depth))
            .is_none_or(|panel| panel.item_indices != self.panels[depth].items)
        {
            return true;
        }
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => self.cancel(),
            WindowEvent::SurfaceResized(size) => {
                let geometry = self.host.resize(depth, device, Some(*size));
                self.panels[depth]
                    .atlas
                    .set_scale_factor(geometry.device_scale().get());
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let geometry = self.host.resize(depth, device, None);
                self.panels[depth]
                    .atlas
                    .set_scale_factor(geometry.device_scale().get());
            }
            WindowEvent::PointerMoved { position, .. }
            | WindowEvent::PointerEntered { position, .. } => {
                let scale = self.host.geometry(depth).unwrap().device_scale().get();
                self.request.as_mut().unwrap().session.hover_panel(
                    depth,
                    position.x as f32 / scale,
                    position.y as f32 / scale + self.panels[depth].scroll,
                );
                self.select_help(depth);
            }
            WindowEvent::PointerButton {
                state,
                position,
                primary: true,
                button,
                ..
            } if button.clone().mouse_button() == Some(winit::event::MouseButton::Left) => {
                let scale = self.host.geometry(depth).unwrap().device_scale().get();
                let session = &mut self.request.as_mut().unwrap().session;
                session.hover_panel(
                    depth,
                    position.x as f32 / scale,
                    position.y as f32 / scale + self.panels[depth].scroll,
                );
                if *state == ElementState::Released
                    && let Some(index) = session.activate_panel(depth)
                {
                    self.finish(index);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.navigate(&event.logical_key);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * 30.0,
                    MouseScrollDelta::PixelDelta(p) => {
                        p.y as f32 / self.host.geometry(depth).unwrap().device_scale().get()
                    }
                    _ => 0.0,
                };
                let panel = self.request.as_ref().unwrap().session.panel(depth).unwrap();
                let visible = self.host.geometry(depth).unwrap().logical_size().height();
                self.panels[depth].scroll = (self.panels[depth].scroll - delta)
                    .clamp(0.0, (panel.bounds.3 - visible).max(0.0));
                self.request
                    .as_mut()
                    .unwrap()
                    .session
                    .submenu_panels
                    .truncate(depth);
            }
            WindowEvent::RedrawRequested => {
                self.paint(depth, device, queue, renderer);
                return true;
            }
            _ => {}
        }
        self.host.request_redraw();
        true
    }

    fn navigate(&mut self, key: &winit::keyboard::Key) {
        if super::interaction::cancels(key, self.modifiers) {
            self.cancel();
            return;
        }
        let Some(request) = self.request.as_mut() else {
            return;
        };
        if let Some(index) = super::interaction::key(&mut request.session, key, self.modifiers) {
            self.finish(index);
        } else {
            self.reveal_selection();
        }
    }

    fn reveal_selection(&mut self) {
        let session = &self.request.as_ref().unwrap().session;
        let depth = session.submenu_panels.len();
        let Some(geometry) = self.host.geometry(depth) else {
            return;
        };
        let panel = session.active_panel();
        let Some(y) = usize::try_from(panel.hover_index)
            .ok()
            .and_then(|i| panel.item_offsets.get(i))
            .copied()
        else {
            return;
        };
        let visible = geometry.logical_size().height();
        if y < self.panels[depth].scroll {
            self.panels[depth].scroll = y;
        }
        if y + panel.item_height > self.panels[depth].scroll + visible {
            self.panels[depth].scroll = (y + panel.item_height - visible).max(0.0);
        }
    }

    fn paint(
        &mut self,
        depth: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) {
        let Some(request) = self.request.as_ref() else {
            return;
        };
        let Some(panel) = request.session.panel(depth) else {
            return;
        };
        let mut local = panel.clone();
        local.x = 0.0;
        local.y = -self.panels[depth].scroll;
        local.bounds.0 = local.x;
        local.bounds.1 = local.y;
        self.host.draw(depth, device, queue, |target| {
            renderer.begin_draw(target).paint_menu(
                &MenuPanelPaint {
                    panel: &local,
                    all_items: &request.session.all_items,
                    title: if depth == 0 {
                        request.session.title.as_deref()
                    } else {
                        None
                    },
                    face_fg: request.session.face_fg,
                    face_bg: request.session.face_bg,
                    font_face: request
                        .fonts
                        .font_bindings()
                        .faces
                        .get(&neomacs_display_protocol::FaceId::new(0)),
                },
                &mut self.panels[depth].atlas,
            );
        });
    }
}

impl Drop for MenuPresentation {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
#[path = "controller_test.rs"]
mod tests;
