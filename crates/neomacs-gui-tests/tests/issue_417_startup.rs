#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunResult, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};

#[test]
fn gnu_initial_gui_frame_is_created_between_early_and_regular_init() {
    check_startup(GuiBackend::LinuxX11, Editor::Gnu);
}

#[test]
fn x11_initial_gui_frame_is_created_between_early_and_regular_init() {
    check_startup(GuiBackend::LinuxX11, Editor::Neomacs);
}

#[test]
fn wayland_initial_gui_frame_is_created_between_early_and_regular_init() {
    check_startup(GuiBackend::LinuxWayland, Editor::Neomacs);
}

#[test]
fn successful_early_init_exit_needs_no_gui_frame() {
    for backend in [GuiBackend::LinuxX11, GuiBackend::LinuxWayland] {
        check_case(backend, Editor::Neomacs, StartupCase::EarlyExit(0));
    }
}

#[test]
fn unsuccessful_early_init_exit_preserves_its_status_without_a_gui_frame() {
    for backend in [GuiBackend::LinuxX11, GuiBackend::LinuxWayland] {
        check_case(backend, Editor::Neomacs, StartupCase::EarlyExit(23));
    }
}

#[test]
fn x11_has_no_visible_native_window_while_early_init_is_running() {
    check_case(
        GuiBackend::LinuxX11,
        Editor::Neomacs,
        StartupCase::ObserveEarlyInit,
    );
}

#[test]
fn invalid_initial_frame_exits_promptly_without_a_gui_frame() {
    for backend in [GuiBackend::LinuxX11, GuiBackend::LinuxWayland] {
        check_case(backend, Editor::Neomacs, StartupCase::InvalidInitialFrame);
    }
}

#[test]
fn caught_frame_creation_error_allows_normal_initial_frame_creation() {
    for (backend, editor) in [
        (GuiBackend::LinuxX11, Editor::Gnu),
        (GuiBackend::LinuxX11, Editor::Neomacs),
        (GuiBackend::LinuxWayland, Editor::Neomacs),
    ] {
        check_case(backend, editor, StartupCase::RecoverableFrameError);
    }
}

#[test]
fn gnu_invisible_initial_frame_stays_unmapped_until_made_visible() {
    check_case(
        GuiBackend::LinuxX11,
        Editor::Gnu,
        StartupCase::InitiallyInvisible,
    );
}

#[test]
fn x11_invisible_initial_frame_stays_unmapped_until_made_visible() {
    check_case(
        GuiBackend::LinuxX11,
        Editor::Neomacs,
        StartupCase::InitiallyInvisible,
    );
}

#[test]
fn retiring_raw_invisible_frame_preserves_initial_window_readiness() {
    for editor in [Editor::Gnu, Editor::Neomacs] {
        check_case(
            GuiBackend::LinuxX11,
            editor,
            StartupCase::RetireRawInvisibleBeforeInitial,
        );
    }
}

#[derive(Clone, Copy)]
enum Editor {
    Gnu,
    Neomacs,
}

#[derive(Clone, Copy)]
enum StartupCase {
    Complete,
    EarlyExit(i32),
    ObserveEarlyInit,
    InvalidInitialFrame,
    RecoverableFrameError,
    InitiallyInvisible,
    RetireRawInvisibleBeforeInitial,
}

fn check_startup(backend: GuiBackend, editor: Editor) {
    check_case(backend, editor, StartupCase::Complete);
}

