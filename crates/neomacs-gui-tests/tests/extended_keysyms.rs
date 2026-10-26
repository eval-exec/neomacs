//! #398: real compositor/X11 key events must dispatch GNU-compatible bindings.
//! Requires Xvfb, xdotool and xkbcomp; Wayland also needs Sway and wtype.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
fn extended_native_key_bindings_match_initialized_gnu_on_wayland() {
    compare(GuiBackend::LinuxWayland);
}

#[test]
fn extended_native_key_bindings_match_initialized_gnu_on_x11() {
    compare(GuiBackend::LinuxX11);
}

fn compare(backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/issue398/gui-{}-{}-{}",
        backend.slug(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/extended-keysyms.el");
    let oracle_root = artifacts.join("gnu");
    let oracle_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&oracle_root)
        .unwrap();
    install_x11_keymap(oracle_session.env(), &oracle_root).unwrap();
    let oracle_state = oracle_root.join("state.json");
    let oracle_ready = oracle_root.join("ready");
    let input = inject_after_ready(
        GuiBackend::LinuxX11,
        oracle_session.env().to_vec(),
        oracle_ready.clone(),
    );
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
            .or_else(|| std::env::var_os("NEOMACS_GNU_EMACS_BINARY"))
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                oracle_state.display().to_string(),
            ),
            (
                "NEOMACS_EXTENDED_KEYS_READY".into(),
                oracle_ready.display().to_string(),
            ),
        ],
    };
    command.env.extend(oracle_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner.run(
        &command,
        &GuiArtifactSet::new(&oracle_root, GuiBackend::LinuxX11, "extended-keysyms"),
        &GuiRunOptions::with_timeout(Duration::from_secs(30)),
    );
    input.join().unwrap().unwrap();
    let output = output.unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0), "GNU: {output:#?}");
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&oracle_state).unwrap()).unwrap();
    assert_event_names(&expected);

    let session = match backend {
        GuiBackend::LinuxWayland => neomacs_infra::display::start_sway(
            &artifacts,
            "output * resolution 1000x800\nxwayland disable\nseat seat0 fallback true\n",
        ),
        _ => DisplayHarness::for_backend(backend).start_session(&artifacts),
    }
    .unwrap();
    if backend == GuiBackend::LinuxX11 {
        install_x11_keymap(session.env(), &artifacts).unwrap();
    }
    let ready = artifacts.join("ready");
    let input = inject_after_ready(backend, session.env().to_vec(), ready.clone());
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("extended-keysyms", fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_EXTENDED_KEYS_READY", ready.to_string_lossy());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan.run_with(
        &mut ProcessGuiCommandRunner,
        GuiRunOptions::with_timeout(Duration::from_secs(30)),
    );
    input.join().unwrap().unwrap();
    let result = result.unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert_eq!(result.gui_state, Some(expected));
    assert!(result.png_bytes.unwrap_or_default() > 0);
    assert!(result.frame_snapshot_json_bytes.unwrap_or_default() > 0);
}

fn assert_event_names(state: &serde_json::Value) {
    let names: Vec<_> = state["observations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event[0].as_str().unwrap())
        .collect();
    let mut expected: Vec<String> = (13..=35).map(|n| format!("f{n}")).collect();
    expected.extend(
        [
            "undo",
            "redo",
            "menu",
            "XF86Back",
            "XF86Forward",
            "XF86Copy",
            "XF86Cut",
            "XF86Paste",
            "C-S-f20",
        ]
        .map(str::to_owned),
    );
    assert_eq!(
        names, expected,
        "native key events must execute every binding"
    );
}

fn inject_after_ready(
    backend: GuiBackend,
    env: Vec<(String, String)>,
    ready: PathBuf,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(25);
        let pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&ready)
                && let Ok(pid) = pid.trim().parse::<u32>()
            {
                break pid.to_string();
            }
            if Instant::now() >= deadline {
                return Err(format!("fixture did not become ready: {}", ready.display()));
            }
            thread::sleep(Duration::from_millis(20));
        };
        let keys = native_keysyms();
        match backend {
            GuiBackend::LinuxX11 => {
                let window = loop {
                    if let Ok(output) =
                        command(&env, "xdotool", &["search", "--onlyvisible", "--pid", &pid])
                        && let Some(window) = output.lines().next()
                    {
                        break window.to_owned();
                    }
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "native X11 window for PID {pid} was not published; visible windows: {}",
                            visible_x11_windows(&env)
                        ));
                    }
                    thread::sleep(Duration::from_millis(20));
                };
                command(&env, "xdotool", &["windowfocus", "--sync", &window])?;
                let mut args: Vec<&str> =
                    vec!["key", "--clearmodifiers", "--delay", "80", "Shift_L"];
                args.extend(keys.iter().map(String::as_str));
                args.push("ctrl+shift+F20");
                command(&env, "xdotool", &args)?;
            }
            GuiBackend::LinuxWayland => {
                // Activate the virtual keyboard's modifier state before the
                // measured sequence. F13 is the first nonmodifier key, and
                // every measured key must dispatch exactly once.
                let mut args: Vec<&str> = vec!["-k", "Shift_L", "-s", "100"];
                for key in &keys {
                    args.extend(["-k", key.as_str(), "-s", "80"]);
                }
                args.extend([
                    "-M", "ctrl", "-M", "shift", "-k", "F20", "-m", "shift", "-m", "ctrl",
                ]);
                let program =
                    std::env::var_os("NEOMACS_GUI_WTYPE").unwrap_or_else(|| "wtype".into());
                command(&env, Path::new(&program), &args)?;
            }
            _ => unreachable!("Linux-only test backend"),
        }
        Ok(())
    })
}

