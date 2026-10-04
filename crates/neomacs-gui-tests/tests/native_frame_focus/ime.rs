//! Issue #458: real fcitx5 commits must stay text, including keysym collisions.
use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};

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
    fs::write(
        config.join("fcitx5/profile"),
        include_str!("../../fixtures/issue-458-fcitx-profile"),
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
    use std::os::unix::fs::PermissionsExt;
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
    let log = fs::File::create(artifacts.join("fcitx.log")).unwrap();
    let _fcitx = ChildGuard(
        Command::new("fcitx5")
            .args([
                "-D",
                "--disable=wayland,notificationitem,notifications,kimpanel",
            ])
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("fcitx5"),
    );
    let deadline = Instant::now() + Duration::from_secs(8);
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
            Instant::now() < deadline,
            "fcitx5 did not start; see {}",
            artifacts.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let is_neomacs = binary.file_name().is_some_and(|name| name == "neomacs");
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
            let active = Command::new("fcitx5-remote")
                .arg("-n")
                .envs(env.iter().map(|(k, v)| (k, v)))
                .output()
                .map_err(|e| e.to_string())?;
            if String::from_utf8_lossy(&active.stdout).trim() != "pinyin" {
                return Err(format!("Pinyin not active: {active:?}"));
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
