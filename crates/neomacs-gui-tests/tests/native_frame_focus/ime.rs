//! Issue #458: real fcitx5 commits must stay text, including keysym collisions.
use super::*;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Stdio};

fn keycode_191_is_f13(pke: &str) -> bool {
    pke.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next() == Some("keycode")
            && parts.next() == Some("191")
            && parts.next() == Some("=")
            && parts.next() == Some("F13")
    })
}

const IME_STAGES: &[(&str, char)] = &[
    ("ffca", 'ￊ'),
    ("ff0d", '－'),
    ("ff66", 'ｦ'),
    ("4e2d", '中'),
    ("3042", 'あ'),
    ("d55c", '한'),
];

fn ime_stage_index(stage: &str) -> Option<usize> {
    let hex = stage.strip_prefix("ime-")?;
    IME_STAGES.iter().position(|(item, _)| *item == hex)
}

/// Primary text the stage must grow. Punctuation is already committed.
fn primary_before_ime_stage(stage: &str) -> Option<String> {
    let index = ime_stage_index(stage)?;
    let mut text = String::from("，（）；");
    for (_, character) in IME_STAGES.iter().take(index) {
        text.push(*character);
    }
    Some(text)
}

fn release_flag(line: &str) -> Option<u8> {
    if !line.contains("KeyEvent:") {
        return None;
    }
    let rest = line.split_once("Release:")?.1;
    match rest.as_bytes().first() {
        Some(b'0') => Some(0),
        Some(b'1') => Some(1),
        _ => None,
    }
}

fn result_flag(line: &str) -> Option<u8> {
    let rest = line.split_once("result:")?.1;
    match rest.as_bytes().first() {
        Some(b'0') => Some(0),
        Some(b'1') => Some(1),
        _ => None,
    }
}

struct FcitxPress {
    trigger: bool,
    return_key: bool,
    result: u8,
}

fn keycode_of(line: &str) -> Option<u8> {
    let rest = line.split_once("keycode:")?.1.trim_start();
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// `None` when a key press has no handling line or no keycode. Releases are
/// ignored, so they can follow the last press without changing the shape.
fn fcitx_presses(log: &str) -> Option<Vec<FcitxPress>> {
    let lines: Vec<&str> = log.lines().collect();
    let mut presses = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(0) = release_flag(line) else {
            continue;
        };
        let result = lines.get(index + 1).and_then(|next| result_flag(next))?;
        let keycode = keycode_of(line)?;
        presses.push(FcitxPress {
            trigger: keycode == 30 && line.contains("Key(Control+Shift+U"),
            return_key: keycode == 36,
            result,
        });
    }
    Some(presses)
}

/// Shape A: this stage's trigger is the last press and its result is 1.
/// Shape B: this stage's trigger was never logged, and the log ends at or
/// before the previous stage's Return (no uncommitted non-trigger press
/// after that Return).
fn fcitx_logged_no_keys_from_this_stage(log: &str, stage_index: usize) -> bool {
    let Some(presses) = fcitx_presses(log) else {
        return false;
    };
    if presses.is_empty() {
        return false;
    }
    let trigger_count = presses.iter().filter(|press| press.trigger).count();
    if trigger_count == stage_index + 1 {
        let last = &presses[presses.len() - 1];
        return last.trigger && last.result == 1;
    }
    if stage_index == 0 || trigger_count != stage_index {
        return false;
    }
    let Some(return_at) = presses.iter().rposition(|press| press.return_key) else {
        return false;
    };
    !presses[return_at + 1..]
        .iter()
        .any(|press| !press.trigger && !press.return_key)
}

fn trace_tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// `Some` only when fcitx logged no key from this stage after the trigger,
/// xdotool delivered the hex and Return, and the editor buffer did not change.
/// `None` stays a hard failure. The label does not name the XIM layer.
fn external_fcitx5_no_keys_after_trigger_report(
    fcitx_log: &str,
    stage: &str,
    injection_ok: bool,
    buffer_unchanged: bool,
    harness_tail: &str,
) -> Option<String> {
    if !injection_ok || !buffer_unchanged {
        return None;
    }
    let stage_index = ime_stage_index(stage)?;
    if !fcitx_logged_no_keys_from_this_stage(fcitx_log, stage_index) {
        return None;
    }
    Some(format!(
        "EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER\nstage: {stage}\nfcitx key_trace tail:\n{}\nharness tail:\n{harness_tail}",
        trace_tail(fcitx_log, 40)
    ))
}

