//! #395: compositor activation must route real Wayland keys to that frame.

use super::*;
use neomacs_gui_tests::{CommandSpec, GuiArtifactSet, GuiCommandRunner};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn native_wayland_focus_routes_typing_to_new_and_reactivated_frames() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/issue395-wayland-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 1400x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n",
    )
    .expect("private Sway compositor");
    let runtime = session
        .env()
        .iter()
        .find(|(key, _)| key == "XDG_RUNTIME_DIR")
        .map(|(_, value)| Path::new(value))
        .expect("private Sway runtime directory");
    let ipc = wait_for_sway_ipc(&artifacts, runtime).expect("private Sway IPC socket");
    let driver = WaylandInput {
        env: session.env().to_vec(),
        artifacts: artifacts.clone(),
        ipc,
    };
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            "native-wayland-frame-focus",
            root.join("crates/neomacs-gui-tests/fixtures/native-frame-focus.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_GUI_FOCUS_CONTROL", artifacts.display().to_string())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "512");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let (run, observation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(45)),
            )
        });
        let observation = exercise_wayland_input(&driver, || run.is_finished());
        fs::write(artifacts.join("stop"), "stop").unwrap();
        (run.join().expect("GUI runner thread"), observation)
    });
    let run = run.expect("Wayland focus artifacts");
    assert!(
        observation.is_ok(),
        "{observation:?}; artifacts: {}",
        artifacts.display()
    );
    assert!(!run.timed_out, "{run:#?}");
    assert_eq!(run.exit_code, Some(0), "{run:#?}");
    assert!(run.frame_snapshot_json_bytes.unwrap_or_default() > 0);
}

fn exercise_wayland_input(
    driver: &WaylandInput,
    finished: impl Fn() -> bool,
) -> Result<(), String> {
    let ready = wait_for_state(&driver.artifacts, "ready", 0, &finished)?;
    let pid = ready["pid"].as_u64().ok_or("fixture PID absent")?;
    let primary = driver.wait_for_window(pid, None, &finished)?;
    driver.focus(primary, &finished)?;
    driver.keys(&["p"])?;
    expect_buffers(&driver.artifacts, "initial", "p", "", &finished)?;

    // Pace modifier transitions and prefix keys like ordinary native input,
    // rather than delivering the whole shortcut in one compositor batch.
    driver.keys(&[
        "-M", "ctrl", "-s", "80", "-k", "x", "-s", "80", "-m", "ctrl", "-s", "80", "-k", "5", "-s",
        "80", "-k", "2",
    ])?;
    let secondary = driver.wait_for_window(pid, Some(primary), &finished)?;
    // A real compositor should activate the new frame. Do not repair focus
    // here: doing so would hide the first half of the author's reproduction.
    driver.wait_for_focus(secondary, &finished)?;
    driver.keys(&["n"])?;
    expect_buffers(&driver.artifacts, "new-frame", "p", "n", &finished)?;

    driver.focus(primary, &finished)?;
    driver.keys(&["aa"])?;
    expect_buffers(&driver.artifacts, "focus-primary", "paa", "n", &finished)?;
    driver.focus(secondary, &finished)?;
    driver.keys(&["bb"])?;
    expect_buffers(
        &driver.artifacts,
        "focus-secondary",
        "paa",
        "nbb",
        &finished,
    )
}

struct WaylandInput {
    env: Vec<(String, String)>,
    artifacts: PathBuf,
    ipc: PathBuf,
}

impl WaylandInput {
    fn tool(&self, program: impl AsRef<Path>, args: &[&str]) -> Result<String, String> {
        let command = CommandSpec {
            program: program.as_ref().to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            env: self.env.clone(),
        };
        let output = ProcessGuiCommandRunner
            .run(
                &command,
                &GuiArtifactSet::new(
                    self.artifacts.join("tools"),
                    GuiBackend::LinuxWayland,
                    "input",
                ),
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

    fn ipc(&self, args: &[&str]) -> Result<Value, String> {
        let socket = self.ipc.to_string_lossy();
        let mut command = vec!["-s", socket.as_ref(), "-r"];
        command.extend_from_slice(args);
        let output = self.tool("swaymsg", &command)?;
        serde_json::from_str(&output).map_err(|error| format!("Sway IPC: {error}: {output}"))
    }

    fn windows(&self, pid: u64) -> Result<Vec<(u64, bool)>, String> {
        fn collect(node: &Value, pid: u64, windows: &mut Vec<(u64, bool)>) {
            if node["pid"].as_u64() == Some(pid)
                && node["type"] == "con"
                && let Some(id) = node["id"].as_u64()
            {
                windows.push((id, node["focused"] == true));
            }
            for kind in ["nodes", "floating_nodes"] {
                if let Some(children) = node[kind].as_array() {
                    for child in children {
                        collect(child, pid, windows);
                    }
                }
            }
        }
        let mut windows = Vec::new();
        collect(&self.ipc(&["-t", "get_tree"])?, pid, &mut windows);
        Ok(windows)
    }

    fn wait_for_window(
        &self,
        pid: u64,
        exclude: Option<u64>,
        finished: &impl Fn() -> bool,
    ) -> Result<u64, String> {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some((id, _)) = self
                .windows(pid)?
                .into_iter()
                .find(|(id, _)| Some(*id) != exclude)
            {
                return Ok(id);
            }
            if finished() || Instant::now() >= deadline {
                return Err(format!(
                    "Wayland window for PID {pid}, excluding {exclude:?}, absent"
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_focus(&self, id: u64, finished: &impl Fn() -> bool) -> Result<(), String> {
        fn is_focused(node: &Value, id: u64) -> bool {
            if node["id"].as_u64() == Some(id) {
                return node["focused"] == true;
            }
            ["nodes", "floating_nodes"].into_iter().any(|kind| {
                node[kind]
                    .as_array()
                    .is_some_and(|children| children.iter().any(|child| is_focused(child, id)))
            })
        }
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if is_focused(&self.ipc(&["-t", "get_tree"])?, id) {
                return Ok(());
            }
            if finished() || Instant::now() >= deadline {
                return Err(format!(
                    "Wayland container {id} did not receive compositor focus"
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn focus(&self, id: u64, finished: &impl Fn() -> bool) -> Result<(), String> {
        let command = format!("[con_id={id}] focus");
        let result = self.ipc(&[&command])?;
        if !result.as_array().is_some_and(|items| {
            !items.is_empty() && items.iter().all(|item| item["success"] == true)
        }) {
            return Err(format!("Sway focus failed: {result}"));
        }
        self.wait_for_focus(id, finished)
    }

    fn keys(&self, keys: &[&str]) -> Result<(), String> {
        let program = std::env::var_os("NEOMACS_GUI_WTYPE").unwrap_or_else(|| "wtype".into());
        // Activate each new virtual keyboard before sending meaningful input.
        let mut args = vec!["-k", "Shift_L", "-s", "100"];
        args.extend_from_slice(keys);
        self.tool(Path::new(&program), &args).map(|_| ())
    }
}

fn wait_for_sway_ipc(artifacts: &Path, runtime: &Path) -> Result<PathBuf, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(socket) = fs::read_dir(artifacts)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("sway-ipc."))
            })
        {
            // The compositor was started through its held /proc directory.
            // Connecting through the long artifact path can exceed Unix
            // sockaddr_un's limit even though that directory lists the socket.
            return Ok(runtime.join(socket.file_name().expect("listed socket name")));
        }
        if Instant::now() >= deadline {
            return Err("private Sway IPC socket absent".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
