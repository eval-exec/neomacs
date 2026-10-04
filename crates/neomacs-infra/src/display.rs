//! Deterministic, isolated display sessions for GUI test suites.
//!
//! Extracted verbatim from neomacs-gui-tests' harness so every suite (GUI
//! today, perf and others next) mounts the same environment: an Xvfb over
//! loopback TCP with no `/tmp` lock, or a weston-headless compositor with a
//! private runtime directory, each owned by the caller's artifact root and
//! torn down deterministically.  See `DisplaySession` for the ownership
//! contract.

use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Output resolution and compositor scale are one test environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandOutput {
    Standard,
    HiDpi4k,
    HiDpi8k,
}

/// Normal tests avoid background rasterization. The patterned profile exists
/// to exercise genuine compositor startup latency in presentation regressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WestonDesktop {
    Solid,
    DefaultPattern,
}

#[derive(Debug)]
pub struct DisplaySession {
    child: Option<Child>,
    env: Vec<(String, String)>,
    cleanup_dir: Option<PathBuf>,
    runtime_directory: Option<RuntimeDirectory>,
    /// Keeps a `/proc/<pid>/fd/<n>` runtime address valid for the session's
    /// lifetime: the address resolves through this open descriptor, so the
    /// File must outlive every consumer of the env.  Sway sessions use this
    /// directly; weston sessions hold theirs inside [`RuntimeDirectory`].
    _held_directory: Option<fs::File>,
}

impl DisplaySession {
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// The degenerate session: no owned process and no environment, for
    /// scenarios that run on the machine's current desktop (macOS, Windows)
    /// rather than a harness-owned display.
    pub fn current_desktop() -> Self {
        Self {
            child: None,
            env: Vec::new(),
            cleanup_dir: None,
            runtime_directory: None,
            _held_directory: None,
        }
    }
}

impl Drop for DisplaySession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = self.cleanup_dir.take() {
            let _ = fs::remove_dir_all(path);
        }
        self.runtime_directory.take();
    }
}

/// An owned compositor runtime, independent of the user's desktop and TMPDIR.
#[derive(Debug)]
struct RuntimeDirectory {
    path: PathBuf,
    address: PathBuf,
    _directory: fs::File,
}

impl RuntimeDirectory {
    fn new(artifact_root: &Path) -> io::Result<Self> {
        fs::create_dir_all(artifact_root)?;
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(io::Error::other)?;
        let path = artifact_root.join(format!(
            "wayland-runtime-{:016x}",
            u64::from_ne_bytes(nonce)
        ));
        fs::create_dir(&path)?;
        let opened = (|| {
            set_owner_only_dir_permissions(&path)?;
            let directory = fs::File::open(&path)?;
            cfg_select! {
                target_os = "linux" => {
                    use std::os::fd::AsRawFd;
                    // /proc follows the held directory, so long checkout paths
                    // cannot exceed sockaddr_un's limit. No global env mutation.
                    let address = PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), directory.as_raw_fd()));
                }
                _ => { let address = path.clone(); }
            }
            Ok(Self {
                path: path.clone(),
                address,
                _directory: directory,
            })
        })();
        if opened.is_err() {
            let _ = fs::remove_dir(&path);
        }
        opened
    }
}

impl Drop for RuntimeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn start_weston_headless(
    artifact_root: &Path,
    output: WaylandOutput,
) -> io::Result<DisplaySession> {
    start_weston_with_desktop(artifact_root, output, WestonDesktop::Solid)
}

