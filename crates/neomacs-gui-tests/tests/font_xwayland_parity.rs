//! #359 evidence at the backend boundary: attested GNU GTK3/XWayland versus
//! native Neo Wayland. Stop red on the first difference; do not choose a new
//! product reference or change policy to make this diagnostic pass.
#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FONTS: [(&str, &str); 2] = [
    (
        "IBMPlexSans-Regular.ttf",
        "b47ebd72b5ce53b60f909033ff84d6fc12358cc147e2981f8c321ad3f36246b6",
    ),
    (
        "IBMPlexSans-Bold.ttf",
        "22f59d358e8b9b9fb38ac6ee26a7ea7263ffa1c89a9056c101acff5add91f5d6",
    ),
];

#[test]
#[ignore = "diagnostic requires real gsd-xsettings, full GNOME schemas and explicit reference choice on divergence"]
fn attested_gnu_xwayland_and_native_wayland_raster_preferences() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let root = neomacs_infra::workspace_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = directory(root.join(format!(
        "tmp/neomacs-gui-tests/font-xwayland-{}-{nonce}",
        std::process::id()
    )));
    eprintln!("XWAYLAND evidence artifacts: {}", artifacts.display());
    let oracle = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("explicit attested oracle"),
    );
    let checked = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(checked.is_attested());
    assert_eq!(
        fs::canonicalize(&oracle).unwrap(),
        checked.executable_path()
    );
    let prefix = oracle.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .args([
            oracle.as_os_str(),
            "31.1".as_ref(),
            prefix
                .join("share/gnu-emacs-oracle/build-manifest")
                .as_os_str(),
            "a360712c9d272d950d8d8255ef74570f7e90b7d9".as_ref(),
        ])
        .output()
        .unwrap();
    fs::write(artifacts.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &check.stderr).unwrap();
    assert!(check.status.success(), "oracle checker: {check:?}");
    let check_text = String::from_utf8_lossy(&check.stdout);
    assert!(check_text.contains("oracle profile: linux-gtk3"));
    eprintln!("{check_text}");
    let fonts = directory(artifacts.join("fonts"));
    for (filename, hash) in FONTS {
        let url = format!(
            "https://raw.githubusercontent.com/IBM/plex/2e90fe366fdc27c12682db13e5aa20ded3d90908/packages/plex-sans/fonts/complete/ttf/{filename}"
        );
        let source = neomacs_infra::pinned::pinned_file(filename, &url, hash).unwrap();
        fs::copy(source, fonts.join(filename)).unwrap();
    }
    let config = artifacts.join("fonts.conf");
    let cache = directory(artifacts.join("font-cache"));
    fs::write(
        &config,
        format!(
            r#"<fontconfig><dir>{}</dir><cachedir>{}</cachedir>
<alias><family>monospace</family><prefer><family>IBM Plex Sans</family></prefer></alias>
<match target="pattern"><edit name="dpi" mode="assign"><double>96</double></edit>
<edit name="hinting" mode="assign"><bool>true</bool></edit>
<edit name="hintstyle" mode="assign"><const>hintslight</const></edit>
<edit name="autohint" mode="assign"><bool>false</bool></edit>
<edit name="antialias" mode="assign"><bool>true</bool></edit>
<edit name="rgba" mode="assign"><const>none</const></edit></match></fontconfig>"#,
            xml(&fonts),
            xml(&cache)
        ),
    )
    .unwrap();
    let sway = PathBuf::from(
        std::env::var_os("NEOMACS_GUI_SWAY").expect("direct unwrapped Sway executable"),
    );
    let mut magic = [0; 4];
    fs::File::open(sway)
        .unwrap()
        .read_exact(&mut magic)
        .unwrap();
    assert_eq!(
        &magic, b"\x7fELF",
        "use unwrapped Sway, not a shell wrapper which can orphan children"
    );
    let helper = root.join("scripts/wayland-font-parity/bridge.py");
    let report = artifacts.join("session-env.json");
    let session = neomacs_infra::display::start_sway(&artifacts, &format!(
        "output * resolution 1000x900 scale 1\nxwayland force\nseat seat0 fallback true\ndefault_border none\ndefault_floating_border none\nfocus_follows_mouse no\nfor_window [all] floating enable\nexec python3 {} --report-display {}\n", word(&helper), word(&report))).unwrap();
    let initial_environment = session.env().to_vec();
    let mut display = OwnedDisplay {
        session: Some(session),
        env: initial_environment,
    };
    let environment = wait_json(&report, Duration::from_secs(12));
    display.env = string_env(&environment);
    assert!(environment["DISPLAY"].as_str().is_some());
    let bridge_dir = artifacts.join("bridge"); // The helper creates this fresh leaf.
    let mut command = Command::new("python3");
    command
        .arg(&helper)
        .arg("--serve")
        .arg("--artifacts")
        .arg(&bridge_dir)
        .arg("--gsd")
        .arg(std::env::var_os("NEOMACS_GUI_GSD_XSETTINGS").expect("real gsd-xsettings"))
        .envs(display.env.iter().cloned())
        .env_remove("WAYLAND_SOCKET")
        .env("FONTCONFIG_FILE", &config);
    for schemas in std::env::split_paths(
        &std::env::var_os("NEOMACS_GUI_GSD_SCHEMAS").expect("complete enum schemas"),
    ) {
        command.arg("--schema-dir").arg(schemas);
    }
    let mut bridge = OwnedProcess::spawn(command, &artifacts.join("bridge-process"));
    let ready = wait_json(&bridge_dir.join("ready.json"), Duration::from_secs(15));
    let settings = string_env(&ready["environment"]);
    let mut sequence = 0;
    // Prove the REAL bridge changes all active raster fields and X resources,
    // then restores them, before treating either editor as an oracle arm.
    policy(&bridge_dir, &mut sequence, "rgba", "full", "bgr");
    let restored = policy(&bridge_dir, &mut sequence, "grayscale", "slight", "rgb");
    fs::write(
        artifacts.join("bridge-live-proof.json"),
        serde_json::to_vec_pretty(&json!({"startup":ready,"restored":restored})).unwrap(),
    )
    .unwrap();
    let neo = PathBuf::from(
        std::env::var_os("NEOMACS_GUI_TEST_BINARY").expect("matching fresh Neo runtime"),
    );
    let mut gnu_live = editor(
        &root, &artifacts, "gnu-live", &oracle, true, &settings, &config,
    );
    let mut neo_live = editor(
        &root, &artifacts, "neo-live", &neo, false, &settings, &config,
    );
    place(&gnu_live, &display);
    place(&neo_live, &display);
    let gnu_start = observe(&mut gnu_live, &display, "startup", 0, &fonts);
    let neo_start = observe(&mut neo_live, &display, "startup", 0, &fonts);
    assert_eq!(gnu_start.state["font_render_setting_feature"], true);
    assert_eq!(gnu_start.state["xft_settings_bound"], true);
    let mut matrix = Vec::new();
    let mut difference = record_pair(
        &artifacts,
        &mut matrix,
        "live-startup",
        "grayscale",
        "slight",
        &restored,
        &gnu_start,
        &neo_start,
        Value::Null,
    );
    let mut changed = None;
    if !difference {
        // Real notifications only, with the ordinary dynamic-setting handler.
        let wire = policy(&bridge_dir, &mut sequence, "none", "none", "rgb");
        let g = observe(
            &mut gnu_live,
            &display,
            "changed",
            gnu_start.state["event_count"].as_u64().unwrap() + 1,
            &fonts,
        );
        let n = observe(
            &mut neo_live,
            &display,
            "changed",
            neo_start.state["event_count"].as_u64().unwrap() + 1,
            &fonts,
        );
        assert!(
            g.state["xft_settings"].as_str().is_some(),
            "GNU must observe actual Xft values"
        );
        save(
            &artifacts.join("live-events.json"),
            &json!({"gnu_start":gnu_start,"neo_start":neo_start,"gnu_changed":g,"neo_changed":n}),
        );
        difference = record_pair(
            &artifacts,
            &mut matrix,
            "live-changed",
            "none",
            "none",
            &wire,
            &g,
            &n,
            Value::Null,
        );
        changed = Some((g, n));
    }
    gnu_live.stop();
    neo_live.stop();
    if !difference {
        'matrix: for aa in ["none", "grayscale", "rgba"] {
            for hint in ["none", "slight", "full"] {
                let wire = policy(&bridge_dir, &mut sequence, aa, hint, "rgb");
                let name = format!("{aa}-{hint}");
                let mut gnu = editor(
                    &root,
                    &artifacts,
                    &format!("gnu-{name}"),
                    &oracle,
                    true,
                    &settings,
                    &config,
                );
                let mut neomacs = editor(
                    &root,
                    &artifacts,
                    &format!("neo-{name}"),
                    &neo,
                    false,
                    &settings,
                    &config,
                );
                place(&gnu, &display);
                place(&neomacs, &display);
                let g = observe(&mut gnu, &display, "startup", 0, &fonts);
                let n = observe(&mut neomacs, &display, "startup", 0, &fonts);
                let live_control = if aa == "none" && hint == "none" {
                    let (gc, nc) = changed.as_ref().unwrap();
                    json!({"gnu_live_fresh_metrics_equal":comparable_metrics(&gc.state)==comparable_metrics(&g.state),
                        "gnu_live_fresh_ink_equal":gc.ink==g.ink,
                        "neo_live_fresh_metrics_equal":comparable_metrics(&nc.state)==comparable_metrics(&n.state),
                        "neo_live_fresh_ink_equal":nc.ink==n.ink})
                } else {
                    Value::Null
                };
                difference = record_pair(
                    &artifacts,
                    &mut matrix,
                    "fresh",
                    aa,
                    hint,
                    &wire,
                    &g,
                    &n,
                    live_control,
                );
                gnu.stop();
                neomacs.stop();
                if difference {
                    break 'matrix;
                }
            }
        }
    }
    save(
        &bridge_dir.join("control.json"),
        &json!({"sequence":sequence+1,"stop":true}),
    );
    bridge.wait_success(Duration::from_secs(12));
    let cleanup = wait_json(&bridge_dir.join("cleanup.json"), Duration::from_secs(2));
    assert!(
        cleanup
            .as_array()
            .unwrap()
            .iter()
            .all(|child| !child["returncode"].is_null())
    );
    save(
        &artifacts.join("owned-cleanup.json"),
        &json!({"bridge_children":cleanup,"editors_reaped":true}),
    );
    display.shutdown();
    assert!(
        !difference,
        "REFERENCE DECISION REQUIRED: GNU-XWayland differs from Neo-Wayland; stopped without changing product policy. See {}",
        artifacts.display()
    );
}

