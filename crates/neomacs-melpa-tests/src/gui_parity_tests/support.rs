use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;

use neomacs_gui_tests::{DisplayHarness, DisplaySession, GuiBackend};

use crate::{
    DirectEditorChild, EmacsRuntime, EvalOutcome, MelpaSandbox, PreparedPackageSet,
    direct_probe_process_error, direct_probe_script, read_direct_probe_file,
    read_direct_probe_outcome, workspace_root, wrap_direct_probe_logs,
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

    fn apply(&self, command: &mut std::process::Command) {
        let cpus = self.cpus.clone();
        // SAFETY: `pre_exec` runs after fork. The closure performs only
        // stack-local CPU-set operations and the async-signal-safe
        // sched_setaffinity syscall, then returns immediately.
        unsafe {
            command.pre_exec(move || {
                let mut selected = std::mem::zeroed::<libc::cpu_set_t>();
                libc::CPU_ZERO(&mut selected);
                for cpu in &cpus {
                    libc::CPU_SET(*cpu, &mut selected);
                }
                if libc::sched_setaffinity(
                    0,
                    std::mem::size_of::<libc::cpu_set_t>(),
                    &raw const selected,
                ) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
}

impl PackageGuiPair {
    pub(super) fn run(
        label: &str,
        packages: &PreparedPackageSet,
        probe_elisp: &str,
    ) -> Result<GuiPairOutcome, String> {
        let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
            .start_session(workspace_root().join("tmp/melpa/gui-display"))
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

    let mut command = runtime.command();
    affinity.apply(&mut command);
    sandbox.configure(&mut command);
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

fn normalize_gui_stderr(stderr: String) -> Result<String, String> {
    // GNU's GIO warning prefixes the same infrastructure condition with a
    // process id and wall-clock timestamp. Preserve the exact condition while
    // classifying only those two host-only fields. Every unrelated byte and
    // line remains visible, including leading blank lines and diagnostics
    // adjacent to the known warning.
    const MARKER: &str = "): GLib-GIO-CRITICAL **: ";
    const CONDITION: &str = "g_settings_schema_source_lookup: assertion 'source != NULL' failed";
    const CANONICAL: &str =
        "GLib-GIO-CRITICAL: g_settings_schema_source_lookup: assertion 'source != NULL' failed";

    let mut normalized = String::with_capacity(stderr.len());
    for segment in stderr.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        let mentions_known_warning = line.contains("GLib-GIO-CRITICAL") || line.contains(CONDITION);
        if !mentions_known_warning {
            normalized.push_str(line);
            normalized.push_str(newline);
            continue;
        }

        let Some(after_program) = line.strip_prefix("(emacs:") else {
            return Err(format!("malformed volatile GNU GIO warning: {line:?}"));
        };
        let Some((pid, after_marker)) = after_program.split_once(MARKER) else {
            return Err(format!("malformed volatile GNU GIO warning: {line:?}"));
        };
        let Some(timestamp) = after_marker.strip_suffix(&format!(": {CONDITION}")) else {
            return Err(format!("malformed volatile GNU GIO warning: {line:?}"));
        };
        if pid.is_empty()
            || !pid.bytes().all(|byte| byte.is_ascii_digit())
            || !valid_gio_timestamp(timestamp)
        {
            return Err(format!("malformed volatile GNU GIO warning: {line:?}"));
        }
        normalized.push_str(CANONICAL);
        normalized.push_str(newline);
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
        "after\n",
    );
    assert_eq!(
        normalize_gui_stderr(stderr.into()).expect("normalize exact volatile GNU warning"),
        concat!(
            "before\n\n",
            "GLib-GIO-CRITICAL: ",
            "g_settings_schema_source_lookup: assertion 'source != NULL' failed\n",
            "after\n",
        )
    );
    assert!(
        normalize_gui_stderr(
            "GLib-GIO-CRITICAL: g_settings_schema_source_lookup: assertion 'source != NULL' failed\n"
                .into()
        )
        .is_err(),
        "already-normalized or malformed warning input must fail closed"
    );
}