pub fn start_weston_with_desktop(
    artifact_root: &Path,
    output: WaylandOutput,
    desktop: WestonDesktop,
) -> io::Result<DisplaySession> {
    let (width, height, scale) = match output {
        WaylandOutput::Standard => (1280, 800, 1),
        // Weston takes logical dimensions: this produces a 3840x2160 output.
        WaylandOutput::HiDpi4k => (1920, 1080, 2),
        WaylandOutput::HiDpi8k => (3840, 2160, 2),
    };
    let runtime_directory = RuntimeDirectory::new(artifact_root)?;
    let runtime_dir = &runtime_directory.address;
    fs::create_dir_all(artifact_root)?;
    let log_path = artifact_root.join("weston-headless.log");

    let socket = format!("neomacs-infra-weston-{}", std::process::id());
    let mut command = Command::new("weston");
    match desktop {
        WestonDesktop::Solid => {
            command
                .arg("--config")
                .arg(crate::crate_root!().join("fixtures/weston-headless.ini"));
        }
        WestonDesktop::DefaultPattern => {
            command.arg("--no-config");
        }
    }
    let mut child = command
        .arg("--backend=headless")
        .arg("--renderer=pixman")
        .arg(format!("--socket={socket}"))
        .arg("--idle-time=0")
        .arg(format!("--width={width}"))
        .arg(format!("--height={height}"))
        .arg(format!("--scale={scale}"))
        // Deliberately no `--fake-seat`.  It only exists in newer weston, and
        // the ubuntu-24.04 runner ships weston 13.0.0, where it is a fatal
        // `unhandled option: --fake-seat` that kills the compositor before it
        // creates its socket -- so the harness's 5s wait lapsed and every
        // Wayland scenario failed at startup rather than composing.
        //
        // Nothing here needs the seat either: the weston backend is used only
        // for composition and geometry scenarios, and the tests that actually
        // synthesise input (`native_scrolling`) run on sway instead.  Measured
        // with weston 15: passing and omitting the flag give identical results
        // across every weston scenario, so the option bought nothing and cost
        // the whole Wayland half of this suite on the runner.  If a scenario
        // ever does need a seat, probe `weston --help` for the option rather
        // than assuming it, or it will break the runner again.
        .arg(format!("--log={}", log_path.display()))
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let socket_path = runtime_dir.join(&socket);
    if wait_for_path(&socket_path, Duration::from_secs(5)) {
        Ok(DisplaySession {
            child: Some(child),
            env: vec![
                ("XDG_RUNTIME_DIR".to_string(), path_to_string(runtime_dir)),
                ("WAYLAND_DISPLAY".to_string(), socket),
                locale_pin(),
            ],
            cleanup_dir: None,
            runtime_directory: Some(runtime_directory),
            _held_directory: None,
        })
    } else {
        let _ = child.kill();
        let _ = child.wait();
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "weston did not create Wayland socket {}; log: {}",
                socket_path.display(),
                read_log_tail(&log_path)
            ),
        ))
    }
}

/// Log file name a bench session writes below its artifact root. One file per
/// `WestonBenchSession::start` call: a launch retry overwrites it, so the
/// retained log is the attempt that produced the outcome.
pub const WESTON_BENCH_LOG_FILE: &str = "weston-bench.log";

/// Geometry and protocol policy for one [`WestonBenchSession`].
///
/// Reproducibility contract: every field is pinned by the caller (a scenario
/// spec), never inherited from the machine. A bench session therefore renders
/// identically on a developer laptop and on the perf runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WestonBenchConfig {
    pub width: u32,
    pub height: u32,
    /// Also expose the compositor's Xwayland display. Neomacs connects over
    /// Wayland directly; X11-only GNU Emacs builds need this to run beside it
    /// in cross-editor comparisons.
    pub xwayland: bool,
    pub desktop: WestonDesktop,
}

/// One benchmark display session: a headless weston compositor whose whole
/// environment is owned by the caller's artifact root, plus the connection
/// environment clients must receive.
///
/// Ownership: dropping the session kills the compositor and removes its
/// private `XDG_RUNTIME_DIR`. The launch-flake retry policy (fresh compositor
/// per attempt) belongs to the caller: drop and start again.
#[derive(Debug)]
pub struct WestonBenchSession {
    session: DisplaySession,
    wayland_display: String,
    x_display: Option<String>,
    artifact_root: PathBuf,
}

