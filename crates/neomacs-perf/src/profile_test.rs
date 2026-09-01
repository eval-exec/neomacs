use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::{
    Frontend, NativeProfiler, PerfCallGraph, PerfCapture, PerfCaptureConfiguration, PerfHarness,
    PerfSamplingEvent, ProfileArtifact, ProfileGate, ProfileRejection, ProfileRequest,
    ProfileScope, ProfileVerdict, RunArtifact, RunReport, RunVerdict, ScenarioId,
    perf_data_sample_count, profile_verdict,
};

#[test]
fn captured_profile_artifact_links_raw_data_report_and_scenario_run_without_timings() {
    let artifact = ProfileArtifact {
        schema_version: ProfileArtifact::SCHEMA_VERSION,
        profile_id: "rust-lsp-typing-profile-42".to_string(),
        scenario: ScenarioId::RustLspTyping,
        frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        editor: PathBuf::from("target/profiling/neomacs"),
        iterations: NonZeroU32::new(40).expect("non-zero literal"),
        profiler: NativeProfiler::Perf,
        scope: ProfileScope::EditLoop,
        configuration: PerfCaptureConfiguration {
            event: PerfSamplingEvent::UserCpuClock,
            frequency_hz: NonZeroU32::new(999).expect("non-zero literal"),
            call_graph: PerfCallGraph::Dwarf {
                stack_size_bytes: NonZeroU32::new(16_384).expect("non-zero literal"),
            },
        },
        run_artifact_path: PathBuf::from("artifact.json"),
        verdict: ProfileVerdict::Captured {
            perf_data_path: PathBuf::from("perf.data"),
            hotspot_report_path: PathBuf::from("perf-report.txt"),
            sample_count: NonZeroU64::new(8_192).expect("non-zero literal"),
        },
    };

    let json = serde_json::to_string_pretty(&artifact).expect("serialize profile artifact");
    let decoded: ProfileArtifact =
        serde_json::from_str(&json).expect("deserialize profile artifact");

    assert_eq!(decoded, artifact);
    assert_eq!(decoded.schema_version, 2);
    assert!(json.contains(r##""event": "user-cpu-clock""##));
    assert!(json.contains(r##""scope": "edit-loop""##));
    assert!(json.contains(r##""perf_data_path": "perf.data""##));
    assert!(!json.contains("measurements"));
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_forwards_only_acknowledged_enable_disable_sequence() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-sequence-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile gate directory");
    let mut gate =
        ProfileGate::start(scratch.path(), Duration::from_secs(2)).expect("start profile gate");
    let control = gate.control_paths().to_owned();
    let fake_perf = thread::spawn(move || {
        let mut control_reader = fs::OpenOptions::new()
            .read(true)
            .open(&control.command)
            .expect("open perf command FIFO");
        let mut ack_writer = fs::OpenOptions::new()
            .write(true)
            .open(&control.acknowledgement)
            .expect("open perf acknowledgement FIFO");
        let mut forwarded = Vec::new();
        for expected in [b"enable\n".as_slice(), b"disable\n".as_slice()] {
            let mut command = vec![0; expected.len()];
            control_reader
                .read_exact(&mut command)
                .expect("read forwarded perf command");
            assert_eq!(command, expected);
            forwarded.push(command);
            // perf's current wire format includes a trailing NUL after the
            // documented acknowledgement line.
            ack_writer
                .write_all(b"ack\n\0")
                .expect("acknowledge command");
        }
        forwarded
    });

    let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
    let mut response = BufReader::new(client.try_clone().expect("clone profile gate client"));
    for command in ["enable\n", "disable\n"] {
        client
            .write_all(command.as_bytes())
            .expect("send sampling boundary");
        let mut line = String::new();
        response
            .read_line(&mut line)
            .expect("read gate acknowledgement");
        assert_eq!(line, "ack\n");
    }
    drop(response);
    drop(client);

    gate.finish().expect("complete acknowledged profile gate");
    assert_eq!(
        fake_perf.join().expect("join fake perf endpoint"),
        [b"enable\n".to_vec(), b"disable\n".to_vec()]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_rejects_a_command_after_disable() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-after-disable-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile gate directory");
    let mut gate =
        ProfileGate::start(scratch.path(), Duration::from_secs(2)).expect("start profile gate");
    let control = gate.control_paths().to_owned();
    let fake_perf = thread::spawn(move || {
        let mut control_reader = fs::OpenOptions::new()
            .read(true)
            .open(&control.command)
            .expect("open perf command FIFO");
        let mut ack_writer = fs::OpenOptions::new()
            .write(true)
            .open(&control.acknowledgement)
            .expect("open perf acknowledgement FIFO");
        for expected in [b"enable\n".as_slice(), b"disable\n".as_slice()] {
            let mut command = vec![0; expected.len()];
            control_reader
                .read_exact(&mut command)
                .expect("read forwarded perf command");
            assert_eq!(command, expected);
            ack_writer
                .write_all(b"ack\n\0")
                .expect("acknowledge command");
        }
    });

    let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
    let mut response = BufReader::new(client.try_clone().expect("clone profile gate client"));
    client
        .write_all(b"enable\ndisable\nenable\n")
        .expect("send sequence with an extra command");
    for _ in 0..2 {
        let mut line = String::new();
        response
            .read_line(&mut line)
            .expect("read gate acknowledgement");
        assert_eq!(line, "ack\n");
    }
    let mut rejection = String::new();
    response
        .read_line(&mut rejection)
        .expect("read gate rejection");
    assert_eq!(rejection, "error\n");
    drop(response);
    drop(client);

    let error = gate
        .finish()
        .expect_err("post-disable command must fail profiling");
    assert!(error.contains("sampling already finished, received `enable`"));
    fake_perf.join().expect("join fake perf endpoint");
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_rejects_disable_before_enable() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-order-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile gate directory");
    let mut gate =
        ProfileGate::start(scratch.path(), Duration::from_secs(2)).expect("start profile gate");
    let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
    client
        .write_all(b"disable\n")
        .expect("send invalid first boundary");
    let mut response = String::new();
    BufReader::new(client)
        .read_line(&mut response)
        .expect("read rejection response");
    assert_eq!(response, "error\n");

    let error = gate.finish().expect_err("invalid transition must fail");
    assert!(error.contains("expected `enable`, received `disable`"));
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_rejects_malformed_and_incomplete_commands() {
    for (command, expected_error) in [
        (b"start\n".as_slice(), "unknown edit-loop profile command"),
        (
            b"enable".as_slice(),
            "editor disconnected with an incomplete sampling command",
        ),
    ] {
        let scratch = tempfile::Builder::new()
            .prefix("neomacs-perf-gate-malformed-")
            .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
            .expect("create workspace-local profile gate directory");
        let mut gate =
            ProfileGate::start(scratch.path(), Duration::from_secs(2)).expect("start profile gate");
        let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
        client.write_all(command).expect("send invalid command");
        if !command.ends_with(b"\n") {
            client
                .shutdown(std::net::Shutdown::Write)
                .expect("close incomplete command stream");
        }
        let mut response = String::new();
        BufReader::new(client)
            .read_line(&mut response)
            .expect("read rejection response");
        assert_eq!(response, "error\n");

        let error = gate.finish().expect_err("malformed command must fail");
        assert!(error.contains(expected_error), "unexpected error: {error}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_rejects_missing_perf_acknowledgement() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-missing-ack-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile gate directory");
    let mut gate =
        ProfileGate::start(scratch.path(), Duration::from_millis(150)).expect("start profile gate");
    let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
    client
        .write_all(b"enable\n")
        .expect("request sampling without a perf endpoint");
    drop(client);

    let error = gate
        .finish()
        .expect_err("missing perf acknowledgement must fail");
    assert!(
        error.contains("timed out while waiting for perf acknowledgement"),
        "unexpected error: {error}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_rejects_disconnect_before_disable() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-incomplete-sequence-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile gate directory");
    let mut gate =
        ProfileGate::start(scratch.path(), Duration::from_secs(2)).expect("start profile gate");
    let control = gate.control_paths().to_owned();
    let fake_perf = thread::spawn(move || {
        let mut control_reader = fs::OpenOptions::new()
            .read(true)
            .open(&control.command)
            .expect("open perf command FIFO");
        let mut ack_writer = fs::OpenOptions::new()
            .write(true)
            .open(&control.acknowledgement)
            .expect("open perf acknowledgement FIFO");
        let mut command = [0_u8; b"enable\n".len()];
        control_reader
            .read_exact(&mut command)
            .expect("read enable command");
        assert_eq!(&command, b"enable\n");
        ack_writer
            .write_all(b"ack\n\0")
            .expect("acknowledge enable command");
    });
    let mut client = TcpStream::connect(gate.endpoint()).expect("connect editor-side client");
    client.write_all(b"enable\n").expect("enable sampling");
    let mut response = String::new();
    BufReader::new(client.try_clone().expect("clone profile gate client"))
        .read_line(&mut response)
        .expect("read enable acknowledgement");
    assert_eq!(response, "ack\n");
    drop(client);

    let error = gate
        .finish()
        .expect_err("disconnect while enabled must fail");
    assert!(error.contains("disconnected before disabling edit-loop sampling"));
    fake_perf.join().expect("join fake perf endpoint");
}

#[test]
fn malformed_perf_data_is_rejected_by_the_binary_parser() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-malformed-data-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let perf_data = scratch.path().join("perf.data");
    fs::write(&perf_data, b"not perf data").expect("write malformed profile");

    let error = perf_data_sample_count(&perf_data).expect_err("malformed profile must fail");
    assert!(error.contains("failed to parse native profile"));
}

#[test]
fn unavailable_profile_target_persists_a_rejected_diagnostic_artifact() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-profile-rejection-")
        .tempdir_in(workspace_root.join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let request = ProfileRequest::new(
        ScenarioId::RustLspTyping,
        scratch.path().join("missing-neomacs"),
        NonZeroU32::new(3).expect("non-zero literal"),
        NativeProfiler::Perf,
    );

    let report = PerfHarness::new(scratch.path())
        .profile(&request)
        .expect("persist rejected profile");

    assert_eq!(
        report.artifact.verdict,
        ProfileVerdict::Rejected {
            reason: ProfileRejection::InfrastructureFailure {
                message: format!(
                    "missing editor executable {}",
                    scratch.path().join("missing-neomacs").display()
                ),
            },
        }
    );
    assert!(matches!(
        report.run.artifact.verdict,
        RunVerdict::InfrastructureFailure { .. }
    ));
    assert!(report.artifact_path.ends_with("profile.json"));
    assert!(report.run.artifact_path.ends_with("artifact.json"));
    assert!(
        report
            .artifact_path
            .starts_with(scratch.path().join("tmp/perf-profiles"))
    );
}

#[test]
fn every_gate_protocol_failure_maps_to_a_rejected_profile() {
    for message in [
        "unknown edit-loop profile command \"start\\n\"",
        "edit-loop profile gate timed out while waiting for perf acknowledgement",
        "editor disconnected before disabling edit-loop sampling",
    ] {
        let run = RunReport {
            artifact: RunArtifact {
                schema_version: 1,
                run_id: "profile-gate-rejection".to_string(),
                scenario: ScenarioId::RustLspTyping,
                frontend: Frontend::Batch,
                editor: PathBuf::from("target/profiling/neomacs"),
                iterations: 1,
                started_unix_ms: 1,
                total_elapsed_us: 1,
                verdict: RunVerdict::InfrastructureFailure {
                    message: message.to_string(),
                },
                files: Vec::new(),
            },
            artifact_path: PathBuf::from("tmp/perf-profiles/run/artifact.json"),
        };

        assert_eq!(
            profile_verdict(&run),
            ProfileVerdict::Rejected {
                reason: ProfileRejection::InfrastructureFailure {
                    message: message.to_string(),
                },
            }
        );
    }
}

#[test]
fn native_perf_support_is_compile_time_gated_to_linux() {
    assert_eq!(
        NativeProfiler::Perf.platform_rejection().is_none(),
        cfg!(target_os = "linux")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn batch_capture_distinguishes_edit_loop_from_whole_process_scope() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-batch-profile-command-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile command directory");
    let mut edit_loop = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let edit_command = edit_loop
        .wrap(Command::new("neomacs"), Frontend::Batch)
        .expect("wrap edit-loop batch command");
    let edit_arguments = edit_command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        edit_arguments
            .iter()
            .any(|argument| argument == "--delay=-1")
    );
    assert!(
        edit_arguments
            .iter()
            .any(|argument| argument.starts_with("--control=fifo:"))
    );
    assert!(
        edit_command
            .get_envs()
            .any(|(name, value)| name == "NEOMACS_PERF_GATE_PORT" && value.is_some())
    );

    let whole_directory = scratch.path().join("whole");
    fs::create_dir(&whole_directory).expect("create whole-process profile directory");
    let mut whole_process = PerfCapture::new(
        &whole_directory,
        PerfCaptureConfiguration::standard(),
        ProfileScope::WholeProcess,
        Duration::from_secs(2),
    );
    let whole_command = whole_process
        .wrap(Command::new("neomacs"), Frontend::Batch)
        .expect("wrap whole-process batch command");
    let whole_arguments = whole_command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        !whole_arguments
            .iter()
            .any(|argument| argument == "--delay=-1")
    );
    assert!(
        !whole_arguments
            .iter()
            .any(|argument| argument.starts_with("--control="))
    );
    assert!(
        !whole_command
            .get_envs()
            .any(|(name, _)| name == "NEOMACS_PERF_GATE_PORT")
    );
}

#[test]
fn gui_capture_profiles_only_the_app_via_the_frontend_hook() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gui-profile-command-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let mut capture = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let command = capture
        .wrap(
            Command::new("tools/bench/gui-run.sh"),
            Frontend::Gui {
                width: 1200,
                height: 800,
            },
        )
        .expect("wrap GUI command");

    assert_eq!(command.get_program(), "tools/bench/gui-run.sh");
    let environment = command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(environment.get("GUI_PERF_EVENT"), Some(&"cpu-clock:u"));
    assert_eq!(environment.get("GUI_PERF_FREQUENCY"), Some(&"999"));
    assert_eq!(environment.get("GUI_PERF_CALL_GRAPH"), Some(&"dwarf,16384"));
    assert!(
        environment
            .get("GUI_PERF_CONTROL")
            .is_some_and(|control| control.starts_with("fifo:"))
    );
    assert!(environment.contains_key("NEOMACS_PERF_GATE_PORT"));
    assert!(
        PathBuf::from(
            environment
                .get("GUI_PERF_RECORD")
                .expect("GUI capture path")
        )
        .ends_with("perf.data")
    );
}

#[test]
fn tui_capture_profiles_only_the_app_inside_the_private_pty() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-tui-profile-command-")
        .tempdir_in(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let mut capture = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let command = capture
        .wrap(
            Command::new("python3"),
            Frontend::Tui {
                rows: 40,
                columns: 120,
            },
        )
        .expect("wrap TUI command");

    assert_eq!(command.get_program(), "python3");
    let environment = command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(environment.get("PTY_PERF_EVENT"), Some(&"cpu-clock:u"));
    assert_eq!(environment.get("PTY_PERF_FREQUENCY"), Some(&"999"));
    assert_eq!(environment.get("PTY_PERF_CALL_GRAPH"), Some(&"dwarf,16384"));
    assert!(
        environment
            .get("PTY_PERF_CONTROL")
            .is_some_and(|control| control.starts_with("fifo:"))
    );
    assert!(environment.contains_key("NEOMACS_PERF_GATE_PORT"));
    assert!(
        PathBuf::from(
            environment
                .get("PTY_PERF_RECORD")
                .expect("PTY capture path")
        )
        .ends_with("perf.data")
    );
}
