//! GUI visual test harness for Neomacs.
//!
//! The crate keeps GUI checks text-first for automation: each scenario has an
//! explicit display backend and writes stable JSON/PNG/log artifacts under
//! `target/neomacs-gui-tests`.

use std::fs;
pub mod interaction;
/// Account-free Telega frontend fixture: deterministic synthetic chats,
/// locally generated avatars, and a `telega-server`-protocol mock process.
pub mod telega_fixture;
use std::io;
use std::io::Read;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiBackend {
    LinuxX11,
    LinuxWayland,
    Macos,
    Windows,
}

impl GuiBackend {
    pub fn runner_kind(self) -> RunnerKind {
        match self {
            Self::LinuxX11 => RunnerKind::Xvfb,
            Self::LinuxWayland => RunnerKind::WestonHeadless,
            Self::Macos | Self::Windows => RunnerKind::CurrentDesktopSession,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::LinuxX11 => "linux-x11",
            Self::LinuxWayland => "linux-wayland",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }

    fn winit_unix_backend(self) -> Option<&'static str> {
        match self {
            Self::LinuxX11 => Some("x11"),
            Self::LinuxWayland => Some("wayland"),
            Self::Macos | Self::Windows => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    Xvfb,
    WestonHeadless,
    CurrentDesktopSession,
}

impl RunnerKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Xvfb => "xvfb",
            Self::WestonHeadless => "weston-headless",
            Self::CurrentDesktopSession => "current-desktop-session",
        }
    }
}

/// A capability a GUI test can require from its host.
///
/// Tests state what they need, not which platform they were written on. A
/// backend advertises what it can provide; the harness either runs the test
/// or reports exactly what is missing, so a platform gap is never a silent
/// pass and never a bare `#![cfg(target_os = ...)]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u32)]
pub enum Capability {
    /// A window-server session in which a real frame can be shown.
    Display,
    /// Fixture, Elisp, and control-socket driving plus snapshot/readback
    /// observation. This is the portable tier.
    ScenarioDriver,
    /// Real OS input injection (xdotool/wtype, Quartz events, SendInput).
    /// Available in principle on every desktop backend; the native driver's
    /// own preflight decides whether this host can use it.
    NativeInput,
    /// Real desktop pixel capture (ScreenCaptureKit, compositor capture).
    ScreenCapture,
    /// A harness-owned compositor (Xvfb, Weston) that the test may start.
    CompositorControl,
    /// A pinned GNU Emacs oracle is configured for comparisons.
    GnuOracle,
    /// The window server will actually make a window drawable: an unlocked
    /// console, an awake display, and a window on the active Space. Without
    /// it a frame can be composed but never presented - macOS reports
    /// `NSWindowOcclusionStateVisible == false` and wgpu returns `Occluded` -
    /// so a paint-proof test must not treat that as a rendering failure.
    SurfacePresent,
}

impl Capability {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::ScenarioDriver => "scenario-driver",
            Self::NativeInput => "native-input",
            Self::ScreenCapture => "screen-capture",
            Self::CompositorControl => "compositor-control",
            Self::GnuOracle => "gnu-oracle",
            Self::SurfacePresent => "surface-present",
        }
    }

    pub const fn all() -> [Capability; 7] {
        [
            Self::Display,
            Self::ScenarioDriver,
            Self::NativeInput,
            Self::ScreenCapture,
            Self::CompositorControl,
            Self::GnuOracle,
            Self::SurfacePresent,
        ]
    }

    const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A set of capabilities advertised by, or required from, a host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn from_capabilities(capabilities: &[Capability]) -> Self {
        let mut set = Self::empty();
        for capability in capabilities {
            set.insert(*capability);
        }
        set
    }

    pub fn insert(&mut self, capability: Capability) {
        self.0 |= capability.bit();
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    /// Required capabilities this set does not provide, in requirement order.
    pub fn missing(self, required: &[Capability]) -> Vec<Capability> {
        required
            .iter()
            .copied()
            .filter(|capability| !self.contains(*capability))
            .collect()
    }

    pub fn names(self) -> Vec<&'static str> {
        Capability::all()
            .into_iter()
            .filter(|capability| self.contains(*capability))
            .map(Capability::slug)
            .collect()
    }
}

