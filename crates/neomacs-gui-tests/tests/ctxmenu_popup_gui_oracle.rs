#![cfg(target_os = "linux")]
//! M-CTXMENU: right-click popup menus must appear, on GNU's terms.
//!
//! GNU contract (source, emacs-31.1 a360712c):
//! - A mouse key bound to a menu keymap pops the menu from `read_char'
//!   itself: `read_char_x_menu_prompt' (src/keyboard.c:10094) fires when
//!   the previous event was a mouse event and the pending map is a
//!   keymap (call site src/keyboard.c:2921), so the DOWN click opens the
//!   menu at the pointer.  `context-menu-entry' (lisp/mouse.el:689-703)
//!   is this shape, and `C-down-mouse-3' (mouse.el:3805) likewise.
//! - `x-popup-menu' displays old-fashioned list menus
//!   (src/menu.c:1352-1364, `list_of_panes') and lists of keymaps
//!   (src/menu.c:1318-1345, `keymap_panes'), not just single keymaps.
//!   `mouse-buffer-menu', flyspell-emacs-popup, and other list-form
//!   callers rely on it.
//!
//! The same X11 input sequence is injected into GNU Emacs and Neomacs on
//! one Xvfb display; each editor is checked against the contract.  The
//! "menu appeared" oracle is a screenshot diff, so the assertion does
//! not depend on either editor's menu implementation details.

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    ProcessGuiCommandRunner,
};
#[path = "native_menus/popup_trace.rs"]
#[allow(unused_imports)]
mod popup_trace;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/ctxmenu-popup.el";
const SCENARIO: &str = "ctxmenu-popup-oracle";
const READY_ENV: &str = "NEOMACS_CTXMENU_READY";
const RESULT_ENV: &str = "NEOMACS_CTXMENU_RESULT";
const STOP_ENV: &str = "NEOMACS_CTXMENU_STOP";

fn gnu_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GNU_GUI_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"))
}

