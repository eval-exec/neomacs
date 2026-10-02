//! Opt-in real Wayland ownership regression. No fake native windows/driver.
use super::{gpu_startup::PendingGpu, tests::make_test_app};
use crate::thread_comm::{FrameRef, InputEvent, WindowCommand};
use std::{os::unix::net::UnixStream, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::wayland::{EventLoopBuilderExtWayland, WaylandConnection},
};

#[test]
#[ignore = "requires explicitly selected private genuine compositor socket"]
fn deferred_gui_native_gated_pending_close_recreates_and_preserves_default_close() {
    let socket =
        std::env::var("NEOMACS_TEST_WAYLAND_SOCKET").expect("selected compositor required");
    let connection = WaylandConnection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
    let mut builder = EventLoop::builder();
    // Rust harness thread only; production uses OS-main without this exception.
    builder
        .with_any_thread(true)
        .with_wayland_connection(connection);
    let event_loop = builder.build().unwrap();
    struct Probe {
        done: Arc<std::sync::atomic::AtomicBool>,
    }
    impl ApplicationHandler for Probe {
        fn window_event(
            &mut self,
            _owner: &dyn ActiveEventLoop,
            _id: winit::window::WindowId,
            _event: WindowEvent,
        ) {
        }
        fn can_create_surfaces(&mut self, owner: &dyn ActiveEventLoop) {
            for close in [WindowEvent::CloseRequested, WindowEvent::Destroyed] {
                let (emacs, render) = crate::thread_comm::ThreadComms::new().split();
                let mut app = make_test_app();
                app.comms = render;
                app.comms.keep_alive_without_frames = true;
                app.frame_windows.adopt_primary_frame_id(0x42);
                let (reply, ready) = crossbeam_channel::bounded(1);
                app.frame_windows.await_ready(0x42, reply);
                assert!(matches!(
                    ready.try_recv(),
                    Err(crossbeam_channel::TryRecvError::Empty)
                ));
                let window: Arc<dyn winit::window::Window> =
                    Arc::from(owner.create_window(Default::default()).unwrap());
                let stale = window.id();
                let weak = Arc::downgrade(&window);
                let (gate, receive) = crossbeam_channel::bounded(1);
                // Genuine native window, controlled adapter phase (no foreign
                // call entered): receiver cannot become Ready until gate opens.
                app.gpu_startup = Some(PendingGpu::Adapters {
                    window,
                    reply: receive,
                });
                app.window_event(owner, stale, close);
                assert!(
                    weak.upgrade().is_none(),
                    "OS-owner must release pending native window"
                );
                assert!(ready.try_recv().unwrap().is_err());
                assert!(matches!(
                    emacs.input_rx.try_recv().unwrap(),
                    InputEvent::WindowClose {
                        emacs_frame_id: 0x42
                    }
                ));
                assert!(!app.lifecycle_flags.is_shutting_down());
                assert!(
                    app.gpu_startup_cancelled
                        .load(std::sync::atomic::Ordering::Relaxed)
                );
                assert!(
                    gate.try_send(Err("late result".into())).is_err(),
                    "cancelled generation must reject worker completion"
                );
                app.handle_window(WindowCommand::CreateWindow {
                    frame: FrameRef::Frame(0x43),
                    width: 640,
                    height: 480,
                    title: "recreated".into(),
                    geometry_hints: neovm_core::window::GuiFrameGeometryHints {
                        base_width: 0,
                        base_height: 0,
                        min_width: 1,
                        min_height: 1,
                        width_inc: 1,
                        height_inc: 1,
                    },
                });
                let replacement: Arc<dyn winit::window::Window> =
                    Arc::from(owner.create_window(Default::default()).unwrap());
                let replacement_id = replacement.id();
                let (_gate, receive) = crossbeam_channel::bounded(1);
                app.gpu_startup = Some(PendingGpu::Adapters {
                    window: replacement,
                    reply: receive,
                });
                app.window_event(owner, stale, WindowEvent::Destroyed);
                assert_eq!(
                    app.gpu_startup.as_ref().unwrap().window_id(),
                    replacement_id
                );
                assert_eq!(app.frame_windows.primary_event_frame_id(), 0x43);
                app.window_event(owner, replacement_id, WindowEvent::CloseRequested);
                assert!(!app.lifecycle_flags.is_shutting_down());
            }
            // Ordinary non-daemon pending close still requests process shutdown.
            let mut ordinary = make_test_app();
            let window: Arc<dyn winit::window::Window> =
                Arc::from(owner.create_window(Default::default()).unwrap());
            let id = window.id();
            let (_gate, receive) = crossbeam_channel::bounded(1);
            ordinary.gpu_startup = Some(PendingGpu::Adapters {
                window,
                reply: receive,
            });
            ordinary.window_event(owner, id, WindowEvent::CloseRequested);
            assert!(ordinary.lifecycle_flags.is_shutting_down());
            assert!(
                ordinary
                    .gpu_startup_cancelled
                    .load(std::sync::atomic::Ordering::Relaxed)
            );
            self.done.store(true, std::sync::atomic::Ordering::Relaxed);
            owner.exit();
        }
    }
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    event_loop.run_app(Probe { done: done.clone() }).unwrap();
    assert!(done.load(std::sync::atomic::Ordering::Relaxed));
}
