//! Portable-dump identity comes from the editor, not filename conventions or
//! `--fingerprint`: a compatible executable can silently start without a dump.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use neomacs_melpa_test_support::{MelpaSandbox, output_with_timeout};
use serde::Deserialize;

use crate::harness::{configure_benchmark_environment, sha256_file};
use crate::{EditorProvenance, PortableDumpProvenance};

// A tag line followed by the verbatim filename avoids loading JSON Lisp and
// preserves spaces, backslashes and newlines in filenames. No trailing newline
// is appended to a loaded filename, and the reader must not trim it.
const STATUS_EXPRESSION: &str = r#"(if (not (fboundp 'pdumper-stats))
    "unavailable\n"
  (let ((stats (pdumper-stats)))
    (if (not (cdr (assq 'dumped-with-pdumper stats)))
        "not-loaded\n"
      (let ((file (cdr (assq 'dump-file-name stats))))
        (unless (and (stringp file) (file-name-absolute-p file))
          (error "pdumper-stats returned no absolute dump filename"))
        (concat "loaded\n" file)))))"#;

pub(crate) fn probe(
    editor: &Path,
    sandbox: &MelpaSandbox,
) -> Result<PortableDumpProvenance, String> {
    let mut command = Command::new(editor);
    configure_benchmark_environment(&mut command, sandbox);
    command.args(["--batch", "-Q", "--eval"]);
    command.arg(format!("(princ {STATUS_EXPRESSION})"));
    let output = output_with_timeout(&mut command, Duration::from_secs(30))
        .map_err(|error| format!("failed to query editor portable dump: {error:?}"))?;
    if !output.status.success() {
        return Err(format!(
            "editor portable dump query exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let status = String::from_utf8(output.stdout)
        .map_err(|error| format!("editor portable dump output was not UTF-8: {error}"))?;
    parse_status(&status)
}

pub(crate) fn status_path(provenance: &Path) -> PathBuf {
    provenance.with_file_name("portable-dump-status.txt")
}

/// Run before scenario startup/fixtures on every frontend. This confirms the
/// measured process agrees with the batch preflight; it is outside edit-loop
/// gates, but its small I/O cost is part of whole-process timing/profiling.
pub(crate) fn configure_capture(command: &mut Command, provenance: &Path) {
    command.arg("--eval").arg(format!(
        r#"(let ((status {STATUS_EXPRESSION}))
  (with-temp-buffer
    (insert status)
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region (point-min) (point-max)
                    (getenv "NEOMACS_PERF_DUMP_STATUS") nil 'silent))))"#
    ));
    command.env("NEOMACS_PERF_DUMP_STATUS", status_path(provenance));
}

pub(crate) fn parse_status(status: &str) -> Result<PortableDumpProvenance, String> {
    match status {
        "unavailable\n" => return Ok(PortableDumpProvenance::Unavailable),
        "not-loaded\n" => return Ok(PortableDumpProvenance::NotLoaded),
        _ => {}
    }
    let name = status
        .strip_prefix("loaded\n")
        .filter(|name| !name.is_empty() && Path::new(name).is_absolute())
        .ok_or_else(|| format!("invalid portable dump status {status:?}"))?;
    let path = fs::canonicalize(name)
        .map_err(|error| format!("failed to resolve loaded portable dump {name:?}: {error}"))?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("failed to inspect loaded portable dump {name:?}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("loaded portable dump is not a file: {name:?}"));
    }
    Ok(PortableDumpProvenance::Loaded {
        path: path.to_string_lossy().into_owned(),
        sha256: sha256_file(&path)?,
        size_bytes: metadata.len(),
    })
}

pub(crate) fn verify_run(provenance: &Path) -> Result<(), String> {
    // Scenario manifests own additional fields; all share the typed editor
    // record. Read that record rather than duplicating every manifest schema.
    #[derive(Deserialize)]
    struct Manifest {
        editor: EditorProvenance,
    }
    let raw = fs::read(provenance)
        .map_err(|error| format!("failed to read editor provenance: {error}"))?;
    let expected: Manifest = serde_json::from_slice(&raw)
        .map_err(|error| format!("invalid editor provenance: {error}"))?;
    let status = fs::read_to_string(status_path(provenance))
        .map_err(|error| format!("measured editor did not report portable dump state: {error}"))?;
    let actual = parse_status(&status)?;
    if actual != expected.editor.portable_dump {
        return Err(format!(
            "portable dump changed between preflight and workload: expected {:?}, observed {actual:?}",
            expected.editor.portable_dump
        ));
    }
    Ok(())
}
