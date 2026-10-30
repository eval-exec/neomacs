#![cfg(target_os = "linux")]
//! A live shm buffer must survive its pool growing.
//!
//! wl_shm lets a client grow a pool at any time (`wl_shm_pool.resize`).
//! wayland-server implements that with `mremap(..., MREMAP_MAYMOVE)`, which
//! can move the mapping; `wl_shm_buffer_get_data()` follows the new address,
//! but a compositor that cached the old one keeps reading unmapped memory.
//! Weston 16.0.0's pixman renderer did exactly that (`pixman_image_create_bits`
//! at attach; the address was never revalidated), so growing a decoration
//! `SlotPool` during a decorated resize killed the compositor with SIGSEGV in
//! `fast_composite_scaled_bilinear`, and every client lost its connection.
//!
//! This client replays the protocol sequence without Neomacs, winit or wgpu:
//! attach a buffer from a one-page pool, wait until it is on screen, grow the
//! pool to two gigabytes (an in-place mremap of that size is not available,
//! so the mapping moves), then damage the surface without re-attaching the
//! buffer.  The compositor must survive the growth and present another frame;
//! an unpatched Weston dies during the post-growth repaint.  The patch that
//! keeps this green is `scripts/ci/patches/weston-16.0.0-pixman-shm-pool-resize.patch`,
//! applied by `scripts/ci/setup-weston.sh` and by the dev shell
//! (`nix/dependencies.nix`).

use neomacs_gui_tests::{DisplayHarness, WaylandOutput};
use std::{
    fs::File,
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

/// The smallest pool that can hold the probe buffer (8x8 Argb8888).
const INITIAL_POOL_BYTES: i32 = 8 * 8 * 4;
/// Grow to just under `i32::MAX`: wayland-server mremaps the fd, and no
/// address space has two gigabytes of free mapping directly after a one-page
/// pool, so the mapping moves and the cached pointer becomes stale.
const GROWN_POOL_BYTES: i32 = i32::MAX - 4095;

struct State {
    /// Frame callbacks completed, proving the compositor presented a frame.
    frames: u32,
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
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore xdg_toplevel::XdgToplevel);

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frames += 1;
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        _: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            proxy.ack_configure(serial);
        }
    }
}

fn display_value(env: &[(String, String)], key: &str) -> String {
    env.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| panic!("Wayland display environment is missing {key}"))
}

/// Pump the queue until one more frame callback completes.
fn wait_for_frame(
    queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
    deadline: Instant,
    stage: &str,
) {
    let target = state.frames + 1;
    while state.frames < target {
        assert!(
            Instant::now() < deadline,
            "{stage}: the compositor did not present a frame (alive but idle, or its \
             connection died)"
        );
        queue
            .blocking_dispatch(state)
            .unwrap_or_else(|error| panic!("{stage}: compositor connection failed: {error}"));
    }
}

#[test]
fn a_live_shm_buffer_survives_its_pool_growing() {
    let artifacts = neomacs_infra::workspace_root()
        .join("target/neomacs-gui-tests")
        .join(format!("wayland-shm-pool-growth-{}", std::process::id()));
    std::fs::create_dir_all(&artifacts).expect("create shm-growth artifact directory");
    let session = DisplayHarness::WestonHeadless(WaylandOutput::Standard)
        .start_session(&artifacts)
        .expect("start Weston for the shm pool growth probe");
    let socket = PathBuf::from(display_value(session.env(), "XDG_RUNTIME_DIR"))
        .join(display_value(session.env(), "WAYLAND_DISPLAY"));
    let connection = Connection::from_socket(
        UnixStream::connect(&socket).expect("connect to the test compositor"),
    )
    .expect("wrap the compositor connection");
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).expect("read the compositor's global registry");
    let qh = queue.handle();
    let mut state = State { frames: 0 };

    let compositor: wl_compositor::WlCompositor =
        globals.bind(&qh, 4..=4, ()).expect("compositor global");
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).expect("wl_shm global");
    let wm_base: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=1, ()).expect("xdg_wm_base global");

    let pool_fd = rustix::fs::memfd_create(c"shm-pool-growth", rustix::fs::MemfdFlags::CLOEXEC)
        .expect("create the probe pool's memfd");
    let pool_file = File::from(pool_fd);
    pool_file
        .set_len(INITIAL_POOL_BYTES as u64)
        .expect("size the initial pool");
    let pool = shm.create_pool(pool_file.as_fd(), INITIAL_POOL_BYTES, &qh, ());
    let buffer = pool.create_buffer(0, 8, 8, 8 * 4, wl_shm::Format::Argb8888, &qh, ());

    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("shm-pool-growth".to_string());
    surface.commit();
    // The first configure arrives only after the role is committed.
    let deadline = Instant::now() + Duration::from_secs(10);
    queue
        .roundtrip(&mut state)
        .expect("receive the initial xdg configure");

    surface.attach(Some(&buffer), 0, 0);
    surface.frame(&qh, ());
    surface.commit();
    // One completed frame proves the surface is on screen and the renderer
    // has cached this buffer's pixels.
    wait_for_frame(&mut queue, &mut state, deadline, "initial presentation");

    // Grow the pool: the fd first, then the protocol request.  wayland-server
    // mremaps the pool; the surface still references the buffer created in
    // the old mapping.
    pool_file
        .set_len(GROWN_POOL_BYTES as u64)
        .expect("grow the probe pool's backing file");
    pool.resize(GROWN_POOL_BYTES);

    // Force a repaint without re-attaching: the compositor must use the live
    // buffer through its current mapping, not the address cached at attach.
    surface.damage(0, 0, 8, 8);
    surface.frame(&qh, ());
    surface.commit();
    wait_for_frame(
        &mut queue,
        &mut state,
        deadline,
        "presentation after the pool grew",
    );

    assert!(
        state.frames >= 2,
        "the compositor must present both the initial and the post-growth frame"
    );
}
