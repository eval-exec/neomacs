//! Win32 serial `make-serial-process` error shapes, executed on the Windows
//! runner. GNU's `serial_open` raises plain `error` texts -- `Could not open
//! %s` for both the `CreateFile` refusal and the CRT fd step
//! (src/w32.c:11107-11111) -- where the termios arm raises `file-error` with
//! an errno. These rows pin that difference through the full
//! `make-serial-process` path; the configured-device half of the contract
//! needs COM hardware, so only the open-failure and pre-open windows are
//! covered here.

#[cfg(windows)]
use crate::test_utils::runtime_startup_eval_one;

/// The open failure is a PLAIN error whose text interpolates the port exactly
/// as `SSDATA (port_obj)` handed it to `CreateFile` (src/w32.c:11098-11108),
/// and the failed open leaves no half-registered process behind: `error`
/// unwinds before `register_process` (src/process.c:3212 vs :3283).
#[cfg(windows)]
#[test]
fn make_serial_process_failed_open_reports_w32_error_text() {
    crate::test_utils::init_test_tracing();
    // Elisp reads "\\\\.\\COM47" as \\.\COM47 and the printer re-doubles each
    // backslash, so the expected text repeats the same escape sequence.
    let result = runtime_startup_eval_one(
        r#"(let ((before (length (process-list))))
             (list (condition-case error-data
                       (make-serial-process :port "\\\\.\\COM47" :speed 9600)
                     (error error-data))
                   (- (length (process-list)) before)))"#,
    );
    assert_eq!(result, r#"OK ((error "Could not open \\\\.\\COM47") 0)"#);
}

/// The same refusal for a bare port name: `%s` prints the port as given, with
/// no `\\.\` canonicalization of its own.
#[cfg(windows)]
#[test]
fn make_serial_process_failed_open_interpolates_the_bare_port_name() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        r#"(condition-case error-data
             (make-serial-process :port "COM47" :speed 9600)
           (error error-data))"#,
    );
    assert_eq!(result, r#"OK (error "Could not open COM47")"#);
}

/// `CHECK_FIXNUM` on `:speed` runs in `Fmake_serial_process`
/// (src/process.c:3200-3201), before `serial_open` at :3212, so a non-fixnum
/// speed reports the type error even for a port that does not exist. This is
/// the platform-independent half of the keyword window, reached on Windows
/// through the same caller.
#[cfg(windows)]
#[test]
fn make_serial_process_speed_check_precedes_the_device_open() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        r#"(condition-case error-data
             (make-serial-process :port "COM47" :speed 'fast)
           (wrong-type-argument error-data))"#,
    );
    assert_eq!(result, "OK (wrong-type-argument fixnump fast)");
}
