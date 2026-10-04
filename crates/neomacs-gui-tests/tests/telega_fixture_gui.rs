//! Account-free Telega frontend integration on a private display.
//!
//! These tests mount the real pinned Telega Lisp frontend (provisioned by
//! neomacs-infra), point its real `telega-server-command` at the offline
//! fixture mock, and assert on what the real root buffer/REDISPLAY produced:
//! rendered chat rows, per-row avatar pixels, and page-up/page-down viewport
//! motion.  Telega's process filter, callback dispatch, image construction,
//! and scrolling are never mocked.
//!
//! Scope: this is frontend/process/rendering integration, not TDLib/MTProto
//! correctness.  It proves nothing about the user's reported avatar
//! disappearance; it is determinism and rendering coverage for the frontend.
//! Delayed file delivery and photo replacement are checked as real frontend
//! events; see `fixtures/telega-fixture.md` for the coverage boundary.
//!
//! Isolation and cleanup: every test runs on a harness-owned Xvfb display
//! with a fresh HOME/XDG/TMPDIR tree below the fixture root, an owned
//! runtime directory published through `/proc/<pid>/fd/<fd>`, no inherited
//! user display, Wayland socket, D-Bus session, `EMACSLOADPATH`, or Telegram
//! path.  The editor is spawned in its own process group; teardown sends a
//! graceful quit and then signals exactly that recorded group, so the mock
//! and editor cannot leak (see `fixture_tears_down_its_owned_process_group`).

#![cfg(target_os = "linux")]

use neomacs_gui_tests::telega_fixture::mock::{
    ControlCommand, FIXTURE_ROOT_ENV, FIXTURE_SCENARIO_ENV, LogLine,
};
use neomacs_gui_tests::telega_fixture::scenario::{
    AVATAR_COLOR, AvatarAvailability, CHAT_COUNT, REPLACED_CHAT_AVATAR_COLOR, REPLACEMENT_COLOR,
};
use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use neomacs_infra::packages::{self, PathGnuDriver};
use serde_json::Value;
use std::cell::Cell;
use std::fs;
use std::io::Write as _;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Minimum interior pixels for one rendered 256px-source avatar (the visible
/// circle is ~13px across, so ~130 interior pixels survive clipping).
const ONE_AVATAR_PIXELS: usize = 50;
/// Horizontal band at the line beginning that owns the avatar image.
const AVATAR_BAND_WIDTH: f32 = 48.0;

fn chat_title(index: usize) -> String {
    format!("Synthetic Group {:02}", index + 1)
}

/// The color every ready avatar renders with; chat 01 is distinct so a
/// replacement can be attributed to that exact row.
fn expected_ready_color(index: usize) -> [u8; 3] {
    if index == 0 {
        REPLACED_CHAT_AVATAR_COLOR
    } else {
        AVATAR_COLOR
    }
}

#[test]
fn mock_answers_the_telega_server_version_probe() {
    let output = Command::new(mock_binary())
        .arg("-h")
        .output()
        .expect("run the fixture mock version probe");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("Version "), "{stdout}");
    let version = stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("Version "))
        .and_then(|rest| rest.split_whitespace().next())
        .expect("version number");
    let (major, minor, _patch) = parse_version(version);
    assert!(
        (major, minor) >= (0, 7),
        "fixture mock must satisfy telega-server-min-version 0.7.7, got {version}"
    );
}

