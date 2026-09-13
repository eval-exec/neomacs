//! Replay the decoration scale/detach sequence without Neomacs, winit or wgpu.
//! Run with `cargo run -p neomacs-gui-tests --example wayland_subsurface_scale`.
//! Optional arguments: `cached|before-detach` and `weston|desktop`.
//! The parent is roleless; its child has a subsurface role. No visible window/focus.
//! Weston 15 rejects the default cached replay; a protocol error is the
//! diagnostic result, not an expected-success test assertion.

fn main() {
    cfg_select! {
        target_os = "linux" => { linux::run(); }
        _ => { panic!("this diagnostic requires Linux Wayland"); }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use neomacs_gui_tests::{DisplayHarness, WaylandOutput};
    use std::{
        fs::File,
        os::{fd::AsFd, unix::net::UnixStream},
        path::PathBuf,
    };
    use wayland_client::{
        Connection, Dispatch, QueueHandle, delegate_noop,
        globals::{GlobalListContents, registry_queue_init},
        protocol::{
            wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor,
            wl_subsurface, wl_surface,
        },
    };

    struct State;

    #[derive(Clone, Copy, Debug, Default, strum::EnumString)]
    #[strum(serialize_all = "kebab-case")]
    enum CommitTiming {
        #[default]
        Cached,
        BeforeDetach,
    }

    #[derive(Clone, Copy, Debug, Default, strum::EnumString)]
    #[strum(serialize_all = "kebab-case")]
    enum ProbeDisplay {
        #[default]
        Weston,
        Desktop,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    delegate_noop!(State: ignore wl_compositor::WlCompositor);
    delegate_noop!(State: ignore wl_subcompositor::WlSubcompositor);
    delegate_noop!(State: ignore wl_subsurface::WlSubsurface);
    delegate_noop!(State: ignore wl_surface::WlSurface);
    delegate_noop!(State: ignore wl_shm::WlShm);
    delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
    delegate_noop!(State: ignore wl_buffer::WlBuffer);

    fn buffer(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<State>,
        width: i32,
        height: i32,
    ) -> wl_buffer::WlBuffer {
        let fd =
            rustix::fs::memfd_create(c"decoration-scale-probe", rustix::fs::MemfdFlags::CLOEXEC)
                .unwrap();
        let file = File::from(fd);
        let bytes = width * height * 4;
        file.set_len(bytes as u64).unwrap();
        let pool = shm.create_pool(file.as_fd(), bytes, qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );
        pool.destroy();
        buffer
    }

    pub fn run() {
        let mut args = std::env::args().skip(1);
        let timing: CommitTiming = args
            .next()
            .map(|arg| arg.parse().expect("cached or before-detach"))
            .unwrap_or_default();
        let display: ProbeDisplay = args
            .next()
            .map(|arg| arg.parse().expect("weston or desktop"))
            .unwrap_or_default();
        let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/neomacs-gui-tests/wayland-subsurface-scale");
        let harness = match display {
            ProbeDisplay::Weston => DisplayHarness::WestonHeadless(WaylandOutput::Standard),
            ProbeDisplay::Desktop => DisplayHarness::CurrentDesktopSession,
        };
        let session = harness.start_session(&artifacts).unwrap();
        let get = |key: &str| {
            session
                .env()
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .or_else(|| std::env::var(key).ok())
                .expect("Wayland display environment")
        };
        let socket = PathBuf::from(get("XDG_RUNTIME_DIR")).join(get("WAYLAND_DISPLAY"));
        let conn = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<State>(&conn).unwrap();
        let qh = queue.handle();
        let mut state = State;
        let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 4..=4, ()).unwrap();
        let subcompositor: wl_subcompositor::WlSubcompositor =
            globals.bind(&qh, 1..=1, ()).unwrap();
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).unwrap();
        let parent = compositor.create_surface(&qh, ());
        let child = compositor.create_surface(&qh, ());
        let _subsurface = subcompositor.get_subsurface(&child, &parent, &qh, ());
        let parent_buffer = buffer(&shm, &qh, 64, 64);
        let initial = buffer(&shm, &qh, 833, 44);
        let scaled = buffer(&shm, &qh, 1666, 88);

        child.attach(Some(&initial), 0, 0);
        child.commit();
        parent.attach(Some(&parent_buffer), 0, 0);
        parent.commit();
        queue.roundtrip(&mut state).expect("initial scale-1 state");
        println!("Applied scale-1 subsurface buffer: 833x44");

        child.set_buffer_scale(2);
        child.attach(Some(&scaled), 0, 0);
        child.commit();
        queue.roundtrip(&mut state).expect("cached scale-2 state");
        println!("Cached valid scale-2 subsurface buffer: 1666x88");

        match timing {
            CommitTiming::Cached => {}
            CommitTiming::BeforeDetach => {
                parent.commit();
                queue
                    .roundtrip(&mut state)
                    .expect("apply scale-2 state before detach");
                println!("Applied cached scale-2 state before detach");
            }
        }

        child.attach(None, 0, 0);
        child.commit();
        parent.commit();
        queue
            .roundtrip(&mut state)
            .expect("detach cached scale-2 subsurface");
        println!("Detached scale-2 subsurface without protocol error");
    }
}
