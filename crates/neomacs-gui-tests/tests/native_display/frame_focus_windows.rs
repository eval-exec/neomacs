//! #395: observed Windows focus must select the frame receiving real keys.

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    ffi::c_void,
    fs,
    path::{Path, PathBuf},
    ptr::NonNull,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL,
            VK_MENU, VK_SHIFT,
        },
        WindowsAndMessaging::{
            EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowThreadProcessId,
            IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
        },
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditorWindow(NonNull<c_void>);

#[derive(Clone, Copy, Debug)]
enum FrameRole {
    Primary,
    Secondary,
}

impl FrameRole {
    fn title(self) -> &'static str {
        match self {
            Self::Primary => "NEOMACS-FOCUS-PRIMARY",
            Self::Secondary => "NEOMACS-FOCUS-SECONDARY",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct FocusTarget {
    window: EditorWindow,
    role: FrameRole,
}

#[test]
fn native_windows_focus_routes_typing_to_the_focused_frame() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/native-display-frame-focus-{}",
        std::process::id()
    ));
    fs::create_dir_all(artifacts.parent().unwrap()).unwrap();
    fs::create_dir(&artifacts).expect("fresh Windows focus artifacts");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs.exe"));
    let plan = GuiTestPlan::new(
        GuiBackend::Windows,
        &root,
        &artifacts,
        GuiScenario::new(
            "native-display-frame-focus",
            root.join("crates/neomacs-gui-tests/fixtures/native-frame-focus.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_GUI_FOCUS_CONTROL", artifacts.to_string_lossy())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    // The existing runner owns the child and kills/waits on timeout, including
    // when the input observation fails before the Lisp fixture can finish.
    let (run, observation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(60)),
            )
        });
        let observation = exercise(&artifacts, &|| run.is_finished());
        fs::write(artifacts.join("stop"), "stop").expect("stop Windows focus fixture");
        (run.join().expect("GUI runner thread"), observation)
    });
    let run = run.expect("Windows focus GUI artifacts");
    assert!(
        observation.is_ok(),
        "{observation:?}; artifacts: {artifacts:?}"
    );
    assert!(!run.timed_out, "{run:#?}");
    assert_eq!(run.exit_code, Some(0), "{run:#?}");
    assert!(
        run.png_bytes.unwrap_or_default() > 0,
        "missing GUI readback"
    );
    assert!(run.frame_snapshot_json_bytes.unwrap_or_default() > 0);
}

fn exercise(artifacts: &Path, finished: &dyn Fn() -> bool) -> Result<(), String> {
    let observation = Observation {
        artifacts,
        deadline: Instant::now() + Duration::from_secs(45),
        finished,
    };
    let state = observation.state("fixture readiness", 0)?;
    let pid = state["pid"].as_u64().ok_or("fixture PID missing")? as u32;
    let primary = observation.window(pid, FrameRole::Primary, None)?;
    activate(primary, &observation)?;
    let _release = ReleaseModifiers;
    type_text(primary, "p", &observation)?;
    observation.buffers("initial", primary, "p", "")?;

    send(&[
        key(VK_CONTROL, false),
        key(b'X'.into(), false),
        key(b'X'.into(), true),
        key(VK_CONTROL, true),
    ])?;
    tap(b'5'.into())?;
    tap(b'2'.into())?;
    let secondary = observation.window(pid, FrameRole::Secondary, Some(primary.window))?;
    // Unlike headless X11, Windows must activate the newly created window.
    // Observe that native result before typing; neither select nor focus it
    // from Lisp, and do not repair a failed automatic activation here.
    type_text(secondary, "n", &observation)?;
    observation.buffers("new-frame", secondary, "p", "n")?;
    activate(primary, &observation)?;
    type_text(primary, "aa", &observation)?;
    observation.buffers("focus-primary", primary, "paa", "n")?;
    activate(secondary, &observation)?;
    type_text(secondary, "bb", &observation)?;
    observation.buffers("focus-secondary", secondary, "paa", "nbb")
}

struct Observation<'a> {
    artifacts: &'a Path,
    deadline: Instant,
    finished: &'a dyn Fn() -> bool,
}