fn parse_version(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.').map(|part| part.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[test]
fn telega_frontend_renders_synthetic_chats_and_paged_avatars_on_a_private_display() {
    let fixture = Fixture::start(AvatarAvailability::ReadyOnDisk, "render-paging");
    let checkpoint = fixture.wait_for_ready();
    assert_eq!(checkpoint["chats"].as_u64(), Some(CHAT_COUNT as u64));
    assert_eq!(checkpoint["auth"].as_str(), Some("\"Ready\""));
    assert_eq!(
        checkpoint["selected-buffer"].as_str(),
        Some("*Telega Root*"),
        "the fixture must show the real root buffer: {checkpoint}"
    );
    fixture.assert_isolation();
    fixture.assert_no_fixture_drift();

    // --- Page 1: every fully visible synthetic row owns its avatar. ---
    let initial_start = fixture.window_start();
    assert_eq!(initial_start, 1, "the root buffer must start at the top");
    let (top_snapshot, top_png, top_start) = fixture.corresponding_state("page-top");
    assert_eq!(
        top_start, 1,
        "the viewport moved while capturing the top page"
    );
    let top_rows = fixture.assert_visible_avatar_rows(&top_snapshot, &top_png, "top page");
    assert!(
        top_rows.len() >= 8,
        "the first page must expose many fully visible synthetic rows: {:?}",
        top_rows.iter().map(|row| row.index).collect::<Vec<_>>()
    );
    assert_eq!(
        top_rows.first().map(|row| row.index),
        Some(0),
        "same-width chat orders must place Synthetic Group 01 first: {top_rows:?}"
    );
    let top_text = snapshot_text(&top_snapshot);
    assert!(
        top_text.contains(&chat_title(0)),
        "rendered root buffer is missing synthetic chat titles: {:.400}",
        top_text
    );

    // --- Page down through real input: window-start advances each time. ---
    let mut starts = vec![initial_start];
    let mut bottom_reached = false;
    for page in 1..=6 {
        if !fixture.page_down_advances_or_bottom() {
            bottom_reached = true;
            break;
        }
        let start = fixture.window_start();
        assert!(
            start > *starts.last().expect("previous start"),
            "PageDown {page} must advance window-start: {starts:?} -> {start}"
        );
        starts.push(start);
        let (snapshot, png, checked_start) =
            fixture.corresponding_state(&format!("page-down-{page}"));
        assert_eq!(
            checked_start, start,
            "viewport moved while capturing page {page}"
        );
        let rows = fixture.assert_visible_avatar_rows(&snapshot, &png, &format!("page {page}"));
        assert!(
            !rows.is_empty(),
            "page {page} (window-start {start}) must still render fully visible avatar rows"
        );
        if fixture.window_end() >= fixture.buffer_size().saturating_add(1) {
            bottom_reached = true;
            break;
        }
    }
    assert!(
        starts.len() >= 3,
        "the fixture must page through at least three viewports: {starts:?}"
    );
    assert!(
        bottom_reached,
        "repeated PageDown must eventually reach the buffer end: {starts:?}"
    );

    // --- Page back up: window-start descends and returns to the top. ---
    let mut page = 0;
    while let Some(previous) = starts.pop() {
        if previous == 1 {
            break;
        }
        page += 1;
        fixture.press_page_up();
        fixture.wait_for_window_start_below(previous);
        let start = fixture.window_start();
        let (snapshot, png, checked_start) =
            fixture.corresponding_state(&format!("page-up-{page}"));
        assert_eq!(
            checked_start, start,
            "viewport moved while capturing page-up {page}"
        );
        fixture.assert_visible_avatar_rows(&snapshot, &png, &format!("page-up {page}"));
    }
    fixture.wait_for_window_start(1);
    assert_eq!(
        fixture.window_start(),
        1,
        "paging back up must return the viewport to the top: {starts:?}"
    );
    let (returned_snapshot, returned_png, _) = fixture.corresponding_state("page-returned");
    let returned_rows =
        fixture.assert_visible_avatar_rows(&returned_snapshot, &returned_png, "returned top page");
    assert_eq!(
        returned_rows.first().map(|row| row.index),
        Some(0),
        "returning to the top must restore Synthetic Group 01 as the first row"
    );
    fixture.assert_no_fixture_drift();
}

#[test]
fn telega_frontend_ingests_a_delayed_avatar_update_file() {
    let fixture = Fixture::start(AvatarAvailability::DelayedUntilDownload, "delayed-ingest");
    fixture.wait_for_ready();

    // Ready checkpoint: Telega asked the fixture to download the avatar.  The
    // fixture materializes the bytes only at that point, and completion is
    // withheld until the test issues the explicit delivery checkpoint.
    fixture.wait_for_mock(|log| {
        log.iter().any(|line| {
            line.record == "request" && line.type_name.as_deref() == Some("downloadFile")
        })
    });
    let before = fixture.read_checkpoint().expect("checkpoint");
    assert_eq!(
        before["avatar-spec-initials"].as_bool(),
        Some(true),
        "an undelivered avatar renders Telega's initials placeholder: {before}"
    );
    assert_ne!(
        before["file-table-downloaded"].as_bool(),
        Some(true),
        "Telega's file table must not report the avatar downloaded before delivery: {before}"
    );
    assert_ne!(
        before["avatar-file-downloaded"].as_bool(),
        Some(true),
        "the chat's photo file must not be completed before delivery: {before}"
    );
    let (_, before_png, _) = fixture.corresponding_state("before-delivery");
    let before_pixels = PixelCounts::measure(&before_png);
    assert_eq!(
        before_pixels.of(AVATAR_COLOR),
        0,
        "no undelivered avatar may render photo pixels: {before_pixels:?}"
    );
    assert_eq!(before_pixels.of(REPLACED_CHAT_AVATAR_COLOR), 0);

    // Deliver chat 01's file; its unique color makes later assertions decisive.
    let delivered_file_id = 5_000;
    fixture.control(ControlCommand::DeliverFile {
        file_id: delivered_file_id,
    });
    fixture.wait_for_mock(|log| {
        log.iter().any(|line| {
            line.record == "event"
                && line.type_name.as_deref() == Some("updateFile")
                && line.file_id == Some(delivered_file_id)
        })
    });

    // The real frontend must ingest the update: Telega's own file table
    // carries the delivered path and the cached avatar spec now references
    // the photo.
    let ingested = fixture.wait_for(
        "waiting for Telega to ingest the delivered avatar",
        Duration::from_secs(20),
        || {
            let checkpoint = fixture.read_checkpoint()?;
            let path = checkpoint["file-table-path"].as_str()?;
            let ready = checkpoint["file-table-downloaded"].as_bool() == Some(true)
                && path.ends_with("avatar-5000.png")
                && checkpoint["avatar-spec-references-photo"].as_bool() == Some(true);
            ready.then_some(checkpoint)
        },
    );
    assert_ne!(
        ingested["avatar-spec-initials"].as_bool(),
        Some(true),
        "the delivered avatar must no longer be the initials placeholder: {ingested}"
    );
    fixture.screenshot("after-delivery.png");
    fixture.assert_no_fixture_drift();
}

/// Delivery must repaint the photo, beyond merely updating Telega's file
/// table and cached SVG. No test-side image refresh is used.
#[test]
fn telega_frontend_repaints_a_delayed_avatar_after_update_file() {
    let fixture = Fixture::start(AvatarAvailability::DelayedUntilDownload, "delayed-repaint");
    fixture.wait_for_ready();
    fixture.wait_for_mock(|log| {
        log.iter().any(|line| {
            line.record == "request" && line.type_name.as_deref() == Some("downloadFile")
        })
    });
    fixture.control(ControlCommand::DeliverFile { file_id: 5_000 });
    fixture.wait_for_mock(|log| {
        log.iter().any(|line| {
            line.record == "event"
                && line.type_name.as_deref() == Some("updateFile")
                && line.file_id == Some(5_000)
        })
    });
    fixture.wait_for(
        "waiting for Telega to ingest the delivered avatar",
        Duration::from_secs(20),
        || {
            let checkpoint = fixture.read_checkpoint()?;
            (checkpoint["avatar-spec-references-photo"].as_bool() == Some(true)).then_some(())
        },
    );
    let delivered = fixture
        .wait_for_pixels(|pixels| pixels.of(REPLACED_CHAT_AVATAR_COLOR) >= ONE_AVATAR_PIXELS);
    assert_eq!(
        delivered.of(AVATAR_COLOR),
        0,
        "only the delivered avatar may show photo pixels: {delivered:?}"
    );
    let (snapshot, png, _) = fixture.corresponding_state("after-delivery");
    let delivered_rows =
        fixture.assert_visible_avatar_rows_with(&snapshot, &png, "delivered avatar", &|index| {
            (index == 0).then_some(REPLACED_CHAT_AVATAR_COLOR)
        });
    assert_eq!(delivered_rows.len(), 1, "only chat 01 was delivered");
    fixture.assert_no_fixture_drift();
}

#[test]
fn telega_frontend_replaces_a_chat_photo_without_restarting() {
    let fixture = Fixture::start(AvatarAvailability::ReadyOnDisk, "replacement");
    fixture.wait_for_ready();

    let (before_snapshot, before_png, _) = fixture.corresponding_state("before-replacement");
    let before_rows =
        fixture.assert_visible_avatar_rows(&before_snapshot, &before_png, "before replacement");
    assert_eq!(
        before_rows.first().map(|row| row.index),
        Some(0),
        "the replaced chat must be the first visible row"
    );

    fixture.control(ControlCommand::ReplacePhoto { chat_id: 1_000 });
    fixture.wait_for_mock(|log| {
        log.iter().any(|line| {
            line.record == "event"
                && line.type_name.as_deref() == Some("updateChatPhoto")
                && line.chat_id == Some(1_000)
        })
    });
    fixture.wait_for_pixels(|pixels| pixels.of(REPLACEMENT_COLOR) >= ONE_AVATAR_PIXELS);

    // The first row now renders the replacement photo; every other fully
    // visible row keeps its original avatar.
    let (after_snapshot, after_png, _) = fixture.corresponding_state("after-replacement");
    let after_rows = fixture.assert_visible_avatar_rows_with(
        &after_snapshot,
        &after_png,
        "after replacement",
        &|index| {
            if index == 0 {
                Some(REPLACEMENT_COLOR)
            } else {
                Some(AVATAR_COLOR)
            }
        },
    );
    assert_eq!(
        after_rows.iter().filter(|row| row.index == 0).count(),
        1,
        "the replaced chat row must still be rendered exactly once"
    );
    assert!(
        !after_rows
            .iter()
            .any(|row| row.color == REPLACED_CHAT_AVATAR_COLOR),
        "the old photo color must disappear from every visible row: {after_rows:?}"
    );
    fixture.screenshot("after-replacement.png");
    fixture.assert_no_fixture_drift();
}

/// Teardown contract: a started fixture owns exactly one process group (the
/// editor plus the mock it spawned) and one private display.  After teardown
/// neither may survive.
#[test]
fn fixture_tears_down_its_owned_process_group_and_display() {
    let fixture = Fixture::start(AvatarAvailability::ReadyOnDisk, "teardown");
    fixture.wait_for_ready();
    let cleanup = CleanupProbe::capture(&fixture);
    let mock_log = fixture.mock_log_path().to_path_buf();
    drop(fixture);
    cleanup.assert_released();

    let log = fs::read_to_string(&mock_log).unwrap_or_default();
    assert!(
        log.lines().any(|line| {
            serde_json::from_str::<LogLine>(line).is_ok_and(|record| {
                record.record == "ready"
                    && matches!(record.stage.as_deref(), Some("eof") | Some("stopped"))
            })
        }),
        "the mock must observe the editor's pipe closing or an explicit stop"
    );
}

/// A failing scenario body must still release the owned process group and the
/// private display through unwinding teardown.
#[test]
fn fixture_cleans_up_after_a_panicking_scenario() {
    let cleanup_slot = Cell::new(None);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture = Fixture::start(AvatarAvailability::ReadyOnDisk, "failure-cleanup");
        fixture.wait_for_ready();
        cleanup_slot.set(Some(CleanupProbe::capture(&fixture)));
        panic!("simulated fixture failure");
    }));
    assert!(outcome.is_err(), "the simulated failure must unwind");
    cleanup_slot
        .take()
        .expect("capture cleanup evidence before the simulated failure")
        .assert_released();
}

