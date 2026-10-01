#![cfg(windows)]

use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

fn subsystem(path: &std::path::Path) -> u16 {
    let pe = fs::read(path).unwrap();
    let header = u32::from_le_bytes(pe[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&pe[header..header + 4], b"PE\0\0");
    u16::from_le_bytes(pe[header + 24 + 68..header + 24 + 70].try_into().unwrap())
}

#[test]
fn launcher_is_gui_and_editor_remains_console() {
    assert_eq!(subsystem(env!("CARGO_BIN_EXE_runneomacs").as_ref()), 2);
    assert_eq!(subsystem(env!("CARGO_BIN_EXE_neomacs").as_ref()), 3);
}

// Reuse this test executable as a console-subsystem child. This observes the
// launcher's public process boundary without linking editor initialization.
#[test]
#[ignore = "invoked only by launcher_preserves_arguments_and_hides_child_console"]
fn launcher_child_probe() {
    use windows_sys::Win32::{
        System::Console::GetConsoleWindow, UI::WindowsAndMessaging::IsWindowVisible,
    };
    let output = std::env::var_os("NEOMACS_LAUNCHER_PROBE").unwrap();
    // SAFETY: GetConsoleWindow returns a borrowed window handle accepted by
    // IsWindowVisible (including NULL when no console is associated).
    let visible = unsafe { IsWindowVisible(GetConsoleWindow()) != 0 };
    let args: Vec<Vec<u16>> = std::env::args_os()
        .skip(1)
        .map(|a| a.encode_wide().collect())
        .collect();
    let cwd: Vec<u16> = std::env::current_dir()
        .unwrap()
        .as_os_str()
        .encode_wide()
        .collect();
    fs::write(output, serde_json::to_vec(&(visible, args, cwd)).unwrap()).unwrap();
}

#[test]
fn launcher_preserves_arguments_and_hides_child_console() {
    let root = neomacs_infra::crate_root!().join("../../tmp");
    fs::create_dir_all(&root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("launcher 中文 ")
        .tempdir_in(root)
        .unwrap();
    let launcher = directory.path().join("runneomacs.exe");
    fs::copy(env!("CARGO_BIN_EXE_runneomacs"), &launcher).unwrap();
    fs::copy(
        std::env::current_exe().unwrap(),
        directory.path().join("neomacs.exe"),
    )
    .unwrap();
    let output = directory.path().join("result.json");
    let arguments = [
        "--exact",
        "launcher_child_probe",
        "--ignored",
        "--nocapture",
        "--skip",
        "中文 with spaces",
        "--skip",
        "quotes\"and\\slashes\\",
        "",
    ];
    let status = Command::new(launcher)
        .args(arguments)
        .current_dir(directory.path())
        .env("NEOMACS_LAUNCHER_PROBE", &output)
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Ok(bytes) = fs::read(&output) {
            if let Ok(result) = serde_json::from_slice::<(bool, Vec<Vec<u16>>, Vec<u16>)>(&bytes) {
                break result;
            }
        }
        assert!(
            Instant::now() < deadline,
            "launcher child did not report its state"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(!result.0, "launcher exposed its child's console window");
    assert_eq!(
        result.1,
        arguments
            .iter()
            .map(|a| a.encode_utf16().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&result.2))
            .canonicalize()
            .unwrap(),
        directory.path().canonicalize().unwrap()
    );
}

#[test]
fn missing_editor_shows_an_error_dialog_and_returns_failure() {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, SendMessageW, WM_CLOSE},
    };
    struct Search {
        pid: u32,
        window: HWND,
    }
    unsafe extern "system" fn find_launcher_window(window: HWND, data: LPARAM) -> i32 {
        // SAFETY: EnumWindows invokes this callback synchronously with our
        // live Search pointer and a valid borrowed top-level window handle.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == search.pid {
            search.window = window;
            0
        } else {
            1
        }
    }
    let root = neomacs_infra::crate_root!().join("../../tmp");
    fs::create_dir_all(&root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("missing-editor-")
        .tempdir_in(root)
        .unwrap();
    let launcher = directory.path().join("runneomacs.exe");
    fs::copy(env!("CARGO_BIN_EXE_runneomacs"), &launcher).unwrap();
    let mut child = Command::new(launcher).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let mut search = Search {
            pid: child.id(),
            window: std::ptr::null_mut(),
        };
        // SAFETY: Search stays live during enumeration. The callback selects
        // only a window owned by this launcher, regardless of its title.
        let closed = unsafe {
            EnumWindows(
                Some(find_launcher_window),
                &mut search as *mut Search as LPARAM,
            );
            if !search.window.is_null() {
                SendMessageW(search.window, WM_CLOSE, 0, 0);
                true
            } else {
                false
            }
        };
        if closed {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("missing editor did not produce a launcher error dialog");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!child.wait().unwrap().success());
}

#[test]
fn console_editor_preserves_batch_output_and_exit_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_neomacs"))
        .args([
            "--batch",
            "-Q",
            "--eval",
            "(progn (princ \"launcher-cli-ok\") (kill-emacs 23))",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("launcher-cli-ok"));
}
