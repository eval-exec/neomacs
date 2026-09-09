//! Opt-in compositor test. Run with WAYLAND_DEBUG=1 to inspect xdg_popup roles.

use super::{MenuPresentation, MenuRequest, MenuSession};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

#[test]
#[ignore = "requires a live Linux Wayland compositor and GPU; creates temporary windows"]
fn linux_wayland_native_menu_smoke() {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    let mut builder = EventLoop::builder();
    builder.with_wayland().with_any_thread(true);
    let event_loop = builder.build().expect("Wayland event loop");
    let painted = Arc::new(Mutex::new(HashSet::new()));
    let observed = painted.clone();
    event_loop
        .run_app(Smoke {
            menus: MenuPresentation::default(),
            graphics: None,
            parent: None,
            start: Instant::now(),
            opened: false,
            painted,
        })
        .expect("native popup event loop");
    assert!(
        observed.lock().unwrap().len() >= 2,
        "both native menu panels must receive redraws"
    );
}

struct Graphics {
    surface: wgpu::Surface<'static>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    renderer: neomacs_renderer_wgpu::WgpuRenderer,
}

struct Smoke {
    menus: MenuPresentation,
    graphics: Option<Graphics>,
    parent: Option<Arc<dyn Window>>,
    start: Instant,
    opened: bool,
    painted: Arc<Mutex<HashSet<WindowId>>>,
}

impl ApplicationHandler for Smoke {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let parent: Arc<dyn Window> = Arc::from(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Neomacs native menu smoke test")
                        .with_surface_size(winit::dpi::LogicalSize::new(200.0, 100.0)),
                )
                .unwrap(),
        );
        let mut descriptor =
            crate::wgpu_instance_descriptor_with_display(event_loop.owned_display_handle());
        descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(parent.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .unwrap();
        let (device, queue) = pollster::block_on(neomacs_renderer_wgpu::request_renderer_device(
            &adapter,
            "native menu smoke",
        ))
        .unwrap();
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let size = parent.surface_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .unwrap();
        surface.configure(&device, &config);
        let renderer = neomacs_renderer_wgpu::WgpuRenderer::with_device(
            device.clone(),
            queue.clone(),
            config.width,
            config.height,
            config.format,
            parent.scale_factor() as f32,
        );
        self.graphics = Some(Graphics {
            surface,
            instance,
            adapter,
            device,
            queue,
            renderer,
        });
        parent.request_redraw();
        self.parent = Some(parent);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.graphics.as_mut() else {
            return;
        };
        if self
            .menus
            .event(id, &event, &gpu.device, &gpu.queue, &mut gpu.renderer)
        {
            if matches!(event, WindowEvent::RedrawRequested) {
                self.painted.lock().unwrap().insert(id);
            }
            self.menus
                .sync(
                    event_loop,
                    &gpu.instance,
                    &gpu.adapter,
                    &gpu.device,
                    gpu.renderer.surface_format(),
                )
                .unwrap();
            return;
        }
        if self.parent.as_ref().is_some_and(|p| p.id() == id)
            && matches!(event, WindowEvent::RedrawRequested)
        {
            if let wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) = gpu.surface.get_current_texture()
            {
                let view = output.texture.create_view(&Default::default());
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("smoke parent"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                gpu.queue.submit(Some(encoder.finish()));
                self.parent.as_ref().unwrap().pre_present_notify();
                gpu.queue.present(output);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        let elapsed = self.start.elapsed();
        if elapsed > Duration::from_secs(5) {
            self.menus.close();
            event_loop.exit();
            return;
        }
        if !self.opened && elapsed > Duration::from_millis(300) {
            let mut root = neomacs_display_protocol::PopupMenuItem {
                label: "Submenu wider than the parent window".into(),
                shortcut: String::new(),
                enabled: true,
                separator: false,
                submenu: true,
                depth: 0,
            };
            let mut items = vec![root.clone()];
            root.submenu = false;
            root.depth = 1;
            for i in 0..30 {
                root.label = format!("Native submenu item {i}");
                items.push(root.clone());
            }
            let mut session = MenuSession::new(
                0.0,
                0.0,
                items,
                Some("Native menus".into()),
                14.0,
                18.0,
                8.4,
            );
            session.move_hover(1);
            assert!(session.open_submenu());
            self.menus.open(MenuRequest {
                token: neomacs_display_protocol::menu::MenuToken::fresh(),
                frame_id: 1,
                parent: self.parent.as_ref().unwrap().clone(),
                session,
                fonts: neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer::with_size(
                    0.0, 0.0,
                ),
                placement: neomacs_display_protocol::PopupPlacement::at(
                    neomacs_display_protocol::Point::new(180.0, 80.0),
                ),
            });
            let gpu = self.graphics.as_ref().unwrap();
            self.menus
                .sync(
                    event_loop,
                    &gpu.instance,
                    &gpu.adapter,
                    &gpu.device,
                    gpu.renderer.surface_format(),
                )
                .unwrap();
            self.opened = true;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
}

impl Drop for Smoke {
    fn drop(&mut self) {
        self.menus.close();
        self.graphics.take();
        self.parent.take();
    }
}
