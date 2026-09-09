//! One menu session and its native popup chain.

use super::session::MenuSession;
use crate::presentation::PopupHost;
use neomacs_display_protocol::{
    Point, PopupConstraintPolicy, PopupPlacement, PopupPreferredSide, Rect, menu::MenuPanelPaint,
};
use neomacs_renderer_wgpu::WgpuRenderer;
use std::sync::Arc;
use winit::{
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

pub(crate) struct MenuRequest {
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
    modifiers: winit::keyboard::ModifiersState,
}

struct PanelState {
    scroll: f32,
    items: Vec<usize>,
    atlas: neomacs_renderer_wgpu::WgpuGlyphAtlas,
}

impl MenuPresentation {
    pub fn open(&mut self, request: MenuRequest) -> bool {
        if !self.lifetime.show(request.token) {
            return false;
        }
        self.truncate(0);
        self.request = Some(request);
        true
    }

    pub fn close(&mut self) {
        self.lifetime.close();
        self.truncate(0);
        self.request = None;
    }

    fn truncate(&mut self, len: usize) {
        self.host.truncate(len);
        self.panels.truncate(len);
    }

    pub fn owner(&self) -> Option<u64> {
        self.request.as_ref().map(|r| r.frame_id)
    }

    pub fn owns_parent(&self, id: WindowId) -> bool {
        self.request.as_ref().is_some_and(|r| r.parent.id() == id)
    }

    pub fn take_result(&mut self) -> Option<neomacs_display_protocol::menu::MenuResult> {
        self.lifetime.take_result()
    }

    pub fn hide(&mut self, token: neomacs_display_protocol::menu::MenuToken) -> bool {
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
        event_loop: &dyn ActiveEventLoop,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
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
        for depth in common..wanted {
            // Wayland popup parents must be mapped before creating/grabbing a child.
            if depth > 0 && !self.host[depth - 1].presented {
                break;
            }
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
            let popup = self.host.open(
                event_loop,
                request.parent.clone(),
                placement,
                (panel.bounds.2, panel.bounds.3),
                instance,
                adapter,
                device,
                format,
            )?;
            let mut atlas = neomacs_renderer_wgpu::WgpuGlyphAtlas::new_with_scale(
                device,
                popup.window.scale_factor() as f32,
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
        if let WindowEvent::ModifiersChanged(modifiers) = event {
            self.modifiers = modifiers.state();
        }
        let Some(depth) = self.host.iter().position(|p| p.window.id() == id) else {
            // Some platforms retain keyboard focus on the owner window.
            if self.request.as_ref().is_some_and(|r| r.parent.id() == id) {
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
                    self.cancel();
                    return true;
                }
                if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                    self.cancel();
                    return false;
                }
                if let WindowEvent::KeyboardInput { event, .. } = event {
                    if event.state == ElementState::Pressed {
                        self.navigate(&event.logical_key);
                        for popup in self.host.iter() {
                            popup.window.request_redraw();
                        }
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
                self.host[depth].resize(device, size.width, size.height);
                self.panels[depth]
                    .atlas
                    .set_scale_factor(self.host[depth].window.scale_factor() as f32);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = self.host[depth].window.surface_size();
                self.host[depth].resize(device, size.width, size.height);
                self.panels[depth]
                    .atlas
                    .set_scale_factor(self.host[depth].window.scale_factor() as f32);
            }
            WindowEvent::PointerMoved { position, .. }
            | WindowEvent::PointerEntered { position, .. } => {
                let scale = self.host[depth].window.scale_factor() as f32;
                self.request.as_mut().unwrap().session.hover_panel(
                    depth,
                    position.x as f32 / scale,
                    position.y as f32 / scale + self.panels[depth].scroll,
                );
            }
            WindowEvent::PointerButton {
                state,
                position,
                primary: true,
                button,
                ..
            } if button.clone().mouse_button() == Some(winit::event::MouseButton::Left) => {
                let scale = self.host[depth].window.scale_factor() as f32;
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
                        p.y as f32 / self.host[depth].window.scale_factor() as f32
                    }
                    _ => 0.0,
                };
                let panel = self.request.as_ref().unwrap().session.panel(depth).unwrap();
                let visible = self.host[depth].config.height as f32
                    / self.host[depth].window.scale_factor() as f32;
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
        for popup in self.host.iter() {
            popup.window.request_redraw();
        }
        true
    }

    fn navigate(&mut self, key: &winit::keyboard::Key) {
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
        let Some(popup) = self.host.get(depth) else {
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
        let visible = popup.config.height as f32 / popup.window.scale_factor() as f32;
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
        let popup = &mut self.host[depth];
        let output = match popup.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                popup.surface.configure(device, &popup.config);
                popup.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                popup.window.request_redraw();
                return;
            }
            _ => return,
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu clear"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        queue.submit(Some(encoder.finish()));
        let mut local = panel.clone();
        local.x = 0.0;
        local.y = -self.panels[depth].scroll;
        local.bounds.0 = local.x;
        local.bounds.1 = local.y;
        let scale = neomacs_display_protocol::DeviceScale::new(popup.window.scale_factor() as f32)
            .expect("native popup scale");
        let neomacs_display_protocol::SurfaceState::Drawable(surface) =
            neomacs_display_protocol::SurfaceState::from_device_size(
                popup.config.width,
                popup.config.height,
                scale,
            )
            .expect("native popup geometry")
        else {
            return;
        };
        renderer
            .begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                &view, surface,
            ))
            .paint_menu(
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
        popup.window.pre_present_notify();
        queue.present(output);
        popup.presented = true;
    }
}

impl Drop for MenuPresentation {
    fn drop(&mut self) {
        self.close();
    }
}