/// Keep credentials outside DisplaySession's cleanup directory so an
/// authentication failure cannot masquerade as successful display teardown.
struct CleanupProbe {
    pgid: i32,
    display: String,
    authority: PathBuf,
}

impl CleanupProbe {
    fn capture(fixture: &Fixture) -> Self {
        let authority = fixture.root.join("cleanup-probe.Xauthority");
        fs::copy(
            fixture.env_value("XAUTHORITY").expect("private XAUTHORITY"),
            &authority,
        )
        .expect("preserve the private display credentials for teardown verification");
        fs::set_permissions(&authority, fs::Permissions::from_mode(0o600))
            .expect("restrict the copied private display credentials");
        let probe = Self {
            pgid: fixture.pgid(),
            display: fixture.env_value("DISPLAY").expect("private DISPLAY"),
            authority,
        };
        assert!(
            process_group_alive(probe.pgid),
            "the owned group must be live"
        );
        assert!(
            probe.display_alive(),
            "the authenticated display probe must succeed before teardown"
        );
        probe
    }

    fn display_alive(&self) -> bool {
        Command::new("xdpyinfo")
            .env("DISPLAY", &self.display)
            .env("XAUTHORITY", &self.authority)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("execute the authenticated display probe")
            .success()
    }

    fn assert_released(self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_group_alive(self.pgid) {
            assert!(
                Instant::now() < deadline,
                "process group {} leaked after fixture teardown",
                self.pgid
            );
            thread::sleep(Duration::from_millis(50));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.display_alive() {
            assert!(
                Instant::now() < deadline,
                "private display {} leaked after fixture teardown",
                self.display
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}

// ---------------------------------------------------------------------------
// Fixture harness
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    neomacs_infra::workspace_root()
}

fn neomacs_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target/release/neomacs"))
}

fn mock_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neomacs-telega-fixture-mock"))
}

fn process_group_alive(pgid: i32) -> bool {
    // Signal 0 probes existence without delivering a signal.
    unsafe { libc::kill(-pgid, 0) == 0 }
}

fn signal_process_group(pgid: i32, signal: i32) {
    // Only ever signals the group recorded at spawn time.
    unsafe {
        libc::kill(-pgid, signal);
    }
}

/// An owned runtime directory addressed through a held descriptor.  On Linux
/// the published path is `/proc/<pid>/fd/<fd>`, which is short enough for
/// Unix sockets at any checkout depth and still resolves to the owned
/// directory (verified by `assert_isolation`).
struct OwnedRuntimeDirectory {
    path: PathBuf,
    published: PathBuf,
    _held: fs::File,
}

impl OwnedRuntimeDirectory {
    fn create(root: &Path) -> std::io::Result<Self> {
        let path = root.join("runtime");
        fs::create_dir_all(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        let held = fs::File::open(&path)?;
        let published = PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            held.as_raw_fd()
        ));
        Ok(Self {
            path,
            published,
            _held: held,
        })
    }
}