#[derive(Clone, PartialEq, serde::Serialize)]
struct Observation {
    state: Value,
    ink: Vec<u64>,
}

fn record_pair(
    artifacts: &Path,
    matrix: &mut Vec<Value>,
    phase: &str,
    aa: &str,
    hint: &str,
    wire: &Value,
    g: &Observation,
    n: &Observation,
    live_control: Value,
) -> bool {
    let gm = comparable_metrics(&g.state);
    let nm = comparable_metrics(&n.state);
    let metrics_equal = gm == nm;
    let ink_equal = g.ink == n.ink;
    let controls_equal = live_control.is_null()
        || live_control
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == true);
    let difference = !metrics_equal || !ink_equal || !controls_equal;
    matrix.push(
        json!({"phase":phase,"antialias":aa,"hinting":hint,"rgba":"rgb","bridge":wire,
        "gnu":g,"neo":n,"gnu_metrics":gm,"neo_metrics":nm,"metrics_equal":metrics_equal,
        "ink_equal":ink_equal,"live_fresh_control":live_control}),
    );
    save(
        &artifacts.join("matrix.json"),
        &json!({"matrix":matrix,"requested_fresh_cells":9,
        "stopped_on_difference":difference,"native_pgtk_attested":false}),
    );
    eprintln!(
        "{phase} {aa}-{hint}: metrics_equal={metrics_equal}, ink_equal={ink_equal}, controls_equal={controls_equal}; {}",
        artifacts.display()
    );
    difference
}

