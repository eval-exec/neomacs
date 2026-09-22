use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::portable_dump::{configure_capture, parse_status, status_path, verify_run};
use crate::{EditorCapabilities, EditorKind, EditorProvenance, PortableDumpProvenance};

fn scratch() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("perf-portable-dump-")
        .tempdir_in(root)
        .unwrap()
}

fn write_manifest(directory: &Path, dump: PortableDumpProvenance) -> PathBuf {
    let editor = EditorProvenance {
        path: "/repo/neomacs".into(),
        executable_sha256: "unchanged-executable".into(),
        executable_size_bytes: 42,
        pdump_fingerprint: "same-compatible-fingerprint".into(),
        portable_dump: dump,
        version: "Neomacs test-build".into(),
        kind: EditorKind::Neomacs,
        capabilities: EditorCapabilities {
            native_compilation: false,
            tree_sitter: false,
            dynamic_modules: false,
            video_playback: false,
            webview: false,
            embedded_terminal: false,
        },
    };
    let path = directory.join("input-provenance.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"editor": editor, "scenario_field": true})).unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn dump_status_distinguishes_missing_dump_from_unavailable_api_and_invalid_output() {
    assert_eq!(
        parse_status("not-loaded\n").unwrap(),
        PortableDumpProvenance::NotLoaded
    );
    assert_eq!(
        parse_status("unavailable\n").unwrap(),
        PortableDumpProvenance::Unavailable
    );
    for malformed in [
        "",
        "nil",
        "loaded\n",
        "loaded\nrelative.pdump",
        "not-loaded\nextra",
    ] {
        assert!(parse_status(malformed).is_err(), "{malformed:?}");
    }
}

#[test]
fn loaded_dump_paths_are_preserved_and_identified_by_content() {
    let scratch = scratch();
    let dump = scratch.path().join("dump with spaces and a\nnewline.pdump");
    fs::write(&dump, b"abc").unwrap();
    let status = format!("loaded\n{}", dump.display());
    let before = parse_status(&status).unwrap();
    assert_eq!(
        before,
        PortableDumpProvenance::Loaded {
            path: dump.canonicalize().unwrap().to_string_lossy().into_owned(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            size_bytes: 3,
        }
    );
    fs::write(&dump, b"abd").unwrap();
    assert_ne!(before, parse_status(&status).unwrap());
    fs::remove_file(&dump).unwrap();
    assert!(parse_status(&status).is_err());
}

#[test]
fn measured_process_must_report_the_preflight_dump_and_unchanged_bytes() {
    let scratch = scratch();
    let dump = scratch.path().join("neomacs.pdump");
    fs::write(&dump, b"before").unwrap();
    let status = format!("loaded\n{}", dump.display());
    let provenance = write_manifest(scratch.path(), parse_status(&status).unwrap());
    assert!(
        verify_run(&provenance)
            .unwrap_err()
            .contains("did not report")
    );
    fs::write(status_path(&provenance), &status).unwrap();
    verify_run(&provenance).unwrap();

    fs::write(status_path(&provenance), "not-loaded\n").unwrap();
    assert!(
        verify_run(&provenance)
            .unwrap_err()
            .contains("changed between")
    );
    fs::write(status_path(&provenance), &status).unwrap();
    fs::write(&dump, b"after!").unwrap();
    assert!(
        verify_run(&provenance)
            .unwrap_err()
            .contains("changed between")
    );
}

#[test]
fn capture_expression_is_an_editor_argument_and_path_is_environment_data() {
    let scratch = scratch();
    let provenance = scratch
        .path()
        .join("a \"quoted\" path/input-provenance.json");
    let mut command = Command::new("editor");
    command.args(["--batch", "-Q"]);
    configure_capture(&mut command, &provenance);
    command.args(["--load", "fixture.el"]);
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert_eq!(&args[..3], ["--batch", "-Q", "--eval"]);
    assert!(args[3].contains("pdumper-stats"));
    assert!(!args[3].contains("quoted"));
    assert_eq!(&args[4..], ["--load", "fixture.el"]);
    assert!(command.get_envs().any(|(name, value)| {
        name == "NEOMACS_PERF_DUMP_STATUS" && value == Some(status_path(&provenance).as_os_str())
    }));
}
