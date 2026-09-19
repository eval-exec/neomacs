//! Present-path contract: a window that renders, resizes, and renders again
//! must show the SECOND frame's content at the SECOND geometry on every
//! wgpu backend the suite runs on.
//!
//! This is the distilled CI GUI wipeout: runners have no Vulkan, so wgpu
//! falls to the GL backend, where presents after a surface resize landed a
//! cleared buffer (white center, black edges) while the scheduler reported
//! Submitted and the ingest held the correct frame.  A dedicated
//! winit+wgpu window — no editor, no redisplay — decides whether the fault
//! lives in the present path itself (a backend quirk to encode) or in
//! neomacs's usage of it.  The backend follows WGPU_BACKEND, exactly as CI
//! selects it, so this one test runs the GL contract on runners and the
//! Vulkan contract wherever lavapipe exists.

#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use neomacs_gui_tests::DisplayHarness;
use neomacs_infra::display::DisplaySession;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId};

const INITIAL: (u32, u32) = (320, 240);
const RESIZED: (u32, u32) = (480, 360);

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    PresentingRed,
    PresentingBlue,
}

struct ContractFrame {
    window: Arc<dyn Window>,
    surface: wgpu::Surface<'static>,
    instance: wgpu::Instance,
    backend: wgpu::Backend,
    device: wgpu::Device,
    queue: wgpu::Queue,
    clear: wgpu::Color,
    last_size: (u32, u32),
}

struct ContractApp {
    session: std::mem::ManuallyDrop<DisplaySession>,
    artifacts: PathBuf,
    frame: Option<ContractFrame>,
    phase: Phase,
    red_confirmed: Arc<AtomicBool>,
    start: Instant,
}

fn configure_and_present(frame: &mut ContractFrame) {
    let size = frame.window.surface_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    // The resize policy the engine encodes: GL's emulated swapchain needs a
    // rebuilt window surface after a size change (raw wgpu presents a
    // stale-geometry buffer otherwise -- 0.67 blue, the stale-height
    // fraction); every other backend reconfigures in place.  The contract
    // asserts the policy that ships, on each backend.
    let resized = frame.last_size != (size.width, size.height);
    if resized && frame.backend == wgpu::Backend::Gl {
        frame.surface = frame
            .instance
            .create_surface(frame.window.clone())
            .expect("rebuild GL surface after resize");
    }
    frame.last_size = (size.width, size.height);
    frame.surface.configure(&frame.device, &config);
    let output = match frame.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(output)
        | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
        other => panic!("present contract lost its surface: {other:?}"),
    };
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = frame
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(frame.clear),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        ..Default::default()
    });
    frame.queue.submit([encoder.finish()]);
    frame.queue.present(output);
}

fn capture(session: &DisplaySession, xid: &str, path: &PathBuf) {
    let mut command = Command::new("import");
    command.arg("-window").arg(xid);
    for (key, value) in session.env() {
        command.env(key, value);
    }
    let status = command.arg(path).status().expect("run import");
    assert!(status.success(), "window capture failed for {xid}");
}

fn near_color_ratio(image: &image::DynamicImage, target: [u8; 3]) -> f64 {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut hits = 0_u64;
    for (_, _, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, _] = pixel.0;
        if r.abs_diff(target[0]) <= 16 && g.abs_diff(target[1]) <= 16 && b.abs_diff(target[2]) <= 16
        {
            hits += 1;
        }
    }
    hits as f64 / (width as f64 * height as f64)
}

fn x_window_id(window: &Arc<dyn Window>) -> String {
    match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::Xlib(handle) => format!("0x{:x}", handle.window),
        RawWindowHandle::Xcb(handle) => format!("0x{:x}", u64::from(handle.window.get())),
        other => panic!("present contract expects an X11 window, got {other:?}"),
    }
}