fn check_case(backend: GuiBackend, editor: Editor, case: StartupCase) {
    let root = neomacs_infra::workspace_root();
    let name = match (editor, backend) {
        (Editor::Gnu, _) => "gnu-early-init",
        (Editor::Neomacs, GuiBackend::LinuxX11) => "x11-early-init",
        (Editor::Neomacs, GuiBackend::LinuxWayland) => "wayland-early-init",
        _ => unreachable!("this native regression uses isolated Linux displays"),
    };
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).unwrap();
    let identity = format!(
        "issue-417-{name}-{}-{:016x}",
        std::process::id(),
        u64::from_ne_bytes(nonce)
    );
    let temporary = root.join("tmp");
    let init = temporary.join(&identity);
    let artifacts = if matches!(
        case,
        StartupCase::InitiallyInvisible
            | StartupCase::RetireRawInvisibleBeforeInitial
            | StartupCase::InvalidInitialFrame
    ) {
        temporary.join("neomacs-gui-tests").join(&identity)
    } else {
        root.join("target/neomacs-gui-tests").join(&identity)
    };
    fs::create_dir_all(&init).unwrap();
    fs::create_dir_all(&artifacts).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/issue-417-startup");
    for file in ["early-init.el", "init.el"] {
        fs::copy(fixture.join(file), init.join(file)).unwrap();
    }
    let trace = artifacts.join("startup-trace.json");
    let ready = artifacts.join("early-init-ready.json");
    let invalid_frame_ready = artifacts.join("invalid-frame-ready.json");
    let release = temporary.join(format!("{identity}.release"));
    let invisible_ready = artifacts.join("invisible-initial-ready.json");
    let visible_ready = artifacts.join("visible-initial-ready.json");
    let visible_ack = temporary.join(format!("{identity}.visible-ack"));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = match editor {
        Editor::Gnu => std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs")),
        Editor::Neomacs => std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs")),
    };
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(name, init.join("init.el")),
    )
    .with_program(binary)
    .with_args([
        "--no-site-file".to_owned(),
        "--no-site-lisp".to_owned(),
        format!("--init-directory={}", init.display()),
    ])
    .with_env("HOME", init.to_string_lossy())
    .with_env("XDG_CONFIG_HOME", init.to_string_lossy())
    .with_env("TMPDIR", temporary.to_string_lossy())
    .with_env("GDK_BACKEND", "x11")
    .with_env("NEOMACS_ISSUE_417_TRACE", trace.to_string_lossy());
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    plan = match case {
        StartupCase::Complete => plan,
        StartupCase::EarlyExit(status) => {
            plan.with_env("NEOMACS_ISSUE_417_EARLY_EXIT", status.to_string())
        }
        StartupCase::ObserveEarlyInit => plan
            .with_env("NEOMACS_ISSUE_417_EARLY_READY", ready.to_string_lossy())
            .with_env("NEOMACS_ISSUE_417_EARLY_RELEASE", release.to_string_lossy()),
        StartupCase::InvalidInitialFrame => plan
            .with_env("NEOMACS_ISSUE_417_INVALID_INITIAL_FRAME", "1")
            .with_env(
                "NEOMACS_ISSUE_417_INVALID_FRAME_READY",
                invalid_frame_ready.to_string_lossy(),
            ),
        StartupCase::RecoverableFrameError => {
            plan.with_env("NEOMACS_ISSUE_417_RECOVERABLE_FRAME_ERROR", "1")
        }
        StartupCase::InitiallyInvisible | StartupCase::RetireRawInvisibleBeforeInitial => plan
            .with_env("NEOMACS_ISSUE_417_INVISIBLE_INITIAL", "1")
            .with_env(
                "NEOMACS_ISSUE_417_INVISIBLE_READY",
                invisible_ready.to_string_lossy(),
            )
            .with_env(
                "NEOMACS_ISSUE_417_INVISIBLE_RELEASE",
                release.to_string_lossy(),
            )
            .with_env(
                "NEOMACS_ISSUE_417_VISIBLE_READY",
                visible_ready.to_string_lossy(),
            )
            .with_env(
                "NEOMACS_ISSUE_417_VISIBLE_ACK",
                visible_ack.to_string_lossy(),
            ),
    };
    if matches!(case, StartupCase::RetireRawInvisibleBeforeInitial) {
        plan = plan.with_env("NEOMACS_ISSUE_417_RETIRE_RAW_INVISIBLE", "1");
    }
    let result = match case {
        StartupCase::ObserveEarlyInit => {
            run_at_early_init_barrier(plan, session.env(), &ready, &release)
        }
        StartupCase::Complete | StartupCase::EarlyExit(_) | StartupCase::RecoverableFrameError => {
            run(plan)
        }
        StartupCase::InvalidInitialFrame => {
            run_invalid_initial_frame_with_prompt_exit(plan, &invalid_frame_ready)
        }
        StartupCase::InitiallyInvisible | StartupCase::RetireRawInvisibleBeforeInitial => {
            run_at_invisible_initial_frame_barrier(
                plan,
                session.env(),
                &invisible_ready,
                &release,
                &visible_ready,
                &visible_ack,
            )
        }
    };
    assert!(!result.timed_out, "{result:#?}");
    match case {
        StartupCase::InvalidInitialFrame => assert!(
            result.exit_code.is_some_and(|status| status != 0),
            "invalid initial frame must fail startup: {result:#?}"
        ),
        StartupCase::Complete
        | StartupCase::ObserveEarlyInit
        | StartupCase::RecoverableFrameError
        | StartupCase::InitiallyInvisible
        | StartupCase::RetireRawInvisibleBeforeInitial => {
            assert_eq!(result.exit_code, Some(0), "{result:#?}")
        }
        StartupCase::EarlyExit(status) => {
            assert_eq!(result.exit_code, Some(status), "{result:#?}")
        }
    }
    let events: Vec<Value> =
        serde_json::from_slice(&fs::read(&trace).unwrap_or_else(|error| {
            panic!("missing startup trace {trace:?}: {error}; {result:#?}")
        }))
        .unwrap();
    eprintln!("startup trace {}: {events:#?}", trace.display());
    match case {
        StartupCase::Complete
        | StartupCase::ObserveEarlyInit
        | StartupCase::InitiallyInvisible
        | StartupCase::RetireRawInvisibleBeforeInitial => assert_lifecycle(&events),
        StartupCase::RecoverableFrameError => {
            assert_early_state(&events);
            let caught = &events[2];
            assert_eq!(caught["tag"], "caught-frame-error");
            assert_eq!(caught["error_condition"], "wrong-type-argument");
            assert_eq!(caught["selected_kind"], "tty");
            assert_eq!(caught["display_kind"], "tty");
            assert_eq!(
                caught["frame_kinds"],
                json!(["tty"]),
                "failed frame creation must not leave a live GUI frame"
            );
            let continued: Vec<_> = events
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != 2)
                .map(|(_, event)| event.clone())
                .collect();
            assert_lifecycle(&continued);
        }
        StartupCase::EarlyExit(_) | StartupCase::InvalidInitialFrame => {
            assert_eq!(events.len(), 1, "unexpected trace after initialization");
            assert_early_state(&events);
            assert!(
                result.png_bytes.is_none(),
                "termination before GUI initialization produced a presentation: {result:#?}"
            );
        }
    }
}

