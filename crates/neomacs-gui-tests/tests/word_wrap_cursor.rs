//! #704: native character movement and full redisplay must agree at word wrap.
//!
//! Linux X11/XTEST coverage in a private Xvfb; this does not establish Wayland
//! input parity. Set NEOVM_FORCE_ORACLE_PATH to the pinned GNU 31.1 executable
//! and select `test(word_wrap_cursor::)` in the existing `gui` nextest target.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
    run_input_tool,
};

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
enum Mode {
    Fundamental,
    Org,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fundamental => "fundamental",
            Self::Org => "org",
        }
    }
}

#[test]
fn native_word_wrap_cursor_survives_property_invalidation() {
    native(Mode::Fundamental);
}

#[test]
fn native_org_word_wrap_cursor_survives_property_invalidation() {
    native(Mode::Org);
}

fn native(mode: Mode) {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    run(&binary, "native", mode);
}

#[test]
fn gnu_word_wrap_cursor_survives_property_invalidation() {
    gnu(Mode::Fundamental);
}

#[test]
fn gnu_org_word_wrap_cursor_survives_property_invalidation() {
    gnu(Mode::Org);
}

fn gnu(mode: Mode) {
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "GNU control requires the pinned reference"
    );
    eprintln!("{}", oracle.stamp());
    run(oracle.executable(), "gnu", mode);
}

fn run(binary: &Path, engine: &str, mode: Mode) {
    let root = neomacs_infra::workspace_root();
    let mode = mode.as_str();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/word-wrap-{engine}-{mode}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new(
            "word-wrap-cursor",
            root.join("crates/neomacs-gui-tests/fixtures/word-wrap-cursor.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_WORD_WRAP_CONTROL", artifacts.display().to_string())
    .with_env("NEOMACS_WORD_WRAP_MODE", mode)
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let (result, observation) = thread::scope(|scope| {
        let runner = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(40)),
            )
        });
        let observation = exercise(&artifacts, session.env(), || runner.is_finished());
        fs::write(artifacts.join("stop"), b"stop").unwrap();
        (runner.join().unwrap(), observation)
    });
    let result = result.unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert!(
        observation.is_ok(),
        "{observation:?}; artifacts: {}",
        artifacts.display()
    );
}

fn exercise(
    artifacts: &Path,
    env: &[(String, String)],
    finished: impl Fn() -> bool,
) -> Result<(), String> {
    let mut request = 0;
    let mut sample = || -> Result<Value, String> {
        request += 1;
        fs::write(artifacts.join("request.pending"), request.to_string())
            .map_err(|e| e.to_string())?;
        fs::rename(artifacts.join("request.pending"), artifacts.join("request"))
            .map_err(|e| e.to_string())?;
        wait(artifacts, &finished, || {
            let state: Value =
                serde_json::from_slice(&fs::read(artifacts.join("state.json")).ok()?).ok()?;
            (state["request"] == request).then_some(state)
        })
    };
    let mut state = sample()?;
    let window = wait(artifacts, &finished, || {
        xdotool(
            env,
            &[
                "search",
                "--onlyvisible",
                "--pid",
                &state["pid"].to_string(),
            ],
        )
        .ok()?
        .lines()
        .last()
        .map(str::to_owned)
    })?;
    xdotool(env, &["windowfocus", "--sync", &window])?;
    // Resize using observed text geometry; frame column requests alone race
    // asynchronous font and native-window initialization on both editors.
    for _ in 0..5 {
        state = sample()?;
        let edges = &state["inside_edges"];
        let delta = 10 * state["char_width"].as_i64().ok_or("missing font width")?
            - (edges[2].as_i64().unwrap() - edges[0].as_i64().unwrap());
        if delta == 0 {
            break;
        }
        let geometry = xdotool(env, &["getwindowgeometry", "--shell", &window])?;
        let width: i64 = geometry
            .lines()
            .find_map(|l| l.strip_prefix("WIDTH="))
            .ok_or("missing X11 width")?
            .parse()
            .map_err(|_| "invalid X11 width")?;
        xdotool(
            env,
            &[
                "windowsize",
                "--sync",
                &window,
                &(width + delta).to_string(),
                "500",
            ],
        )?;
        thread::sleep(Duration::from_millis(150));
    }
    state = sample()?;
    let edges = &state["inside_edges"];
    if edges[2].as_i64().unwrap() - edges[0].as_i64().unwrap()
        != 10 * state["char_width"].as_i64().unwrap()
    {
        return Err(format!("text width did not settle at ten columns: {state}"));
    }
    let mut trace = Vec::new();
    for expected in 1i64..=5 {
        // XTEST to the focused window; --window would send ignored XSendEvent.
        xdotool(env, &["key", "--clearmodifiers", "Right"])?;
        state = wait(artifacts, &finished, || {
            let next = sample().ok()?;
            // Pinned GNU: points 2..5 cover the remaining a's and space on
            // row zero; point 6 covers the first b on the continuation row.
            // Wait for that key's rendered geometry, not just its command.
            let (column, row) = if expected == 5 { (0, 1) } else { (expected, 0) };
            (next["moves"] == expected
                && next["point"] == expected + 1
                && next["cursor"][1].as_i64()
                    == next["char_width"].as_i64().map(|width| column * width)
                && next["cursor"][2].as_i64()
                    == next["char_height"].as_i64().map(|height| row * height))
            .then_some(next)
        })?;
        trace.push(state.clone());
    }
    // Observe completed natural redisplay rather than post-command-hook's
    // previous cursor. This independent GNU-established position is the first
    // character of the continuation row, not a self-comparison of stale data.
    let before = wait(artifacts, &finished, || {
        let next = sample().ok()?;
        (next["cursor"][1] == 0 && next["cursor"][2] == next["char_height"]).then_some(next)
    })?;
    fs::write(artifacts.join("invalidate"), b"invalidate").map_err(|e| e.to_string())?;
    wait(artifacts, &finished, || {
        artifacts.join("invalidated").exists().then_some(())
    })?;
    let after = sample()?;
    fs::write(
        artifacts.join("comparison.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"trace":trace,"before":before,"after":after}),
        )
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    for observed in [&before, &after] {
        if observed["point"] != 6
            || observed["text"] != "aaaa bbbbbb\n"
            || observed["cursor"][1] != 0
            || observed["cursor"][2] != observed["char_height"]
        {
            return Err(format!(
                "first b must retain the continuation-row cursor: before={before}, after={after}"
            ));
        }
    }
    if before["chars_modified_tick"] != after["chars_modified_tick"]
        || before["modified_tick"] == after["modified_tick"]
    {
        return Err(format!(
            "control did not invalidate properties while preserving text: before={before}, after={after}"
        ));
    }
    Ok(())
}

fn wait<T>(
    artifacts: &Path,
    finished: &impl Fn() -> bool,
    mut observe: impl FnMut() -> Option<T>,
) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = observe() {
            return Ok(value);
        }
        if let Ok(error) = fs::read_to_string(artifacts.join("error.json")) {
            return Err(error);
        }
        if finished() || Instant::now() >= deadline {
            return Err(format!(
                "GUI acknowledgment timed out: {}",
                artifacts.display()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn xdotool(env: &[(String, String)], args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("xdotool");
    command.args(args).envs(env.iter().map(|(k, v)| (k, v)));
    let output = run_input_tool(command, Duration::from_secs(5));
    if !output.status.success() {
        return Err(format!(
            "xdotool {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
