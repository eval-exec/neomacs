use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use neomacs_test_env::{CanonicalEnvironment, CanonicalProfile, PinnedCommand, bounded_temp_dir};

use neovm_core::emacs_core::format_eval_result_with_eval;
use neovm_core::emacs_core::load::{
    apply_runtime_startup_state, create_bootstrap_evaluator_cached,
};
use neovm_core::emacs_core::pdump;

thread_local! {
    static RUNTIME_TEMPLATE: RefCell<Option<neovm_core::emacs_core::pdump::types::DumpContextState>> =
        const { RefCell::new(None) };
}

/// The canonical environment shared by every comparison in this test process.
///
/// The two `TempDir`s are kept alive for the whole process so the paths the
/// spawned GNU and the in-process Neomacs both see remain valid (and equal).
/// `bounded_temp_dir` keeps them short enough for any unix socket a form binds.
struct CanonicalHarness {
    _home: tempfile::TempDir,
    _tmp: tempfile::TempDir,
    environment: CanonicalEnvironment,
}

fn canonical_harness() -> &'static CanonicalHarness {
    static HARNESS: OnceLock<CanonicalHarness> = OnceLock::new();
    HARNESS.get_or_init(|| {
        let home = bounded_temp_dir("nv-compat-home-").expect("create canonical compat HOME");
        let tmp = bounded_temp_dir("nv-compat-tmp-").expect("create canonical compat TMPDIR");
        let environment = CanonicalEnvironment::new(
            CanonicalProfile::Oracle,
            home.path(),
            tmp.path(),
            workspace_root(),
        )
        .expect("canonical compat environment");
        CanonicalHarness {
            _home: home,
            _tmp: tmp,
            environment,
        }
    })
}

/// The attested GNU oracle every comparison in this test crate runs against.
///
/// Resolution and identity attestation are centralized in
/// `neomacs_test_oracle::gnu`: `NEOVM_FORCE_ORACLE_PATH` when set, otherwise
/// the pinned reference check on the `emacs` found on `PATH`.  A missing or
/// mismatched oracle PANICS here; a parity test must never skip or compare
/// against an unpinned editor.
pub fn oracle_emacs_path() -> PathBuf {
    neomacs_test_oracle::gnu::gnu_oracle().executable_path()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn repo_root() -> PathBuf {
    workspace_root()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn gnu_window_c_path() -> Option<PathBuf> {
    let mut dir = repo_root();
    for _ in 0..5 {
        let candidate = dir.join("emacs-mirror/emacs/src/window.c");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn elisp_string(path: &Path) -> String {
    format!(
        "\"{}\"",
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn write_temp_elisp_file(
    prefix: &str,
    suffix: &str,
    content: &str,
) -> Result<tempfile::TempPath, String> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile()
        .map_err(|e| format!("failed to create temp Elisp file: {e}"))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write temp Elisp file: {e}"))?;
    file.flush()
        .map_err(|e| format!("failed to flush temp Elisp file: {e}"))?;
    Ok(file.into_temp_path())
}

/// Cap the oracle process's address space so a runaway evaluation dies
/// at the rlimit instead of in the system OOM killer. Mirrors
/// crates/neovm-oracle-tests/src/common.rs apply_virtual_memory_limit.
#[cfg(unix)]
fn apply_memory_limit(cmd: &mut PinnedCommand, mem_limit_bytes: u64) {
    if !neomacs_infra::ADDRESS_SPACE_LIMIT_SUPPORTED {
        return;
    }
    cmd.limit_address_space(mem_limit_bytes);
}

/// RLIMIT_AS is Unix-only; non-Unix hosts run the oracle unlimited.
#[cfg(not(unix))]
fn apply_memory_limit(_cmd: &mut PinnedCommand, _mem_limit_bytes: u64) {}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn run_oracle_eval(form: &str) -> Result<String, String> {
    let oracle_bin = oracle_emacs_path();

    let form_path = write_temp_elisp_file("neovm-oracle-form-", ".el", form)?;
    let program = r#"(condition-case err
    (let ((source-buf (generate-new-buffer " *neovm-oracle-form*"))
          (last nil))
      (unwind-protect
          (progn
            (with-current-buffer source-buf
              (insert-file-contents (getenv "NEOVM_ORACLE_FORM_FILE"))
              (goto-char (point-min)))
            (condition-case nil
                (while t
                  (setq last (eval (read source-buf) t)))
              (end-of-file last))
            (princ (concat "OK " (prin1-to-string last))))
        (when (buffer-live-p source-buf)
          (kill-buffer source-buf)))))
  (error
   (princ (concat "ERR " (prin1-to-string err)))))"#;

    let mem_limit_mb = std::env::var("NEOVM_ORACLE_MEM_LIMIT_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(500);
    let mem_limit_bytes = mem_limit_mb * 1024 * 1024;

    // The canonical environment (`CanonicalProfile::Oracle`) pins the locale,
    // TZ, login/full name, HOME, TMPDIR and umask the expectations were
    // recorded under; the in-process Neomacs side receives the same answers
    // through `lisp_environment_form` below.
    let mut cmd = canonical_harness().environment.command(oracle_bin);
    cmd.env("NEOVM_ORACLE_FORM_FILE", form_path.as_os_str())
        .env("EMACSNATIVELOADPATH", "/dev/null")
        .args([
            "--batch",
            "-Q",
            "--eval",
            "(setq native-comp-jit-compilation nil inhibit-automatic-native-compilation t native-comp-enable-subr-trampolines nil)",
            "--eval",
            program,
        ]);

    apply_memory_limit(&mut cmd, mem_limit_bytes);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run GNU Emacs oracle: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "GNU Emacs oracle failed: status={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up

/// Runtime workspace root: nextest's NEXTEST_WORKSPACE_ROOT when present,
/// the compile-time constant otherwise (see neovm-core test_utils).
pub fn workspace_root() -> std::path::PathBuf {
    std::env::var_os("NEXTEST_WORKSPACE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_WORKSPACE_DIR")))
}

pub fn run_neovm_eval(form: &str) -> Result<String, String> {
    let mut eval = RUNTIME_TEMPLATE.with(|slot| {
        if slot.borrow().is_none() {
            let mut template = create_bootstrap_evaluator_cached()
                .map_err(|_| "NeoVM bootstrap failed".to_string())?;
            apply_runtime_startup_state(&mut template)
                .map_err(|_| "NeoVM runtime startup failed".to_string())?;
            // The spawned GNU above runs under the canonical environment; an
            // in-process evaluator has no child environment to pin, so it gets
            // the same `getenv` and identity answers through the Lisp-visible
            // seams.  Applied after startup state, which derives identity from
            // the real host.
            let canonical = canonical_harness().environment.lisp_environment_form();
            template
                .eval_str(&canonical)
                .map_err(|_| "NeoVM canonical environment failed".to_string())?;
            *slot.borrow_mut() = Some(pdump::snapshot_evaluator(&template));
        }

        let template = slot.borrow();
        let snapshot = template
            .as_ref()
            .ok_or_else(|| "NeoVM runtime template unavailable".to_string())?;
        pdump::restore_snapshot(snapshot).map_err(|e| format!("NeoVM runtime clone failed: {e}"))
    })?;
    eval.set_lexical_binding(true);
    let result = eval.eval_str(form);
    Ok(format_eval_result_with_eval(&eval, &result))
}