fn neomacs_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn xdotool(env: &[(String, String)], args: &[&str]) {
    let output = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output()
        .expect("xdotool must be installed for the context menu oracle");
    assert!(
        output.status.success(),
        "xdotool {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn xdotool_allow_failure(env: &[(String, String)], args: &[&str]) {
    let _ = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output();
}

/// Pick the largest visible window.  On a bare Xvfb there is no window
/// manager, so the editor's own window is the meaningful one; tiny
/// helper windows are ignored.
fn find_window(env: &[(String, String)]) -> Option<String> {
    let search = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", ".*"])
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output()
        .ok()?;
    if !search.status.success() {
        return None;
    }
    let ids = String::from_utf8_lossy(&search.stdout);
    let mut best: Option<(u64, String)> = None;
    for id in ids.split_whitespace() {
        let Ok(output) = Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", id])
            .envs(env.iter().map(|(key, value)| (key, value)))
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let geometry = String::from_utf8_lossy(&output.stdout);
        let dimension = |name: &str| {
            geometry
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|value| value.trim().parse::<u64>().ok())
        };
        let (Some(width), Some(height)) = (dimension("WIDTH="), dimension("HEIGHT=")) else {
            continue;
        };
        if width < 200 || height < 100 {
            continue;
        }
        let area = width * height;
        let is_larger = match &best {
            None => true,
            Some((best_area, _)) => area > *best_area,
        };
        if is_larger {
            best = Some((area, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

fn capture(env: &[(String, String)], artifacts: &Path, name: &str) -> image::RgbImage {
    let path = artifacts.join(format!("{name}.png"));
    let output = Command::new("import")
        .args(["-window", "root"])
        .arg(&path)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "screenshot {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    image::open(&path).unwrap().to_rgb8()
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// The bounding box of pixels that changed more than the tolerance, over
/// the whole screen: whatever menu or window appeared.
fn changed_rect(before: &image::RgbImage, after: &image::RgbImage) -> Option<Rect> {
    let (width, height) = (before.width(), before.height());
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if before
                .get_pixel(x, y)
                .0
                .iter()
                .zip(after.get_pixel(x, y).0)
                .any(|(a, b)| a.abs_diff(b) > 8)
            {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    (right > left && bottom > top).then(|| Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Per-editor selection clicks, as offsets from the pointer that opened the
/// menu.  GTK (GNU) and the neomacs popup draw different row heights, so the
/// first selectable row sits at a different distance under each.
#[derive(Clone, Copy)]
struct SelectOffsets {
    case1: (i32, i32),
    case2: (i32, i32),
    case2b: (i32, i32),
}

const GNU_SELECTS: SelectOffsets = SelectOffsets {
    case1: (70, 45),
    case2: (70, 55),
    case2b: (70, 48),
};

const NEO_SELECTS: SelectOffsets = SelectOffsets {
    // Measured off the Weston leg's `xdg_popup .configure(52, 40, 150,
    // 48)` from a click at (60, 40): 24px rows, popup top-left at the
    // click minus (8, 0).  Case 1 is title 0-24, item 24-48; case 2 adds
    // the pane-label row (item 48-72); case 2b's first pane item is also
    // row 24-48 (the separator comes after it).
    case1: (40, 36),
    case2: (40, 60),
    case2b: (40, 36),
};

/// Inject the repro sequence once the fixture signals readiness: right
/// click on a keymap binding, two list-form popups, then the armed
/// `context-menu-mode' right click.  Each menu is located by screenshot
/// diff; selection clicks stay at the current pointer because GTK pops
/// the menu with the pointer already on the first item (verified against
/// the oracle: the item row sits at the click that opened the menu).
/// Every case ends with Escape so no menu survives into the next one.
fn inject_sequence(
    env: &[(String, String)],
    ready: &Path,
    artifacts: &Path,
    stop: &Path,
    label: &str,
    selects: SelectOffsets,
) {
    let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(20));
    while !ready.exists() {
        assert!(
            !deadline.is_expired(),
            "{label} fixture did not become ready"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let window = loop {
        if let Some(window) = find_window(env) {
            break window;
        }
        assert!(
            !deadline.is_expired(),
            "{label} did not map a visible window"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let window = window.as_str();
    xdotool_allow_failure(env, &["windowfocus", window]);
    xdotool_allow_failure(env, &["windowraise", window]);
    std::thread::sleep(Duration::from_millis(800));
    let observations = artifacts.to_path_buf();
    // Open a menu with `button' at window-relative (x, y), confirm it
    // appeared against `before', optionally select the item at
    // window-relative offset (select_dx, select_dy) from the opening
    // click — menus pop with their top-left at the pointer, and the item
    // row sits under the header rows — and make sure no menu survives
    // into the next case.  The select is addressed window-relative rather
    // than as a root delta off `getmouselocation': under the Weston leg
    // that read can return a stale position while a popup grab is up,
    // which silently retargets the click.
    // Escape is sent only when a selection click failed to close the
    // menu: a stray Escape reaches the editor, echoes "Quit", and that
    // echo leaking into the next diff reads as a phantom change.
    let open_select_and_close = |case: &str,
                                 before: &image::RgbImage,
                                 x: i32,
                                 y: i32,
                                 button: &str,
                                 select: Option<(i32, i32)>|
     -> Option<Rect> {
        xdotool(
            env,
            &[
                "mousemove",
                "--window",
                window,
                &x.to_string(),
                &y.to_string(),
            ],
        );
        xdotool(env, &["click", button]);
        std::thread::sleep(Duration::from_millis(700));
        let shot = capture(env, &observations, case);
        let rect = changed_rect(before, &shot);
        if rect.is_some() {
            if let Some((dx, dy)) = select {
                xdotool(
                    env,
                    &[
                        "mousemove",
                        "--window",
                        window,
                        &(x + dx).to_string(),
                        &(y + dy).to_string(),
                    ],
                );
                xdotool(env, &["click", "1"]);
                std::thread::sleep(Duration::from_millis(400));
                let settled = capture(env, &observations, &format!("{case}-settled"));
                if changed_rect(&shot, &settled).is_none() {
                    // The click landed on a non-selectable row and the
                    // menu is still up: close it explicitly.
                    xdotool_allow_failure(env, &["key", "--clearmodifiers", "Escape"]);
                    std::thread::sleep(Duration::from_millis(300));
                }
            } else {
                xdotool_allow_failure(env, &["key", "--clearmodifiers", "Escape"]);
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        rect
    };
    let baseline = capture(env, &observations, "00-baseline");
    // Case 1: [down-mouse-3] bound to a menu keymap; title row then the
    // item, so the item sits ~45px below the pointer (GTK) or ~36px
    // (neomacs's 24px rows).
    let case1 = open_select_and_close("01-case1-open", &baseline, 60, 40, "3", Some(selects.case1));
    // Case 2: old-fashioned list menu via `x-popup-menu'; title, pane
    // label, then the item (~55px below the pointer on GTK, ~60px on
    // neomacs).
    let base2 = capture(env, &observations, "10-base2");
    let case2 = open_select_and_close("11-case2-open", &base2, 60, 120, "2", Some(selects.case2));
    // Case 2b: list of keymaps; the pane title, then the first item
    // (~48px down on GTK, ~36px on neomacs: between the panes GTK draws
    // the pane separator, and a click there dismisses nothing — it hits
    // no item row).
    let base3 = capture(env, &observations, "20-base3");
    let case2b =
        open_select_and_close("21-case2b-open", &base3, 60, 160, "2", Some(selects.case2b));
    // Case 3: arm context-menu-mode and right-click like the report.
    // Appearance only; the context menu's row layout varies.
    xdotool(env, &["key", "--clearmodifiers", "ctrl+c"]);
    std::thread::sleep(Duration::from_millis(150));
    xdotool(env, &["key", "--clearmodifiers", "c"]);
    std::thread::sleep(Duration::from_millis(400));
    let base4 = capture(env, &observations, "30-base4");
    let case3 = open_select_and_close("31-case3-open", &base4, 300, 200, "3", None);
    let show = |rect: &Option<Rect>| match rect {
        Some(rect) => format!("Some({},{},{}x{})", rect.x, rect.y, rect.width, rect.height),
        None => "None".to_string(),
    };
    let summary = format!(
        "{label} observations: case1={} case2={} case2b={} case3={}",
        show(&case1),
        show(&case2),
        show(&case2b),
        show(&case3)
    );
    std::fs::write(
        artifacts.join(format!("{label}-observations.txt")),
        &summary,
    )
    .unwrap();
    eprintln!("{summary}");
    std::fs::write(stop, "stop").unwrap();
}

struct EditorRun {
    observations_path: PathBuf,
    result: String,
}

/// The GNU leg runs the attested gtk3 oracle as a plain X11 client of the
/// harness Xvfb, exactly like the original repro.
fn run_gnu(
    program: PathBuf,
    session_env: &[(String, String)],
    artifact_root: &Path,
    runner: &mut ProcessGuiCommandRunner,
) -> EditorRun {
    let label = "gnu";
    let artifacts = artifact_root.join(label);
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("ready");
    let result = artifacts.join("result.el");
    let stop = artifacts.join("stop");
    for path in [&ready, &result, &stop] {
        let _ = std::fs::remove_file(path);
    }
    let observations_path = artifacts.join(format!("{label}-observations.txt"));

    let mut env: Vec<(String, String)> = vec![
        (READY_ENV.to_string(), ready.display().to_string()),
        (RESULT_ENV.to_string(), result.display().to_string()),
        (STOP_ENV.to_string(), stop.display().to_string()),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
    ];
    env.extend(session_env.iter().cloned());

    let input_env = env.clone();
    let input_ready = ready.clone();
    let input_stop = stop.clone();
    let input_artifacts = artifacts.clone();
    let input = std::thread::spawn(move || {
        inject_sequence(
            &input_env,
            &input_ready,
            &input_artifacts,
            &input_stop,
            label,
            GNU_SELECTS,
        )
    });

    let command = CommandSpec {
        program,
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            neomacs_infra::crate_root!()
                .join("../..")
                .join(FIXTURE)
                .display()
                .to_string(),
        ],
        env,
    };
    let output = runner
        .run(
            &command,
            &GuiArtifactSet::new(artifact_root, GuiBackend::LinuxX11, SCENARIO),
            &GuiRunOptions::with_timeout(Duration::from_secs(90)),
        )
        .expect("GNU context menu run should start");
    input.join().expect("input injection must not panic");
    assert!(
        output.exit_code == Some(0),
        "GNU exited {:?}; stderr:\n{}",
        output.exit_code,
        output.stderr
    );
    let result_text = std::fs::read_to_string(&result)
        .unwrap_or_else(|error| panic!("GNU did not write {result:?}: {error}"));
    EditorRun {
        observations_path,
        result: result_text,
    }
}

/// The neomacs leg runs as a Wayland client under Weston (X11 backend, kiosk
/// shell) on a private Xvfb — the repo's native-menu harness environment.
/// Neomacs presents popup menus through its Wayland backend; the forked
/// winit has no X11 popup ("Popups are not implemented for X11"), so a bare
/// Xvfb leg cannot exercise menus at all.
fn run_neomacs(root: &Path, program: PathBuf, artifact_root: &Path) -> EditorRun {
    let label = "neomacs";
    let artifacts = artifact_root.join(label);
    let _ = std::fs::remove_dir_all(&artifacts);
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("ready");
    let result = artifacts.join("result.el");
    let stop = artifacts.join("stop");
    let observations_path = artifacts.join(format!("{label}-observations.txt"));

    // Keep the handle alive: this short alias avoids Unix socket path limits.
    let directory = std::fs::File::open(&artifacts).unwrap();
    let runtime = format!("/proc/{}/fd/{}", std::process::id(), directory.as_raw_fd());
    // The compositor's X11 backend enables its output through MIT-SHM, which
    // an X server denies to TCP clients; the local socket is the only
    // transport that carries it.
    let display =
        neomacs_infra::display::start_local_xvfb_with_size(&artifacts, 1280, 800).unwrap();
    let log = std::fs::File::create(artifacts.join("weston.log")).unwrap();
    let compositor =
        Command::new(std::env::var_os("NEOMACS_GUI_WESTON").unwrap_or_else(|| "weston".into()))
            .args([
                "--backend=x11",
                "--renderer=pixman",
                "--shell=kiosk-shell.so",
                "--no-config",
                "--width=1000",
                "--height=700",
                "--socket=ctxmenu-test",
            ])
            .envs(display.env().iter().map(|(k, v)| (k, v)))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env_remove("WAYLAND_DISPLAY")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("install Weston with X11 backend and kiosk shell");
    let mut compositor = OwnedChild(compositor);
    wait_until("Wayland socket", &artifacts, || {
        compositor_alive(&mut compositor, &artifacts);
        artifacts.join("ctxmenu-test").exists().then_some(())
    });

    let editor = Command::new(program)
        .args(["-Q", "-l"])
        .arg(root.join(FIXTURE))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("WAYLAND_DISPLAY", "ctxmenu-test")
        .env_remove("DISPLAY")
        .env("WINIT_UNIX_BACKEND", "wayland")
        // Pin the software Vulkan backend the rest of the suite presents
        // through: the runner image has no EGL screen.
        .env("WGPU_BACKEND", "vulkan")
        .env(READY_ENV, artifacts.join("ready").display().to_string())
        .env(
            RESULT_ENV,
            artifacts.join("result.el").display().to_string(),
        )
        .env(STOP_ENV, artifacts.join("stop").display().to_string())
        .env("WAYLAND_DEBUG", "1")
        .env("RUST_LOG", "warn,neomacs_display_runtime=debug")
        .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
        .stdout(Stdio::null())
        .stderr(Stdio::from(
            std::fs::File::create(artifacts.join("wayland.log")).unwrap(),
        ))
        .stdin(Stdio::null())
        .spawn()
        .expect("fresh-build the editor runtime");
    let mut editor = OwnedChild(editor);

    // Lisp readiness can precede the first GPU swap; Weston maps the click
    // target only after the first content commit, so wait for the surface's
    // compositor frame callback before sending input.
    wait_until("fixture readiness", &artifacts, || {
        compositor_alive(&mut compositor, &artifacts);
        editor_alive(&mut editor, &artifacts);
        let presented = std::fs::read_to_string(artifacts.join("wayland.log"))
            .ok()
            .map(|log| popup_trace::primary_surface_presented(&log))
            .unwrap_or(false);
        (ready.exists() && presented).then_some(())
    });

    let input_env: Vec<(String, String)> = display.env().to_vec();
    let input_ready = ready.clone();
    let input_stop = stop.clone();
    let input_artifacts = artifacts.clone();
    let input = std::thread::spawn(move || {
        inject_sequence(
            &input_env,
            &input_ready,
            &input_artifacts,
            &input_stop,
            label,
            NEO_SELECTS,
        )
    });
    input.join().expect("input injection must not panic");

    let status = wait_until("editor shutdown", &artifacts, || {
        editor.0.try_wait().unwrap()
    });
    assert!(
        status.success(),
        "neomacs exited {status}; artifacts: {}",
        artifacts.display()
    );
    let result_text = std::fs::read_to_string(&result)
        .unwrap_or_else(|error| panic!("neomacs did not write {result:?}: {error}"));
    EditorRun {
        observations_path,
        result: result_text,
    }
}

/// Kill a child when the handle drops; the suite must not leak compositors.
struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn compositor_alive(child: &mut OwnedChild, artifacts: &Path) {
    if let Some(status) = child.0.try_wait().unwrap() {
        panic!(
            "compositor exited unexpectedly: {status}; artifacts: {}",
            artifacts.display()
        );
    }
}

fn editor_alive(child: &mut OwnedChild, artifacts: &Path) {
    if let Some(status) = child.0.try_wait().unwrap() {
        panic!(
            "neomacs exited before input: {status}; log: {:?}; artifacts: {}",
            std::fs::read_to_string(artifacts.join("neomacs.log"))
                .unwrap_or_else(|_| "<missing>".into()),
            artifacts.display()
        );
    }
}

fn wait_until<T>(what: &str, artifacts: &Path, poll: impl FnMut() -> Option<T>) -> T {
    let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(30));
    let mut poll = poll;
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(
            !deadline.is_expired(),
            "timed out waiting for {what}; artifacts: {}",
            artifacts.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn observed(run: &EditorRun) -> String {
    std::fs::read_to_string(&run.observations_path)
        .unwrap_or_else(|error| panic!("observations missing: {error}"))
}

/// The keymap-menu half of the GNU contract: menus popped by a mouse key
/// bound to a menu keymap, read through `read_char' itself.  This is the
/// `read_char_x_menu_prompt' path (src/keyboard.c:2921/10094).
fn assert_keymap_menu_contract(label: &str, run: &EditorRun) {
    let observations = observed(run);
    let events = run.result.trim();
    println!("{label} events: {events}");
    println!("{label} {observations}");
    assert!(
        observations.contains("case1=Some("),
        "{label}: the [down-mouse-3] keymap binding must pop a menu (GNU read_char_x_menu_prompt, src/keyboard.c:2921/10094)"
    );
    assert!(
        events.contains("keymap-choose"),
        "{label}: selecting the popped menu item must run its command; events: {events:?}"
    );
    if events.contains("(context-menu-mode-armed t)") {
        assert!(
            observations.contains("case3=Some("),
            "{label}: context-menu-mode right-click must pop the context menu"
        );
    }
}

/// The list-menu half: `x-popup-menu' displaying old-fashioned list menus
/// (`list_of_panes', src/menu.c:1352-1364) and lists of keymaps
/// (`keymap_panes', src/menu.c:1318-1345).
fn assert_list_menu_contract(label: &str, run: &EditorRun) {
    let observations = observed(run);
    let events = run.result.trim();
    println!("{label} events: {events}");
    println!("{label} {observations}");
    assert!(
        observations.contains("case2=Some("),
        "{label}: the old-fashioned list menu must display (GNU src/menu.c:1352-1364 list_of_panes)"
    );
    assert!(
        events.contains("(call 1 choose)"),
        "{label}: the list menu selection must return the item value `choose'; events: {events:?}"
    );
    assert!(
        observations.contains("case2b=Some("),
        "{label}: the list of keymaps must display (GNU src/menu.c:1318-1345 keymap_panes)"
    );
    // A dismissal records nothing at all, not nil: GNU's xmenu.c:2152
    // makes Cancel with a `t' position equivalent to C-g ("Make `Cancel'
    // equivalent to C-g" — `quit', which condition-case does not catch
    // and menu.c:1132 does not mark MENU_FOR_CLICK).  So the selection
    // click must land on an item row; the offset above aims at the first
    // selectable row, under the pane title and past the pane separator.
    assert!(
        events.contains("(call 2"),
        "{label}: the keymap-list popup must return a selection, not a quit; events: {events:?}"
    );
    let call2 = events
        .split("(call 2")
        .nth(1)
        .unwrap_or("missing")
        .split(')')
        .next()
        .unwrap_or("unterminated");
    assert!(
        !call2.trim().is_empty() && call2.trim() != "nil" && !events.contains("(error 2"),
        "{label}: the keymap-list selection must return a value; events: {events:?}"
    );
}

/// The GNU contract, asserted identically for GNU Emacs and Neomacs.
fn assert_contract(label: &str, run: &EditorRun) {
    assert_keymap_menu_contract(label, run);
    assert_list_menu_contract(label, run);
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, xdotool, ImageMagick, and
// Weston (X11 backend, kiosk shell) for the neomacs leg.
fn right_click_menu_popups_behave_like_gnu() {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join("target/neomacs-gui-tests");
    std::fs::create_dir_all(&artifact_root).unwrap();
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("Xvfb display session should start");
    let mut runner = ProcessGuiCommandRunner;

    let gnu = run_gnu(gnu_binary(), session.env(), &artifact_root, &mut runner);
    assert_contract("GNU", &gnu);
    drop(session);

    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the context menu oracle"
    );
    let neo = run_neomacs(&root, binary, &artifact_root);
    assert_contract("Neomacs", &neo);
}
