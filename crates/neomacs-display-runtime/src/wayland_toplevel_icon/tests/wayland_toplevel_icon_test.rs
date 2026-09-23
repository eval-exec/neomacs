use super::*;
use winit::event_loop::EventLoop;
use winit::platform::wayland::EventLoopBuilderExtWayland;

#[test]
#[ignore = "requires a private Wayland compositor with xdg-toplevel-icon-v1"]
fn private_wayland_compositor_accepts_native_toplevel_icon() {
    let mut builder = EventLoop::builder();
    EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
    let event_loop = builder
        .build()
        .expect("connect to private Wayland compositor");
    struct IconTest;
    impl winit::application::ApplicationHandler for IconTest {
        fn can_create_surfaces(&mut self, event_loop: &dyn winit::event_loop::ActiveEventLoop) {
            let window = event_loop
                .create_window(crate::window_identity::apply_platform_window_identity(
                    winit::window::WindowAttributes::default(),
                    event_loop,
                ))
                .expect("create private Wayland window");
            let pixels = crate::window_icon::load_window_icon().expect("decode canonical icon");
            let mut service = WaylandToplevelIconService::new();
            service
                .apply(window.as_ref(), &pixels)
                .expect("apply native Wayland toplevel icon");
            assert!(matches!(
                service.backend,
                Some(WaylandIconBackend::Protocol(_))
            ));
            service.shutdown();
            event_loop.exit();
        }
        fn window_event(
            &mut self,
            _: &dyn winit::event_loop::ActiveEventLoop,
            _: winit::window::WindowId,
            _: winit::event::WindowEvent,
        ) {
        }
    }
    event_loop
        .run_app(IconTest)
        .expect("run private Wayland icon test");
}
