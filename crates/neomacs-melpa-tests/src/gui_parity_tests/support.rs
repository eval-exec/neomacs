use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;

use neomacs_gui_tests::{DisplayHarness, DisplaySession, GuiBackend};

use crate::{
    DirectEditorChild, EmacsRuntime, EvalOutcome, MelpaSandbox, PreparedPackageSet,
    direct_probe_process_error, direct_probe_script, read_direct_probe_file,
    read_direct_probe_outcome, wrap_direct_probe_logs,
};

const GUI_TIMEOUT: Duration = Duration::from_secs(180);
const GUI_LOG_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuiPairOutcome {
    pub gnu_emacs: EvalOutcome,
    pub neomacs: EvalOutcome,
    pub gnu_behavior: EvalOutcome,
    pub neomacs_behavior: EvalOutcome,
}

struct GuiEditorOutcome {
    behavior: EvalOutcome,
    with_logs: EvalOutcome,
}

/// Own one real display and sequential GNU/Neomacs graphical evaluations.
///
/// Callers supply only package setup plus the behavioral probe. This adapter
/// owns every startup/probe/result/log file, validates the existing direct
/// outcome envelope to exact EOF, and reaps each editor process tree before
/// reusing the display for the other editor.
pub(super) struct PackageGuiPair;

struct LinuxCpuAffinity {
    cpus: Vec<usize>,
}

impl LinuxCpuAffinity {
    fn current_first(limit: usize) -> Result<Self, String> {
        let mut allowed = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
        let status = unsafe {
            libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &raw mut allowed)
        };
        if status != 0 {
            return Err(format!(
                "failed to inspect GUI child CPU affinity: {}",
                std::io::Error::last_os_error()
            ));
        }
        let cpus = (0..libc::CPU_SETSIZE as usize)
            .filter(|cpu| unsafe { libc::CPU_ISSET(*cpu, &allowed) })
            .take(limit)
            .collect::<Vec<_>>();
        if cpus.is_empty() {
            return Err("GUI child CPU affinity contains no allowed CPUs".into());
        }
        Ok(Self { cpus })
    }

    fn apply(&self, command: &mut neomacs_test_env::PinnedCommand) {
        command.limit_to_cpus(&self.cpus);
    }
}

impl PackageGuiPair {
    pub(super) fn run(
        label: &str,
        packages: &PreparedPackageSet,
        probe_elisp: &str,
    ) -> Result<GuiPairOutcome, String> {
        let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
            .start_session(neomacs_infra::melpa_cache_root().join("gui-display"))
            .map_err(|error| format!("failed to start owned Xvfb display: {error}"))?;
        let affinity = LinuxCpuAffinity::current_first(4)?;

        // Sequential use avoids focus, selection, and redisplay contention on
        // the shared real display while still holding one display owner.
        let gnu_emacs = run_editor(
            &display,
            &EmacsRuntime::gnu_emacs().with_timeout(GUI_TIMEOUT),
            label,
            packages,
            probe_elisp,
            &affinity,
        );
        let neomacs = run_editor(
            &display,
            &EmacsRuntime::neomacs().with_timeout(GUI_TIMEOUT),
            label,
            packages,
            probe_elisp,
            &affinity,
        );
        match (gnu_emacs, neomacs) {
            (Ok(gnu_emacs), Ok(neomacs)) => Ok(GuiPairOutcome {
                gnu_emacs: gnu_emacs.with_logs,
                neomacs: neomacs.with_logs,
                gnu_behavior: gnu_emacs.behavior,
                neomacs_behavior: neomacs.behavior,
            }),
            (gnu_emacs, neomacs) => Err(paired_gui_failure(label, gnu_emacs, neomacs)),
        }
    }
}

fn paired_gui_failure(
    label: &str,
    gnu_emacs: Result<GuiEditorOutcome, String>,
    neomacs: Result<GuiEditorOutcome, String>,
) -> String {
    fn phase(result: Result<GuiEditorOutcome, String>) -> String {
        match result {
            Ok(outcome) => format!("OK {}", outcome.with_logs),
            Err(error) => format!("ERROR {error}"),
        }
    }

    format!(
        "paired GUI probe `{label}` failed:\nGNU Emacs: {}\nNeomacs: {}",
        phase(gnu_emacs),
        phase(neomacs),
    )
}