fn github_actions_warning_line(report: &str) -> String {
    format!("::warning::{}", report.replace(['\n', '\r'], " | "))
}

fn read_log(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

fn classify_gnu_ime_failure(
    is_neomacs: bool,
    artifacts: &Path,
    observation: Result<(), String>,
) -> Result<(), String> {
    let Err(error) = observation else {
        return Ok(());
    };
    if is_neomacs || !error.starts_with("ime-") || error.contains("xdotool") {
        return Err(error);
    }
    let stage = error.split(':').next().unwrap_or("");
    let unchanged = primary_before_ime_stage(stage)
        .is_some_and(|before| editor_buffer_unchanged(artifacts, &before));
    let harness_tail = format!(
        "xdotool delivered the hex and Return\neditor state:\n{}",
        read_log(&artifacts.join("state.json"))
    );
    match external_fcitx5_no_keys_after_trigger_report(
        &read_log(&artifacts.join("fcitx.log")),
        stage,
        true,
        unchanged,
        &harness_tail,
    ) {
        Some(report) => {
            // The buffer is stuck, so later stages in this GNU run would fail
            // for that reason. The hit is the result of the run.
            emit_nonfatal_fcitx_annotation(&report, stage, artifacts);
            Ok(())
        }
        None => Err(error),
    }
}

fn emit_nonfatal_fcitx_annotation(report: &str, stage: &str, artifacts: &Path) {
    eprintln!("{report}");
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(artifacts.join("external-fcitx5-no-keys-after-trigger.log"))
    {
        use std::io::Write;
        let _ = writeln!(file, "EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER {stage}");
    }
    if std::env::var_os("GITHUB_ACTIONS").is_some() || std::env::var_os("CI").is_some() {
        println!("{}", github_actions_warning_line(report));
    }
}

fn editor_buffer_unchanged(artifacts: &Path, primary: &str) -> bool {
    let Ok(bytes) = fs::read(artifacts.join("state.json")) else {
        return false;
    };
    let Ok(state) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    state.get("primary").and_then(Value::as_str) == Some(primary)
        && state.get("secondary").and_then(Value::as_str) == Some("")
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn fcitx5_fullwidth_punctuation_is_text_and_f13_stays_a_key() {
    if std::env::var("NEOMACS_GUI_TEST_BACKEND").ok().as_deref() != Some("x11") {
        eprintln!(
            "set NEOMACS_GUI_TEST_BACKEND=x11; requires fcitx5 with pinyin and unicode, dbus-daemon and xdotool"
        );
        return;
    }
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/issue-458-{}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    wait_for_x11(session.env()).unwrap();
    let config = artifacts.join("config");
    fs::create_dir_all(config.join("fcitx5/conf")).unwrap();
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/issue-458-fcitx-profile"
        ),
        config.join("fcitx5/profile"),
    )
    .unwrap();
    // Pinyin's default semicolon starts quickphrase instead of punctuation.
    fs::write(
        config.join("fcitx5/conf/pinyin.conf"),
        "QuickPhraseKey=\nCloudPinyinEnabled=False\n",
    )
    .unwrap();
    fs::write(
        config.join("fcitx5/conf/unicode.conf"),
        "[DirectUnicodeMode]\n0=Control+Shift+U\n",
    )
    .unwrap();
    let runtime = artifacts.join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let mut env = session.env().to_vec();
    env.extend([
        ("XDG_CONFIG_HOME".into(), config.display().to_string()),
        ("XDG_RUNTIME_DIR".into(), runtime.display().to_string()),
        ("XMODIFIERS".into(), "@im=fcitx".into()),
        ("GTK_IM_MODULE".into(), "fcitx".into()),
    ]);
    let mut bus = ChildGuard(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdout(Stdio::piped())
            .spawn()
            .expect("private D-Bus daemon"),
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    env.push(("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into()));
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let is_neomacs = binary.file_name().is_some_and(|name| name == "neomacs");
    let log = fs::File::create(artifacts.join("fcitx.log")).unwrap();
    // key_trace is GNU-only. The Neo arm's fcitx command stays the same.
    let mut fcitx_args = vec![
        "-D",
        "--disable=wayland,notificationitem,notifications,kimpanel",
    ];
    if !is_neomacs {
        fcitx_args.extend(["--verbose", "key_trace=5"]);
    }
    let _fcitx = ChildGuard(
        Command::new("fcitx5")
            .args(&fcitx_args)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("fcitx5"),
    );
    let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(8));
    loop {
        let ready = Command::new("fcitx5-remote")
            .arg("--check")
            .envs(env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap()
            .status
            .success();
        if ready {
            break;
        }
        assert!(
            !deadline.is_expired(),
            "fcitx5 did not start; see {}",
            artifacts.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
    // fcitx5 startup restores Xvfb's evdev map, which clears an earlier F13
    // binding. Bind after it is ready and before the editor starts. Xvfb has
    // no F13, so `xdotool key F13` would borrow keycode 8, inject, and revert.
    // Both editors lose that race. The f13 assertion is unchanged.
    let bound = Command::new("xmodmap")
        .args(["-e", "keycode 191 = F13"])
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output()
        .expect("xmodmap");
    assert!(
        bound.status.success(),
        "permanent F13 binding failed: {bound:?}"
    );
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new(
            "issue-458",
            root.join("crates/neomacs-gui-tests/fixtures/issue-458-ime.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_GUI_FOCUS_CONTROL", artifacts.display().to_string())
    // Capture later input/redisplay frames, not just the empty startup frame.
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "512");
    for (key, value) in &env {
        plan = plan.with_env(key, value);
    }
    let (run, observation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(45)),
            )
        });
        let observation = (|| {
            let ready = wait_for_state(&artifacts, "ready", 0, &|| run.is_finished())?;
            let window =
                wait_for_window(&env, &ready["pid"].to_string(), None, &|| run.is_finished())?;
            focus_window(&env, &window)?;
            // The editor creates its XIM input context asynchronously after
            // the window exists.  `-s` addresses the CURRENT input context,
            // so switching before it exists is a no-op and the context is
            // born with the keyboard layout.  Wait for the daemon to see a
            // context (state 0 = closed) before switching.
            let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(8));
            loop {
                let state = Command::new("fcitx5-remote")
                    .envs(env.iter().map(|(k, v)| (k, v)))
                    .output()
                    .map_err(|e| e.to_string())?;
                if String::from_utf8_lossy(&state.stdout).trim() != "0" {
                    break;
                }
                if run.is_finished() || deadline.is_expired() {
                    return Err(format!(
                        "fcitx5 never saw the editor's input context: {state:?}"
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
            for args in [["-s", "pinyin"].as_slice(), ["-o"].as_slice()] {
                let output = Command::new("fcitx5-remote")
                    .args(args)
                    .envs(env.iter().map(|(k, v)| (k, v)))
                    .output()
                    .map_err(|e| e.to_string())?;
                if !output.status.success() {
                    return Err(format!("fcitx5-remote {args:?}: {output:?}"));
                }
            }
            // Confirm the active IM instead of treating absent IME as a pass.
            // fcitx5 creates the input context for a freshly focused client
            // and loads the input-method addon asynchronously; `-n` reports
            // the current context's IM and is empty until that handshake
            // settles.  Sample it to a deadline: the assertion is that the
            // editor is talking to the IM, not that the daemon answered
            // within the first milliseconds after the switch.
            let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(8));
            loop {
                let active = Command::new("fcitx5-remote")
                    .arg("-n")
                    .envs(env.iter().map(|(k, v)| (k, v)))
                    .output()
                    .map_err(|e| e.to_string())?;
                if String::from_utf8_lossy(&active.stdout).trim() == "pinyin" {
                    break;
                }
                if run.is_finished() || deadline.is_expired() {
                    return Err(format!("Pinyin not active: {active:?}"));
                }
                thread::sleep(Duration::from_millis(20));
            }
            // ASCII comma is transformed by fcitx5 into an actual U+FF0C commit.
            xdotool(&env, &["key", "comma"])?;
            expect_buffers(&artifacts, "comma", "，", "", &|| run.is_finished())?;
            // '(' and ')' are committed by Pinyin, exercising real named-key collisions.
            xdotool(&env, &["key", "shift+9", "shift+0", "semicolon"])?;
            expect_buffers(&artifacts, "punctuation", "，（）；", "", &|| {
                run.is_finished()
            })?;
            // Commit colliding and non-Latin scalars through fcitx itself.
            // This avoids synthetic Unicode keysyms: text and key identity
            // are the distinction under test, and GNU GTK can classify an
            // injected keysym differently from an input-method commit.
            let mut expected = String::from("，（）；");
            for (hex, character) in [
                ("ffca", 'ￊ'),
                ("ff0d", '－'),
                ("ff66", 'ｦ'),
                ("4e2d", '中'),
                ("3042", 'あ'),
                ("d55c", '한'),
            ] {
                xdotool(&env, &["key", "ctrl+shift+u"])?;
                type_text(&env, hex)?;
                xdotool(&env, &["key", "Return"])?;
                expected.push(character);
                expect_buffers(&artifacts, &format!("ime-{hex}"), &expected, "", &|| {
                    run.is_finished()
                })?;
            }
            let inactive = Command::new("fcitx5-remote")
                .arg("-c")
                .envs(env.iter().map(|(k, v)| (k, v)))
                .output()
                .map_err(|e| e.to_string())?;
            if !inactive.status.success() {
                return Err(format!("fcitx5 deactivate: {inactive:?}"));
            }
            // `-c` closes the input context asynchronously.  A key sent while
            // fcitx still owns the context can be filtered by the IM instead
            // of reaching the editor, which is exactly the distinction this
            // stage measures, so wait for the closed state first.  The
            // session has no window manager, so restore the app's X focus
            // explicitly before typing.
            let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(8));
            loop {
                let state = Command::new("fcitx5-remote")
                    .envs(env.iter().map(|(k, v)| (k, v)))
                    .output()
                    .map_err(|e| e.to_string())?;
                if String::from_utf8_lossy(&state.stdout).trim() != "2" {
                    break;
                }
                if run.is_finished() || deadline.is_expired() {
                    return Err(format!(
                        "fcitx5 did not deactivate the input context: {state:?}"
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
            focus_window(&env, &window)?;
            // fcitx5 can restore the evdev map again when an input context is
            // activated or closed. Read the map here, immediately before the
            // injection, so a missing binding is a harness error and cannot
            // be reported as an f13 product miss.
            let keymap = Command::new("xmodmap")
                .args(["-pke"])
                .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .output()
                .map_err(|error| format!("xmodmap -pke: {error}"))?;
            if !keymap.status.success() {
                return Err(format!(
                    "xmodmap -pke: {}",
                    String::from_utf8_lossy(&keymap.stderr)
                ));
            }
            let pke = String::from_utf8_lossy(&keymap.stdout);
            if !keycode_191_is_f13(&pke) {
                let line = pke
                    .lines()
                    .find(|line| line.split_whitespace().nth(1) == Some("191"))
                    .unwrap_or("(keycode 191 absent)");
                return Err(format!("F13 binding lost before injection: {line}"));
            }
            xdotool(&env, &["key", "F13"])?;
            expect_buffers(
                &artifacts,
                "f13",
                "，（）；ￊ－ｦ中あ한",
                "F13",
                &|| run.is_finished(),
            )?;
            Ok::<_, String>(())
        })();
        fs::write(artifacts.join("stop"), "stop").unwrap();
        (run.join().unwrap(), observation)
    });
    let run = run.unwrap();
    // A signature hit is non-fatal. Every other GNU failure still fails here.
    let observation = classify_gnu_ime_failure(is_neomacs, &artifacts, observation);
    assert!(
        observation.is_ok(),
        "{observation:?}; artifacts: {}",
        artifacts.display()
    );
    assert!(!run.timed_out, "{run:#?}");
    assert_eq!(run.exit_code, Some(0), "{run:#?}");
    if is_neomacs {
        let snapshot =
            fs::read_to_string(&run.artifacts.frame_snapshot_json).expect("GUI snapshot");
        let text =
            fs::read_to_string(&run.artifacts.frame_snapshot_txt).expect("GUI text snapshot");
        assert!(
            text.contains("，（）；ￊ－ｦ中あ한"),
            "committed text must be visible: {text}"
        );
        assert!(
            snapshot.contains('，'),
            "committed text must reach redisplay"
        );
    }
}

#[test]
fn keycode_191_line_is_f13_only_when_the_first_keysym_is_f13() {
    assert!(keycode_191_is_f13(
        "keycode   8 =\nkeycode 191 = F13 NoSymbol F13\n"
    ));
    assert!(!keycode_191_is_f13(
        "keycode 191 = XF86Tools NoSymbol XF86Tools\n"
    ));
    assert!(!keycode_191_is_f13("keycode 19 = F13\n"));
    assert!(!keycode_191_is_f13("(keycode 191 absent)\n"));
}

fn completed_unicode_stage() -> &'static str {
    "\
KeyEvent: Key(Control+Shift+U states=5) Release:0 keycode: 30
KeyEvent handling time: 0ms result:1
KeyEvent: Key(f states=0) Release:0 keycode: 41
KeyEvent handling time: 0ms result:1
KeyEvent: Key(Return states=0) Release:0 keycode: 36
KeyEvent handling time: 0ms result:1
"
}

#[test]
fn external_fcitx5_no_keys_after_trigger_uses_key_trace_injection_and_buffer() {
    // Shape A. Four earlier commits, then the captured 3042 tail: the last
    // press is that stage's trigger at result:1, and only releases follow.
    let shape_a_tail = "\
D2026-11-19 09:47:54.132569 instance.cpp:948] KeyEvent: Key(Control+Shift+U states=5) rawKey: Key(Control+Shift+U states=5) origKey: Key(Control+Shift+U states=5) Release:0 keycode: 30 program: emacs-31.1
D2026-11-19 09:47:54.133393 inputcontext.cpp:342] KeyEvent handling time: 0ms result:1
D2026-11-19 09:47:54.135636 instance.cpp:948] KeyEvent: Key(Control+Shift+Shift_L states=5) rawKey: Key(Control+Shift+Shift_L states=5) origKey: Key(Control+Shift+Shift_L states=5) Release:1 keycode: 50 program: emacs-31.1
D2026-11-19 09:47:54.135723 inputcontext.cpp:342] KeyEvent handling time: 0ms result:0
D2026-11-19 09:47:54.150910 instance.cpp:948] KeyEvent: Key(u states=0) rawKey: Key(u states=0) origKey: Key(u states=0) Release:1 keycode: 30 program: emacs-31.1
D2026-11-19 09:47:54.151058 inputcontext.cpp:342] KeyEvent handling time: 0ms result:0
";
    let shape_a = completed_unicode_stage().repeat(4) + shape_a_tail;
    let harness = "xdotool delivered the hex and Return\nprimary unchanged\nsecondary empty\n";
    let report =
        external_fcitx5_no_keys_after_trigger_report(&shape_a, "ime-3042", true, true, harness)
            .expect("shape A");
    assert!(report.starts_with("EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER"));
    assert!(report.contains("Key(Control+Shift+U"));
    assert!(report.contains("stage: ime-3042"));
    assert!(report.contains("harness tail:\n"));
    assert!(report.contains("primary unchanged"));
    let warning = github_actions_warning_line(&report);
    assert!(warning.starts_with("::warning::EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER"));
    assert!(!warning.contains('\n'));
    assert!(warning.contains("fcitx key_trace tail:"));
    assert!(warning.contains("harness tail:"));

    // Pass: this stage's trigger is followed by its hex press and Return.
    let pass = completed_unicode_stage().repeat(4);
    assert!(
        external_fcitx5_no_keys_after_trigger_report(&pass, "ime-4e2d", true, true, harness)
            .is_none()
    );

    // Shape B. The log ends at the previous stage's Return, so ime-4e2d's
    // trigger was never logged.
    let shape_b = completed_unicode_stage().repeat(3);
    let report_b =
        external_fcitx5_no_keys_after_trigger_report(&shape_b, "ime-4e2d", true, true, harness)
            .expect("shape B");
    assert!(report_b.starts_with("EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER"));
    assert!(report_b.contains("keycode: 36"));
    assert!(report_b.contains("harness tail:\n"));

    // Near-miss: fcitx logged a hex press after this stage's trigger.
    let hex_after_trigger = shape_b.clone()
        + "\
KeyEvent: Key(Control+Shift+U states=5) Release:0 keycode: 30
KeyEvent handling time: 0ms result:1
KeyEvent: Key(4 states=0) Release:0 keycode: 13
KeyEvent handling time: 0ms result:1
";
    assert!(
        external_fcitx5_no_keys_after_trigger_report(
            &hex_after_trigger,
            "ime-4e2d",
            true,
            true,
            harness
        )
        .is_none()
    );

    // Near-miss: the trigger was logged but not consumed.
    let not_consumed = shape_a.replacen(
        "KeyEvent handling time: 0ms result:1\nD2026-11-19 09:47:54.135636",
        "KeyEvent handling time: 0ms result:0\nD2026-11-19 09:47:54.135636",
        1,
    );
    assert!(
        external_fcitx5_no_keys_after_trigger_report(
            &not_consumed,
            "ime-3042",
            true,
            true,
            harness
        )
        .is_none()
    );

    // Near-miss: the buffer grew, including literal hex. xdotool failed.
    assert!(
        external_fcitx5_no_keys_after_trigger_report(&shape_a, "ime-3042", true, false, harness)
            .is_none()
    );
    assert!(
        external_fcitx5_no_keys_after_trigger_report(&shape_a, "ime-3042", false, true, harness)
            .is_none()
    );
    assert!(
        external_fcitx5_no_keys_after_trigger_report("", "ime-3042", true, true, harness).is_none()
    );
}