impl WestonBenchSession {
    /// Start one compositor and wait until it can accept clients: the
    /// Wayland socket exists, and when [`WestonBenchConfig::xwayland`] is
    /// set, weston has reported its X server on a display.
    pub fn start(artifact_root: &Path, config: WestonBenchConfig) -> io::Result<Self> {
        if config.width == 0 || config.height == 0 {
            return Err(io::Error::other(
                "bench display dimensions must be positive",
            ));
        }
        fs::create_dir_all(artifact_root)?;
        let runtime_directory = RuntimeDirectory::new(artifact_root)?;
        let runtime_dir = runtime_directory.address.clone();
        let log_path = artifact_root.join(WESTON_BENCH_LOG_FILE);
        let log = fs::File::create(&log_path)?;

        let socket = format!("neomacs-infra-bench-{}", std::process::id());
        let mut command = Command::new("weston");
        match config.desktop {
            WestonDesktop::Solid => {
                command.arg("--config").arg(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/weston-headless.ini"),
                );
            }
            WestonDesktop::DefaultPattern => {
                command.arg("--no-config");
            }
        }
        let child = command
            .arg("--backend=headless")
            // GL exposes the GPU-backed Wayland surface capabilities clients
            // need to select a hardware adapter. Pixman made Neomacs choose
            // llvmpipe even on hosts with a working discrete Vulkan GPU.
            .arg("--renderer=gl")
            // Xwayland is opt-in because only X11 clients need it; when absent
            // the session has no X server to wait for or tear down.
            .args(config.xwayland.then_some("--xwayland"))
            .arg("--width")
            .arg(config.width.to_string())
            .arg("--height")
            .arg(config.height.to_string())
            .arg("--idle-time=0")
            .arg(format!("--socket={socket}"))
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .stdout(Stdio::null())
            // The same log carries weston's own diagnostics and the Xwayland
            // display announcement the wait below parses.
            .stderr(Stdio::from(log))
            .spawn()?;

        let mut pending = PendingBenchSession {
            child: Some(child),
            runtime_directory: Some(runtime_directory),
        };
        let socket_path = runtime_dir.join(&socket);
        if !wait_for_path(&socket_path, Duration::from_secs(5)) {
            pending.kill();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "weston did not create Wayland socket {}; log: {}",
                    socket_path.display(),
                    read_log_tail(&log_path)
                ),
            ));
        }
        let x_display = if config.xwayland {
            match wait_for_xwayland_display(&log_path, Duration::from_secs(5)) {
                Some(display) => Some(display),
                None => {
                    pending.kill();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "weston did not report an Xwayland display in {}; log: {}",
                            log_path.display(),
                            read_log_tail(&log_path)
                        ),
                    ));
                }
            }
        } else {
            None
        };
        let mut session = pending.into_session();
        session
            .env
            .push(("XDG_RUNTIME_DIR".to_string(), path_to_string(&runtime_dir)));
        session.env.push(("WAYLAND_DISPLAY".to_string(), socket));
        if let Some(display) = &x_display {
            session.env.push(("DISPLAY".to_string(), display.clone()));
        }
        session.env.push(locale_pin());
        let wayland_display = session
            .env
            .iter()
            .find(|(name, _)| name == "WAYLAND_DISPLAY")
            .map(|(_, value)| value.clone())
            .expect("WAYLAND_DISPLAY was just pushed");
        Ok(Self {
            session,
            wayland_display,
            x_display,
            artifact_root: artifact_root.to_path_buf(),
        })
    }

    pub fn env(&self) -> &[(String, String)] {
        &self.session.env
    }

    pub fn wayland_display(&self) -> &str {
        &self.wayland_display
    }

    /// The `:N` display of the session's Xwayland server, when requested.
    pub fn x_display(&self) -> Option<&str> {
        self.x_display.as_deref()
    }

    /// The caller-owned directory holding this session's log.
    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }
}

/// Own every partially-started bench resource until the socket (and optional
/// Xwayland display) hand ownership to a live [`WestonBenchSession`].
#[derive(Debug)]
struct PendingBenchSession {
    child: Option<Child>,
    runtime_directory: Option<RuntimeDirectory>,
}

impl PendingBenchSession {
    fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn into_session(mut self) -> DisplaySession {
        DisplaySession {
            child: self.child.take(),
            env: Vec::new(),
            cleanup_dir: None,
            runtime_directory: self.runtime_directory.take(),
            _held_directory: None,
        }
    }
}