struct Fixture {
    root: PathBuf,
    provisioned_elpa: PathBuf,
    session: neomacs_infra::display::DisplaySession,
    runtime: OwnedRuntimeDirectory,
    child: Option<Child>,
    pgid: i32,
    window_id: Option<String>,
    mock_log: PathBuf,
    control: PathBuf,
    checkpoint_path: PathBuf,
    snapshot_path: PathBuf,
    snapshot_ready: PathBuf,
    snapshot_request: PathBuf,
    stderr: PathBuf,
    snapshot_sequence: Cell<u64>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Fixture {
    fn start(availability: AvatarAvailability, name: &str) -> Self {
        let workspace = workspace_root();
        let root = workspace
            .join("target/neomacs-gui-tests")
            .join(format!("tf-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        for directory in [
            "home",
            "tmp",
            "xdg/config",
            "xdg/cache",
            "xdg/data",
            "xdg/state",
            "telega-db",
            "empty-elpa",
        ] {
            fs::create_dir_all(root.join(directory)).expect("create fixture subdirectory");
        }
        let runtime = OwnedRuntimeDirectory::create(&root).expect("create owned runtime directory");

        let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
            .start_session(&root)
            .expect("start the private Xvfb session");

        // Provisioning is the only step allowed to touch the network, and it
        // happens before the scenario starts, from the locked manifest.
        let (telega_name, telega_version) = packages::TELEGA_PIN;
        let driver = PathGnuDriver::resolve().expect("resolve the GNU Emacs install driver");
        let provisioned = packages::provision(&packages::pin(telega_name, telega_version), &driver)
            .expect("provision pinned Telega from its locked source");
        let elpa_dir = provisioned.package_user_dir();
        let load_path = package_load_path(&elpa_dir);

        let paths = Paths::new(&root);
        let mut command = Command::new(neomacs_binary());
        command
            .args(["-Q", "-l"])
            .arg(workspace.join("crates/neomacs-gui-tests/fixtures/telega-fixture-gui.el"))
            .envs(session.env().iter().map(|(k, v)| (k, v)))
            .env("HOME", root.join("home"))
            .env("TMPDIR", root.join("tmp"))
            .env("XDG_RUNTIME_DIR", &runtime.published)
            .env("XDG_CONFIG_HOME", root.join("xdg/config"))
            .env("XDG_CACHE_HOME", root.join("xdg/cache"))
            .env("XDG_DATA_HOME", root.join("xdg/data"))
            .env("XDG_STATE_HOME", root.join("xdg/state"))
            .env(FIXTURE_ROOT_ENV, &root)
            .env(FIXTURE_SCENARIO_ENV, availability.name())
            .env("NEOMACS_TELEGA_MOCK", mock_binary())
            .env("NEOMACS_TELEGA_SOURCE", provisioned.source_file())
            .env("NEOMACS_TELEGA_LOAD_PATH", &load_path)
            .env("NEOMACS_TELEGA_EMPTY_ELPA", root.join("empty-elpa"))
            .env("NEOMACS_GUI_FRAME_SNAPSHOT_JSON", &paths.snapshot)
            .env("WINIT_UNIX_BACKEND", "x11")
            .env("RUST_LOG", "warn")
            // Never route private tests at the user's session, bus, loading
            // overrides, or package discovery paths.
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("EMACSLOADPATH")
            .env_remove("EMACSNATIVELOADPATH")
            .env_remove("EMACSDATA")
            .env_remove("EMACSDOC")
            .env_remove("EMACSPATH")
            .env_remove("NEOMACS_PACKAGE_USER_DIR")
            .env_remove("NEOMACS_DEBUG_FIRST_FRAME_READBACK")
            .env_remove("NEOMACS_DEBUG_SURFACE_READBACK")
            .env_remove("NEOMACS_DEBUG_SURFACE_READBACK_PNG")
            .env_remove("NEOMACS_GUI_STATE_JSON")
            .stdout(Stdio::from(
                fs::File::create(&paths.stdout).expect("stdout log"),
            ))
            .stderr(Stdio::from(
                fs::File::create(&paths.stderr).expect("stderr log"),
            ))
            // Own exactly one process group so teardown can never signal an
            // unrelated process.
            .process_group(0);
        let child = command
            .spawn()
            .expect("spawn neomacs on the private display");
        let pgid = child.id() as i32;

        let mut fixture = Self {
            root: root.clone(),
            provisioned_elpa: elpa_dir,
            session,
            runtime,
            child: Some(child),
            pgid,
            window_id: None,
            mock_log: paths.mock_log,
            control: paths.control,
            checkpoint_path: paths.checkpoint,
            snapshot_path: paths.snapshot,
            snapshot_ready: paths.snapshot_ready,
            snapshot_request: paths.snapshot_request,
            stderr: paths.stderr,
            snapshot_sequence: Cell::new(0),
        };
        fixture.window_id = Some(fixture.wait_for_window());
        fixture
    }

    fn pgid(&self) -> i32 {
        self.pgid
    }

    fn mock_log_path(&self) -> &Path {
        &self.mock_log
    }

    fn display_env(&self) -> Vec<(String, String)> {
        self.session.env().to_vec()
    }

    fn env_value(&self, key: &str) -> Option<String> {
        self.display_env()
            .into_iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }

    fn child_pid(&self) -> u32 {
        self.child.as_ref().expect("child is running").id()
    }

    fn wait_for_window(&mut self) -> String {
        let pid = self.child_pid().to_string();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.assert_alive("waiting for the editor window");
            let mut command = Command::new("xdotool");
            command.args(["search", "--pid", &pid]);
            for (key, value) in self.display_env() {
                command.env(key, value);
            }
            if let Ok(output) = command.output()
                && output.status.success()
                && let Some(id) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .map(str::to_string)
            {
                return id;
            }
            assert!(
                Instant::now() < deadline,
                "no private X11 window appeared for pid {pid}: {}",
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn assert_alive(&mut self, what: &str) {
        if let Some(child) = self.child.as_mut()
            && let Some(status) = child.try_wait().expect("poll fixture child")
        {
            panic!(
                "neomacs exited with {status} while {what}; scenario error: {}; stderr:\n{}",
                self.scenario_error().unwrap_or_else(|| "none".to_string()),
                self.stderr_tail()
            );
        }
    }

    fn scenario_error(&self) -> Option<String> {
        let contents = fs::read_to_string(self.root.join("scenario-error.json")).ok()?;
        Some(contents.trim().to_string())
    }

    fn stderr_tail(&self) -> String {
        let contents = fs::read_to_string(&self.stderr).unwrap_or_default();
        contents
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn diagnostics(&self) -> String {
        format!(
            "scenario-error={:?}\nmock-log-tail:\n{}\nstderr-tail:\n{}",
            self.scenario_error(),
            self.mock_log_text()
                .lines()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .join("\n"),
            self.stderr_tail()
        )
    }

    fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut() -> Option<T>,
    ) -> T {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(value) = probe() {
                return value;
            }
            if let Some(error) = self.scenario_error() {
                panic!("scenario failed while {what}: {error}");
            }
            assert!(
                Instant::now() < deadline,
                "timed out after {timeout:?} while {what}; {}",
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(40));
        }
    }

    fn read_checkpoint(&self) -> Option<Value> {
        let bytes = fs::read(&self.checkpoint_path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Ready checkpoint: Telega finished fetching chats, the real root buffer
    /// is displayed, and a fresh snapshot really shows rendered synthetic
    /// rows.  Readiness never depends on how many requests the frontend
    /// happened to make (caching may legitimately avoid them).
    fn wait_for_ready(&self) -> Value {
        let checkpoint = self.wait_for(
            "waiting for synthetic chats to load",
            Duration::from_secs(90),
            || {
                let checkpoint = self.read_checkpoint()?;
                let ready = checkpoint["chats-loaded"] == Value::Bool(true)
                    && checkpoint["chats"].as_u64() == Some(CHAT_COUNT as u64)
                    && checkpoint["selected-buffer"].as_str() == Some("*Telega Root*")
                    && checkpoint["window-start"].as_u64().is_some();
                ready.then_some(checkpoint)
            },
        );
        self.wait_for_rendered_rows(8);
        // Chat arrivals may preserve a point near the old insertion boundary.
        // Establish the initial viewport through the real command binding.
        self.press_key("ctrl+Home");
        self.wait_for_window_start(1);
        self.read_checkpoint().unwrap_or(checkpoint)
    }

    fn wait_for_rendered_rows(&self, minimum_titles: usize) -> Value {
        self.wait_for(
            &format!("waiting for {minimum_titles} rendered chat rows"),
            Duration::from_secs(90),
            || {
                let snapshot = self.snapshot("ready");
                let rendered = rendered_rows(&snapshot)
                    .iter()
                    .filter(|row| synthetic_index(&row.text).is_some())
                    .count();
                (rendered >= minimum_titles).then_some(snapshot)
            },
        )
    }

    fn window_start(&self) -> u64 {
        self.read_checkpoint()
            .and_then(|checkpoint| checkpoint["window-start"].as_u64())
            .expect("checkpoint carries window-start")
    }

    fn window_end(&self) -> u64 {
        self.read_checkpoint()
            .and_then(|checkpoint| checkpoint["window-end"].as_u64())
            .unwrap_or(0)
    }

    fn buffer_size(&self) -> u64 {
        self.read_checkpoint()
            .and_then(|checkpoint| checkpoint["buffer-size"].as_u64())
            .unwrap_or(u64::MAX)
    }

    fn wait_for_window_start(&self, expected: u64) {
        self.wait_for(
            &format!("waiting for window-start {expected}"),
            Duration::from_secs(15),
            || (self.window_start() == expected).then_some(()),
        );
    }

    fn wait_for_window_start_below(&self, previous: u64) {
        self.wait_for(
            &format!("waiting for window-start below {previous}"),
            Duration::from_secs(15),
            || {
                let start = self.window_start();
                (start < previous).then_some(())
            },
        );
    }

    /// Press PageDown once and wait for either a real advance or the buffer
    /// bottom.  Returns false when the bottom was reached without advancing.
    fn page_down_advances_or_bottom(&self) -> bool {
        let before = self.window_start();
        self.press_page_down();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let start = self.window_start();
            if start > before {
                return true;
            }
            if self.window_end() >= self.buffer_size().saturating_add(1) {
                return false;
            }
            assert!(
                Instant::now() < deadline,
                "PageDown neither advanced nor reached the buffer end; {}",
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(40));
        }
    }

    fn press_key(&self, key: &str) {
        let window = self.window_id.as_deref().expect("window id");
        self.xdotool(&["windowfocus", window]);
        self.xdotool(&["key", key]);
    }

    fn press_page_down(&self) {
        self.press_key("Next");
    }

    fn press_page_up(&self) {
        self.press_key("Prior");
    }

    fn xdotool(&self, args: &[&str]) {
        let mut command = Command::new("xdotool");
        command.args(args);
        for (key, value) in self.display_env() {
            command.env(key, value);
        }
        let output = command
            .output()
            .expect("run xdotool on the private display");
        assert!(
            output.status.success(),
            "xdotool {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn request_snapshot(&self, token: &str) -> Value {
        fs::write(&self.snapshot_request, token).expect("write snapshot request");
        self.wait_for(
            &format!("waiting for frame snapshot {token}"),
            Duration::from_secs(20),
            || {
                let ready = fs::read_to_string(&self.snapshot_ready).ok()?;
                (ready.trim() == token).then_some(())
            },
        );
        let bytes = fs::read(&self.snapshot_path).expect("read frame snapshot");
        serde_json::from_slice(&bytes).expect("frame snapshot is JSON")
    }

    fn snapshot(&self, label: &str) -> Value {
        let sequence = self.snapshot_sequence.get() + 1;
        self.snapshot_sequence.set(sequence);
        self.request_snapshot(&format!("{label}-{}-{sequence}", std::process::id()))
    }

    fn screenshot(&self, name: &str) -> PathBuf {
        let window = self.window_id.as_deref().expect("window id");
        let path = self.root.join(name);
        let mut command = Command::new("import");
        command.arg("-window").arg(window).arg(&path);
        for (key, value) in self.display_env() {
            command.env(key, value);
        }
        let output = command.output().expect("run import on the private display");
        assert!(
            output.status.success() && path.is_file(),
            "private-display screenshot failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        path
    }

    /// Capture a snapshot and screenshot while the viewport is stationary.
    fn corresponding_state(&self, label: &str) -> (Value, PathBuf, u64) {
        for _ in 0..5 {
            let start_before = self.window_start();
            let snapshot = self.snapshot(label);
            let png = self.screenshot(&format!("{label}.png"));
            let start_after = self.window_start();
            if start_before == start_after {
                return (snapshot, png, start_after);
            }
        }
        panic!("the viewport kept moving while capturing {label}");
    }

    /// Assert for every fully visible synthetic chat row that the row starts
    /// with an avatar image glyph and that the expected color really appears
    /// in that row's leading band of the corresponding private screenshot.
    fn assert_visible_avatar_rows(
        &self,
        snapshot: &Value,
        screenshot: &Path,
        label: &str,
    ) -> Vec<VerifiedRow> {
        self.assert_visible_avatar_rows_with(snapshot, screenshot, label, &|index| {
            Some(expected_ready_color(index))
        })
    }

    fn assert_visible_avatar_rows_with(
        &self,
        snapshot: &Value,
        screenshot: &Path,
        label: &str,
        expected: &dyn Fn(usize) -> Option<[u8; 3]>,
    ) -> Vec<VerifiedRow> {
        let image = image::open(screenshot)
            .expect("decode the private-display screenshot")
            .to_rgb8();
        let rows = rendered_rows(snapshot);
        assert!(
            !rows.is_empty(),
            "{label}: frame snapshot has no fully visible text rows"
        );
        let mut verified = Vec::new();
        for row in rows {
            let Some(index) = synthetic_index(&row.text) else {
                continue;
            };
            let Some(color) = expected(index) else {
                continue;
            };
            assert!(
                row.first_glyph_is_image,
                "{label}: {} ({}) must begin with an avatar image glyph: {:?}",
                row.text,
                chat_title(index),
                row
            );
            // Match the snapshot row/text to the image glyph's own bounds:
            // its offset within the text area plus its realized advance.
            let frame_y = row.text_area_y + row.pixel_y;
            let image_x = row.text_area_x + row.first_glyph_offset_x;
            // A zero advance means the glyph was not explicitly measured;
            // fall back to the full leading band.  The 16px floor keeps a
            // column-width advance from shrinking the probe below the
            // rendered avatar.
            let image_width = if row.first_glyph_pixel_width > 0.0 {
                row.first_glyph_pixel_width.clamp(16.0, AVATAR_BAND_WIDTH)
            } else {
                AVATAR_BAND_WIDTH
            };
            let matched =
                count_color_in_box(&image, image_x, frame_y, image_width, row.height_px, color);
            assert!(
                matched >= ONE_AVATAR_PIXELS,
                "{label}: {} has only {matched} pixels of {color:?} inside its avatar \
                 image bounds (x={image_x}, w={image_width}, row band y={frame_y}, h={}); \
                 expected the avatar image to be visible there",
                row.text,
                row.height_px
            );
            verified.push(VerifiedRow { index, color });
        }
        verified
    }

    fn wait_for_pixels(&self, predicate: impl Fn(&PixelCounts) -> bool) -> PixelCounts {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let counts = PixelCounts::measure(&self.screenshot("pixel-poll.png"));
            if predicate(&counts) {
                return counts;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for rendered pixels: {counts:?}; {}",
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn control(&self, command: ControlCommand) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.control)
            .expect("open fixture control channel");
        let line = serde_json::to_string(&command).expect("encode control command");
        writeln!(file, "{line}").expect("append control command");
        file.flush().expect("flush control command");
    }

    fn mock_log(&self) -> Vec<LogLine> {
        self.mock_log_text()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn mock_log_text(&self) -> String {
        fs::read_to_string(&self.mock_log).unwrap_or_default()
    }

    /// Whether the mock already recorded a terminal `ready` stage
    /// (`eof`/`stopped`) for its session.
    fn mock_session_ended(&self) -> bool {
        self.mock_log_text().lines().any(|line| {
            serde_json::from_str::<LogLine>(line).is_ok_and(|record| {
                record.record == "ready"
                    && matches!(record.stage.as_deref(), Some("eof") | Some("stopped"))
            })
        })
    }

    fn wait_for_mock(&self, predicate: impl Fn(&[LogLine]) -> bool) {
        self.wait_for(
            "waiting for a mock readiness checkpoint",
            Duration::from_secs(30),
            || predicate(&self.mock_log()).then_some(()),
        );
    }

    fn assert_no_fixture_drift(&self) {
        let log = self.mock_log();
        let unsupported: Vec<_> = log
            .iter()
            .filter(|line| line.record == "unsupported")
            .collect();
        assert!(
            unsupported.is_empty(),
            "the fixture mock rejected {} unmodeled request(s): {unsupported:?}",
            unsupported.len()
        );
        let violations: Vec<_> = log
            .iter()
            .filter(|line| line.record == "violation")
            .collect();
        assert!(
            violations.is_empty(),
            "the fixture mock rejected {} unknown address(es) or malformed input: {violations:?}",
            violations.len()
        );
    }

    /// Verify the process environment from /proc and the scenario's own
    /// isolation report: the private display/runtime is really ours, and no
    /// user session, bus, package, or loading override is reachable.
    fn assert_isolation(&self) {
        let pid = self.child_pid();
        let environ = fs::read(format!("/proc/{pid}/environ")).expect("read child environ");
        let environ: Vec<(String, String)> = environ
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| {
                let text = String::from_utf8_lossy(entry);
                let (key, value) = text.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect();
        let value = |key: &str| {
            environ
                .iter()
                .find_map(|(name, value)| (name == key).then_some(value.clone()))
        };
        let private_display = self
            .env_value("DISPLAY")
            .expect("session publishes DISPLAY");
        assert_eq!(
            value("DISPLAY").as_deref(),
            Some(private_display.as_str()),
            "the editor must run on the harness display"
        );
        let runtime = value("XDG_RUNTIME_DIR").expect("runtime directory is set");
        assert_eq!(
            fs::canonicalize(&runtime).expect("runtime directory resolves"),
            fs::canonicalize(&self.runtime.path).expect("owned runtime directory"),
            "the editor runtime directory must be the fixture-owned one"
        );
        assert!(
            !environ.iter().any(|(name, _)| name == "WAYLAND_SOCKET"),
            "WAYLAND_SOCKET must be absent, not empty"
        );
        assert!(
            value("WAYLAND_DISPLAY").is_none_or(|display| display.is_empty()),
            "WAYLAND_DISPLAY must not route to a user session"
        );
        assert!(
            value("DBUS_SESSION_BUS_ADDRESS").is_none(),
            "the fixture must not reach the user's D-Bus session"
        );
        assert!(
            value("EMACSLOADPATH").is_none(),
            "EMACSLOADPATH must be stripped so no personal load path is reachable"
        );
        let home = value("HOME").expect("HOME is set");
        assert!(
            home.starts_with(&self.root.to_string_lossy().to_string()),
            "HOME {home} escapes the fixture root {}",
            self.root.display()
        );

        let isolation: Value = serde_json::from_slice(
            &fs::read(self.root.join("isolation.json")).expect("scenario isolation report"),
        )
        .expect("isolation report is JSON");
        for key in [
            "telega-directory",
            "telega-database-dir",
            "telega-cache-dir",
        ] {
            let path = isolation[key].as_str().unwrap_or_default();
            assert!(
                path.starts_with(&self.root.to_string_lossy().to_string()),
                "{key} {path} escapes the fixture root"
            );
        }
        // The scenario reports the runtime directory it received; the
        // authoritative check is that it canonicalizes to the owned directory
        // even though it is published through /proc/<pid>/fd/<fd>.
        let isolation_runtime = isolation["runtime-dir"].as_str().unwrap_or_default();
        assert_eq!(
            fs::canonicalize(isolation_runtime).expect("scenario runtime directory"),
            fs::canonicalize(&self.runtime.path).expect("owned runtime directory"),
            "the scenario runtime directory must be the fixture-owned one"
        );
        assert_eq!(
            isolation["server-command"].as_str(),
            Some(mock_binary().to_string_lossy().as_ref()),
            "Telega must launch the fixture mock through telega-server-command"
        );
        assert_eq!(isolation["server-logfile"].as_str(), Some("nil"));
        assert_eq!(isolation["graphic"].as_bool(), Some(true));
        assert_eq!(
            isolation["root"].as_str(),
            Some(self.root.to_string_lossy().as_ref())
        );
        let provisioned = self.provisioned_elpa.to_string_lossy().to_string();
        let source = isolation["source"].as_str().unwrap_or_default();
        assert!(
            source.starts_with(&provisioned),
            "the mounted Telega source {source} must come from the provisioned tree"
        );
        let load_path = isolation["load-path"].as_array().expect("load-path report");
        assert!(!load_path.is_empty());
        for entry in load_path {
            let entry = entry.as_str().unwrap_or_default();
            assert!(
                entry.starts_with(&provisioned),
                "load-path entry {entry} is outside the provisioned package tree"
            );
        }
    }

    fn shutdown(&mut self) {
        if self.child.is_none() {
            return;
        }
        // Graceful quit first; only then signal the recorded group.
        let _ = fs::write(self.root.join("quit-request"), "quit");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut exited = false;
        if let Some(child) = self.child.as_mut() {
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    exited = true;
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        if !exited {
            signal_process_group(self.pgid, libc::SIGTERM);
            let deadline = Instant::now() + Duration::from_secs(2);
            if let Some(child) = self.child.as_mut() {
                while Instant::now() < deadline {
                    if child.try_wait().ok().flatten().is_some() {
                        exited = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        if !exited {
            signal_process_group(self.pgid, libc::SIGKILL);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        // Give the mock (a child of the editor in the same group) a moment to
        // observe the closed pipe and record its terminal stage; then kill
        // anything left in the recorded group.  Never signal anything else.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !self.mock_session_ended() {
            thread::sleep(Duration::from_millis(25));
        }
        if process_group_alive(self.pgid) {
            signal_process_group(self.pgid, libc::SIGKILL);
        }
        self.window_id = None;
    }
}

impl std::fmt::Debug for Fixture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Fixture")
            .field("root", &self.root)
            .field("pgid", &self.pgid)
            .finish()
    }
}

fn package_load_path(elpa_dir: &Path) -> String {
    let mut directories: Vec<String> = fs::read_dir(elpa_dir)
        .expect("read the provisioned elpa directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    directories.sort();
    assert!(
        !directories.is_empty(),
        "the provisioned elpa directory has no package trees"
    );
    directories.join(":")
}

struct Paths {
    stdout: PathBuf,
    stderr: PathBuf,
    mock_log: PathBuf,
    control: PathBuf,
    checkpoint: PathBuf,
    snapshot: PathBuf,
    snapshot_ready: PathBuf,
    snapshot_request: PathBuf,
}

impl Paths {
    fn new(root: &Path) -> Self {
        Self {
            stdout: root.join("neomacs.stdout.log"),
            stderr: root.join("neomacs.stderr.log"),
            mock_log: root.join("mock-log.jsonl"),
            control: root.join("control.jsonl"),
            checkpoint: root.join("checkpoint.json"),
            snapshot: root.join("frame-snapshot.json"),
            snapshot_ready: root.join("snapshot-ready"),
            snapshot_request: root.join("snapshot-request"),
        }
    }
}

/// One synthetic row whose avatar was verified in the corresponding
/// screenshot.
#[derive(Clone, Copy, Debug)]
struct VerifiedRow {
    index: usize,
    color: [u8; 3],
}

/// A fully visible rendered text row from the frame snapshot.
#[derive(Clone, Debug)]
struct FrameRow {
    text: String,
    pixel_y: f32,
    height_px: f32,
    text_area_x: f32,
    text_area_y: f32,
    /// True when the first visible (non-stretch) glyph is an image.
    first_glyph_is_image: bool,
    /// Horizontal offset of that glyph inside the text area, in pixels.
    first_glyph_offset_x: f32,
    /// Realized advance of that glyph in pixels (0 when unmeasured).
    first_glyph_pixel_width: f32,
}

/// Index of the synthetic chat a row title names, ignoring UI chrome rows.
fn synthetic_index(text: &str) -> Option<usize> {
    let start = text.find("Synthetic Group ")? + "Synthetic Group ".len();
    let number: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let number: usize = number.parse().ok()?;
    (1..=CHAT_COUNT).contains(&number).then_some(number - 1)
}

/// Extract fully visible text rows (row band inside the window's text area)
/// from every window matrix of the snapshot.
fn rendered_rows(snapshot: &Value) -> Vec<FrameRow> {
    let mut rows = Vec::new();
    let Some(frames) = snapshot["frames"].as_array() else {
        return rows;
    };
    for frame in frames {
        let Some(matrices) = frame["window_matrices"].as_array() else {
            continue;
        };
        for entry in matrices {
            let Some(bounds) = entry.get("text_pixel_bounds") else {
                continue;
            };
            let (Some(area_x), Some(area_y), Some(area_h)) = (
                bounds["x"].as_f64(),
                bounds["y"].as_f64(),
                bounds["height"].as_f64(),
            ) else {
                continue;
            };
            let Some(matrix_rows) = entry["matrix"]["rows"].as_array() else {
                continue;
            };
            for row in matrix_rows {
                if row["enabled"].as_bool() == Some(false)
                    || row["mode_line"].as_bool() == Some(true)
                    // A row scrolled horizontally no longer starts at the
                    // line beginning; exclude it rather than mis-attributing
                    // the avatar position.
                    || row["truncated_left"].as_bool() == Some(true)
                {
                    continue;
                }
                let (Some(pixel_y), Some(height_px)) =
                    (row["pixel_y"].as_f64(), row["height_px"].as_f64())
                else {
                    continue;
                };
                // Deliberately exclude clipped partial rows: the row band must
                // sit entirely inside the window's text area.
                if pixel_y < 0.0 || pixel_y + height_px > area_h + 0.5 {
                    continue;
                }
                let Some(glyphs) = row["glyphs"].as_array() else {
                    continue;
                };
                let Some(text_glyphs) = glyphs.get(1).and_then(Value::as_array) else {
                    continue;
                };
                let mut text = String::new();
                let mut pen_x = 0.0_f32;
                let mut first_visible: Option<(bool, f32, f32)> = None;
                for glyph in text_glyphs {
                    append_glyph_text(glyph, &mut text);
                    let advance = glyph["pixel_width"].as_f64().unwrap_or(0.0) as f32;
                    // The avatar must be the first visible glyph of the line;
                    // stretch/filler glyphs before it do not occupy pixels.
                    if first_visible.is_none() && !is_stretch_glyph(glyph) {
                        first_visible = Some((is_image_glyph(glyph), pen_x, advance));
                    }
                    pen_x += advance;
                }
                if text.trim().is_empty() {
                    continue;
                }
                let (first_glyph_is_image, first_glyph_offset_x, first_glyph_pixel_width) =
                    first_visible.unwrap_or((false, 0.0, 0.0));
                rows.push(FrameRow {
                    text,
                    pixel_y: pixel_y as f32,
                    height_px: height_px as f32,
                    text_area_x: area_x as f32,
                    text_area_y: area_y as f32,
                    first_glyph_is_image,
                    first_glyph_offset_x,
                    first_glyph_pixel_width,
                });
            }
        }
    }
    rows
}

fn is_image_glyph(glyph: &Value) -> bool {
    glyph
        .get("glyph_type")
        .and_then(Value::as_object)
        .is_some_and(|variant| variant.contains_key("Image"))
}

fn is_stretch_glyph(glyph: &Value) -> bool {
    glyph
        .get("glyph_type")
        .and_then(Value::as_object)
        .is_some_and(|variant| variant.contains_key("Stretch"))
}

fn append_glyph_text(glyph: &Value, text: &mut String) {
    let Some(variant) = glyph.get("glyph_type").and_then(Value::as_object) else {
        return;
    };
    if let Some(Value::Object(fields)) = variant.get("Char")
        && let Some(Value::String(ch)) = fields.get("ch")
    {
        text.push_str(ch);
    }
    if let Some(Value::Object(fields)) = variant.get("Composite")
        && let Some(Value::String(composite)) = fields.get("text")
    {
        text.push_str(composite);
    }
}

/// Count pixels of COLOR in the frame-relative box, clamped to the image.
fn count_color_in_box(
    image: &image::RgbImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [u8; 3],
) -> usize {
    let (image_width, image_height) = image.dimensions();
    let x0 = x.max(0.0).floor() as u32;
    let y0 = y.max(0.0).floor() as u32;
    let x1 = ((x + width).min(image_width as f32)).ceil() as u32;
    let y1 = ((y + height).min(image_height as f32)).ceil() as u32;
    let mut count = 0;
    for py in y0..y1 {
        for px in x0..x1 {
            let pixel = image.get_pixel(px, py).0;
            if pixel
                .iter()
                .zip(color.iter())
                .all(|(channel, target)| channel.abs_diff(*target) <= 6)
            {
                count += 1;
            }
        }
    }
    count
}

/// Counts pixels of the fixture's exact avatar colors in a private-display
/// screenshot (whole-frame sanity counts).
#[derive(Clone, Copy, Debug, Default)]
struct PixelCounts {
    pixels: [usize; 3],
}

impl PixelCounts {
    fn measure(path: &Path) -> Self {
        let image = image::open(path)
            .expect("decode the private-display screenshot")
            .to_rgb8();
        let mut counts = [0_usize; 3];
        let targets = [AVATAR_COLOR, REPLACED_CHAT_AVATAR_COLOR, REPLACEMENT_COLOR];
        for pixel in image.pixels() {
            let rgb = pixel.0;
            for (index, target) in targets.iter().enumerate() {
                if rgb
                    .iter()
                    .zip(target.iter())
                    .all(|(channel, target)| channel.abs_diff(*target) <= 6)
                {
                    counts[index] += 1;
                }
            }
        }
        Self { pixels: counts }
    }

    fn of(&self, color: [u8; 3]) -> usize {
        if color == AVATAR_COLOR {
            self.pixels[0]
        } else if color == REPLACED_CHAT_AVATAR_COLOR {
            self.pixels[1]
        } else if color == REPLACEMENT_COLOR {
            self.pixels[2]
        } else {
            0
        }
    }
}

/// Concatenated character text of a frame snapshot (glyph order).
fn snapshot_text(snapshot: &Value) -> String {
    let mut text = String::new();
    fn walk(value: &Value, text: &mut String) {
        match value {
            Value::Object(map) => {
                if map.contains_key("glyph_type") {
                    append_glyph_text(value, text);
                }
                for child in map.values() {
                    walk(child, text);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, text);
                }
            }
            _ => {}
        }
    }
    walk(snapshot, &mut text);
    text
}
