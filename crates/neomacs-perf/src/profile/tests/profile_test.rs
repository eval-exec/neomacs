use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::{
    CaptureRoute, Frontend, NativeProfiler, PerfCallGraph, PerfCapture, PerfCaptureConfiguration,
    PerfHarness, PerfSamplingEvent, PerfSamplingRate, ProfileArtifact, ProfileGate,
    ProfileRejection, ProfileReportStyle, ProfileRequest, ProfileScope, ProfileVerdict,
    RunArtifact, RunReport, RunVerdict, ScenarioId, perf_data_sample_count, profile_verdict,
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
        video_file: None,
        iterations: NonZeroU32::new(40).expect("non-zero literal"),
        profiler: NativeProfiler::Perf,
        scope: ProfileScope::EditLoop,
        report_style: ProfileReportStyle::SelfTime,
        configuration: PerfCaptureConfiguration {
            event: PerfSamplingEvent::UserCpuClock,
            sampling: PerfSamplingRate::Frequency {
                frequency_hz: NonZeroU32::new(999).expect("non-zero literal"),
            },
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
    assert_eq!(decoded.schema_version, 5);
    assert!(json.contains(r##""event": "user-cpu-clock""##));
    assert!(json.contains(r##""scope": "edit-loop""##));
    assert!(json.contains(r##""report_style": "self-time""##));
    assert!(json.contains(r##""perf_data_path": "perf.data""##));
    assert!(!json.contains("measurements"));

    // Published schema-3 artifacts predate report selection; they always
    // rendered a call graph and must retain that interpretation.
    let mut legacy = serde_json::to_value(&artifact).expect("serialize legacy fixture");
    legacy["schema_version"] = serde_json::json!(3);
    legacy.as_object_mut().unwrap().remove("report_style");
    legacy["configuration"]
        .as_object_mut()
        .unwrap()
        .remove("sampling");
    legacy["configuration"]["frequency_hz"] = serde_json::json!(999);
    let decoded: ProfileArtifact = serde_json::from_value(legacy).expect("read schema 3");
    assert_eq!(decoded.schema_version, 3);
    assert_eq!(decoded.report_style, ProfileReportStyle::CallGraph);
    let mut schema_four = serde_json::to_value(&artifact).unwrap();
    schema_four["schema_version"] = serde_json::json!(4);
    schema_four["configuration"]
        .as_object_mut()
        .unwrap()
        .remove("sampling");
    schema_four["configuration"]["frequency_hz"] = serde_json::json!(999);
    let decoded: ProfileArtifact = serde_json::from_value(schema_four).expect("read schema 4");
    assert_eq!(decoded.schema_version, 4);
    assert_eq!(decoded.configuration, artifact.configuration);
    assert_eq!(decoded.report_style, ProfileReportStyle::SelfTime);
}

#[cfg(target_os = "linux")]
#[test]
fn edit_loop_gate_forwards_only_acknowledged_enable_disable_sequence() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gate-sequence-")
        .tempdir_in(crate::workspace_root().join("tmp"))
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
        .tempdir_in(crate::workspace_root().join("tmp"))
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
        .tempdir_in(crate::workspace_root().join("tmp"))
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
            .tempdir_in(crate::workspace_root().join("tmp"))
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
        .tempdir_in(crate::workspace_root().join("tmp"))
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
        .tempdir_in(crate::workspace_root().join("tmp"))
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
        .tempdir_in(crate::workspace_root().join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let perf_data = scratch.path().join("perf.data");
    fs::write(&perf_data, b"not perf data").expect("write malformed profile");

    let error = perf_data_sample_count(&perf_data).expect_err("malformed profile must fail");
    assert!(error.contains("failed to parse native profile"));
}

#[test]
fn unavailable_profile_target_persists_a_rejected_diagnostic_artifact() {
    let workspace_root = crate::workspace_root();
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-profile-rejection-")
        .tempdir_in(workspace_root.join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let request = ProfileRequest::new(
        ScenarioId::RustLspTyping,
        scratch.path().join("missing-neomacs"),
        NonZeroU32::new(3).expect("non-zero literal"),
        NativeProfiler::Perf,
    )
    .with_report_style(ProfileReportStyle::SelfTime);

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
    assert_eq!(report.artifact.report_style, ProfileReportStyle::SelfTime);
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
                host: crate::HostProvenance {
                    operating_system: "linux".to_string(),
                    architecture: "x86_64".to_string(),
                    kernel_release: None,
                    cpu_model: None,
                    logical_cpu_count: 1,
                    allowed_cpus: None,
                    selected_cpu: None,
                    scaling_governor: None,
                    perf_event_paranoid: None,
                    continuous_integration: false,
                },
                iterations: 1,
                started_unix_ms: 1,
                total_elapsed_us: 1,
                native_video_execution: None,
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
        .tempdir_in(crate::workspace_root().join("tmp"))
        .expect("create workspace-local profile command directory");
    let mut edit_loop = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let edit_command = edit_loop
        .wrap(Command::new("neomacs"), CaptureRoute::Direct)
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
        .wrap(Command::new("neomacs"), CaptureRoute::Direct)
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
fn gui_capture_profiles_only_the_app_in_the_harness_owned_session() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gui-profile-command-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let mut capture = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    // The GUI frontend launches the editor directly (the display session is
    // harness-owned infrastructure), so the capture is a Direct wrap: perf
    // record profiles the editor process, never the compositor.
    let mut editor = Command::new("target/release/neomacs");
    editor.arg("-Q");
    let command = capture
        .wrap(editor, CaptureRoute::Direct)
        .expect("wrap GUI command");

    assert_eq!(command.get_program(), "perf");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.iter().any(|argument| argument == "record"));
    assert!(arguments.iter().any(|argument| argument == "--"));
    assert!(
        arguments
            .iter()
            .any(|argument| argument.starts_with("--control=fifo:"))
    );
    let environment = command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert!(environment.contains_key("NEOMACS_PERF_GATE_PORT"));
    assert!(
        !environment.keys().any(|name| name.starts_with("GUI_PERF_")),
        "the GUI adapter perf contract retired with tools/bench/gui-run.sh"
    );
    assert!(
        arguments
            .iter()
            .any(|argument| argument.ends_with("perf.data")),
        "perf record writes its data below the run directory"
    );
}

#[test]
fn tui_capture_profiles_only_the_app_inside_the_private_pty() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-tui-profile-command-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .expect("create workspace-local profile scratch directory");
    let mut capture = PerfCapture::new(
        scratch.path(),
        PerfCaptureConfiguration::standard(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let command = capture
        .wrap(Command::new("python3"), CaptureRoute::Adapter("PTY"))
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

fn instruction_lbr_configuration() -> PerfCaptureConfiguration {
    PerfCaptureConfiguration {
        event: PerfSamplingEvent::UserCoreInstructions,
        sampling: PerfSamplingRate::Period {
            sample_period: NonZeroU64::new(4_000_000).unwrap(),
        },
        call_graph: PerfCallGraph::Lbr,
    }
}

#[test]
fn instruction_profile_metadata_preserves_period_and_rejects_ambiguous_rates() {
    let configuration = instruction_lbr_configuration();
    let json = serde_json::to_value(configuration).unwrap();
    assert_eq!(json["event"], "user-core-instructions");
    assert_eq!(json["sampling"]["kind"], "period");
    assert_eq!(json["sampling"]["sample_period"], 4_000_000);
    assert_eq!(json["call_graph"]["kind"], "lbr");
    assert!(json.get("frequency_hz").is_none());
    assert_eq!(
        serde_json::from_value::<PerfCaptureConfiguration>(json.clone()).unwrap(),
        configuration
    );
    let mut ambiguous = json.clone();
    ambiguous["frequency_hz"] = serde_json::json!(999);
    assert!(serde_json::from_value::<PerfCaptureConfiguration>(ambiguous).is_err());
    let mut missing = json.clone();
    missing.as_object_mut().unwrap().remove("sampling");
    assert!(serde_json::from_value::<PerfCaptureConfiguration>(missing).is_err());
    let mut zero_period = json;
    zero_period["sampling"]["sample_period"] = serde_json::json!(0);
    assert!(serde_json::from_value::<PerfCaptureConfiguration>(zero_period).is_err());
    let request = ProfileRequest::new(
        ScenarioId::Scrolling,
        "neomacs",
        NonZeroU32::new(1).unwrap(),
        NativeProfiler::Perf,
    )
    .with_capture_configuration(configuration);
    assert_eq!(request.configuration, configuration);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_instruction_capture_keeps_edit_loop_gate_and_exact_record_options() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-instructions-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .unwrap();
    let mut capture = PerfCapture::new(
        scratch.path(),
        instruction_lbr_configuration(),
        ProfileScope::EditLoop,
        Duration::from_secs(2),
    );
    let command = capture
        .wrap(Command::new("neomacs"), CaptureRoute::Direct)
        .unwrap();
    let arguments: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    for pair in [
        ["--event", "cpu_core/instructions/u"],
        ["--count", "4000000"],
        ["--call-graph", "lbr"],
    ] {
        assert!(arguments.windows(2).any(|a| a == pair));
    }
    assert!(!arguments.iter().any(|a| a == "--freq"));
    assert!(arguments.iter().any(|a| a == "--delay=-1"));
    assert!(arguments.iter().any(|a| a.starts_with("--control=fifo:")));
    assert!(
        command
            .get_envs()
            .any(|(name, value)| name == "NEOMACS_PERF_GATE_PORT" && value.is_some())
    );
}

#[cfg(target_os = "linux")]
#[test]
fn tui_adapter_clears_the_opposite_inherited_sampling_rate() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-instruction-adapter-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .unwrap();
    for (configuration, retained, removed) in [
        (
            instruction_lbr_configuration(),
            "PTY_PERF_PERIOD",
            "PTY_PERF_FREQUENCY",
        ),
        (
            PerfCaptureConfiguration::standard(),
            "PTY_PERF_FREQUENCY",
            "PTY_PERF_PERIOD",
        ),
    ] {
        let mut capture = PerfCapture::new(
            scratch.path(),
            configuration,
            ProfileScope::WholeProcess,
            Duration::from_secs(2),
        );
        let mut adapter = Command::new("python3");
        adapter
            .env("PTY_PERF_PERIOD", "123")
            .env("PTY_PERF_FREQUENCY", "456");
        let command = capture.wrap(adapter, CaptureRoute::Adapter("PTY")).unwrap();
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert!(environment[std::ffi::OsStr::new(retained)].is_some());
        assert_eq!(environment[std::ffi::OsStr::new(removed)], None);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn pty_runner_uses_period_recording_without_a_frequency_argument() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-instruction-argv-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .unwrap();
    let fake_perf = scratch.path().join("perf");
    fs::write(
        &fake_perf,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FAKE_PERF_ARGUMENTS\"\n: > \"$SENTINEL\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake_perf, fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = vec![scratch.path().to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let arguments_path = scratch.path().join("arguments");
    let output = Command::new("python3")
        .arg(crate::workspace_root().join("tools/bench/pty-run.py"))
        .arg("neomacs")
        .env("PATH", std::env::join_paths(path).unwrap())
        .env("SENTINEL", scratch.path().join("sentinel"))
        .env("FAKE_PERF_ARGUMENTS", &arguments_path)
        .env("PTY_TIMEOUT", "5")
        .env("PTY_PERF_RECORD", scratch.path().join("perf.data"))
        .env("PTY_PERF_EVENT", "instructions:u")
        .env("PTY_PERF_PERIOD", "4000000")
        .env("PTY_PERF_FREQUENCY", "999")
        .env("PTY_PERF_CALL_GRAPH", "lbr")
        .env("PTY_PERF_CONTROL", "fifo:command,ack")
        .env_remove("PTY_PERF_STAT")
        .env_remove("PTY_CPU")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arguments = fs::read_to_string(arguments_path).unwrap();
    assert!(arguments.contains("--event\ninstructions:u\n"));
    assert!(arguments.contains("--count\n4000000\n"));
    assert!(arguments.contains("--call-graph\nlbr\n"));
    assert!(arguments.contains("--delay=-1\n--control=fifo:command,ack\n"));
    assert!(!arguments.contains("--freq"));
}

#[cfg(target_os = "linux")]
#[test]
fn failed_profile_can_cancel_an_edit_loop_gate_before_any_editor_connects() {
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-failed-gate-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .unwrap();
    let mut capture = PerfCapture::new(
        scratch.path(),
        instruction_lbr_configuration(),
        ProfileScope::EditLoop,
        Duration::from_secs(300),
    );
    let _command = capture
        .wrap(Command::new("neomacs"), CaptureRoute::Direct)
        .unwrap();
    assert!(scratch.path().join("perf-control.fifo").exists());
    capture.cancel_gate();
    assert!(!scratch.path().join("perf-control.fifo").exists());
    assert!(!scratch.path().join("perf-ack.fifo").exists());
}

#[test]
fn self_time_report_disables_inline_expansion_without_changing_call_graph_reports() {
    let input = std::path::Path::new("perf.data");
    let self_time = ProfileReportStyle::SelfTime.report_arguments(input);
    assert!(self_time.iter().any(|argument| argument == "--no-inline"));
    assert!(
        self_time
            .windows(2)
            .any(|arguments| arguments == ["--call-graph", "none"])
    );
    let call_graph = ProfileReportStyle::CallGraph.report_arguments(input);
    assert!(!call_graph.iter().any(|argument| argument == "--no-inline"));
    assert!(
        call_graph
            .windows(2)
            .any(|arguments| arguments == ["--call-graph", "fractal,0.5"])
    );
}