impl Drop for PendingBenchSession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Poll weston's log for its Xwayland readiness line and return the display.
///
/// Weston prints `xserver listening on display :N` once its embedded X
/// server accepts connections, which is the compositor's own readiness
/// statement — polling it avoids guessing a display number or sleeping.
fn wait_for_xwayland_display(log_path: &Path, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(display) = xwayland_display_from_log(log_path) {
            return Some(display);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn xwayland_display_from_log(log_path: &Path) -> Option<String> {
    let contents = fs::read_to_string(log_path).ok()?;
    const MARKER: &str = "xserver listening on display ";
    let line = contents.lines().rfind(|line| line.contains(MARKER))?;
    let start = line.find(MARKER)? + MARKER.len();
    let display: String = line[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == ':')
        .collect();
    (display.starts_with(':') && display.len() > 1).then_some(display)
}

pub fn start_xvfb(artifact_root: &Path) -> io::Result<DisplaySession> {
    start_xvfb_with_size(artifact_root, 1280, 800)
}

/// An isolated framebuffer large enough for every pixel of a scaled compositor.
pub fn start_xvfb_with_size(
    artifact_root: &Path,
    width: u32,
    height: u32,
) -> io::Result<DisplaySession> {
    if width == 0 || height == 0 {
        return Err(io::Error::other("Xvfb dimensions must be positive"));
    }
    // X's conventional Unix socket and lock live below system /tmp. Package
    // and GUI tests deliberately never use that filesystem. Run Xvfb over
    // loopback TCP without a lock instead, and keep its cwd/logs in one exact
    // owned directory below the caller-provided workspace-local root.
    fs::create_dir_all(artifact_root)?;
    let base = 90 + (std::process::id() % 1000);
    let mut last_err = None;
    for offset in 0..8u32 {
        let display_number = base + offset * 1000;
        match start_xvfb_on(artifact_root, display_number, width, height) {
            Ok(session) => return Ok(session),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other("no Xvfb display candidate worked")))
}

fn start_xvfb_on(
    artifact_root: &Path,
    display_number: u32,
    width: u32,
    height: u32,
) -> io::Result<DisplaySession> {
    let port_number = u16::try_from(6000 + display_number)
        .map_err(|_| io::Error::other(format!("X display {display_number} has no TCP port")))?;
    let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port_number);
    let display = format!("127.0.0.1:{display_number}");
    let session_dir = artifact_root.join(format!("xvfb-{}-{display_number}", std::process::id()));
    fs::create_dir(&session_dir)?;
    let mut pending = PendingXvfbSession::new(session_dir.clone());
    set_owner_only_dir_permissions(&session_dir)?;
    let stdout_path = session_dir.join("xvfb.stdout");
    let stderr_path = session_dir.join("xvfb.stderr");
    let authority_path = session_dir.join("Xauthority");
    let mut cookie = [0_u8; 16];
    getrandom::fill(&mut cookie).map_err(|error| {
        io::Error::other(format!("failed to create Xauthority cookie: {error}"))
    })?;
    let cookie = cookie
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let xauth = Command::new("xauth")
        .arg("-f")
        .arg(&authority_path)
        .arg("add")
        .arg(&display)
        .arg("MIT-MAGIC-COOKIE-1")
        .arg(cookie)
        .output()?;
    if !xauth.status.success() {
        let diagnostic = String::from_utf8_lossy(&xauth.stderr);
        return Err(io::Error::other(format!(
            "xauth failed for owned display {display}: {diagnostic}"
        )));
    }
    let stdout = fs::File::create(&stdout_path)?;
    let stderr = fs::File::create(&stderr_path)?;
    let child = Command::new("Xvfb")
        .arg(format!(":{display_number}"))
        .arg("-screen")
        .arg("0")
        .arg(format!("{width}x{height}x24"))
        .arg("-nolisten")
        .arg("unix")
        .arg("-listen")
        .arg("tcp")
        .arg("-nolock")
        // DisplaySession owns the server lifetime. Short-lived probe clients
        // must not reset it while the editor establishes its first connection.
        .arg("-noreset")
        .arg("-auth")
        .arg(&authority_path)
        .current_dir(&session_dir)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    pending.child = Some(child);

    if wait_for_authenticated_x11_display(
        pending
            .child
            .as_mut()
            .expect("pending Xvfb owns its spawned child"),
        &display,
        &authority_path,
        Duration::from_secs(5),
    )? {
        return Ok(pending.into_session(vec![
            ("DISPLAY".to_string(), display),
            ("XAUTHORITY".to_string(), path_to_string(&authority_path)),
            // Winit 0.31 chooses Wayland before X11 and no longer reads
            // WINIT_UNIX_BACKEND. An inherited desktop socket must never
            // route private Xvfb tests onto the user's display.
            ("WAYLAND_DISPLAY".to_string(), String::new()),
            ("WAYLAND_SOCKET".to_string(), String::new()),
            ("GDK_BACKEND".to_string(), "x11".to_string()),
            locale_pin(),
        ]));
    }
    let diagnostics = read_log_tail(&stderr_path);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "Xvfb did not come up on loopback display {display} ({endpoint}); stderr: {diagnostics}"
        ),
    ))
}

