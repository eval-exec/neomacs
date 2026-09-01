use std::cell::RefCell;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use neovm_core::emacs_core::format_eval_result_with_eval;
use neovm_core::emacs_core::load::{
    apply_runtime_startup_state, create_bootstrap_evaluator_cached,
};
use neovm_core::emacs_core::pdump;

thread_local! {
    static RUNTIME_TEMPLATE: RefCell<Option<neovm_core::emacs_core::pdump::types::DumpContextState>> =
        const { RefCell::new(None) };
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn oracle_emacs_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("NEOVM_FORCE_ORACLE_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let mut dir = repo_root();
    for _ in 0..4 {
        let candidate = dir.join("emacs-mirror/emacs/src/emacs");
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
pub fn oracle_enabled() -> bool {
    oracle_emacs_path().is_some()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
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

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn run_oracle_eval(form: &str) -> Result<String, String> {
    let Some(oracle_bin) = oracle_emacs_path() else {
        return Err("GNU Emacs oracle binary not found".to_string());
    };

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

    let mut cmd = Command::new(oracle_bin);
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

    unsafe {
        cmd.pre_exec(move || {
            let rlim = libc::rlimit {
                rlim_cur: mem_limit_bytes as libc::rlim_t,
                rlim_max: mem_limit_bytes as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

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
pub fn run_neovm_eval(form: &str) -> Result<String, String> {
    let mut eval = RUNTIME_TEMPLATE.with(|slot| {
        if slot.borrow().is_none() {
            let mut template = create_bootstrap_evaluator_cached()
                .map_err(|_| "NeoVM bootstrap failed".to_string())?;
            apply_runtime_startup_state(&mut template)
                .map_err(|_| "NeoVM runtime startup failed".to_string())?;
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