impl Observation<'_> {
    fn poll<T>(
        &self,
        stage: &str,
        mut read: impl FnMut() -> Result<Option<T>, String>,
    ) -> Result<T, String> {
        let deadline = self.deadline.min(Instant::now() + Duration::from_secs(12));
        loop {
            if let Some(value) = read()? {
                return Ok(value);
            }
            if (self.finished)() || Instant::now() >= deadline {
                return Err(format!(
                    "{stage}: observation timed out; foreground={:?}; ensure an interactive Windows desktop",
                    foreground()
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn state(&self, stage: &str, text_bytes: usize) -> Result<Value, String> {
        self.poll(stage, || {
            let state = fs::read(self.artifacts.join("state.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            Ok(state.filter(|state| {
                state["primary"].as_str().map(str::len).unwrap_or(0)
                    + state["secondary"].as_str().map(str::len).unwrap_or(0)
                    >= text_bytes
            }))
        })
    }

    fn window(
        &self,
        pid: u32,
        role: FrameRole,
        exclude: Option<EditorWindow>,
    ) -> Result<FocusTarget, String> {
        self.poll(&format!("{role:?} native window for PID {pid}"), || {
            Ok(windows_for_pid(pid)?
                .into_iter()
                .find(|window| Some(*window) != exclude)
                .map(|window| FocusTarget { window, role }))
        })
    }

    fn buffers(
        &self,
        stage: &str,
        target: FocusTarget,
        primary: &str,
        secondary: &str,
    ) -> Result<(), String> {
        let state = self.state(stage, primary.len() + secondary.len())?;
        fs::write(
            self.artifacts.join(format!("{stage}.json")),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .map_err(|error| error.to_string())?;
        if state["primary"] != primary
            || state["secondary"] != secondary
            || state["selected"] != target.role.title()
        {
            return Err(format!(
                "{stage}: incorrect editing target; expected {target:?}, primary={primary:?}, secondary={secondary:?}; observed {state}"
            ));
        }
        Ok(())
    }
}

#[allow(unsafe_code)]
fn windows_for_pid(pid: u32) -> Result<Vec<EditorWindow>, String> {
    struct Windows {
        pid: u32,
        windows: Vec<EditorWindow>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        // SAFETY: EnumWindows calls synchronously with our live stack object.
        let windows = unsafe { &mut *(data as *mut Windows) };
        let mut owner = 0;
        // SAFETY: HWND came from EnumWindows; owner is writable for this call.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner);
            if owner == windows.pid
                && IsWindowVisible(hwnd) != 0
                && GetWindow(hwnd, GW_OWNER).is_null()
                && let Some(hwnd) = NonNull::new(hwnd)
            {
                windows.windows.push(EditorWindow(hwnd));
            }
        }
        1
    }
    let mut windows = Windows {
        pid,
        windows: Vec::new(),
    };
    // SAFETY: the callback and its pointer remain valid until enumeration ends.
    if unsafe { EnumWindows(Some(collect), &mut windows as *mut Windows as LPARAM) } == 0 {
        return Err(format!("EnumWindows: {}", std::io::Error::last_os_error()));
    }
    Ok(windows.windows)
}

#[allow(unsafe_code)]
fn foreground() -> Option<EditorWindow> {
    // SAFETY: GetForegroundWindow takes no pointers and returns a borrowed HWND.
    NonNull::new(unsafe { GetForegroundWindow() }).map(EditorWindow)
}

#[allow(unsafe_code)]
fn activate(target: FocusTarget, observation: &Observation<'_>) -> Result<(), String> {
    // SAFETY: these are borrowed HWNDs enumerated for this test's own child.
    unsafe {
        ShowWindow(target.window.0.as_ptr(), SW_RESTORE);
        SetForegroundWindow(target.window.0.as_ptr());
    }
    wait_focused(target, observation)
}

fn wait_focused(target: FocusTarget, observation: &Observation<'_>) -> Result<(), String> {
    observation.poll(&format!("native focus on {target:?}"), || {
        Ok((foreground() == Some(target.window)).then_some(()))
    })?;
    eprintln!("Observed native Windows keyboard focus: {target:?}");
    Ok(())
}

fn key(vk: u16, released: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if released { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[allow(unsafe_code)]
fn send(keys: &[INPUT]) -> Result<(), String> {
    // SAFETY: the correctly sized INPUT slice stays valid throughout SendInput.
    let sent = unsafe {
        SendInput(
            keys.len() as u32,
            keys.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != keys.len() {
        return Err(format!(
            "SendInput accepted {sent}/{} events: {}; check desktop session and input integrity level",
            keys.len(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn tap(vk: u16) -> Result<(), String> {
    send(&[key(vk, false), key(vk, true)])?;
    thread::sleep(Duration::from_millis(30));
    Ok(())
}

fn type_text(target: FocusTarget, text: &str, observation: &Observation<'_>) -> Result<(), String> {
    wait_focused(target, observation)?;
    for byte in text.bytes() {
        tap(byte.to_ascii_uppercase().into())?;
    }
    Ok(())
}

struct ReleaseModifiers;
impl Drop for ReleaseModifiers {
    fn drop(&mut self) {
        let _ = send(&[
            key(VK_CONTROL, true),
            key(VK_SHIFT, true),
            key(VK_MENU, true),
        ]);
    }
}