/// Own every partially-created Xvfb resource until startup transfers them to
/// a live `DisplaySession`. `std::process::Child` does not reap on drop, so the
/// explicit guard is required on every fallible setup/readiness edge.
struct PendingXvfbSession {
    child: Option<Child>,
    cleanup_dir: Option<PathBuf>,
}

impl PendingXvfbSession {
    fn new(cleanup_dir: PathBuf) -> Self {
        Self {
            child: None,
            cleanup_dir: Some(cleanup_dir),
        }
    }

    fn into_session(mut self, env: Vec<(String, String)>) -> DisplaySession {
        DisplaySession {
            child: self.child.take(),
            env,
            cleanup_dir: self.cleanup_dir.take(),
            runtime_directory: None,
            _held_directory: None,
        }
    }
}

impl Drop for PendingXvfbSession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(cleanup_dir) = self.cleanup_dir.take() {
            let _ = fs::remove_dir_all(cleanup_dir);
        }
    }
}

/// Start a headless sway compositor for scenarios that synthesise input.
///
/// `artifact_root` is the caller-owned directory that holds the session's
/// artifacts; sway's config and log land beside them, and the directory
/// doubles as the XDG runtime (addressed through /proc so long checkout
/// paths cannot exceed sockaddr_un's limit).  `config` is sway
/// configuration text — resolution, seats, and focus policy are scenario
/// policy, not harness mechanics. Software rendering is the portable default;
/// set `NEOMACS_GUI_SWAY_RENDERER=gles2` (and, if needed,
/// `WLR_RENDER_DRM_DEVICE`) for hardware performance measurements.
pub fn start_sway(artifact_root: &Path, config: &str) -> io::Result<DisplaySession> {
    fs::create_dir_all(artifact_root)?;
    set_owner_only_dir_permissions(artifact_root)?;
    let config_path = artifact_root.join("sway.conf");
    fs::write(&config_path, config)?;
    let log_path = artifact_root.join("sway.log");
    let log = fs::File::create(&log_path)?;
    let held_directory = fs::File::open(artifact_root)?;
    #[cfg(target_os = "linux")]
    let runtime = {
        use std::os::fd::AsRawFd;
        format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            held_directory.as_raw_fd()
        )
    };
    #[cfg(not(target_os = "linux"))]
    let runtime = artifact_root.to_string_lossy().into_owned();

    let program = std::env::var_os("NEOMACS_GUI_SWAY").unwrap_or_else(|| "sway".into());
    let renderer =
        std::env::var("NEOMACS_GUI_SWAY_RENDERER").unwrap_or_else(|_| "pixman".to_owned());
    fs::write(artifact_root.join("sway-renderer-request"), &renderer)?;
    let mut pending = PendingSwaySession::default();
    let child = Command::new(&program)
        .args(["--unsupported-gpu", "--config"])
        .arg(&config_path)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("WLR_BACKENDS", "headless")
        .env("WLR_RENDERER", &renderer)
        .env("WLR_LIBINPUT_NO_DEVICES", "1")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    pending.child = Some(child);

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if pending
            .child
            .as_mut()
            .expect("pending sway owns its child")
            .try_wait()?
            .is_some()
        {
            return Err(io::Error::other(format!(
                "sway exited early: {}",
                read_log_tail(&log_path)
            )));
        }
        if let Some(socket) = fs::read_dir(artifact_root)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                matches!(name, Some(n) if n.starts_with("wayland-") && !n.ends_with(".lock"))
            })
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        {
            return Ok(pending.into_session(
                vec![
                    ("XDG_RUNTIME_DIR".to_string(), runtime),
                    ("WAYLAND_DISPLAY".to_string(), socket),
                    locale_pin(),
                ],
                held_directory,
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "sway socket absent below {}: {}",
                    artifact_root.display(),
                    read_log_tail(&log_path)
                ),
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Own every partially-started sway resource until its socket appears.
#[derive(Default)]
struct PendingSwaySession {
    child: Option<Child>,
}

impl PendingSwaySession {
    fn into_session(mut self, env: Vec<(String, String)>, held: fs::File) -> DisplaySession {
        DisplaySession {
            child: self.child.take(),
            env,
            cleanup_dir: None,
            runtime_directory: None,
            _held_directory: Some(held),
        }
    }
}

impl Drop for PendingSwaySession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_for_authenticated_x11_display(
    child: &mut Child,
    display: &str,
    authority: &Path,
    timeout: Duration,
) -> io::Result<bool> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(false);
        }
        // A TCP listener can belong to another session, and listening alone
        // does not establish X11 readiness. Our fresh cookie identifies this
        // server. xdpyinfo is also used by the real session contract test.
        let mut probe = Command::new("xdpyinfo")
            .env("DISPLAY", display)
            .env("XAUTHORITY", authority)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let probe_deadline = deadline.min(Instant::now() + Duration::from_millis(250));
        let ready = (|| -> io::Result<bool> {
            while Instant::now() < probe_deadline {
                if let Some(status) = probe.try_wait()? {
                    return Ok(status.success());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(false)
        })();
        // Reap on success, timeout, and I/O error alike; Child has no Drop
        // cleanup. kill is harmless after try_wait has reaped an exited probe.
        let _ = probe.kill();
        let _ = probe.wait();
        if ready? {
            return Ok(child.try_wait()?.is_none());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(false)
}

fn wait_for_path(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    path.exists()
}

fn set_owner_only_dir_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn read_log_tail(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .lines()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | "),
        Err(err) => format!("failed to read {}: {err}", path.display()),
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The locale every display session pins into its environment.  A session
/// started under the C locale leaks coding-default divergence between the
/// editors (the mode line's mule-info renders GNU `=--` vs Neomacs `U==`
/// before the engine fix, and font/coding behavior still follows the
/// ambient locale elsewhere), and a runner that comes up without LANG puts
/// every scenario there.  `LC_ALL` outranks LANG/LANGUAGE in POSIX
/// precedence, so one assignment pins every consumer of the session env.
fn locale_pin() -> (String, String) {
    ("LC_ALL".to_string(), "C.UTF-8".to_string())
}

#[cfg(test)]
mod bench_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn weston_available() -> bool {
        Command::new("weston")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[test]
    fn bench_session_starts_and_env_carries_the_socket() {
        if !weston_available() {
            return;
        }
        let root = tempfile::tempdir().expect("temp artifact root");
        let session = WestonBenchSession::start(
            root.path(),
            WestonBenchConfig {
                width: 800,
                height: 600,
                xwayland: true,
                desktop: WestonDesktop::Solid,
            },
        )
        .expect("bench session starts");
        assert!(
            session
                .wayland_display()
                .starts_with("neomacs-infra-bench-")
        );
        let display = session.x_display().expect("xwayland display reported");
        assert!(display.starts_with(':'));
        let env: BTreeMap<&str, &str> = session
            .env()
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        assert_eq!(env.get("WAYLAND_DISPLAY"), Some(&session.wayland_display()));
        assert_eq!(env.get("DISPLAY"), Some(&display));
        assert!(env.get("XDG_RUNTIME_DIR").is_some_and(|p| !p.is_empty()));
        assert_eq!(env.get("LC_ALL"), Some(&"C.UTF-8"));
        assert!(root.path().join(WESTON_BENCH_LOG_FILE).is_file());
        drop(session);
    }
}
