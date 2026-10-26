//! Issue #600: early-init frame defaults and the final startup hook.
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy)]
enum CommandLine {
    NoActions,
    EvalAction,
}

#[derive(Clone, Copy)]
enum InitDirectory {
    Explicit,
    Home,
    Xdg,
    HomeBeforeXdg,
}

#[test]
fn early_init_defaults_are_visible_in_window_setup_hook_without_command_line_actions() {
    check_startup(CommandLine::NoActions, InitDirectory::Explicit);
}

#[test]
fn window_setup_hook_runs_after_command_line_actions_and_frame_defaults() {
    check_startup(CommandLine::EvalAction, InitDirectory::Explicit);
}

#[test]
fn home_init_discovery_loads_early_frame_defaults_and_window_setup_hook() {
    check_startup(CommandLine::NoActions, InitDirectory::Home);
}

#[test]
fn xdg_init_discovery_loads_early_frame_defaults_and_window_setup_hook() {
    check_startup(CommandLine::NoActions, InitDirectory::Xdg);
}

#[test]
fn home_init_directory_takes_precedence_over_xdg_like_gnu() {
    check_startup(CommandLine::NoActions, InitDirectory::HomeBeforeXdg);
}

fn check_startup(command_line: CommandLine, init_directory: InitDirectory) {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
    };
    let root = neomacs_infra::workspace_root();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/issue-600-startup");
    let name = match (init_directory, command_line) {
        (InitDirectory::Explicit, CommandLine::NoActions) => "issue-600-startup-no-actions",
        (InitDirectory::Explicit, CommandLine::EvalAction) => "issue-600-startup-eval-action",
        (InitDirectory::Home, _) => "issue-600-startup-home",
        (InitDirectory::Xdg, _) => "issue-600-startup-xdg",
        (InitDirectory::HomeBeforeXdg, _) => "issue-600-startup-home-before-xdg",
    };
    let artifacts = root.join("tmp/issue600/gui-startup").join(name);
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    // Pin discovery inputs even for the explicit-directory cases. Neither
    // editor may consult the operator's config, package cache or state.
    let home = artifacts.join("home");
    let xdg = artifacts.join("xdg");
    for path in [&home, &xdg] {
        if path.exists() {
            std::fs::remove_dir_all(path).unwrap();
        }
    }
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg).unwrap();
    let target = match init_directory {
        InitDirectory::Explicit => None,
        InitDirectory::Home | InitDirectory::HomeBeforeXdg => Some(home.join(".emacs.d")),
        InitDirectory::Xdg => Some(xdg.join("emacs")),
    };
    if let Some(target) = target {
        std::fs::create_dir_all(&target).unwrap();
        for file in ["early-init.el", "init.el"] {
            std::fs::copy(fixture.join(file), target.join(file)).unwrap();
        }
    }
    if matches!(init_directory, InitDirectory::HomeBeforeXdg) {
        std::fs::create_dir_all(xdg.join("emacs")).unwrap();
        std::fs::write(
            xdg.join("emacs/early-init.el"),
            "(error \"XDG init must not override an existing ~/.emacs.d\")",
        )
        .unwrap();
    }
    let mut args = match init_directory {
        InitDirectory::Explicit => vec![format!("--init-directory={}", fixture.display())],
        InitDirectory::Home | InitDirectory::Xdg | InitDirectory::HomeBeforeXdg => Vec::new(),
    };
    if matches!(command_line, CommandLine::EvalAction) {
        args.extend([
            "--eval".into(),
            "(setq issue-600-command-line-ran t)".into(),
        ]);
    }
    let mut env = session.env().to_vec();
    for (key, path) in [
        ("HOME", home),
        ("XDG_CONFIG_HOME", xdg),
        ("XDG_CACHE_HOME", artifacts.join("cache")),
        ("XDG_DATA_HOME", artifacts.join("data")),
        ("XDG_STATE_HOME", artifacts.join("state")),
    ] {
        std::fs::create_dir_all(&path).unwrap();
        env.push((key.into(), path.to_string_lossy().into_owned()));
    }
    if matches!(command_line, CommandLine::NoActions) {
        // GNU's startup-screen path runs the hook inside command-line-1;
        // the inhibited path runs it in normal-top-level's cleanup.
        env.push(("NEOMACS_GUI_SHOW_STARTUP_SCREEN".into(), "1".into()));
    }
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(name, fixture.join("init.el")),
    )
    .with_program(binary)
    .with_args(args.clone());
    for (key, value) in &env {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state = result.gui_state.expect("startup hook witness");
    assert_eq!(state["early_init_count"], 1, "{state}");
    assert_eq!(state["setup_count"], 1, "{state}");
    let expected = match command_line {
        CommandLine::NoActions => "(t 0 0 nil nil)",
        CommandLine::EvalAction => "(t 0 0 nil t)",
    };
    assert_eq!(state["setup"], expected, "{state}");
    if let Some(gnu_binary) = std::env::var_os("NEOMACS_GUI_STARTUP_ORACLE_BINARY") {
        // The same init directory is valid in GNU. Its GUI does not produce
        // Neomacs's readbacks, so compare its hook witness and process result.
        let mut oracle = GuiTestPlan::new(
            backend,
            &root,
            artifacts.join("gnu"),
            GuiScenario::new(name, fixture.join("init.el")),
        )
        .with_program(PathBuf::from(gnu_binary))
        .with_args(args);
        for (key, value) in &env {
            oracle = oracle.with_env(key.clone(), value.clone());
        }
        let oracle = oracle
            .run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(25)),
            )
            .unwrap();
        assert!(!oracle.timed_out, "{oracle:#?}");
        assert_eq!(oracle.exit_code, Some(0), "{oracle:#?}");
        assert_eq!(oracle.gui_state, Some(state.clone()), "GNU hook witness");
    }
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.frame_snapshot_json).unwrap())
            .unwrap();
    let frame = &snapshot["frames"][0];
    let bands = frame["frame_chrome"]["bands"].as_array().unwrap();
    assert!(
        !bands
            .iter()
            .any(|band| matches!(band["kind"].as_str(), Some("MenuBar" | "ToolBar"))),
        "early-init defaults must also reach rendered chrome: {bands:?}"
    );
    assert!(
        frame["scroll_bars"].as_array().unwrap().is_empty(),
        "explicit nil scroll bars must also reach the rendered frame: {frame}"
    );
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "GUI painted pixels"
    );
}
