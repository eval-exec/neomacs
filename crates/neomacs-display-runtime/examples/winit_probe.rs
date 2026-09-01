use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::Window;

struct ProbeApp {
    started: Instant,
    window: Option<Arc<Window>>,
}

impl ApplicationHandler for ProbeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::info!("probe: resumed");
        if self.window.is_none() {
            tracing::info!("probe: creating window");
            let attrs = Window::default_attributes()
                .with_title("Neomacs Winit Probe")
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 500.0))
                .with_transparent(true);
            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);
                    tracing::info!(
                        "probe: window created id={:?} scale={} size={:?}",
                        window.id(),
                        window.scale_factor(),
                        window.inner_size()
                    );
                    self.window = Some(window);
                }
                Err(err) => {
                    tracing::error!("probe: create_window failed: {:?}", err);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        tracing::info!("probe: window_event {:?} {:?}", window_id, event);
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > Duration::from_secs(5) {
            tracing::info!("probe: timeout exit");
            event_loop.exit();
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::info!("probe: exiting");
    }
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,winit=info,winit_probe=trace")),
        )
        .try_init();

    tracing::info!("probe: starting");

    #[cfg(target_os = "linux")]
    let event_loop = {
        let any_thread = std::env::var("NEOMACS_PROBE_ANY_THREAD")
            .map(|v| v != "0")
            .unwrap_or(true);
        tracing::info!("probe: building x11 event loop");
        let mut builder = EventLoop::builder();
        EventLoopBuilderExtX11::with_any_thread(&mut builder, any_thread);
        let event_loop = builder.build().expect("failed to build x11 event loop");
        tracing::info!("probe: built x11 event loop (any_thread={})", any_thread);
        event_loop
    };

    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new().expect("failed to build event loop");

    tracing::info!("probe: entering run_app");
    let mut app = ProbeApp {
        started: Instant::now(),
        window: None,
    };
    let result = event_loop.run_app(&mut app);
    tracing::info!("probe: run_app returned: {:?}", result);
}