fn run_editor(
    display: &DisplaySession,
    runtime: &EmacsRuntime,
    label: &str,
    packages: &PreparedPackageSet,
    probe_elisp: &str,
    affinity: &LinuxCpuAffinity,
) -> Result<GuiEditorOutcome, String> {
    let sandbox = MelpaSandbox::new(&format!("{label}-gui-{}", runtime.name))?;
    let startup_path = packages.write_startup_file(sandbox.root())?;
    let probe_path = sandbox.root().join("gui-probe.el");
    let outcome_path = sandbox.root().join("gui-outcome.el");
    let outcome_tmp_path = sandbox.root().join("gui-outcome.el.partial");
    let stdout_path = sandbox.root().join("gui-editor.stdout");
    let stderr_path = sandbox.root().join("gui-editor.stderr");
    let mut script = direct_probe_script(label, "", probe_elisp, &outcome_path, &outcome_tmp_path);
    script.push_str("\n(kill-emacs 0)\n");
    fs::write(&probe_path, script).map_err(|error| {
        format!(
            "failed to write {} GUI probe {}: {error}",
            runtime.name,
            probe_path.display()
        )
    })?;

    let stdout = File::create(&stdout_path).map_err(|error| {
        format!(
            "failed to create {} GUI stdout {}: {error}",
            runtime.name,
            stdout_path.display()
        )
    })?;
    let stderr = File::create(&stderr_path).map_err(|error| {
        format!(
            "failed to create {} GUI stderr {}: {error}",
            runtime.name,
            stderr_path.display()
        )
    })?;

    let mut command = sandbox.command(&runtime);
    affinity.apply(&mut command);
    command
        .envs(packages.process_environment())
        .env("RUST_LOG", "off")
        .env("WINIT_UNIX_BACKEND", "x11")
        .args(["--quick", "--geometry", "100x35"]);
    for (name, value) in display.env() {
        command.env(name, value);
    }
    let display_name = display
        .env()
        .iter()
        .find_map(|(name, value)| (name == "DISPLAY").then_some(value))
        .ok_or_else(|| "owned Xvfb session did not publish DISPLAY".to_string())?;
    command
        .arg("--display")
        .arg(display_name)
        .arg("--load")
        .arg(&startup_path)
        .arg("--load")
        .arg(&probe_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let mut child = DirectEditorChild::spawn(&mut command).map_err(|error| {
        format!(
            "failed to launch {} GUI probe `{label}`: {error}",
            runtime.name
        )
    })?;
    let status = child.wait_for_exit(runtime.timeout).map_err(|error| {
        direct_probe_process_error(runtime, label, error, &stdout_path, &stderr_path)
    })?;
    if !status.success() {
        return Err(direct_probe_process_error(
            runtime,
            label,
            format!("GUI editor exited with status {status}"),
            &stdout_path,
            &stderr_path,
        ));
    }

    // Read both logs even on success so oversized/non-UTF-8 output is a
    // protocol failure rather than an unbounded diagnostic artifact.
    let stdout = read_direct_probe_file(&stdout_path, GUI_LOG_LIMIT, "GUI stdout")?;
    let stderr = normalize_gui_stderr(read_direct_probe_file(
        &stderr_path,
        GUI_LOG_LIMIT,
        "GUI stderr",
    )?)?;
    let outcome =
        read_direct_probe_outcome(label, &outcome_path, &outcome_tmp_path).map_err(|error| {
            format!(
                "{} GUI probe `{label}` emitted an invalid exact-EOF outcome: {error}",
                runtime.name
            )
        })?;
    Ok(GuiEditorOutcome {
        behavior: outcome.clone(),
        with_logs: wrap_direct_probe_logs(outcome, stdout, stderr, &sandbox),
    })
}

/// A stderr line that describes the host's graphics or desktop stack rather
/// than editor behaviour. Whether one appears depends on the runner (a
/// compiled GSettings schema source, an X server with DRI3), so the same
/// probe prints it on one machine and not on another; it is classified out
/// of the recorded stderr. Every other line stays visible byte for byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostDiagnostic {
    /// GNU's GLib `g_settings_schema_source_lookup` assertion on a host with
    /// no schema source. GLib prefixes it with the process id and a
    /// wall-clock timestamp and writes a blank line before it.
    GioSchemaSourceMissing,
    /// Mesa's EGL/Vulkan notices on an X server without DRI3 (Xvfb).
    MesaWithoutDri3,
}

enum GuiStderrLine<'a> {
    Host(HostDiagnostic),
    Editor(&'a str),
}

const MESA_WITHOUT_DRI3_LINES: [&str; 4] = [
    "libEGL warning: DRI3 error: Could not get DRI3 device",
    "libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering",
    "MESA: info: vulkan: No DRI3 support detected - required for presentation",
    "Note: you can probably enable DRI3 in your Xorg config",
];

