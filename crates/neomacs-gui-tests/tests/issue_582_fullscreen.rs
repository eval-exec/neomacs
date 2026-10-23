#![cfg(target_os = "linux")]

use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use std::{
    fs,
    os::unix::{fs::DirBuilderExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Plasma titlebar-to-top snapping maximizes a window through the WM.  A Lisp
/// fullscreen request alone cannot cover the externally initiated transition.
#[test]
fn native_kwin_maximize_fills_workarea_for_every_reported_font_height() {
    let mut desktop = Desktop::start("native-maximize", false);
    for (index, height) in [100, 110, 120, 130, 140].into_iter().enumerate() {
        desktop.font(height);
        // Record the baseline policy, then check the reported geometry before
        // asserting the fix's native hint policy.  RED must expose the gap.
        desktop.record_geometry("floating-before-maximize");
        desktop.wm_state("add,maximized_vert,maximized_horz");
        desktop.wait_for_maximized_edges();
        desktop.wait_for_pixel_hints();

        // Exercise an actual font change while the WM owns both dimensions.
        desktop.font([110, 120, 130, 140, 100][index]);
        desktop.wait_for_maximized_edges();
        desktop.wait_for_pixel_hints();
        desktop.wm_state("remove,maximized_vert,maximized_horz");
        desktop.wait_for_floating_hints();
        desktop.resize_columns(91);
    }
}

/// The same native boundary is also checked against GNU Emacs.  Its GTK size
/// hints remain in place while maximized, so only native edge geometry is an
/// oracle here, not the implementation's choice of hint representation.
#[test]
fn gnu_kwin_fullscreen_fills_screen_for_every_reported_font_height() {
    let mut desktop = Desktop::start("gnu-fullscreen", true);
    for height in [100, 110, 120, 130, 140] {
        desktop.font(height);
        desktop.wm_state("add,fullscreen");
        desktop.wait_for_fullscreen_edges();
        desktop.wm_state("remove,fullscreen");
        desktop.wait_for_floating_hints();
    }
}

#[test]
fn native_kwin_fullscreen_fills_screen_for_every_reported_font_height() {
    let mut desktop = Desktop::start("native-fullscreen", false);
    for height in [100, 110, 120, 130, 140] {
        desktop.font(height);
        desktop.wm_state("add,fullscreen");
        desktop.wait_for_fullscreen_edges();
        desktop.wait_for_pixel_hints();
        desktop.wm_state("remove,fullscreen");
        desktop.wait_for_floating_hints();
    }
}

struct OwnedProcess(Child);

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // Both the session bus and KWin are descendants of our own process
        // group.  Never signal a WM belonging to the invoking desktop.
        unsafe { libc::kill(-(self.0.id() as i32), libc::SIGKILL) };
        let _ = self.0.wait();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

struct Desktop {
    // Drop the editor and WM before releasing their isolated X server.
    _editor: OwnedProcess,
    _wm: OwnedProcess,
    session: neomacs_gui_tests::DisplaySession,
    artifacts: PathBuf,
    window: String,
    sequence: u32,
    gnu: bool,
}

impl Desktop {
    fn start(name: &str, gnu: bool) -> Self {
        let root = neomacs_infra::workspace_root();
        let artifacts = root
            .join("target/neomacs-gui-tests/issue-582")
            .join(format!("{name}-{}", std::process::id()));
        fs::create_dir_all(&artifacts).unwrap();
        let temporary = root.join("tmp");
        fs::create_dir_all(&temporary).unwrap();
        let runtime = artifacts.join("runtime");
        fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
        let font = Command::new("fc-match")
            .args(["--format", "%{family}\n%{file}\n", "Times New Roman"])
            .output()
            .expect("fontconfig is required to record the actual test font");
        assert!(font.status.success(), "fc-match: {font:?}");
        fs::write(artifacts.join("font-match.txt"), &font.stdout).unwrap();
        eprintln!(
            "Times New Roman resolves to {}",
            String::from_utf8_lossy(&font.stdout)
        );
        let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
            .start_session(&artifacts)
            .expect("isolated Xvfb must start");
        let config = artifacts.join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("kwinrc"),
            "[Compositing]\nEnabled=false\n[Windows]\nPlacement=Centered\n",
        )
        .unwrap();
        let bus_config = config.join("dbus.xml");
        fs::write(
            &bus_config,
            format!(
                "<busconfig><type>session</type>\n\
                 <listen>unix:abstract=neomacs-issue582-{}-{name}</listen>\n\
                 <auth>EXTERNAL</auth>\n\
                 <policy context=\"default\">\n\
                 <allow own=\"*\"/><allow send_destination=\"*\"/>\n\
                 <allow receive_sender=\"*\"/></policy></busconfig>\n",
                std::process::id()
            ),
        )
        .unwrap();
        let wm = Command::new("dbus-run-session")
            // The normal session configuration creates a socket under /tmp.
            // An abstract socket stays inside this owned Linux bus lifetime.
            .arg(format!("--config-file={}", bus_config.display()))
            .args(["--", "kwin_x11", "--replace"])
            .envs(session.env().iter().cloned())
            .env("TMPDIR", &temporary)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("XDG_CONFIG_HOME", &config)
            .env("QT_QPA_PLATFORM", "xcb")
            .env("KWIN_COMPOSE", "N")
            .env("QT_XCB_GL_INTEGRATION", "none")
            .stdout(log(&artifacts, "kwin.stdout"))
            .stderr(log(&artifacts, "kwin.stderr"))
            .process_group(0)
            .spawn()
            .expect("dbus-run-session and kwin_x11 are required");
        let wm = OwnedProcess(wm);
        poll("isolated KWin owns the display", || {
            let output = tool(session.env(), "wmctrl", &["-m"]);
            (output.status.success() && String::from_utf8_lossy(&output.stdout).contains("KWin"))
                .then_some(())
        });
        let binary = if gnu {
            std::env::var_os("NEOMACS_GNU_GUI_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("emacs"))
        } else {
            std::env::var_os("NEOMACS_GUI_TEST_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("target/release/neomacs"))
        };
        let child = Command::new(binary)
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures/issue-582-fullscreen.el"))
            .envs(session.env().iter().cloned())
            .env("TMPDIR", &temporary)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("GDK_BACKEND", "x11")
            .env("NEOMACS_ISSUE_582_REQUEST", artifacts.join("request.el"))
            .env("NEOMACS_ISSUE_582_ACK", artifacts.join("ack.el"))
            .env("RUST_LOG", "warn")
            .stdout(log(&artifacts, "editor.stdout"))
            .stderr(log(&artifacts, "editor.stderr"))
            .process_group(0)
            .spawn()
            .expect("start GUI editor");
        let editor = OwnedProcess(child);
        let pid = editor.0.id().to_string();
        let window = poll("editor's visible X11 window", || {
            let output = tool(
                session.env(),
                "xdotool",
                &["search", "--onlyvisible", "--pid", &pid],
            );
            output.status.success().then(|| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .last()
                    .expect("search returned a window")
                    .to_owned()
            })
        });
        Self {
            _editor: editor,
            _wm: wm,
            session,
            artifacts,
            window,
            sequence: 0,
            gnu,
        }
    }

    fn font(&mut self, height: u32) {
        let metrics = self.request(Request::Font(height));
        eprintln!("font height {height}: {metrics:?}");
    }

    fn request(&mut self, request: Request) -> Metrics {
        self.sequence += 1;
        let temporary = self.artifacts.join("request.next");
        fs::write(&temporary, request.form(self.sequence)).unwrap();
        fs::rename(temporary, self.artifacts.join("request.el")).unwrap();
        let prefix = format!("({} ", self.sequence);
        poll("public editor request acknowledgement", || {
            let contents = fs::read_to_string(self.artifacts.join("ack.el")).ok()?;
            contents
                .starts_with(&prefix)
                .then(|| Metrics::parse(&contents))
        })
    }

    fn resize_columns(&mut self, columns: u32) {
        let before = self.request(Request::Observe);
        let base = before.pixel_width - before.text_width;
        let metrics = self.request(Request::Columns(columns));
        assert_eq!(metrics.columns, columns);
        assert_eq!(metrics.text_width, columns * metrics.char_width);
        assert_eq!(metrics.pixel_width, columns * metrics.char_width + base);
        poll(
            "native character resize matches acknowledged Lisp pixel width",
            || {
                let outer = self.outer_rect();
                let extents = numbers_after_equals(&self.property("_NET_FRAME_EXTENTS"));
                (outer.width - extents[0] - extents[1] == i64::from(metrics.pixel_width))
                    .then_some(())
            },
        );
        self.record_geometry("character-resize-91");
        self.capture("character-resize-91");
    }

    fn wm_state(&self, action: &str) {
        let output = tool(
            self.session.env(),
            "wmctrl",
            &["-i", "-r", &self.window, "-b", action],
        );
        assert!(output.status.success(), "wmctrl: {output:?}");
    }

    fn property(&self, property: &str) -> String {
        self.xprop(&["-id", &self.window, property])
    }

    fn xprop(&self, args: &[&str]) -> String {
        let output = tool(self.session.env(), "xprop", args);
        assert!(output.status.success(), "xprop: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    fn hints(&self) -> String {
        self.property("WM_NORMAL_HINTS")
    }

    fn wait_for_floating_hints(&self) {
        poll("restored floating native geometry policy", || {
            let state = self.property("_NET_WM_STATE");
            let hints = self.hints();
            let increments = resize_increment(&hints);
            let appropriate = if self.gnu {
                increments.is_some_and(|(x, y)| x > 1 || y > 1)
            } else {
                // X11 must allow the WM to choose arbitrary native pixels,
                // including a future externally initiated maximize request.
                increments.is_none_or(|inc| inc == (1, 1))
            };
            (!state.contains("MAXIMIZED") && !state.contains("FULLSCREEN") && appropriate)
                .then_some(())
        });
        self.record_geometry("floating-restored");
    }

    fn wait_for_pixel_hints(&self) {
        poll(
            "WM-controlled dimensions have no character-grid restriction",
            || {
                let hints = self.hints();
                let allowed = resize_increment(&hints).is_none_or(|inc| inc == (1, 1));
                allowed.then_some(())
            },
        );
    }

    fn wait_for_maximized_edges(&self) {
        let workarea = self.xprop(&["-root", "_NET_WORKAREA"]);
        let numbers = numbers_after_equals(&workarea);
        let expected = if numbers.len() >= 4 {
            Rect {
                x: numbers[0],
                y: numbers[1],
                width: numbers[2],
                height: numbers[3],
            }
        } else {
            // KWin can omit _NET_WORKAREA in a standalone session without
            // Plasma's shell.  This owned display contains no panels or docks,
            // so its independently observed root rectangle is the workarea.
            eprintln!("standalone KWin has no _NET_WORKAREA: {workarea}");
            self.root_rect()
        };
        self.wait_for_edges(expected, "MAXIMIZED");
    }

    fn wait_for_fullscreen_edges(&self) {
        self.wait_for_edges(self.root_rect(), "FULLSCREEN");
    }

    fn wait_for_edges(&self, expected: Rect, state: &str) {
        let mut last = None;
        let mut correct_since = None;
        poll(
            &format!("native {state} outer edges equal {expected:?}"),
            || {
                let actual = self.outer_rect();
                if last != Some(actual) {
                    eprintln!("native {state}: {actual:?}, wanted {expected:?}");
                    last = Some(actual);
                }
                let property = self.property("_NET_WM_STATE");
                let correct_state = if state == "MAXIMIZED" {
                    property.contains("MAXIMIZED_HORZ") && property.contains("MAXIMIZED_VERT")
                } else {
                    property.contains(state)
                };
                if correct_state && actual == expected {
                    let start = correct_since.get_or_insert_with(Instant::now);
                    (start.elapsed() >= Duration::from_millis(350)).then_some(())
                } else {
                    correct_since = None;
                    None
                }
            },
        );
        fs::write(
            self.artifacts
                .join(format!("geometry-{}.txt", self.sequence)),
            format!("{state}: {expected:?}\n{}", self.hints()),
        )
        .unwrap();
        self.capture(state);
    }

    fn capture(&self, stage: &str) {
        let path = self
            .artifacts
            .join(format!("{stage}-{}.png", self.sequence));
        let output = tool(
            self.session.env(),
            "import",
            &["-window", "root", path.to_str().unwrap()],
        );
        assert!(output.status.success(), "native root capture: {output:?}");
    }

    fn record_geometry(&self, stage: &str) {
        fs::write(
            self.artifacts
                .join(format!("{stage}-{}.txt", self.sequence)),
            format!("{:?}\n{}", self.outer_rect(), self.hints()),
        )
        .unwrap();
    }

    fn root_rect(&self) -> Rect {
        let output = tool(self.session.env(), "xwininfo", &["-root"]);
        assert!(output.status.success(), "root xwininfo: {output:?}");
        let info = String::from_utf8(output.stdout).unwrap();
        let field = |label: &str| -> i64 {
            info.lines()
                .find_map(|line| line.trim().strip_prefix(label))
                .unwrap_or_else(|| panic!("missing {label} in {info}"))
                .trim()
                .parse()
                .unwrap()
        };
        Rect {
            x: 0,
            y: 0,
            width: field("Width:"),
            height: field("Height:"),
        }
    }

    fn outer_rect(&self) -> Rect {
        let output = tool(self.session.env(), "xwininfo", &["-id", &self.window]);
        assert!(output.status.success(), "xwininfo: {output:?}");
        let info = String::from_utf8(output.stdout).unwrap();
        let field = |label: &str| -> i64 {
            info.lines()
                .find_map(|line| line.trim().strip_prefix(label))
                .unwrap_or_else(|| panic!("missing {label} in {info}"))
                .trim()
                .parse()
                .unwrap()
        };
        let extents = numbers_after_equals(&self.property("_NET_FRAME_EXTENTS"));
        assert_eq!(extents.len(), 4, "KWin must publish frame extents");
        Rect {
            x: field("Absolute upper-left X:") - extents[0],
            y: field("Absolute upper-left Y:") - extents[2],
            width: field("Width:") + extents[0] + extents[1],
            height: field("Height:") + extents[2] + extents[3],
        }
    }
}

enum Request {
    Font(u32),
    Columns(u32),
    Observe,
}

impl Request {
    fn form(self, sequence: u32) -> String {
        let (command, value) = match self {
            Self::Font(value) => ("font", value),
            Self::Columns(value) => ("columns", value),
            Self::Observe => ("observe", 0),
        };
        format!("({sequence} {command} {value})\n")
    }
}

#[derive(Debug)]
struct Metrics {
    char_width: u32,
    columns: u32,
    text_width: u32,
    pixel_width: u32,
}

impl Metrics {
    fn parse(acknowledgement: &str) -> Self {
        let numbers: Vec<u32> = acknowledgement
            .split_whitespace()
            .filter_map(|word| word.trim_matches(['(', ')']).parse().ok())
            .collect();
        assert_eq!(
            numbers.len(),
            7,
            "invalid acknowledgement: {acknowledgement}"
        );
        Self {
            char_width: numbers[2],
            columns: numbers[4],
            text_width: numbers[5],
            pixel_width: numbers[6],
        }
    }
}

fn resize_increment(hints: &str) -> Option<(u32, u32)> {
    let line = hints
        .lines()
        .find(|line| line.contains("resize increment:"))?;
    let values: Vec<_> = line
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect();
    (values.len() == 2).then(|| (values[0], values[1]))
}

fn numbers_after_equals(property: &str) -> Vec<i64> {
    property
        .split_once('=')
        .map(|(_, values)| {
            values
                .split(',')
                .map(|value| value.trim().parse().expect("numeric X11 property"))
                .collect()
        })
        .unwrap_or_default()
}

fn tool(env: &[(String, String)], program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .envs(env.iter().cloned())
        .env("LC_ALL", "C")
        .env("TMPDIR", neomacs_infra::workspace_root().join("tmp"))
        .output()
        .unwrap_or_else(|error| panic!("required {program}: {error}"))
}

fn log(artifacts: &Path, name: &str) -> Stdio {
    Stdio::from(fs::File::create(artifacts.join(name)).unwrap())
}

fn poll<T>(description: &str, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(value) = observe() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out: {description}");
        thread::sleep(Duration::from_millis(25));
    }
}