impl ApplicationHandler for ContractApp {
    fn can_create_surfaces(&mut self, _: &dyn ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::SurfaceResized(_) | WindowEvent::RedrawRequested => {
                if let Some(frame) = self.frame.as_mut() {
                    configure_and_present(frame);
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        if self.start.elapsed() > Duration::from_secs(30) {
            event_loop.exit();
            return;
        }
        self.make_frame(event_loop);
        let Some(frame) = self.frame.as_mut() else {
            return;
        };
        configure_and_present(frame);
        match self.phase {
            Phase::PresentingRed if self.start.elapsed() > Duration::from_millis(500) => {
                let xid = x_window_id(&frame.window);
                capture(&self.session, &xid, &self.artifacts.join("red.png"));
                let image = image::open(self.artifacts.join("red.png")).unwrap();
                if near_color_ratio(&image, [255, 0, 0]) > 0.90 {
                    self.red_confirmed.store(true, Ordering::SeqCst);
                    self.phase = Phase::PresentingBlue;
                    if let Some(frame) = self.frame.as_mut() {
                        frame.clear = wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        };
                        let _ = frame
                            .window
                            .request_surface_size(PhysicalSize::new(RESIZED.0, RESIZED.1).into());
                    }
                }
            }
            Phase::PresentingBlue if self.start.elapsed() > Duration::from_millis(1000) => {
                let xid = x_window_id(&frame.window);
                capture(&self.session, &xid, &self.artifacts.join("blue.png"));
                let image = image::open(self.artifacts.join("blue.png")).unwrap();
                if near_color_ratio(&image, [0, 0, 255]) > 0.90 {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

impl ContractApp {
    fn make_frame(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.frame.is_some() {
            return;
        }
        let attrs = winit::window::WindowAttributes::default()
            .with_title("present-contract")
            .with_surface_size(PhysicalSize::new(INITIAL.0, INITIAL.1));
        let window = event_loop.create_window(attrs).unwrap();
        let window: Arc<dyn Window> = Arc::from(window);
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("present contract found an adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .expect("present contract created a device");
        let backend = adapter.get_info().backend;
        self.frame = Some(ContractFrame {
            window,
            surface,
            instance,
            backend,
            device,
            queue,
            clear: wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            last_size: INITIAL,
        });
    }
}

#[test]
fn resize_then_present_shows_the_new_frame_on_the_current_backend() {
    let backend_label = std::env::var("WGPU_BACKEND").unwrap_or_else(|_| "default".to_owned());
    let artifact_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("target/neomacs-gui-tests");
    std::fs::create_dir_all(&artifact_root).unwrap();
    let artifacts = artifact_root.join(format!("present-contract-{backend_label}"));
    std::fs::create_dir_all(&artifacts).unwrap();
    for stale in ["red.png", "blue.png"] {
        let _ = std::fs::remove_file(artifacts.join(stale));
    }

    let session = DisplayHarness::Xvfb
        .start_session(&artifact_root)
        .expect("start isolated Xvfb");
    // The contract's event loop is created in this process, so the session
    // environment must become the process environment: point winit at the
    // isolated Xvfb and clear the desktop's Wayland spelling, or the loop
    // binds the operator's compositor from a foreign thread and stalls.
    // Safety: nothing else runs yet in this test process; the event loop,
    // wgpu instance, and all threads are created after this point.
    unsafe {
        for (key, value) in session.env() {
            std::env::set_var(key, value);
        }
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }

    let mut builder = EventLoop::builder();
    EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
    let event_loop = builder.build().unwrap();

    let red_confirmed = Arc::new(AtomicBool::new(false));
    let artifacts_out = artifacts.clone();
    let app = ContractApp {
        session: std::mem::ManuallyDrop::new(session),
        artifacts,
        frame: None,
        phase: Phase::PresentingRed,
        red_confirmed: Arc::clone(&red_confirmed),
        start: Instant::now(),
    };
    event_loop.run_app(app).unwrap();

    assert!(
        red_confirmed.load(Ordering::SeqCst),
        "initial frame never presented red on backend {backend_label}"
    );
    let blue = image::open(artifacts_out.join("blue.png")).expect("resized frame captured");
    assert_eq!(
        (blue.width(), blue.height()),
        RESIZED,
        "resized capture geometry on backend {backend_label}"
    );
    let ratio = near_color_ratio(&blue, [0, 0, 255]);
    assert!(
        ratio > 0.90,
        "resized frame shows {ratio:.2} blue on backend {backend_label}; \
         the present path lost the post-resize frame"
    );
}