fn classify_gui_stderr_line(line: &str) -> Result<GuiStderrLine<'_>, String> {
    const MARKER: &str = "): GLib-GIO-CRITICAL **: ";
    const CONDITION: &str = "g_settings_schema_source_lookup: assertion 'source != NULL' failed";

    if MESA_WITHOUT_DRI3_LINES.contains(&line) {
        return Ok(GuiStderrLine::Host(HostDiagnostic::MesaWithoutDri3));
    }
    if !(line.contains("GLib-GIO-CRITICAL") || line.contains(CONDITION)) {
        return Ok(GuiStderrLine::Editor(line));
    }
    // A line that mentions the GIO condition but is not exactly GLib's form
    // fails closed: only the known host warning may be classified out.
    let malformed = || format!("malformed volatile GNU GIO warning: {line:?}");
    let after_program = line.strip_prefix("(emacs:").ok_or_else(malformed)?;
    let (pid, after_marker) = after_program.split_once(MARKER).ok_or_else(malformed)?;
    let timestamp = after_marker
        .strip_suffix(&format!(": {CONDITION}"))
        .ok_or_else(malformed)?;
    if pid.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
        || !valid_gio_timestamp(timestamp)
    {
        return Err(malformed());
    }
    Ok(GuiStderrLine::Host(HostDiagnostic::GioSchemaSourceMissing))
}

fn normalize_gui_stderr(stderr: String) -> Result<String, String> {
    let mut normalized = String::with_capacity(stderr.len());
    // GLib writes the blank line that precedes its critical message; hold a
    // blank line until the next line shows whose it is.
    let mut held_blank_line = false;
    for segment in stderr.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        if line.is_empty() && !newline.is_empty() {
            if held_blank_line {
                normalized.push('\n');
            }
            held_blank_line = true;
            continue;
        }
        match classify_gui_stderr_line(line)? {
            GuiStderrLine::Host(HostDiagnostic::GioSchemaSourceMissing) => {
                held_blank_line = false;
            }
            GuiStderrLine::Host(HostDiagnostic::MesaWithoutDri3) => {}
            GuiStderrLine::Editor(line) => {
                if std::mem::take(&mut held_blank_line) {
                    normalized.push('\n');
                }
                normalized.push_str(line);
                normalized.push_str(newline);
            }
        }
    }
    if held_blank_line {
        normalized.push('\n');
    }
    Ok(normalized)
}

fn valid_gio_timestamp(timestamp: &str) -> bool {
    let bytes = timestamp.as_bytes();
    bytes.len() == 12
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[8] == b'.'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5 | 8) || byte.is_ascii_digit())
}

#[test]
fn gui_stderr_normalization_preserves_unrelated_diagnostics() {
    let stderr = concat!(
        "before\n\n",
        "(emacs:12345): GLib-GIO-CRITICAL **: 04:23:40.175: ",
        "g_settings_schema_source_lookup: assertion 'source != NULL' failed\n",
        "after\n\n",
        "libEGL warning: DRI3 error: Could not get DRI3 device\n",
        "libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering\n",
        "MESA: info: vulkan: No DRI3 support detected - required for presentation\n",
        "Note: you can probably enable DRI3 in your Xorg config\n",
        "last\n",
    );
    assert_eq!(
        normalize_gui_stderr(stderr.into()).expect("classify exact host diagnostics"),
        "before\nafter\n\nlast\n",
        "host diagnostics and GLib's own blank line are classified out; \
         the editor's blank line before the Mesa notices stays"
    );
    assert_eq!(
        normalize_gui_stderr(
            concat!(
                "\n(emacs:7): GLib-GIO-CRITICAL **: 23:59:59.999: ",
                "g_settings_schema_source_lookup: assertion 'source != NULL' failed\n",
            )
            .into()
        )
        .expect("classify a lone GIO warning"),
        "",
        "a host with no schema source and a host with one record the same stderr"
    );
    assert_eq!(
        normalize_gui_stderr("a\n\n\nb\n".into()).expect("plain blank lines"),
        "a\n\n\nb\n"
    );
    assert!(
        normalize_gui_stderr(
            "GLib-GIO-CRITICAL: g_settings_schema_source_lookup: assertion 'source != NULL' failed\n"
                .into()
        )
        .is_err(),
        "a GIO warning that is not exactly GLib's form must fail closed"
    );
}