fn comparable_metrics(state: &Value) -> Value {
    json!({"frame_char_width":state["frame_char_width"],"frame_char_height":state["frame_char_height"],
        "rows":state["rows"].as_array().unwrap().iter().map(|row| {
            let info=row["font_info"].as_array().unwrap();
            json!({"size":row["size"],"font_info_numeric":&info[2..12],"advance":row["advance"],
                "glyph_advances":row["glyph_regions"].as_array().unwrap().iter().map(|glyph|glyph["advance"].clone()).collect::<Vec<_>>()})
        }).collect::<Vec<_>>()})
}

fn directory(path: PathBuf) -> PathBuf {
    fs::create_dir_all(&path).unwrap();
    path
}
fn xml(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
}
fn word(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
fn save(path: &Path, data: &Value) {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(data).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
fn wait_json(path: &Path, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice(&bytes)
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "JSON readiness timeout: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(40));
    }
}
fn string_env(value: &Value) -> Vec<(String, String)> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect()
}
fn policy(dir: &Path, sequence: &mut u64, aa: &str, hint: &str, rgba: &str) -> Value {
    *sequence += 1;
    save(
        &dir.join("control.json"),
        &json!({"sequence":sequence,"antialias":aa,"hinting":hint,"rgba":rgba}),
    );
    wait_json(
        &dir.join(format!("reply-{sequence}.json")),
        Duration::from_secs(15),
    )
}