fn run(plan: GuiTestPlan) -> GuiRunResult {
    run_with_timeout(plan, Duration::from_secs(25))
}

fn run_with_timeout(plan: GuiTestPlan, timeout: Duration) -> GuiRunResult {
    plan.run_with(
        &mut ProcessGuiCommandRunner,
        GuiRunOptions::with_timeout(timeout),
    )
    .unwrap()
}

/// Cold terminal preload is outside the constructor's prompt-failure budget.
fn run_invalid_initial_frame_with_prompt_exit(plan: GuiTestPlan, ready: &Path) -> GuiRunResult {
    const STARTUP_BUDGET: Duration = Duration::from_secs(45);
    const EXIT_BUDGET: Duration = Duration::from_secs(5);
    thread::scope(|scope| {
        let deadline = Instant::now() + STARTUP_BUDGET;
        let runner = scope.spawn(move || {
            let result = run_with_timeout(plan, STARTUP_BUDGET);
            (SystemTime::now(), result)
        });
        let observation: Value = loop {
            // The invalid constructor can fail before the observer is
            // scheduled again, so accept its atomically published marker
            // even when the owned runner has already finished.
            if let Ok(contents) = fs::read(ready) {
                break serde_json::from_slice(&contents).unwrap();
            }
            assert!(
                !runner.is_finished(),
                "editor exited before its invalid frame construction marker: {ready:?}"
            );
            assert!(
                Instant::now() < deadline,
                "editor did not reach invalid frame construction: {ready:?}"
            );
            thread::sleep(Duration::from_millis(10));
        };
        let events: Vec<Value> = serde_json::from_value(observation["events"].clone()).unwrap();
        assert_early_state(&events);
        assert_eq!(
            events.len(),
            2,
            "unexpected pre-construction trace: {events:#?}"
        );
        assert_eq!(events[1]["tag"], "before-init");
        assert_eq!(events[1]["selected_kind"], "tty");
        assert_eq!(events[1]["frame_kinds"], json!(["tty"]));
        observation["pid"].as_u64().expect("owned editor PID");

        // The atomic file's timestamp also covers a descheduled observer:
        // starting five seconds only when Rust notices the file could hide
        // a slow exit that had already completed by then.
        let published = fs::metadata(ready).unwrap().modified().unwrap();
        while !runner.is_finished() {
            assert!(
                published.elapsed().unwrap() < EXIT_BUDGET,
                "invalid initial frame did not terminate within {EXIT_BUDGET:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let (finished, result) = runner.join().expect("owned invalid-frame runner");
        assert!(
            finished.duration_since(published).unwrap() <= EXIT_BUDGET,
            "invalid initial frame did not terminate promptly: {result:#?}"
        );
        result
    })
}

struct ReleaseEarlyInit(PathBuf);

impl Drop for ReleaseEarlyInit {
    fn drop(&mut self) {
        // Release on assertion unwind too, so the scoped runner can reap its
        // owned editor before the isolated X server is dropped.
        let _ = fs::write(&self.0, b"release\n");
    }
}

fn run_at_early_init_barrier(
    plan: GuiTestPlan,
    display_env: &[(String, String)],
    ready: &Path,
    release: &Path,
) -> GuiRunResult {
    thread::scope(|scope| {
        let runner = scope.spawn(move || run(plan));
        let _release_on_exit = ReleaseEarlyInit(release.to_owned());
        let deadline = Instant::now() + Duration::from_secs(12);
        let observation: Value = loop {
            if let Ok(contents) = fs::read(ready) {
                break serde_json::from_slice(&contents).unwrap();
            }
            assert!(
                !runner.is_finished(),
                "editor exited before reaching the early-init barrier"
            );
            assert!(Instant::now() < deadline, "early-init barrier timed out");
            thread::sleep(Duration::from_millis(10));
        };
        eprintln!("native-window barrier state: {observation:#?}");
        let pid = observation["pid"].as_u64().expect("owned editor PID");
        let tool = |arguments: &[&str]| {
            Command::new("xdotool")
                .args(arguments)
                .envs(display_env.iter().cloned())
                .env("TMPDIR", neomacs_infra::workspace_root().join("tmp"))
                .output()
                .expect("xdotool is required for the X11 native-window barrier")
        };
        // A failed display connection must not masquerade as no owned window.
        let display = tool(&["getdisplaygeometry"]);
        assert!(display.status.success(), "display probe: {display:?}");
        let found = tool(&["search", "--onlyvisible", "--pid", &pid.to_string()]);
        assert_eq!(
            found.status.code(),
            Some(1),
            "a visible GUI window exists during early-init: {found:?}"
        );
        assert!(
            found.stdout.is_empty(),
            "unexpected owned window: {found:?}"
        );
        fs::write(release, b"release\n").unwrap();
        runner.join().expect("owned GUI runner")
    })
}

/// Observe actual X11 mapping while Lisp is paused after initialization.
/// Logical invisibility alone cannot detect a prematurely exposed native window.
fn run_at_invisible_initial_frame_barrier(
    plan: GuiTestPlan,
    display_env: &[(String, String)],
    ready: &Path,
    release: &Path,
    visible_ready: &Path,
    visible_ack: &Path,
) -> GuiRunResult {
    const BUDGET: Duration = Duration::from_secs(45);
    thread::scope(|scope| {
        // The first observation includes cold editor initialization. Share
        // the owned process budget rather than imposing a shorter startup
        // deadline that can expire before a valid after-init observation.
        let deadline = Instant::now() + BUDGET;
        let runner = scope.spawn(move || run_with_timeout(plan, BUDGET));
        let _release_on_exit = ReleaseEarlyInit(release.to_owned());
        let _acknowledge_on_exit = ReleaseEarlyInit(visible_ack.to_owned());
        let read_observation = |file: &Path| {
            loop {
                assert!(
                    !runner.is_finished(),
                    "editor exited before barrier {file:?}"
                );
                if let Ok(contents) = fs::read(file) {
                    break serde_json::from_slice::<Value>(&contents).unwrap();
                }
                assert!(
                    Instant::now() < deadline,
                    "startup barrier timed out: {file:?}"
                );
                thread::sleep(Duration::from_millis(10));
            }
        };
        let hidden = read_observation(ready);
        assert_eq!(hidden["selected_kind"], "gui", "{hidden}");
        assert_eq!(hidden["logical_visibility"], "nil", "{hidden}");
        let pid = hidden["pid"]
            .as_u64()
            .expect("owned editor PID")
            .to_string();
        let tool = |arguments: &[&str]| {
            Command::new("xdotool")
                .args(arguments)
                .envs(display_env.iter().cloned())
                .env("TMPDIR", neomacs_infra::workspace_root().join("tmp"))
                .output()
                .expect("xdotool is required for the initial-frame mapping probe")
        };
        let display = tool(&["getdisplaygeometry"]);
        assert!(display.status.success(), "display probe: {display:?}");

        // Permit either an allocated unmapped window or deferred allocation.
        // Wait for native startup to reach the window before inspecting it;
        // otherwise a fast probe could miss the erroneous asynchronous mapping.
        let allocation_deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let owned = tool(&["search", "--pid", &pid]);
            assert!(
                matches!(owned.status.code(), Some(0 | 1)),
                "window probe: {owned:?}"
            );
            if owned.status.success() || Instant::now() >= allocation_deadline {
                break;
            }
            assert!(!runner.is_finished(), "editor exited while invisible");
            thread::sleep(Duration::from_millis(20));
        }
        let observation_deadline = Instant::now() + Duration::from_millis(500);
        loop {
            assert!(
                !runner.is_finished(),
                "editor exited while observing its invisible frame"
            );
            let mapped = tool(&["search", "--onlyvisible", "--pid", &pid]);
            assert_eq!(
                mapped.status.code(),
                Some(1),
                "initial-frame-alist visibility nil exposed a native window: {mapped:?}; {hidden}"
            );
            assert!(
                mapped.stdout.is_empty(),
                "unexpected visible window: {mapped:?}"
            );
            if Instant::now() >= observation_deadline {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        fs::write(release, b"release\n").unwrap();
        let shown = read_observation(visible_ready);
        assert_eq!(shown["pid"], hidden["pid"], "{shown}");
        assert_eq!(shown["logical_visibility"], "t", "{shown}");
        let mapping_deadline = Instant::now() + Duration::from_secs(8);
        let mapped = loop {
            let mapped = tool(&["search", "--onlyvisible", "--pid", &pid]);
            if mapped.status.success() && !mapped.stdout.is_empty() {
                break mapped;
            }
            assert!(
                !runner.is_finished(),
                "editor exited before mapping its frame"
            );
            assert!(
                Instant::now() < mapping_deadline,
                "frame did not map after make-frame-visible: {mapped:?}"
            );
            thread::sleep(Duration::from_millis(20));
        };
        fs::write(
            ready.with_file_name("native-visibility-observation.json"),
            json!({"pid": hidden["pid"], "initial_logical_visibility": "nil",
                   "initial_native_visible": false, "shown_logical_visibility": "t",
                   "shown_native_windows": String::from_utf8_lossy(&mapped.stdout)})
            .to_string(),
        )
        .unwrap();
        fs::write(visible_ack, b"observed\n").unwrap();
        runner.join().expect("owned GUI runner")
    })
}

fn assert_early_state(events: &[Value]) {
    let first = events
        .first()
        .expect("early-init must record startup state");
    assert_eq!(first["tag"], "early-init");
    // RED must identify the report's first-line GUI exposure, before checking
    // the downstream missing hooks and late default-frame-alist application.
    assert_eq!(
        first["selected_kind"], "tty",
        "GUI exists before early-init: {events:#?}"
    );
    assert_eq!(
        first["frame_kinds"],
        json!(["tty"]),
        "early-init must see exactly the startup terminal frame"
    );
    assert_eq!(first["terminal_kind"], "tty");
    assert_eq!(first["display_kind"], "tty");
}

fn assert_lifecycle(events: &[Value]) {
    assert_early_state(events);
    let tags: Vec<_> = events
        .iter()
        .map(|event| event["tag"].as_str().unwrap())
        .collect();
    assert_eq!(
        tags,
        [
            "early-init",
            "before-init",
            "before-make-frame",
            "after-make-frame",
            "init",
            "after-init",
            "emacs-startup",
            "window-setup",
            "settled",
        ]
    );
    let before = &events[2];
    assert_eq!(before["selected_kind"], "tty");
    assert_eq!(before["frame_kinds"], json!(["tty"]));
    let made = &events[3];
    assert_eq!(made["created_kind"], "gui");
    assert_eq!(made["created_background"], "#00ff00");
    for event in &events[4..] {
        assert_eq!(event["selected_kind"], "gui", "{event}");
        assert_eq!(event["display_kind"], "gui", "{event}");
        let display = event["display_pixels"].as_array().unwrap();
        assert!(
            display.len() == 2
                && display
                    .iter()
                    .all(|dimension| dimension.as_i64().is_some_and(|value| value > 0)),
            "GUI display dimensions must be available by init: {event}"
        );
        assert_eq!(event["frame_kinds"], json!(["gui"]), "{event}");
        assert_eq!(event["terminal_kind"], "absent", "{event}");
        assert_eq!(
            event["background"], "#00ff00",
            "defaults applied too late: {event}"
        );
    }
}
