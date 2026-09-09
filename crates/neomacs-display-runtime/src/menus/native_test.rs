//! Opt-in compositor test. Run with WAYLAND_DEBUG=1 to inspect xdg_popup roles.

use super::{MenuPresentation, MenuRequest, MenuSession};
use crate::presentation::PopupCommit;
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
    run_smoke(false);
}

#[test]
#[ignore = "requires a live Linux Wayland compositor and GPU; creates temporary windows"]
fn linux_wayland_native_menu_tooltip_smoke() {
    run_smoke(true);
}

#[test]
#[ignore = "requires a live Linux Wayland compositor and GPU; creates temporary windows"]
fn linux_wayland_native_menu_replacement_stress() {
    run_replacement_stress(CommitPolicy::BackendStress);
}

#[test]
#[ignore = "requires a live Linux Wayland compositor and GPU; creates temporary windows"]
fn linux_wayland_deferred_menu_replacement_stress() {
    run_replacement_stress(CommitPolicy::PostEvent);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommitPolicy {
    PostEvent,
    /// Intentionally reconcile inside callbacks to exercise the backend fix.
    BackendStress,
}

fn run_replacement_stress(policy: CommitPolicy) {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    let mut builder = EventLoop::builder();
    builder.with_wayland().with_any_thread(true);
    let event_loop = builder.build().expect("Wayland event loop");
    event_loop
        .run_app(ReplacementStress {
            smoke: Smoke {
                policy,
                menus: MenuPresentation::default(),
                graphics: None,
                parent: None,
                start: Instant::now(),
                opened: false,
                painted: Arc::new(Mutex::new(HashSet::new())),
                with_tooltips: false,
                submenu: None,
            },
            pending: false,
            replacements: 0,
        })
        .expect("replacement must not dispatch a scale update to a destroyed popup");
}

struct ReplacementStress {
    smoke: Smoke,
    pending: bool,
    replacements: usize,
}

impl ApplicationHandler for ReplacementStress {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.smoke.can_create_surfaces(event_loop);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let replace = matches!(event, WindowEvent::SurfaceResized(_))
            && self
                .smoke
                .parent
                .as_ref()
                .is_some_and(|parent| parent.id() != id)
            && self.smoke.start.elapsed() < Duration::from_secs(4);
        self.smoke.window_event(event_loop, id, event);
        if replace {
            self.pending = true;
            // Deliberately violate the application scheduling discipline to
            // keep exercising the winit backend regression, independently of
            // Neomacs's production post-event commit policy.
            if self.smoke.policy == CommitPolicy::BackendStress {
                self.about_to_wait(event_loop);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.pending {
            self.smoke.menus.close();
            self.smoke.opened = false;
            self.smoke.submenu = None;
            self.pending = false;
            self.replacements += 1;
        }
        if self.smoke.start.elapsed() > Duration::from_secs(5) {
            assert!(
                self.replacements >= 20,
                "popup replacement was not exercised"
            );
        }
        self.smoke.about_to_wait(event_loop);
    }
}

fn run_smoke(with_tooltips: bool) {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    let mut builder = EventLoop::builder();
    builder.with_wayland().with_any_thread(true);
    let event_loop = builder.build().expect("Wayland event loop");
    let painted = Arc::new(Mutex::new(HashSet::new()));
    let observed = painted.clone();
    event_loop
        .run_app(Smoke {
            policy: CommitPolicy::PostEvent,
            menus: MenuPresentation::default(),
            graphics: None,
            parent: None,
            start: Instant::now(),
            opened: false,
            painted,
            with_tooltips,
            submenu: None,
        })
        .expect("native popup event loop");
    assert_eq!(
        observed.lock().unwrap().len(),
        if with_tooltips { 3 } else { 2 },
        "same-target hover must preserve menu surfaces and a single native tooltip"
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
    policy: CommitPolicy,
    with_tooltips: bool,
    submenu: Option<WindowId>,
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
        self.start = Instant::now();
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
                let mut painted = self.painted.lock().unwrap();
                painted.insert(id);
                if painted.len() == 2 && self.submenu.is_none() {
                    self.submenu = Some(id);
                }
            }
            if self.policy == CommitPolicy::BackendStress {
                let commit = PopupCommit::for_native_test(event_loop);
                self.menus
                    .sync(
                        &commit,
                        &gpu.instance,
                        &gpu.adapter,
                        &gpu.device,
                        &gpu.queue,
                        gpu.renderer.surface_format(),
                    )
                    .unwrap();
            }
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
        let commit = PopupCommit::for_native_test(event_loop);
        let elapsed = self.start.elapsed();
        if elapsed > Duration::from_secs(6) {
            event_loop.exit();
            return;
        }
        if elapsed > Duration::from_secs(5) {
            self.menus.shutdown();
            event_loop
                .set_control_flow(ControlFlow::WaitUntil(self.start + Duration::from_secs(6)));
            return;
        }
        if !self.opened && elapsed > Duration::from_millis(300) {
            let super::HeadingAction::Request(request_id) = self.menus.select_heading(
                super::MenuHeading {
                    frame: 1,
                    parent: self.parent.as_ref().unwrap().id(),
                    key: "help-menu".into(),
                    index: 5,
                    compact: false,
                },
                true,
            ) else {
                panic!("initial heading request")
            };
            let mut root = neomacs_display_protocol::PopupMenuItem {
                help: self
                    .with_tooltips
                    .then(|| "Native tooltip outside the small owner frame".into()),
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
                tooltips: self.with_tooltips.then(|| {
                    neomacs_display_protocol::tooltip::MenuTooltips {
                        appearance: neomacs_display_protocol::tooltip::TooltipRequest {
                            offset: (5, 20),
                            ..Default::default()
                        },
                        delay: Duration::from_millis(200),
                        short_delay: Duration::from_millis(100),
                        recent: Duration::from_secs(1),
                    }
                }),
                request_id: Some(request_id),
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
                    &commit,
                    &gpu.instance,
                    &gpu.adapter,
                    &gpu.device,
                    &gpu.queue,
                    gpu.renderer.surface_format(),
                )
                .unwrap();
            self.opened = true;
        }
        if self.opened {
            let heading = self.menus.heading().unwrap().clone();
            assert_eq!(
                self.menus.select_heading(heading, false),
                super::HeadingAction::Keep
            );
            if self.with_tooltips {
                if let Some(id) = self.submenu {
                    let gpu = self.graphics.as_mut().unwrap();
                    self.menus.event(
                        id,
                        &WindowEvent::PointerMoved {
                            device_id: None,
                            position: winit::dpi::PhysicalPosition::new(10.0, 10.0),
                            primary: true,
                            source: winit::event::PointerSource::Mouse,
                        },
                        &gpu.device,
                        &gpu.queue,
                        &mut gpu.renderer,
                    );
                    self.menus
                        .sync(
                            &commit,
                            &gpu.instance,
                            &gpu.adapter,
                            &gpu.device,
                            &gpu.queue,
                            gpu.renderer.surface_format(),
                        )
                        .unwrap();
                }
            }
        }
        let gpu = self.graphics.as_ref().unwrap();
        self.menus
            .sync(
                &commit,
                &gpu.instance,
                &gpu.adapter,
                &gpu.device,
                &gpu.queue,
                gpu.renderer.surface_format(),
            )
            .unwrap();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
}

impl Drop for Smoke {
    fn drop(&mut self) {
        self.menus.shutdown();
        self.graphics.take();
        self.parent.take();
    }
}