struct OwnedProcess {
    child: Child,
    dir: PathBuf,
    reaped: bool,
}
impl OwnedProcess {
    fn spawn(mut command: Command, dir: &Path) -> Self {
        directory(dir.to_owned());
        command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(fs::File::create(dir.join("stdout.log")).unwrap())
            .stderr(fs::File::create(dir.join("stderr.log")).unwrap());
        let child = command.spawn().unwrap();
        save(
            &dir.join("owned-process.json"),
            &json!({"pid":child.id(),"program":format!("{command:?}")}),
        );
        Self {
            child,
            dir: dir.to_owned(),
            reaped: false,
        }
    }
    fn has_exited(&self) -> bool {
        if self.reaped {
            return true;
        }
        // SAFETY: waitid writes a valid siginfo_t. WNOWAIT retains the owned
        // child's PID even after exit, so its private process group cannot be
        // reused before finish() signals remaining descendants and reaps it.
        unsafe {
            let mut status: libc::siginfo_t = std::mem::zeroed();
            let result = libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut status,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            );
            assert_eq!(result, 0, "waitid: {}", std::io::Error::last_os_error());
            status.si_pid() != 0
        }
    }
    fn signal_group(&self, signal: i32) {
        if self.reaped {
            return;
        }
        // SAFETY: process_group(0) created a private group. The leader remains
        // our unreaped child (including zombie state); its ID cannot be reused.
        unsafe {
            libc::kill(-(self.child.id() as i32), signal);
        }
    }
    fn finish(&mut self) -> std::process::ExitStatus {
        self.signal_group(libc::SIGKILL);
        let status = self.child.wait().unwrap();
        self.reaped = true;
        status
    }
    fn wait_success(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            if self.has_exited() {
                let status = self.finish();
                assert!(
                    status.success(),
                    "owned process failed: {status}; {}",
                    self.dir.display()
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "owned process did not exit: {}",
                self.dir.display()
            );
            thread::sleep(Duration::from_millis(30));
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        self.signal_group(libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline && !self.has_exited() {
            thread::sleep(Duration::from_millis(30));
        }
        self.finish();
    }
}

struct OwnedDisplay {
    session: Option<neomacs_infra::display::DisplaySession>,
    env: Vec<(String, String)>,
}
impl OwnedDisplay {
    fn ipc(&self, args: &[&str]) -> Value {
        let socket = self
            .env
            .iter()
            .find(|(k, _)| k == "SWAYSOCK")
            .unwrap()
            .1
            .as_str();
        let output = Command::new("swaymsg")
            .args(["-s", socket, "-r"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "sway IPC: {output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn shutdown(&mut self) {
        if self.session.is_none() {
            return;
        }
        let socket = self
            .env
            .iter()
            .find(|(k, _)| k == "SWAYSOCK")
            .map(|(_, value)| PathBuf::from(value))
            .or_else(|| {
                // Reporter failure must still allow a graceful owned Sway exit.
                let runtime = &self.env.iter().find(|(key, _)| key == "XDG_RUNTIME_DIR")?.1;
                fs::read_dir(runtime)
                    .ok()?
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name().is_some_and(|name| {
                            name.to_string_lossy().starts_with("sway-ipc.")
                                && name.to_string_lossy().ends_with(".sock")
                        })
                    })
            });
        if let Some(socket) = socket {
            let socket = socket.to_str().unwrap();
            let _ = Command::new("swaymsg")
                .args(["-s", socket, "exit"])
                .output();
            let deadline = Instant::now() + Duration::from_secs(3);
            while Path::new(socket).exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(30));
            }
        }
        self.session.take();
    }
}
impl Drop for OwnedDisplay {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Editor {
    process: OwnedProcess,
    gnu: bool,
}
fn editor(
    root: &Path,
    artifacts: &Path,
    name: &str,
    binary: &Path,
    gnu: bool,
    env: &[(String, String)],
    config: &Path,
) -> Editor {
    let dir = directory(artifacts.join(name));
    fs::write(dir.join("phase"), "startup").unwrap();
    let mut command = Command::new(binary);
    command
        .args([
            "-Q",
            "--eval",
            "(set-frame-parameter nil 'undecorated t)",
            "-l",
        ])
        .arg(root.join("crates/neomacs-gui-tests/fixtures/font-xwayland-parity.el"))
        .envs(env.iter().cloned())
        .env_remove("WAYLAND_SOCKET")
        .env("NEOMACS_FONT_XWAYLAND_CONTROL", &dir)
        .env("FONTCONFIG_FILE", config)
        .env("GTK_CSD", "0")
        .env("NO_AT_BRIDGE", "1");
    for key in [
        "NEOMACS_DEBUG_FIRST_FRAME_READBACK",
        "NEOMACS_DEBUG_SURFACE_READBACK",
        "NEOMACS_DEBUG_SURFACE_READBACK_PNG",
    ] {
        command.env_remove(key);
    }
    if gnu {
        command
            .env("GDK_BACKEND", "x11")
            .env_remove("WAYLAND_DISPLAY");
    } else {
        command
            .env("WINIT_UNIX_BACKEND", "wayland")
            .env("WGPU_BACKEND", "vulkan")
            .env("NEOMACS_RUNTIME_ROOT", root)
            .env_remove("DISPLAY");
    }
    Editor {
        process: OwnedProcess::spawn(command, &dir),
        gnu,
    }
}
impl Editor {
    fn stop(&mut self) {
        fs::write(self.process.dir.join("phase"), "stop").unwrap();
        self.process.wait_success(Duration::from_secs(5));
    }
}
fn mapped(display: &OwnedDisplay, pid: u32, gnu: bool) -> Value {
    fn find(node: &Value, pid: u32) -> Option<Value> {
        if node["pid"].as_u64() == Some(u64::from(pid)) && node["visible"] == true {
            return Some(node.clone());
        }
        ["nodes", "floating_nodes"]
            .into_iter()
            .find_map(|key| node[key].as_array()?.iter().find_map(|n| find(n, pid)))
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(node) = find(&display.ipc(&["-t", "get_tree"]), pid) {
            assert_eq!(node["shell"], if gnu { "xwayland" } else { "xdg_shell" });
            return node;
        }
        assert!(
            Instant::now() < deadline,
            "owned editor PID {pid} did not map"
        );
        thread::sleep(Duration::from_millis(50));
    }
}
// Position both arms before either screenshot: a newly mapped floating window
// can otherwise occlude the first arm's compositor pixels.
fn place(editor: &Editor, display: &OwnedDisplay) {
    let pid = editor.process.child.id();
    let native = mapped(display, pid, editor.gnu);
    let id = native["id"].as_u64().unwrap();
    let y = if editor.gnu { 20 } else { 460 };
    let result=display.ipc(&[&format!("[con_id={id}] floating enable, border none, resize set width 900 px height 400 px, move position 20 px {y} px")]);
    assert!(
        result
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["success"] == true)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let node = mapped(display, pid, editor.gnu);
        if node["rect"]["x"].as_i64() == Some(20)
            && node["rect"]["y"].as_i64() == Some(y)
            && node["rect"]["height"]
                .as_u64()
                .is_some_and(|height| height <= 420)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "editor did not reach disjoint slot: {node}"
        );
        thread::sleep(Duration::from_millis(30));
    }
}
fn observe(
    editor: &mut Editor,
    display: &OwnedDisplay,
    phase: &str,
    events: u64,
    fonts: &Path,
) -> Observation {
    let pid = editor.process.child.id();
    fs::write(editor.process.dir.join("phase"), phase).unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut previous = None;
    loop {
        assert!(
            !editor.process.has_exited(),
            "editor exited: {}",
            editor.process.dir.display()
        );
        let state = fs::read(editor.process.dir.join(format!("{phase}.json")))
            .ok()
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok());
        if let Some(state) = state
            && state["complete"] == true
            && state["stable_samples"].as_u64().unwrap_or(0) >= 2
            && state["event_count"].as_u64().unwrap_or(0) >= events
        {
            assert_eq!(state["pid"].as_u64(), Some(u64::from(pid)));
            let native = mapped(display, pid, editor.gnu);
            let png = editor.process.dir.join(format!("{phase}.png"));
            let captured =
                Command::new(std::env::var_os("NEOMACS_GUI_GRIM").unwrap_or_else(|| "grim".into()))
                    .arg(&png)
                    .envs(display.env.iter().cloned())
                    .env_remove("WAYLAND_SOCKET")
                    .output()
                    .unwrap();
            assert!(captured.status.success(), "grim: {captured:?}");
            let image = image::open(png).unwrap().to_rgba8();
            let mut ink = Vec::new();
            assert_eq!(state["rows"].as_array().unwrap().len(), FONTS.len());
            for (row, (filename, _)) in state["rows"].as_array().unwrap().iter().zip(FONTS) {
                assert_eq!(row["family"], "IBM Plex Sans");
                assert_eq!(row["size"].as_f64(), Some(16.0));
                assert_eq!(
                    fs::canonicalize(row["file"].as_str().unwrap()).unwrap(),
                    fs::canonicalize(fonts.join(filename)).unwrap()
                );
                let bounds: Vec<u32> = serde_json::from_value(row["bounds"].clone()).unwrap();
                let client = &native["window_rect"];
                let x = native["rect"]["x"].as_u64().unwrap() as u32
                    + client["x"].as_u64().unwrap_or(0) as u32
                    + bounds[0];
                let y = native["rect"]["y"].as_u64().unwrap() as u32
                    + client["y"].as_u64().unwrap_or(0) as u32
                    + bounds[1];
                let w = bounds[2] - bounds[0];
                let h = bounds[3] - bounds[1];
                assert!(w > 0 && h > 0 && x + w <= image.width() && y + h <= image.height());
                let crop = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                assert!(crop.pixels().all(|p| p.0[3] == 255));
                let sum = crop
                    .pixels()
                    .map(|p| 765 - u64::from(p.0[0]) - u64::from(p.0[1]) - u64::from(p.0[2]))
                    .sum::<u64>();
                assert!(sum > 0, "visible text ink");
                ink.push(sum);
            }
            let observation = Observation {
                state: state.clone(),
                ink,
            };
            if previous.as_ref()
                == Some(&(
                    comparable_metrics(&state),
                    observation.ink.clone(),
                    state["event_count"].clone(),
                ))
            {
                save(
                    &editor.process.dir.join(format!("{phase}-observed.json")),
                    &json!({"state":state,"ink":observation.ink,"native":native}),
                );
                return observation;
            }
            previous = Some((
                comparable_metrics(&state),
                observation.ink.clone(),
                state["event_count"].clone(),
            ));
        }
        assert!(
            Instant::now() < deadline,
            "editor observation timeout for {phase}: {}",
            editor.process.dir.display()
        );
        thread::sleep(Duration::from_millis(150));
    }
}
