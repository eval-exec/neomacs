//! Issue #852: Wayland click-and-drag selection coalescing regression test.
//!
//! Injects a burst of 300 real Wayland `zwlr_virtual_pointer_v1` motion events
//! during a left-button drag on an isolated headless Sway compositor with
//! `select-active-regions` enabled, and asserts count-based, load-independent
//! invariants:
//!   1. Delivered Lisp motion / selection updates stay within the glyph-cell
//!      changes along the path (GNU's rule; the fixture counts them).
//!   2. The final selected region preserves both the drag start and the exact
//!      final pointer endpoint before button release.

use serde_json::Value;
use std::{
    fs,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_pointer, wl_registry},
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 700;
const BTN_LEFT: u32 = 0x110;

struct State;
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
delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore ZwlrVirtualPointerV1);

struct VirtualPointer {
    pointer: ZwlrVirtualPointerV1,
    queue: EventQueue<State>,
    clock: u32,
}

impl VirtualPointer {
    fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<State>(&connection).unwrap();
        let qh = queue.handle();
        let manager: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ()).unwrap();
        let pointer = manager.create_virtual_pointer(None, &qh, ());
        queue.roundtrip(&mut State).unwrap();
        Self {
            pointer,
            queue,
            clock: 1,
        }
    }

    fn move_abs(&mut self, x: u32, y: u32, roundtrip: bool) {
        self.pointer
            .motion_absolute(self.clock, x, y, WIDTH, HEIGHT);
        self.pointer.frame();
        self.clock = self.clock.wrapping_add(2);
        if roundtrip {
            self.queue.roundtrip(&mut State).unwrap();
        } else {
            self.queue.flush().unwrap();
        }
    }

    fn button(&mut self, pressed: bool) {
        let state = if pressed {
            wl_pointer::ButtonState::Pressed
        } else {
            wl_pointer::ButtonState::Released
        };
        self.pointer.button(self.clock, BTN_LEFT, state);
        self.pointer.frame();
        self.clock = self.clock.wrapping_add(2);
        self.queue.roundtrip(&mut State).unwrap();
    }
}

fn wait_for_json(path: &Path, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn wayland_drag_selection_coalesces_motion_burst_and_preserves_final_endpoint() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/issue852-wayland-drag-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();

    let session = neomacs_infra::display::start_sway(
        &artifacts,
        &format!(
            "output * resolution {WIDTH}x{HEIGHT}\nxwayland disable\nseat seat0 fallback true\ndefault_border none\n"
        ),
    )
    .expect("private Sway compositor");

    let runtime_dir = session
        .env()
        .iter()
        .find(|(k, _)| k == "XDG_RUNTIME_DIR")
        .map(|(_, v)| PathBuf::from(v))
        .expect("Sway XDG_RUNTIME_DIR");
    let wl_display = session
        .env()
        .iter()
        .find(|(k, _)| k == "WAYLAND_DISPLAY")
        .map(|(_, v)| v.clone())
        .expect("Sway WAYLAND_DISPLAY");

    // Register virtual pointer before starting Neomacs so seat0 advertises
    // WL_SEAT_CAPABILITY_POINTER immediately.
    let mut vp = VirtualPointer::connect(&runtime_dir.join(&wl_display));
    vp.move_abs(120, 40, true);

    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/wayland-drag-selection.el");
    let ready_path = artifacts.join("ready.json");
    let pressed_path = artifacts.join("pressed.json");
    let out_path = artifacts.join("result.json");

    let mut cmd = Command::new(&binary);
    cmd.args(["-Q", "-l"])
        .arg(&fixture)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("WINIT_UNIX_BACKEND", "wayland")
        .env("WGPU_BACKEND", "vulkan")
        .env("NEOMACS_DRAG_TEST_READY", &ready_path)
        .env("NEOMACS_DRAG_TEST_PRESSED", &pressed_path)
        .env("NEOMACS_DRAG_TEST_OUT", &out_path);
    for (k, v) in session.env() {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().expect("spawn release neomacs on Wayland");

    let _ready = wait_for_json(&ready_path, Duration::from_secs(20));
    let _target = crate::wayland_readiness::wait_for_pointer_target(
        &runtime_dir,
        &mut child,
        &artifacts,
        120,
        40,
    );

    // Move into the mapped window and press left button; wait causally for
    // `mouse-drag-region` to enter before injecting the 300-step motion burst.
    vp.move_abs(110, 40, true);
    vp.move_abs(120, 40, true);
    vp.button(true);
    let _pressed = wait_for_json(&pressed_path, Duration::from_secs(10));

    const STEPS: u32 = 300;
    for i in 1..=STEPS {
        let x = 120 + (500 * i) / STEPS;
        let y = 40 + (500 * i) / STEPS;
        vp.move_abs(x, y, false);
    }
    vp.queue.roundtrip(&mut State).unwrap();
    vp.button(false);

    let result = wait_for_json(&out_path, Duration::from_secs(20));
    let _ = child.wait();

    let move_count = result["move_count"].as_u64().expect("move_count");
    let selection_count = result["selection_count"].as_u64().expect("selection_count");
    let final_pt = result["final_pt"].as_i64().expect("final_pt");
    let final_mk = result["final_mk"].as_i64().expect("final_mk");
    let expected_start = result["expected_start"].as_i64().expect("expected_start");
    let expected_end = result["expected_end"].as_i64().expect("expected_end");
    let region_active = result["region_active"].as_bool().expect("region_active");

    assert!(
        region_active,
        "drag selection must remain active after button release: {result}"
    );
    // GNU reports a movement only when the pointer leaves the glyph it was
    // last over (`x_note_mouse_movement`, xterm.c:14740-14773; pgtkterm.c
    // does the same on Wayland), so the handled drag steps, and the PRIMARY
    // updates they make, stay within the glyph-cell changes along the path.
    // The X11 row `drag_selection_motion_gui_oracle` checks GNU obeys this.
    let cell_changes = result["cell_changes"].as_u64().expect("cell_changes");
    assert!(
        move_count >= 1 && move_count <= cell_changes,
        "300 Wayland pointer moves gave {move_count} drag steps for {cell_changes} glyph-cell changes: {result}"
    );
    assert!(
        selection_count <= cell_changes,
        "PRIMARY updates ({selection_count}) exceed the glyph-cell changes ({cell_changes}): {result}"
    );
    assert_eq!(
        final_mk, expected_start,
        "drag start mark must match start pointer position: {result}"
    );
    assert_eq!(
        final_pt, expected_end,
        "final drag endpoint must match the last pointer position before release: {result}"
    );
}
