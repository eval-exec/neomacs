//! Evaluator-side browser Worker artifact.
//!
//! This crate deliberately does not use wasm-bindgen. Its tiny, explicit host
//! ABI lets the browser loader wrap the blocking import with JSPI before
//! instantiation; the synchronous Atomics fallback uses the identical import.

std::cfg_select! {
    target_family = "wasm" => {
        mod browser_host;
        mod editor_session;
    }
    _ => {}
}

#[cfg(any(target_family = "wasm", test))]
const INPUT_WAKE: u32 = 1;
#[cfg(any(target_family = "wasm", test))]
const TIMEOUT_WAKE: u32 = 2;
#[cfg(any(target_family = "wasm", test))]
const RESUMED_BIAS: u32 = 0x4e45_0000;
#[cfg(any(target_family = "wasm", test))]
const INVALID_WAKE: u32 = u32::MAX;

#[cfg(any(target_family = "wasm", test))]
const fn resumed_probe_result(wake: u32) -> u32 {
    match wake {
        INPUT_WAKE | TIMEOUT_WAKE => RESUMED_BIAS | wake,
        _ => INVALID_WAKE,
    }
}

/// Suspend at the host input boundary and prove that the Rust Wasm stack
/// resumes afterward. The controlled Worker loader calls this through
/// `WebAssembly.promising` when JSPI is available.
#[cfg(target_family = "wasm")]
#[unsafe(no_mangle)]
pub extern "C" fn neomacs_wasm_worker_probe(timeout_milliseconds: f64) -> u32 {
    resumed_probe_result(browser_host::wait_for_input(timeout_milliseconds))
}

/// Restore the portable runtime image and enter the shared editor session.
#[cfg(target_family = "wasm")]
#[unsafe(no_mangle)]
pub extern "C" fn neomacs_wasm_worker_run() -> u32 {
    std::panic::set_hook(Box::new(|panic| {
        browser_host::report_failure(&format!("editor Worker panicked: {panic}"));
    }));
    match editor_session::run() {
        Ok(exit) if exit.is_success() => {
            browser_host::report_status("editor session exited");
            0
        }
        Ok(exit) => {
            browser_host::report_failure(
                exit.command_loop_error()
                    .unwrap_or("editor command loop failed"),
            );
            2
        }
        Err(error) => {
            browser_host::report_failure(&error);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspension_probe_only_accepts_declared_host_wakes() {
        assert_eq!(resumed_probe_result(INPUT_WAKE), 0x4e45_0001);
        assert_eq!(resumed_probe_result(TIMEOUT_WAKE), 0x4e45_0002);
        assert_eq!(resumed_probe_result(0), INVALID_WAKE);
        assert_eq!(resumed_probe_result(3), INVALID_WAKE);
    }
}
