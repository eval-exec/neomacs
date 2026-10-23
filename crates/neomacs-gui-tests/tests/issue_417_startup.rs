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
    time::{Duration, Instant},
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
    let artifacts = root.join("target/neomacs-gui-tests").join(&identity);
    fs::create_dir_all(&init).unwrap();
    fs::create_dir_all(&artifacts).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/issue-417-startup");
    for file in ["early-init.el", "init.el"] {
        fs::copy(fixture.join(file), init.join(file)).unwrap();
    }
    let trace = artifacts.join("startup-trace.json");
    let ready = artifacts.join("early-init-ready.json");
    let release = temporary.join(format!("{identity}.release"));
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
        StartupCase::InvalidInitialFrame => {
            plan.with_env("NEOMACS_ISSUE_417_INVALID_INITIAL_FRAME", "1")
        }
        StartupCase::RecoverableFrameError => {
            plan.with_env("NEOMACS_ISSUE_417_RECOVERABLE_FRAME_ERROR", "1")
        }
    };
    let result = match case {
        StartupCase::ObserveEarlyInit => {
            run_at_early_init_barrier(plan, session.env(), &ready, &release)
        }
        StartupCase::Complete | StartupCase::EarlyExit(_) | StartupCase::RecoverableFrameError => {
            run(plan)
        }
        StartupCase::InvalidInitialFrame => run_with_timeout(plan, Duration::from_secs(10)),
    };
    assert!(!result.timed_out, "{result:#?}");
    match case {
        StartupCase::InvalidInitialFrame => assert!(
            result.exit_code.is_some_and(|status| status != 0),
            "invalid initial frame must fail startup: {result:#?}"
        ),
        StartupCase::Complete
        | StartupCase::ObserveEarlyInit
        | StartupCase::RecoverableFrameError => {
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
        StartupCase::Complete | StartupCase::ObserveEarlyInit => assert_lifecycle(&events),
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