fn native_keysyms() -> Vec<String> {
    let mut keys: Vec<String> = (13..=35).map(|n| format!("F{n}")).collect();
    keys.extend(
        [
            "Undo",
            "Redo",
            "Menu",
            "XF86Back",
            "XF86Forward",
            "XF86Copy",
            "XF86Cut",
            "XF86Paste",
        ]
        .map(str::to_owned),
    );
    keys
}

fn install_x11_keymap(env: &[(String, String)], artifacts: &Path) -> Result<(), String> {
    // Xvfb's default map puts F24 on a control-modified level. xdotool then
    // sends C-f24 to GNU, so give every tested keysym its own unmodified key.
    // Only the owned private X server is affected; retain real XKB/XTest input.
    let keys = native_keysyms();
    let mut codes = String::new();
    let mut symbols = String::new();
    for (index, key) in keys.iter().enumerate() {
        codes.push_str(&format!("<K{index:03}> = {};\n", 100 + index));
        symbols.push_str(&format!(
            "key <K{index:03}> {{ type=\"ONE_LEVEL\", [ {key} ] }};\n"
        ));
    }
    let keymap = format!(
        "xkb_keymap {{
          xkb_keycodes \"extended\" {{ minimum=8; maximum=255; <CTRL>=37; <SHFT>=50; {codes} }};
          xkb_types \"extended\" {{ include \"complete\" }};
          xkb_compatibility \"extended\" {{ include \"complete\" }};
          xkb_symbols \"extended\" {{
            key <CTRL> {{ [ Control_L ] }}; modifier_map Control {{ <CTRL> }};
            key <SHFT> {{ [ Shift_L ] }}; modifier_map Shift {{ <SHFT> }};
            {symbols}
          }};
        }};"
    );
    let path = artifacts.join("extended-keysyms.xkb");
    std::fs::write(&path, keymap).map_err(|error| error.to_string())?;
    let display = env
        .iter()
        .find(|(key, _)| key == "DISPLAY")
        .ok_or("private DISPLAY missing")?;
    let program = std::env::var_os("NEOMACS_GUI_XKBCOMP").unwrap_or_else(|| "xkbcomp".into());
    command(
        env,
        Path::new(&program),
        &["-w", "0", &path.to_string_lossy(), &display.1],
    )?;
    Ok(())
}

fn command(
    env: &[(String, String)],
    program: impl AsRef<Path>,
    args: &[&str],
) -> Result<String, String> {
    let spec = CommandSpec {
        program: program.as_ref().to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        env: env.to_vec(),
    };
    let artifacts = GuiArtifactSet::new(
        neomacs_infra::workspace_root()
            .join(format!("tmp/issue398/input-tools-{}", std::process::id())),
        GuiBackend::LinuxX11,
        "input",
    );
    let output = ProcessGuiCommandRunner
        .run(
            &spec,
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| format!("{} {args:?}: {error}", program.as_ref().display()))?;
    if output.timed_out || output.exit_code != Some(0) {
        return Err(format!(
            "{} {args:?}: timed_out={}, exit={:?}, stderr={}",
            program.as_ref().display(),
            output.timed_out,
            output.exit_code,
            output.stderr
        ));
    }
    Ok(output.stdout)
}

fn visible_x11_windows(env: &[(String, String)]) -> String {
    command(
        env,
        "xdotool",
        &[
            "search",
            "--onlyvisible",
            "--name",
            ".*",
            "getwindowname",
            "%@",
        ],
    )
    .unwrap_or_else(|error| error)
}