impl GuiBackend {
    /// What this backend can provide on a correctly provisioned host.
    ///
    /// Native input and capture are advertised on every desktop backend; the
    /// out-of-process driver still reports `Blocked` when the host has not
    /// granted the permissions it needs. A backend-level capability says
    /// "this platform can", not "this host is provisioned".
    pub fn capabilities(self) -> CapabilitySet {
        match self {
            Self::LinuxX11 | Self::LinuxWayland => CapabilitySet::from_capabilities(&[
                Capability::Display,
                Capability::ScenarioDriver,
                Capability::NativeInput,
                Capability::ScreenCapture,
                Capability::CompositorControl,
            ]),
            Self::Macos => CapabilitySet::from_capabilities(&[
                Capability::Display,
                Capability::ScenarioDriver,
                Capability::NativeInput,
                Capability::ScreenCapture,
            ]),
            Self::Windows => CapabilitySet::from_capabilities(&[
                Capability::Display,
                Capability::ScenarioDriver,
                Capability::NativeInput,
                Capability::ScreenCapture,
            ]),
        }
    }
}

/// The backend a test should drive: `NEOMACS_GUI_TEST_BACKEND` when set,
/// otherwise the host's native backend.
///
/// One resolver for the whole suite keeps a test from hard-coding a platform
/// while the runner's environment says another.
pub fn requested_backend() -> Option<GuiBackend> {
    let configured = std::env::var("NEOMACS_GUI_TEST_BACKEND").unwrap_or_else(|_| {
        cfg_select! {
            target_os = "macos" => { "macos".to_string() }
            windows => { "windows".to_string() }
            _ => { "x11".to_string() }
        }
    });
    match configured.as_str() {
        "wayland" | "linux-wayland" => Some(GuiBackend::LinuxWayland),
        "x11" | "linux-x11" => Some(GuiBackend::LinuxX11),
        "macos" => Some(GuiBackend::Macos),
        "windows" => Some(GuiBackend::Windows),
        other => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

/// Capabilities of the host running the test, beyond the backend's own.
fn host_capabilities(backend: GuiBackend) -> CapabilitySet {
    let mut capabilities = backend.capabilities();
    if std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS").is_some_and(|value| !value.is_empty()) {
        capabilities.insert(Capability::GnuOracle);
    }
    if screen_presentable(backend) {
        capabilities.insert(Capability::SurfacePresent);
    }
    capabilities
}

/// Whether this backend's window server can make a window drawable now.
///
/// A harness-owned Linux compositor always can. macOS cannot while the
/// console is locked (or the display is asleep): AppKit reports the window
/// as non-visible and wgpu refuses the drawable. A failed probe is treated
/// as presentable so a sandboxed or unusual host is never skipped by
/// accident; only a positive "locked" answer removes the capability.
fn screen_presentable(backend: GuiBackend) -> bool {
    match backend {
        GuiBackend::LinuxX11 | GuiBackend::LinuxWayland | GuiBackend::Windows => true,
        GuiBackend::Macos => !macos_screen_locked(),
    }
}

#[cfg(target_os = "macos")]
fn macos_screen_locked() -> bool {
    let Ok(output) = std::process::Command::new("ioreg")
        .args(["-n", "Root", "-d1", "-a"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    plist_bool_after(&text, "IOConsoleLocked")
        .or_else(|| plist_bool_after(&text, "CGSSessionScreenIsLocked"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn macos_screen_locked() -> bool {
    false
}

/// The boolean that follows `key` in an XML property list, if any.
fn plist_bool_after(text: &str, key: &str) -> Option<bool> {
    let tag = format!("<key>{key}</key>");
    let rest = &text[text.find(&tag)? + tag.len()..];
    match (rest.find("<true/>"), rest.find("<false/>")) {
        (Some(yes), Some(no)) => Some(yes < no),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayHarness {
    Xvfb,
    WestonHeadless(WaylandOutput),
    CurrentDesktopSession,
}

pub use neomacs_infra::display::{WaylandOutput, WestonDesktop, start_weston_with_desktop};
use neomacs_infra::display::{start_weston_headless, start_xvfb};

impl DisplayHarness {
    pub fn for_backend(backend: GuiBackend) -> Self {
        match backend {
            GuiBackend::LinuxX11 => Self::Xvfb,
            GuiBackend::LinuxWayland => Self::WestonHeadless(WaylandOutput::Standard),
            GuiBackend::Macos | GuiBackend::Windows => Self::CurrentDesktopSession,
        }
    }

    pub fn required_env(&self) -> &'static [&'static str] {
        match self {
            Self::Xvfb => &["DISPLAY"],
            Self::WestonHeadless(_) => &["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY"],
            Self::CurrentDesktopSession => &[],
        }
    }

    pub fn start_session(&self, artifact_root: impl AsRef<Path>) -> io::Result<DisplaySession> {
        match self {
            Self::WestonHeadless(output) => start_weston_headless(artifact_root.as_ref(), *output),
            Self::Xvfb => start_xvfb(artifact_root.as_ref()),
            Self::CurrentDesktopSession => Ok(DisplaySession::current_desktop()),
        }
    }
}

pub use neomacs_infra::display::DisplaySession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiArtifactSet {
    pub json: PathBuf,
    pub png: PathBuf,
    pub stderr: PathBuf,
    pub stdout: PathBuf,
    pub gui_state: PathBuf,
    /// Full-fidelity display oracle: serde JSON of the real
    /// `FrameDisplayState`s, written by `neomacs--write-frame-snapshot`.
    pub frame_snapshot_json: PathBuf,
    /// Greppable text (text-faces) rendering of the same snapshot.
    pub frame_snapshot_txt: PathBuf,
    /// Raw `prin1` result of the font-selection oracle form in GNU Emacs.
    pub gnu_font_result: PathBuf,
    /// Raw `prin1` result of the same font-selection oracle form in Neomacs.
    pub neomacs_font_result: PathBuf,
    /// Text diff between `gnu_font_result` and `neomacs_font_result`.
    pub font_oracle_diff: PathBuf,
    /// Neomacs diagnostic log for the scenario.
    pub neomacs_log: PathBuf,
}

impl GuiArtifactSet {
    pub fn new(root: impl Into<PathBuf>, backend: GuiBackend, scenario_name: &str) -> Self {
        let dir = root.into().join(backend.slug());
        Self {
            json: dir.join(format!("{scenario_name}.json")),
            png: dir.join(format!("{scenario_name}.png")),
            stderr: dir.join(format!("{scenario_name}.stderr.log")),
            stdout: dir.join(format!("{scenario_name}.stdout.log")),
            gui_state: dir.join(format!("{scenario_name}.gui-state.json")),
            frame_snapshot_json: dir.join(format!("{scenario_name}.frame-snapshot.json")),
            frame_snapshot_txt: dir.join(format!("{scenario_name}.frame-snapshot.txt")),
            gnu_font_result: dir.join(format!("{scenario_name}.gnu-result.el")),
            neomacs_font_result: dir.join(format!("{scenario_name}.neomacs-result.el")),
            font_oracle_diff: dir.join(format!("{scenario_name}.font-oracle.diff")),
            neomacs_log: dir.join(format!("{scenario_name}.neomacs.log")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiScenario {
    pub name: String,
    pub script: PathBuf,
}

impl GuiScenario {
    pub fn new(name: impl Into<String>, script: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            script: script.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiTestPlan {
    backend: GuiBackend,
    workspace_root: PathBuf,
    artifact_root: PathBuf,
    scenario: GuiScenario,
    program: Option<PathBuf>,
    args: Option<Vec<String>>,
    extra_env: Vec<(String, String)>,
    required_capabilities: Vec<Capability>,
}

impl GuiTestPlan {
    pub fn new(
        backend: GuiBackend,
        workspace_root: impl Into<PathBuf>,
        artifact_root: impl Into<PathBuf>,
        scenario: GuiScenario,
    ) -> Self {
        Self {
            backend,
            workspace_root: workspace_root.into(),
            artifact_root: artifact_root.into(),
            scenario,
            program: None,
            args: None,
            extra_env: Vec::new(),
            required_capabilities: Vec::new(),
        }
    }

    /// Declare a capability this scenario needs from its host.
    ///
    /// When the host lacks it, `run_with` does not launch the editor and
    /// returns `GuiRunStatus::Unsupported` with the missing capability named
    /// in the manifest, instead of a failure that looks like a product bug.
    pub fn require(mut self, capability: Capability) -> Self {
        if !self.required_capabilities.contains(&capability) {
            self.required_capabilities.push(capability);
        }
        self
    }

    pub fn backend(&self) -> GuiBackend {
        self.backend
    }

    pub fn required_capabilities(&self) -> &[Capability] {
        &self.required_capabilities
    }

    /// Declared requirements the host does not provide.
    pub fn missing_capabilities(&self) -> Vec<Capability> {
        host_capabilities(self.backend).missing(&self.required_capabilities)
    }

    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// Replace the default `-Q -l SCENARIO` argv for startup-lifecycle tests
    /// whose public seam is an init directory or another command-line mode.
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = Some(args.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// Use packages prepared by `bash scripts/ci/setup-gui-test-packages.sh`.
    /// Explicit `with_env` package paths supplied before this call win.
    /// Preparing dependencies is separate from running tests: no hidden network
    /// access or mutable package versions are introduced by a GUI scenario.
    pub fn with_svg_packages(mut self) -> Self {
        for (variable, package) in [
            ("NEOMACS_GUI_SVG_LIB_DIR", "svg-lib"),
            ("NEOMACS_GUI_SVG_TAG_MODE_DIR", "svg-tag-mode"),
        ] {
            if !self.extra_env.iter().any(|(key, _)| key == variable) {
                self.extra_env.push((
                    variable.into(),
                    path_to_string(
                        &self
                            .workspace_root
                            .join("target/neomacs-gui-tests/packages")
                            .join(package),
                    ),
                ));
            }
        }
        self
    }

    pub fn command_spec(&self) -> CommandSpec {
        let artifacts = GuiArtifactSet::new(
            &self.artifact_root,
            self.backend,
            self.scenario.name.as_str(),
        );
        let mut command = CommandSpec {
            program: self
                .program
                .clone()
                .unwrap_or_else(|| self.workspace_root.join("target/release/neomacs")),
            args: self.args.clone().unwrap_or_else(|| {
                vec![
                    "-Q".to_string(),
                    "-l".to_string(),
                    path_to_string(&self.scenario.script),
                ]
            }),
            env: vec![
                (
                    "NEOMACS_DEBUG_FIRST_FRAME_READBACK".to_string(),
                    "1".to_string(),
                ),
                (
                    "NEOMACS_DEBUG_SURFACE_READBACK".to_string(),
                    "1".to_string(),
                ),
                (
                    "NEOMACS_DEBUG_SURFACE_READBACK_PNG".to_string(),
                    path_to_string(&artifacts.png),
                ),
                (
                    "NEOMACS_GUI_STATE_JSON".to_string(),
                    path_to_string(&artifacts.gui_state),
                ),
                (
                    "NEOMACS_GUI_FRAME_SNAPSHOT_JSON".to_string(),
                    path_to_string(&artifacts.frame_snapshot_json),
                ),
                (
                    "NEOMACS_GUI_FRAME_SNAPSHOT_TXT".to_string(),
                    path_to_string(&artifacts.frame_snapshot_txt),
                ),
                (
                    "NEOMACS_GUI_FONT_SELECTION_RESULT".to_string(),
                    path_to_string(&artifacts.neomacs_font_result),
                ),
            ],
        };

        if let Some(winit_backend) = self.backend.winit_unix_backend() {
            command
                .env
                .push(("WINIT_UNIX_BACKEND".to_string(), winit_backend.to_string()));
        }
        command.env.extend(self.extra_env.iter().cloned());

        command
    }

    pub fn write_manifest(&self) -> io::Result<GuiArtifactSet> {
        let artifacts = GuiArtifactSet::new(
            &self.artifact_root,
            self.backend,
            self.scenario.name.as_str(),
        );
        let command = self.command_spec();
        self.write_manifest_json(&artifacts, planned_manifest(self, &command, &artifacts))?;
        Ok(artifacts)
    }

    pub fn run_with(
        &self,
        runner: &mut impl GuiCommandRunner,
        options: GuiRunOptions,
    ) -> io::Result<GuiRunResult> {
        let artifacts = self.write_manifest()?;
        let command = self.command_spec();
        let missing = self.missing_capabilities();
        if !missing.is_empty() {
            let reason = format!(
                "backend {} lacks required capabilities: {}",
                self.backend.slug(),
                missing
                    .iter()
                    .map(|capability| capability.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Some(parent) = artifacts.stderr.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&artifacts.stderr, format!("{reason}\n"))?;
            let result = GuiRunResult {
                artifacts: artifacts.clone(),
                status: GuiRunStatus::Unsupported,
                exit_code: None,
                timed_out: false,
                png_bytes: None,
                gui_state_bytes: None,
                frame_snapshot_json_bytes: None,
                frame_snapshot_txt_bytes: None,
                neomacs_font_result_bytes: None,
                neomacs_log_bytes: None,
                stderr_bytes: reason.len() as u64 + 1,
                stdout_bytes: 0,
                gui_state: None,
                readback_diagnostics: Vec::new(),
                failure_reason: Some(reason),
            };
            self.write_manifest_json(&artifacts, result_manifest(self, &command, &result))?;
            return Ok(result);
        }
        let output = runner.run(&command, &artifacts, &options)?;

        if let Some(parent) = artifacts.stderr.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&artifacts.stderr, &output.stderr)?;
        fs::write(&artifacts.stdout, &output.stdout)?;

        let png_bytes = fs::metadata(&artifacts.png)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        // The PNG is the scenario's paint proof only when the scenario asked
        // for a presentable surface. A semantic scenario (snapshot/geometry)
        // can succeed on a host whose windows cannot be presented - a locked
        // macOS console, for instance - and its missing PNG is evidence the
        // paint tier did not run, not a failure of the test's own contract.
        let paint_required = self
            .required_capabilities
            .contains(&Capability::SurfacePresent);
        let status = if paint_required {
            if png_bytes.is_some() && (output.exit_code == Some(0) || output.timed_out) {
                GuiRunStatus::Passed
            } else if output.timed_out {
                GuiRunStatus::TimedOut
            } else {
                GuiRunStatus::Failed
            }
        } else if output.exit_code == Some(0) {
            GuiRunStatus::Passed
        } else if output.timed_out {
            GuiRunStatus::TimedOut
        } else {
            GuiRunStatus::Failed
        };
        let stderr_bytes = output.stderr.len() as u64;
        let stdout_bytes = output.stdout.len() as u64;
        let gui_state = read_gui_state(&artifacts.gui_state)?;
        let gui_state_bytes = fs::metadata(&artifacts.gui_state)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        let frame_snapshot_json_bytes = fs::metadata(&artifacts.frame_snapshot_json)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        let frame_snapshot_txt_bytes = fs::metadata(&artifacts.frame_snapshot_txt)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        let neomacs_font_result_bytes = fs::metadata(&artifacts.neomacs_font_result)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        let neomacs_log_bytes = fs::metadata(&artifacts.neomacs_log)
            .ok()
            .map(|metadata| metadata.len())
            .filter(|len| *len > 0);
        let readback_diagnostics = readback_diagnostics(&output.stderr);
        let failure_reason = failure_reason(status, output.exit_code, png_bytes);
        let result = GuiRunResult {
            artifacts: artifacts.clone(),
            status,
            exit_code: output.exit_code,
            timed_out: output.timed_out,
            png_bytes,
            gui_state_bytes,
            frame_snapshot_json_bytes,
            frame_snapshot_txt_bytes,
            neomacs_font_result_bytes,
            neomacs_log_bytes,
            stderr_bytes,
            stdout_bytes,
            gui_state,
            readback_diagnostics,
            failure_reason,
        };

        self.write_manifest_json(&artifacts, result_manifest(self, &command, &result))?;
        Ok(result)
    }

    fn write_manifest_json(
        &self,
        artifacts: &GuiArtifactSet,
        manifest: serde_json::Value,
    ) -> io::Result<()> {
        if let Some(parent) = artifacts.json.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&artifacts.json, format!("{}\n", manifest))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl CommandSpec {
    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiRunOptions {
    pub timeout: Duration,
}

impl GuiRunOptions {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for GuiRunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiRunStatus {
    Passed,
    Failed,
    TimedOut,
    /// The host lacks a capability the scenario declared it needs. The
    /// editor was never launched; the manifest names what is missing.
    Unsupported,
}

impl GuiRunStatus {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::TimedOut => "timed-out",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiCommandOutput {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait GuiCommandRunner {
    fn run(
        &mut self,
        command: &CommandSpec,
        artifacts: &GuiArtifactSet,
        options: &GuiRunOptions,
    ) -> io::Result<GuiCommandOutput>;
}

/// Run an input-injection tool (wtype, xdotool, ...) with a deadline.
///
/// These tools talk to a live compositor; when the compositor stops
/// delivering input the child can block forever, and a bare `output()` would
/// then hang the whole test until nextest's slow-timeout kills the run
/// without saying why.  Kill the tool at the deadline and fail with its
/// status and stderr instead.
#[must_use]
pub fn run_input_tool(
    mut command: std::process::Command,
    deadline: Duration,
) -> std::process::Output {
    use std::process::Stdio;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn input tool");
    let start = Instant::now();
    loop {
        if child.try_wait().expect("wait for input tool").is_some() {
            return child.wait_with_output().expect("collect input tool output");
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect input tool output");
            panic!(
                "input tool did not finish within {deadline:?}: status={:?} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[derive(Debug, Default)]
pub struct ProcessGuiCommandRunner;

impl GuiCommandRunner for ProcessGuiCommandRunner {
    fn run(
        &mut self,
        command: &CommandSpec,
        _artifacts: &GuiArtifactSet,
        options: &GuiRunOptions,
    ) -> io::Result<GuiCommandOutput> {
        let mut process = Command::new(&command.program);
        process
            .args(&command.args)
            .envs(command.env.iter().map(|(key, value)| (key, value)))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A scenario's session is exactly one window system.  Inheriting the
        // other backend's variables from the developer's desktop (or a CI
        // step) makes winit/wgpu observe a second, usually unreachable,
        // display: a Wayland scenario with DISPLAY set stalls before its
        // first present, and an X11 scenario can silently bind a stale
        // Wayland socket.  Scrub them so the test exercises the backend it
        // names, on every host.
        match command.env_value("WINIT_UNIX_BACKEND") {
            Some("wayland") => {
                process.env_remove("DISPLAY");
            }
            Some("x11") => {
                process.env_remove("WAYLAND_DISPLAY");
                process.env_remove("WAYLAND_SOCKET");
            }
            _ => {}
        }
        let mut child = process.spawn()?;

        let stdout = child.stdout.take().expect("stdout pipe should be captured");
        let stderr = child.stderr.take().expect("stderr pipe should be captured");
        let stdout_reader = thread::spawn(move || read_pipe(stdout));
        let stderr_reader = thread::spawn(move || read_pipe(stderr));

        let start = Instant::now();
        let (exit_code, timed_out) = loop {
            if let Some(status) = child.try_wait()? {
                break (status.code(), false);
            }
            if start.elapsed() >= options.timeout {
                let _ = child.kill();
                let status = child.wait()?;
                break (status.code(), true);
            }
            thread::sleep(Duration::from_millis(50));
        };

        let stdout = stdout_reader.join().unwrap_or_else(|_| Ok(String::new()))?;
        let stderr = stderr_reader.join().unwrap_or_else(|_| Ok(String::new()))?;

        Ok(GuiCommandOutput {
            exit_code,
            timed_out,
            stdout,
            stderr,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiRunResult {
    pub artifacts: GuiArtifactSet,
    pub status: GuiRunStatus,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub png_bytes: Option<u64>,
    pub gui_state_bytes: Option<u64>,
    pub frame_snapshot_json_bytes: Option<u64>,
    pub frame_snapshot_txt_bytes: Option<u64>,
    pub neomacs_font_result_bytes: Option<u64>,
    pub neomacs_log_bytes: Option<u64>,
    pub stderr_bytes: u64,
    pub stdout_bytes: u64,
    pub gui_state: Option<serde_json::Value>,
    pub readback_diagnostics: Vec<String>,
    pub failure_reason: Option<String>,
}

fn read_pipe(mut pipe: impl Read) -> io::Result<String> {
    let mut output = String::new();
    pipe.read_to_string(&mut output)?;
    Ok(output)
}

fn planned_manifest(
    plan: &GuiTestPlan,
    command: &CommandSpec,
    artifacts: &GuiArtifactSet,
) -> serde_json::Value {
    serde_json::json!({
        "status": "planned",
        "scenario": plan.scenario.name,
        "backend": plan.backend.slug(),
        "runner": plan.backend.runner_kind().slug(),
        "capabilities": capabilities_json(plan),
        "command": command_json(command),
        "expected_artifacts": artifacts_json(artifacts),
        "ai_agent_note": "Read this JSON and the stderr log first. Open the PNG only when visual inspection is needed.",
    })
}

/// The capability contract of one scenario: what the backend provides, what
/// the scenario declared it needs, and what is missing.
fn capabilities_json(plan: &GuiTestPlan) -> serde_json::Value {
    serde_json::json!({
        "available": host_capabilities(plan.backend).names(),
        "required": plan
            .required_capabilities
            .iter()
            .map(|capability| capability.slug())
            .collect::<Vec<_>>(),
        "missing": plan
            .missing_capabilities()
            .iter()
            .map(|capability| capability.slug())
            .collect::<Vec<_>>(),
    })
}

fn result_manifest(
    plan: &GuiTestPlan,
    command: &CommandSpec,
    result: &GuiRunResult,
) -> serde_json::Value {
    serde_json::json!({
        "status": result.status.slug(),
        "scenario": plan.scenario.name,
        "backend": plan.backend.slug(),
        "runner": plan.backend.runner_kind().slug(),
        "capabilities": capabilities_json(plan),
        "command": command_json(command),
        "expected_artifacts": artifacts_json(&result.artifacts),
        "observed_artifacts": {
            "png_exists": result.png_bytes.is_some(),
            "png_bytes": result.png_bytes,
            "gui_state_exists": result.gui_state_bytes.is_some(),
            "gui_state_bytes": result.gui_state_bytes,
            "frame_snapshot_json_exists": result.frame_snapshot_json_bytes.is_some(),
            "frame_snapshot_json_bytes": result.frame_snapshot_json_bytes,
            "frame_snapshot_txt_exists": result.frame_snapshot_txt_bytes.is_some(),
            "frame_snapshot_txt_bytes": result.frame_snapshot_txt_bytes,
            "neomacs_font_result_exists": result.neomacs_font_result_bytes.is_some(),
            "neomacs_font_result_bytes": result.neomacs_font_result_bytes,
            "neomacs_log_exists": result.neomacs_log_bytes.is_some(),
            "neomacs_log_bytes": result.neomacs_log_bytes,
            "stderr_bytes": result.stderr_bytes,
            "stdout_bytes": result.stdout_bytes,
        },
        "gui_state": result.gui_state,
        "process": {
            "exit_code": result.exit_code,
            "timed_out": result.timed_out,
        },
        "readback_diagnostics": result.readback_diagnostics,
        "failure_reason": result.failure_reason,
        "ai_agent_note": "Use status, observed_artifacts, stderr, and readback_diagnostics for automated checks. The PNG is supplemental visual evidence.",
    })
}

fn command_json(command: &CommandSpec) -> serde_json::Value {
    serde_json::json!({
        "program": path_to_string(&command.program),
        "args": command.args,
        "env": command.env.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
    })
}

fn artifacts_json(artifacts: &GuiArtifactSet) -> serde_json::Value {
    serde_json::json!({
        "json": path_to_string(&artifacts.json),
        "png": path_to_string(&artifacts.png),
        "stderr": path_to_string(&artifacts.stderr),
        "stdout": path_to_string(&artifacts.stdout),
        "gui_state": path_to_string(&artifacts.gui_state),
        "frame_snapshot_json": path_to_string(&artifacts.frame_snapshot_json),
        "frame_snapshot_txt": path_to_string(&artifacts.frame_snapshot_txt),
        "gnu_font_result": path_to_string(&artifacts.gnu_font_result),
        "neomacs_font_result": path_to_string(&artifacts.neomacs_font_result),
        "font_oracle_diff": path_to_string(&artifacts.font_oracle_diff),
        "neomacs_log": path_to_string(&artifacts.neomacs_log),
    })
}

fn read_gui_state(path: &Path) -> io::Result<Option<serde_json::Value>> {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn readback_diagnostics(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("surface readback") || lower.contains("bottom_band_avg")
        })
        .map(str::to_string)
        .collect()
}

fn failure_reason(
    status: GuiRunStatus,
    exit_code: Option<i32>,
    png_bytes: Option<u64>,
) -> Option<String> {
    match (status, exit_code, png_bytes) {
        (GuiRunStatus::Passed, _, _) => None,
        (GuiRunStatus::TimedOut, _, _) => Some("GUI command timed out".to_string()),
        (GuiRunStatus::Unsupported, _, _) => {
            Some("host lacks a capability the scenario requires".to_string())
        }
        (GuiRunStatus::Failed, Some(0), None) => Some("PNG artifact was not generated".to_string()),
        (GuiRunStatus::Failed, Some(code), _) => {
            Some(format!("GUI command exited with status {code}"))
        }
        (GuiRunStatus::Failed, None, _) => Some("GUI command exited without a status".to_string()),
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod capability_probe_tests {
    use super::*;

    #[test]
    fn a_plist_boolean_follows_its_key() {
        let locked = "<key>IOConsoleLocked</key>\n<true/>\n<key>Other</key>\n<false/>";
        assert_eq!(plist_bool_after(locked, "IOConsoleLocked"), Some(true));
        let unlocked = "<key>IOConsoleLocked</key>\n<false/>\n<key>Other</key>\n<true/>";
        assert_eq!(plist_bool_after(unlocked, "IOConsoleLocked"), Some(false));
        assert_eq!(
            plist_bool_after("<key>Other</key><true/>", "IOConsoleLocked"),
            None
        );
        assert_eq!(
            plist_bool_after("<key>IOConsoleLocked</key>", "IOConsoleLocked"),
            None
        );
    }

    #[test]
    fn a_harness_owned_linux_compositor_is_always_presentable() {
        assert!(screen_presentable(GuiBackend::LinuxX11));
        assert!(screen_presentable(GuiBackend::LinuxWayland));
    }
}
