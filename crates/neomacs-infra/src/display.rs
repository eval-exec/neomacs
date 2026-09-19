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
            command.arg("--config").arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/weston-headless.ini"),
            );
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
            ],
            cleanup_dir: None,
            runtime_directory: Some(runtime_directory),
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

pub fn start_xvfb(artifact_root: &Path) -> io::Result<DisplaySession> {
    // X's conventional Unix socket and lock live below system /tmp. Package
    // and GUI tests deliberately never use that filesystem. Run Xvfb over
    // loopback TCP without a lock instead, and keep its cwd/logs in one exact
    // owned directory below the caller-provided workspace-local root.
    fs::create_dir_all(artifact_root)?;
    let base = 90 + (std::process::id() % 1000);
    let mut last_err = None;
    for offset in 0..8u32 {
        let display_number = base + offset * 1000;
        match start_xvfb_on(artifact_root, display_number) {
            Ok(session) => return Ok(session),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other("no Xvfb display candidate worked")))
}

fn start_xvfb_on(artifact_root: &Path, display_number: u32) -> io::Result<DisplaySession> {
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
        .arg("1280x800x24")
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