#[test]
fn external_fcitx5_no_keys_after_trigger_is_nonfatal_only_on_the_signature() {
    let root = neomacs_infra::workspace_root();
    let dir = root.join(format!(
        "target/neomacs-gui-tests/ximdrop-unit-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let before = primary_before_ime_stage("ime-4e2d").unwrap();
    let state = serde_json::json!({ "primary": before, "secondary": "" });
    fs::write(
        dir.join("state.json"),
        serde_json::to_string(&state).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("fcitx.log"), completed_unicode_stage().repeat(3)).unwrap();
    let error = "ime-4e2d: timed out waiting for input; last state {}".to_string();
    assert!(classify_gnu_ime_failure(false, &dir, Err(error.clone())).is_ok());
    let counted =
        fs::read_to_string(dir.join("external-fcitx5-no-keys-after-trigger.log")).unwrap();
    assert_eq!(counted, "EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER ime-4e2d\n");

    let grew = serde_json::json!({ "primary": format!("{before}4e2d"), "secondary": "" });
    fs::write(
        dir.join("state.json"),
        serde_json::to_string(&grew).unwrap(),
    )
    .unwrap();
    let hard = classify_gnu_ime_failure(false, &dir, Err(error.clone())).unwrap_err();
    assert!(hard.starts_with("ime-4e2d:"));
    assert!(!hard.contains("EXTERNAL-FCITX5-NO-KEYS-AFTER-TRIGGER"));

    fs::write(
        dir.join("state.json"),
        serde_json::to_string(&state).unwrap(),
    )
    .unwrap();
    let neo = classify_gnu_ime_failure(true, &dir, Err(error)).unwrap_err();
    assert!(neo.starts_with("ime-4e2d:"));
    let _ = fs::remove_dir_all(&dir);
}
