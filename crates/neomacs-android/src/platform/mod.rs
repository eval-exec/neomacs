//! Android Activity and window lifecycle adapter.

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowId};

struct AndroidFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<Window>,
}

impl Default for AndroidFrontend {
    fn default() -> Self {
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
        }
    }
}

impl ApplicationHandler for AndroidFrontend {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.transition(LifecycleEvent::Resumed) != LifecycleAction::CreateFrontend {
            return;
        }

        let attributes = Window::default_attributes().with_title("Neomacs");
        let window = event_loop
            .create_window(attributes)
            .unwrap_or_else(|error| panic!("failed to create the Android window: {error}"));
        self.window = Some(window);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android may destroy the native window while retaining the Activity.
        // The renderer will eventually own surface teardown at this point; the
        // tracer bullet drops the window and recreates it on the next resume.
        if self.lifecycle.transition(LifecycleEvent::Suspended) == LifecycleAction::DestroyFrontend
        {
            self.window = None;
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.lifecycle.transition(LifecycleEvent::ExitRequested) == LifecycleAction::Exit
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }
}

/// Android Activity main-loop entrypoint required by `android-activity`.
///
/// This symbol uses Rust's ABI because `android-activity` declares it through
/// `extern "Rust"`. It can be invoked again after Activity recreation, so all
/// Activity-owned state remains local to this call.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let mut builder = EventLoop::builder();
    builder.with_android_app(app);
    let event_loop = builder
        .build()
        .unwrap_or_else(|error| panic!("failed to create the Android event loop: {error}"));

    let mut frontend = AndroidFrontend::default();
    event_loop
        .run_app(&mut frontend)
        .unwrap_or_else(|error| panic!("Android event loop failed: {error}"));
}
