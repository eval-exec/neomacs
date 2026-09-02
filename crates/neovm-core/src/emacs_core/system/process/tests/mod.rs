use super::*;
use crate::emacs_core::wait::{CommandInputWaitOutcome, ProcessOutputWaitOutcome};
use crate::emacs_core::{Context, builtins, format_eval_result};
use crate::heap_types::LispString;
use crate::test_utils::{runtime_startup_eval_all, runtime_startup_eval_one};
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(windows)]
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn process_finite_domains_match_gnu_symbols() {
    assert_eq!(ProcessKind::Real.name(), "real");
    assert_eq!(ProcessKind::Network.name(), "network");
    assert_eq!(ProcessKind::Pipe.name(), "pipe");
    assert_eq!(ProcessKind::Serial.name(), "serial");

    for (keyword, parsed) in [
        (":name", ProcessKeyword::Name),
        (":type", ProcessKeyword::Type),
        (":buffer", ProcessKeyword::Buffer),
        (":command", ProcessKeyword::Command),
        (":coding", ProcessKeyword::Coding),
        (":noquery", ProcessKeyword::Noquery),
        (":stop", ProcessKeyword::Stop),
        (":connection-type", ProcessKeyword::ConnectionType),
        (":filter", ProcessKeyword::Filter),
        (":sentinel", ProcessKeyword::Sentinel),
        (":stderr", ProcessKeyword::Stderr),
        (":file-handler", ProcessKeyword::FileHandler),
        (":host", ProcessKeyword::Host),
        (":service", ProcessKeyword::Service),
        (":family", ProcessKeyword::Family),
        (":local", ProcessKeyword::Local),
        (":remote", ProcessKeyword::Remote),
        (":server", ProcessKeyword::Server),
        (":nowait", ProcessKeyword::Nowait),
        (":log", ProcessKeyword::Log),
        (":tls-parameters", ProcessKeyword::TlsParameters),
        (":use-external-socket", ProcessKeyword::UseExternalSocket),
        (":plist", ProcessKeyword::Plist),
        (":bindtodevice", ProcessKeyword::Bindtodevice),
        (":broadcast", ProcessKeyword::Broadcast),
        (":dontroute", ProcessKeyword::Dontroute),
        (":keepalive", ProcessKeyword::Keepalive),
        (":linger", ProcessKeyword::Linger),
        (":oobinline", ProcessKeyword::Oobinline),
        (":priority", ProcessKeyword::Priority),
        (":reuseaddr", ProcessKeyword::Reuseaddr),
        (":nodelay", ProcessKeyword::Nodelay),
        (":port", ProcessKeyword::Port),
        (":speed", ProcessKeyword::Speed),
        (":process", ProcessKeyword::Process),
        (":bytesize", ProcessKeyword::Bytesize),
        (":stopbits", ProcessKeyword::Stopbits),
        (":parity", ProcessKeyword::Parity),
        (":flowcontrol", ProcessKeyword::Flowcontrol),
        (":summary", ProcessKeyword::Summary),
    ] {
        assert_eq!(ProcessKeyword::from_keyword(keyword), Some(parsed));
        assert_eq!(parsed.keyword(), keyword);
        assert_eq!(
            ProcessKeyword::from_value(&Value::keyword(keyword)),
            Some(parsed)
        );
    }
    assert_eq!(ProcessKeyword::from_keyword("name"), None);

    let status_names: Vec<&str> = ProcessStatusSymbol::gnu_public_domain()
        .iter()
        .map(|status| status.name())
        .collect();
    assert_eq!(
        status_names,
        vec![
            "run", "stop", "exit", "signal", "open", "listen", "closed", "connect", "failed"
        ]
    );
    assert_eq!(
        ProcessStatusSymbol::from_status_value(Value::symbol("run")),
        Some(ProcessStatusSymbol::Run)
    );
    assert_eq!(
        ProcessStatusSymbol::from_status_value(Value::list(vec![
            Value::symbol("exit"),
            Value::fixnum(7)
        ])),
        Some(ProcessStatusSymbol::Exit)
    );
    assert_eq!(
        ProcessStatusSymbol::from_status_value(Value::symbol("bogus")),
        None
    );

    assert_eq!(
        ProcessTtyStream::from_value(&Value::symbol("stdin")),
        Some(ProcessTtyStream::Stdin)
    );
    assert_eq!(
        ProcessTtyStream::from_value(&Value::symbol("stdout")),
        Some(ProcessTtyStream::Stdout)
    );
    assert_eq!(
        ProcessTtyStream::from_value(&Value::symbol("stderr")),
        Some(ProcessTtyStream::Stderr)
    );
    assert_eq!(ProcessTtyStream::Stdin.name(), "stdin");
    assert_eq!(ProcessTtyStream::Stdout.name(), "stdout");
    assert_eq!(ProcessTtyStream::Stderr.name(), "stderr");
    assert_eq!(ProcessTtyStream::from_value(&Value::NIL), None);
    assert_eq!(ProcessTtyStream::from_value(&Value::symbol("stream")), None);

    assert_eq!(
        NetworkAddressFamily::from_symbol_value(&Value::symbol("ipv4")),
        Some(NetworkAddressFamily::Ipv4)
    );
    assert_eq!(
        NetworkAddressFamily::from_symbol_value(&Value::symbol("ipv6")),
        Some(NetworkAddressFamily::Ipv6)
    );
    assert_eq!(
        NetworkAddressFamily::from_symbol_value(&Value::symbol("ip")),
        None
    );
    assert_eq!(NetworkAddressFamily::Ipv4.name(), "ipv4");
    assert_eq!(
        NetworkProcessFamilySymbol::from_symbol_value(&Value::symbol("local")),
        Some(NetworkProcessFamilySymbol::Local)
    );
    assert_eq!(
        NetworkProcessFamilySymbol::from_symbol_value(&Value::symbol("ipv4")),
        Some(NetworkProcessFamilySymbol::Ipv4)
    );
    assert_eq!(
        NetworkProcessFamilySymbol::from_symbol_value(&Value::symbol("ipv6")),
        Some(NetworkProcessFamilySymbol::Ipv6)
    );
    assert_eq!(NetworkProcessFamilySymbol::Local.name(), "local");
    assert!(validate_network_process_family(&Value::fixnum(42)).is_ok());
    assert!(validate_network_process_family(&Value::symbol("bogus")).is_err());
    assert_eq!(
        NetworkLookupHint::from_symbol_value(&Value::symbol("numeric")),
        Some(NetworkLookupHint::Numeric)
    );
    assert_eq!(NetworkLookupHint::Numeric.name(), "numeric");
    assert_eq!(
        NetworkLookupHint::from_symbol_value(&Value::symbol("canonical")),
        None
    );
    assert_eq!(
        NumProcessorsQuery::from_symbol_value(&Value::symbol("all")),
        Some(NumProcessorsQuery::All)
    );
    assert_eq!(
        NumProcessorsQuery::from_symbol_value(&Value::symbol("current")),
        Some(NumProcessorsQuery::Current)
    );
    assert_eq!(NumProcessorsQuery::All.name(), "all");
    assert_eq!(
        NumProcessorsQuery::from_symbol_value(&Value::symbol("default")),
        None
    );
    assert!(validate_network_socket_type(&Value::NIL).is_ok());
    assert!(validate_network_socket_type(&Value::symbol("datagram")).is_ok());
    assert!(validate_network_socket_type(&Value::symbol("seqpacket")).is_ok());
    assert!(validate_network_socket_type(&Value::symbol("bogus")).is_err());

    assert_eq!(
        ProcessConnectionType::from_symbol_value(&Value::symbol("pipe")),
        Some(ProcessConnectionType::Pipe)
    );
    assert_eq!(
        ProcessConnectionType::from_symbol_value(&Value::symbol("pty")),
        Some(ProcessConnectionType::Pty)
    );
    assert_eq!(
        resolve_process_connection_type_use_pty(None, true).unwrap(),
        true
    );
    assert_eq!(
        resolve_process_connection_type_use_pty(Some(&Value::NIL), true).unwrap(),
        true
    );
    assert_eq!(
        resolve_process_connection_type_use_pty(Some(&Value::symbol("pipe")), true).unwrap(),
        false
    );
    assert!(resolve_process_connection_type_use_pty(Some(&Value::T), true).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn waitpid_signal_status_preserves_core_dump_flag_for_sentinel_messages() {
    let raw_status = libc::SIGQUIT | 0x80;
    let status = match process_child_status_change_from_wait(sys::decode_wait_status(raw_status)) {
        ChildStatusChange::Reaped(status) => status,
        other => panic!("a signalled child is a terminal transition, got {other:?}"),
    };

    assert_eq!(
        status,
        process_status_signal_value_with_core(libc::SIGQUIT, true)
    );
    assert_eq!(gnu_process_status_message(status), "quit (core dumped)\n");
}

#[test]
fn char_sequence_to_lisp_string_preserves_nonunicode_char_codes() {
    crate::test_utils::init_test_tracing();
    let code = 0x3F_FF80u32;
    let result = char_sequence_to_lisp_string(&Value::vector(vec![Value::fixnum(code as i64)]))
        .expect("sequence should convert");
    assert!(result.is_multibyte());
    assert_eq!(lisp_string_char_codes(&result), vec![code]);
}

#[test]
fn char_sequence_to_lisp_string_preserves_pua_glyph_char_codes() {
    crate::test_utils::init_test_tracing();
    // U+E0B0 is a real nerd-font PUA glyph that happens to sit inside the
    // legacy raw-byte storage-sentinel range (U+E080..E0FF). The retired
    // storage round-trip silently rewrote it to the eight-bit code 0x3FFFB0
    // (issue #131); building the LispString bytes directly keeps it intact.
    let code = 0xE0B0u32;
    let result = char_sequence_to_lisp_string(&Value::vector(vec![Value::fixnum(code as i64)]))
        .expect("sequence should convert");
    assert_eq!(lisp_string_char_codes(&result), vec![0xE0B0]);
}

/// Decode a `LispString`'s Emacs character codes (test helper).
fn lisp_string_char_codes(string: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = string.as_bytes();
    let mut pos = 0;
    let mut codes = Vec::new();
    while pos < bytes.len() {
        codes.push(crate::emacs_core::emacs_char::string_char_advance(
            bytes, &mut pos,
        ));
    }
    codes
}

#[test]
fn format_network_address_preserves_raw_unibyte_string_payload() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let result =
        builtin_format_network_address(&mut eval, vec![raw]).expect("format-network-address");
    let text = result.as_lisp_string().expect("string result");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

fn install_minimal_special_event_command_runtime(ev: &mut Context) {
    ev.eval_str(
        r#"
(fset 'command-execute
      (lambda (cmd &optional _record keys _special)
        (funcall cmd (aref keys 0))))
(fset 'handle-delete-frame
      (lambda (event)
        (setq neo-last-delete-frame-event event)
        nil))
"#,
    )
    .expect("eval forms");
}

fn eval_one(src: &str) -> String {
    runtime_startup_eval_one(src)
}

fn eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn bootstrap_eval_one(src: &str) -> String {
    runtime_startup_eval_one(src)
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn eval_one_in_context(ev: &mut Context, src: &str) -> String {
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

/// Find the path of a binary, trying common Unix locations and PATH lookup.
fn find_bin_if_available(name: &str) -> Option<String> {
    for dir in &["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
        let path = format!("{}/{}", dir, name);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    let locator = if cfg!(windows) { "where.exe" } else { "which" };
    if let Ok(output) = std::process::Command::new(locator).arg(name).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string());
        }
    }
    None
}

fn find_bin(name: &str) -> String {
    find_bin_if_available(name).unwrap_or_else(|| name.to_string())
}

fn command_writing_stderr(payload: &str) -> Value {
    #[cfg(windows)]
    {
        Value::list(vec![
            Value::string("cmd.exe"),
            Value::string("/D"),
            Value::string("/S"),
            Value::string("/C"),
            Value::string(format!("<nul set /p \"={payload}\" 1>&2")),
        ])
    }
    #[cfg(not(windows))]
    {
        Value::list(vec![
            Value::string(find_bin("sh")),
            Value::string("-c"),
            Value::string(format!("printf %s {payload} >&2")),
        ])
    }
}

fn tmp_file(label: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let root = Path::new(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    std::fs::create_dir_all(&root).expect("create workspace temp root");
    root.join(format!("neovm-{label}-{}-{nonce}.txt", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

fn tmp_dir(label: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let dir = Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("tmp")
        .join(format!("neovm-{label}-{}-{nonce}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn gnu_timer_before(delay: Duration, callback: &str) -> Value {
    let when = SystemTime::now()
        .checked_sub(delay)
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should not precede unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

fn gnu_timer_after(delay: Duration, callback: &str) -> Value {
    let when = SystemTime::now()
        .checked_add(delay)
        .expect("timer deadline should fit in system time")
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should be after unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

#[test]
fn process_file_runs_in_default_directory_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let dir = tmp_dir("process-file-default-directory");
    let dir_with_slash = format!("{dir}/");
    let rendered = eval_one(&format!(
        r#"(let ((default-directory "{dir_with_slash}"))
             (with-temp-buffer
               (process-file "{sh}" nil t nil "-c" "pwd")
               (buffer-string)))"#
    ));
    assert_eq!(rendered, format!("OK \"{dir}\n\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_file_expands_tilde_in_default_directory_like_gnu() {
    // Regression: a buffer visiting a file under $HOME has an abbreviated
    // default-directory like "~/foo/" (GNU abbreviates it identically). The
    // subprocess cwd must be EXPANDED (~ -> $HOME) before chdir, mirroring GNU's
    // encode_current_directory (callproc.c); otherwise the OS cannot chdir to a
    // literal "~" and the subprocess silently runs in the wrong directory. This
    // broke diff-hl/git-gutter, whose `git ls-files` failed with status 128
    // "not a git repository" because git ran in $HOME instead of the repo.
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let home = std::env::var("HOME").expect("HOME set");
    let unique = format!("neomacs-tilde-cwd-test-{}", std::process::id());
    let abs = format!("{home}/{unique}");
    std::fs::create_dir_all(&abs).expect("create test dir under HOME");
    let rendered = eval_one(&format!(
        r#"(let ((default-directory "~/{unique}/"))
             (with-temp-buffer
               (process-file "{sh}" nil t nil "-c" "pwd")
               (buffer-string)))"#
    ));
    let _ = std::fs::remove_dir_all(&abs);
    // The subprocess must run in the EXPANDED dir: pwd reports an absolute path
    // containing the unique subdir, never a literal "~" and never the $HOME
    // fallback (which would lack the unique subdir).
    assert!(
        rendered.contains(&unique),
        "subprocess should run in the expanded ~/{unique}, got: {rendered}"
    );
    assert!(
        !rendered.contains('~'),
        "subprocess cwd must be expanded, not a literal ~: {rendered}"
    );
}

#[cfg(unix)]
#[test]
fn async_process_lookup_uses_dynamic_default_directory_like_gnu() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir("async-default-directory");
    let script = format!("{dir}/neo-rel-script");
    std::fs::write(&script, "#!/bin/sh\necho rel-ok\n").expect("write test script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod executable");
    let dir_with_slash = format!("{dir}/");

    let result = eval_one(&format!(
        r#"(let ((default-directory "{dir_with_slash}")
                 (exec-path nil))
             (list
              (let ((buf (generate-new-buffer "rel-start-out")))
                (unwind-protect
                    (let ((p (start-process "rel-start" buf "neo-rel-script")))
                      (while (process-live-p p)
                        (accept-process-output p 0.1))
                      (list (process-status p)
                            (with-current-buffer buf
                              (not (null (string-match-p "rel-ok" (buffer-string)))))))
                  (ignore-errors (kill-buffer buf))))
             (let ((buf (generate-new-buffer "rel-make-out")))
                (unwind-protect
                    (let ((p (make-process :name "rel-make"
                                           :buffer buf
                                           :command '("neo-rel-script"))))
                      (while (process-live-p p)
                        (accept-process-output p 0.1))
                      (list (process-status p)
                            (with-current-buffer buf
                              (not (null (string-match-p "rel-ok" (buffer-string)))))))
                  (ignore-errors (kill-buffer buf))))
              (let ((buf (generate-new-buffer "rel-file-out")))
                (unwind-protect
                    (let ((p (start-file-process "rel-file" buf "neo-rel-script")))
                      (while (process-live-p p)
                        (accept-process-output p 0.1))
                      (list (process-status p)
                            (with-current-buffer buf
                              (not (null (string-match-p "rel-ok" (buffer-string)))))))
                  (ignore-errors (kill-buffer buf))))))"#
    ));

    assert_eq!(result, "OK ((exit t) (exit t) (exit t))");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn async_process_lookup_expands_home_relative_program_like_gnu() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let home = tmp_dir("async-home-relative-program");
    let bin = format!("{home}/bin");
    std::fs::create_dir_all(&bin).expect("create fake home bin directory");
    let script = format!("{bin}/consult-fd-probe");
    std::fs::write(&script, "#!/bin/sh\nprintf 'home-relative-ok\\n'\n")
        .expect("write home-relative executable");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod executable");

    // GNU `make-process` routes a program beginning with `~` through `openp`.
    // `openp` calls `expand-file-name` for every search-path candidate, and
    // `expand-file-name` resolves the tilde against HOME from the dynamically
    // bound `process-environment`.  The unrelated `exec-path` entry proves that
    // HOME-relative names are not joined underneath an exec-path directory.
    let result = eval_one(&format!(
        r#"(let ((process-environment (copy-sequence process-environment))
                  (exec-path '("/neomacs-unrelated-exec-path")))
             (setenv "HOME" "{home}")
             (let ((buf (generate-new-buffer "home-relative-program-out")))
               (unwind-protect
                   (let ((p (make-process
                             :name "home-relative-program"
                             :buffer buf
                             :connection-type 'pipe
                             :command '("~/bin/consult-fd-probe"))))
                     ;; `(while (process-live-p p) (accept-process-output p T))'
                     ;; is the commonest idiom in Elisp and it has a hole: if
                     ;; the exit is recorded before the loop is first entered,
                     ;; the wait never runs and nothing drains the pipe.  GNU
                     ;; has that hole -- measured at 3/100 on this box -- so
                     ;; this test, which is about TILDE EXPANSION, drains
                     ;; explicitly rather than inheriting a race it does not
                     ;; mean to measure.
                     ;;
                     ;; It is TWO loops on purpose.  Ledger 193 wrote it as
                     ;; `(while (or (process-live-p p)
                     ;;             (accept-process-output p 0.1)))', and `or'
                     ;; SHORT-CIRCUITS: while the process is live the wait is
                     ;; never called at all, so that spelling is a pure-Lisp
                     ;; busy spin.  It terminated only because 193 recorded the
                     ;; child status at `maybe_quit'; with the record back at
                     ;; GNU's own site (inside the wait, ledger 198) the same
                     ;; line spins until the harness kills it -- measured, 5
                     ;; seconds and 334,484 iterations with the pipe never
                     ;; read.
                     (while (process-live-p p)
                       (accept-process-output p 0.1))
                     (while (accept-process-output p 0.1))
                     (list (process-status p)
                           (process-exit-status p)
                           (with-current-buffer buf
                             (not (null (string-match-p
                                         "home-relative-ok"
                                         (buffer-string)))))
                           (process-command p)))
                 (ignore-errors (kill-buffer buf)))))"#
    ));

    assert_eq!(result, "OK (exit 0 t (\"~/bin/consult-fd-probe\"))");
    let _ = std::fs::remove_dir_all(&home);
}

#[cfg(unix)]
#[test]
fn async_shell_command_wrappers_use_dynamic_shell_variables_like_gnu() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir("async-shell-wrapper");
    let script = format!("{dir}/neo-shell");
    std::fs::write(&script, "#!/bin/sh\necho $1:$2\n").expect("write shell script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod executable");
    let dir_with_slash = format!("{dir}/");

    let result = eval_one(&format!(
        r#"(let ((default-directory "{dir_with_slash}")
                 (exec-path nil)
                 (shell-file-name "neo-shell")
                 (shell-command-switch "--switch"))
             (list
              (let ((buf (generate-new-buffer "shell-start-out")))
                (unwind-protect
                    (let ((p (start-process-shell-command "shell-start" buf "payload")))
                      (while (process-live-p p)
                        (accept-process-output p 0.1))
                      (with-current-buffer buf
                        (not (null (string-match-p "--switch:payload" (buffer-string))))))
                  (ignore-errors (kill-buffer buf))))
              (let ((buf (generate-new-buffer "shell-file-out")))
                (unwind-protect
                    (let ((p (start-file-process-shell-command "shell-file" buf "payload")))
                      (while (process-live-p p)
                        (accept-process-output p 0.1))
                      (with-current-buffer buf
                        (not (null (string-match-p "--switch:payload" (buffer-string))))))
                  (ignore-errors (kill-buffer buf))))))"#
    ));

    assert_eq!(result, "OK (t t)");
    let _ = std::fs::remove_dir_all(&dir);
}

// -- ProcessManager unit tests ------------------------------------------

struct BrokenPipeWriter;

impl std::io::Write for BrokenPipeWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn epipe_closes_the_exact_selected_pty_writer() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "pty-epipe".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    pm.processes
        .get_mut(&id)
        .expect("process")
        .live_io
        .pty_writer = Some(Box::new(BrokenPipeWriter));

    assert!(pm.write_process_input_once(id, b"x").is_err());

    let process = pm.processes.get(&id).expect("process remains live");
    assert!(
        process.live_io.pty_writer.is_none(),
        "EPIPE must close the selected PTY endpoint, not an unrelated child stdin"
    );
    assert_eq!(
        process.input_disposition,
        ProcessInputDisposition::Disconnected,
        "EPIPE must make later sends observe GNU's closed outfd"
    );
    assert_eq!(
        process.status,
        Value::symbol("run"),
        "closing the failed writer must leave status settlement to the child reaper"
    );
    assert_eq!(
        pm.write_process_input_once(id, b"again")
            .expect("a disconnected writer is a stable state"),
        ProcessWriteAttempt::NoSource,
        "later writes must not probe a failed transport again"
    );
}

#[test]
fn send_eof_after_epipe_installs_discard_sink_and_clears_queued_input() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "pipe-epipe-eof".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let proc = pm.processes.get_mut(&id).expect("process");
    proc.input_disposition = ProcessInputDisposition::Disconnected;
    proc.write_queue = write_queue_push(
        Value::NIL,
        Value::heap_string(LispString::from_utf8("stale")),
        false,
    );

    send_eof_to_process(pm.processes.get_mut(&id).expect("process"))
        .expect("GNU installs /dev/null even when the old outfd is already -1");

    assert_eq!(
        pm.get(id).map(|proc| proc.input_disposition),
        Some(ProcessInputDisposition::Discard),
        "explicit EOF must replace an EPIPE-disconnected write side with the discard sink"
    );
    pm.flush_process_write_queue(id)
        .expect("discarding queued input cannot fail");
    assert_eq!(
        pm.get(id).map(|proc| proc.write_queue),
        Some(Value::NIL),
        "queued input must drain through the discard sink"
    );
}

#[test]
fn sends_after_epipe_do_not_accumulate_unsendable_input() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "pipe-epipe-queue".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    pm.processes
        .get_mut(&id)
        .expect("process")
        .input_disposition = ProcessInputDisposition::Disconnected;

    pm.send_input(id, &LispString::from_utf8("one"))
        .expect("process still exists");
    pm.send_input(id, &LispString::from_utf8("two"))
        .expect("process still exists");

    assert_eq!(
        pm.get(id).map(|proc| proc.write_queue),
        Some(Value::NIL),
        "a permanently disconnected endpoint must not retain bytes that can never be written"
    );
}

#[test]
fn process_manager_create_and_query() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "test".into(),
        Value::NIL,
        "/bin/echo".into(),
        vec!["hello".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert!(id > 0);
    assert!(pm.get(id).is_some());
    assert_eq!(pm.get(id).unwrap().name, Value::string("test"));
    assert_eq!(
        pm.get(id).unwrap().command,
        Value::list(vec![Value::string("/bin/echo"), Value::string("hello")])
    );
    assert_eq!(pm.get(id).unwrap().proc_type, Value::symbol("real"));
    assert_eq!(pm.get(id).unwrap().childp, Value::T);
    assert_eq!(pm.process_status(id), Some(&Value::symbol("run")));
}

#[test]
fn process_manager_kill() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "p".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert!(pm.kill_process(id));
    assert_eq!(
        pm.process_status(id),
        Some(&Value::list(vec![
            Value::symbol("signal"),
            Value::fixnum(9),
            Value::NIL,
        ]))
    );
}

#[test]
fn process_manager_delete() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "p".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert!(pm.delete_process(id));
    assert!(pm.get(id).is_none());
}

#[test]
fn process_manager_send_input() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "p".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert!(pm.send_input(id, &LispString::from_utf8("hello ")).unwrap());
    assert!(pm.send_input(id, &LispString::from_utf8("world")).unwrap());
    let expected = Value::list(vec![
        Value::cons(
            Value::heap_string(LispString::from_utf8("hello ")),
            Value::cons(Value::fixnum(0), Value::fixnum(6)),
        ),
        Value::cons(
            Value::heap_string(LispString::from_utf8("world")),
            Value::cons(Value::fixnum(0), Value::fixnum(5)),
        ),
    ]);
    assert!(crate::emacs_core::value::equal_value(
        &pm.get(id).unwrap().write_queue,
        &expected,
        0,
    ));
}

#[test]
fn builtin_process_send_string_preserves_raw_unibyte_write_queue_entries() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let buffers = crate::buffer::BufferManager::new();
    let id = pm.create_process(
        "p".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF, b'A']));
    builtin_process_send_string_impl(&mut pm, &buffers, vec![Value::make_process(id), raw])
        .expect("process-send-string");

    let expected = Value::list(vec![Value::cons(
        Value::heap_string(LispString::from_unibyte(vec![0xFF, b'A'])),
        Value::cons(Value::fixnum(0), Value::fixnum(2)),
    )]);
    assert!(crate::emacs_core::value::equal_value(
        &pm.get(id).unwrap().write_queue,
        &expected,
        0,
    ));
}

#[test]
fn process_send_string_accepts_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*send-string-target*");
    buffers.set_current(buffer_id);
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "send-string-proc".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    for (target, text) in [
        (Value::make_buffer(buffer_id), "buf"),
        (Value::string("*send-string-target*"), "name"),
        (Value::NIL, "nil"),
    ] {
        builtin_process_send_string_impl(&mut pm, &buffers, vec![target, Value::string(text)])
            .expect("process-send-string");
    }

    let expected = Value::list(vec![
        Value::cons(
            Value::heap_string(LispString::from_utf8("buf")),
            Value::cons(Value::fixnum(0), Value::fixnum(3)),
        ),
        Value::cons(
            Value::heap_string(LispString::from_utf8("name")),
            Value::cons(Value::fixnum(0), Value::fixnum(4)),
        ),
        Value::cons(
            Value::heap_string(LispString::from_utf8("nil")),
            Value::cons(Value::fixnum(0), Value::fixnum(3)),
        ),
    ]);
    assert_eq!(pm.find_by_buffer_id(buffer_id), Some(id));
    assert!(crate::emacs_core::value::equal_value(
        &pm.get(id).unwrap().write_queue,
        &expected,
        0,
    ));
}

#[test]
fn process_send_region_accepts_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*send-region-target*");
    buffers.set_current(buffer_id);
    buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert region text");
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "send-region-proc".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    for target in [
        Value::make_buffer(buffer_id),
        Value::string("*send-region-target*"),
        Value::NIL,
    ] {
        builtin_process_send_region_impl(
            &mut pm,
            &mut buffers,
            vec![target, Value::fixnum(1), Value::fixnum(4)],
        )
        .expect("process-send-region");
    }

    let entry = || {
        Value::cons(
            Value::heap_string(LispString::from_utf8("abc")),
            Value::cons(Value::fixnum(0), Value::fixnum(3)),
        )
    };
    let expected = Value::list(vec![entry(), entry(), entry()]);
    assert_eq!(pm.find_by_buffer_id(buffer_id), Some(id));
    assert!(crate::emacs_core::value::equal_value(
        &pm.get(id).unwrap().write_queue,
        &expected,
        0,
    ));
}

#[test]
fn process_send_eof_accepts_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*send-eof-target*");
    buffers.set_current(buffer_id);
    let mut pm = ProcessManager::new();
    let _id = pm.create_process(
        "send-eof-proc".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    let buffer_value = Value::make_buffer(buffer_id);
    let name_value = Value::string("*send-eof-target*");
    assert_eq!(
        builtin_process_send_eof_impl(&mut pm, &buffers, vec![buffer_value])
            .expect("process-send-eof buffer"),
        buffer_value
    );
    assert_eq!(
        builtin_process_send_eof_impl(&mut pm, &buffers, vec![name_value])
            .expect("process-send-eof buffer name"),
        name_value
    );
    assert_eq!(
        builtin_process_send_eof_impl(&mut pm, &buffers, vec![Value::NIL])
            .expect("process-send-eof nil"),
        Value::NIL
    );
}

#[test]
fn process_controls_accept_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*control-target*");
    buffers.set_current(buffer_id);
    let mut pm = ProcessManager::new();
    let id = pm.create_process_with_kind(
        "control-pipe".into(),
        Value::make_buffer(buffer_id),
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Pipe,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    let buffer_value = Value::make_buffer(buffer_id);
    assert_eq!(
        builtin_stop_process_impl(&mut pm, &buffers, vec![buffer_value])
            .expect("stop-process buffer"),
        buffer_value
    );
    assert_eq!(pm.get(id).expect("pipe").command, Value::T);

    let name_value = Value::string("*control-target*");
    assert_eq!(
        builtin_continue_process_impl(&mut pm, &buffers, vec![name_value])
            .expect("continue-process buffer name"),
        name_value
    );
    assert_eq!(pm.get(id).expect("pipe").command, Value::NIL);

    assert_eq!(
        builtin_stop_process_impl(&mut pm, &buffers, vec![Value::NIL]).expect("stop-process nil"),
        Value::NIL
    );
    let signal_err =
        builtin_signal_process_impl(&mut pm, &buffers, vec![buffer_value, Value::symbol("TERM")])
            .expect_err("signal-process buffer should reject connection process");
    match signal_err {
        Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            assert_eq!(
                signal.data,
                vec![Value::string("Cannot signal process control-pipe")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    let running_child_err = builtin_process_running_child_p_impl(
        &pm,
        &buffers,
        vec![Value::string("*control-target*")],
    )
    .expect_err("process-running-child-p buffer name should reject connection process");
    match running_child_err {
        Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            assert_eq!(
                signal.data,
                vec![Value::string("Process control-pipe is not a subprocess")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn continue_process_cancels_a_pending_stop_transition_like_gnu() {
    crate::test_utils::init_test_tracing();
    let buffers = crate::buffer::BufferManager::new();
    let mut processes = ProcessManager::new();
    let id = processes.create_process(
        "stopped".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    {
        let process = processes.get_mut(id).expect("process");
        process.status = process_status_run_value();
        process.pending_status = process_status_stop_value(19);
        process.status_notify_pending = true;
    }

    builtin_continue_process_impl(&mut processes, &buffers, vec![Value::make_process(id)])
        .expect("continue-process");

    let process = processes.get(id).expect("process");
    assert_eq!(process.status, process_status_run_value());
    assert_eq!(process.pending_status, Value::NIL);
    assert!(!process.status_notify_pending);
}

#[cfg(unix)]
#[test]
fn internal_default_signal_process_targets_only_the_named_pid_like_gnu() {
    crate::test_utils::init_test_tracing();
    let directory = tempfile::tempdir().expect("tempdir");
    let ready = directory.path().join("ready");
    let descendant_finished = directory.path().join("descendant-finished");
    let python = find_bin("python3");
    let descendant = format!(
        "import time; from pathlib import Path; time.sleep(0.2); Path({descendant_finished:?}).write_text('done')"
    );
    let parent = format!(
        "import subprocess, time; from pathlib import Path; subprocess.Popen([{python:?}, '-c', {descendant:?}]); Path({ready:?}).write_text('ready'); time.sleep(30)"
    );

    let mut processes = ProcessManager::new();
    let buffers = crate::buffer::BufferManager::new();
    let id = processes.create_process(
        "signal-one-pid".into(),
        Value::NIL,
        python,
        vec!["-c".into(), parent],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(id, false).expect("spawn parent");
    for _ in 0..100 {
        if ready.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "parent did not spawn its descendant");

    builtin_internal_default_signal_process_impl(
        &mut processes,
        &buffers,
        vec![Value::make_process(id), Value::symbol("TERM")],
    )
    .expect("internal-default-signal-process");

    for _ in 0..100 {
        if descendant_finished.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        descendant_finished.exists(),
        "signal-process killed the process group instead of only the named PID"
    );
    processes.delete_process(id);
}

#[test]
fn delete_process_accepts_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let object_buffer = buffers.create_buffer("*delete-object-target*");
    let name_buffer = buffers.create_buffer("*delete-name-target*");
    let nil_buffer = buffers.create_buffer("*delete-nil-target*");
    let message_buffer = buffers.create_buffer("*delete-message-target*");
    let mut pm = ProcessManager::new();
    let object_id = pm.create_process(
        "delete-object-proc".into(),
        Value::make_buffer(object_buffer),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let name_id = pm.create_process(
        "delete-name-proc".into(),
        Value::make_buffer(name_buffer),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let nil_id = pm.create_process(
        "delete-nil-proc".into(),
        Value::make_buffer(nil_buffer),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let message_id = pm.create_process(
        "delete-message-proc".into(),
        Value::make_buffer(message_buffer),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    builtin_delete_process_impl(&mut pm, &buffers, vec![Value::make_buffer(object_buffer)])
        .expect("delete-process buffer");
    builtin_delete_process_impl(
        &mut pm,
        &buffers,
        vec![Value::string("*delete-name-target*")],
    )
    .expect("delete-process buffer name");
    buffers.set_current(nil_buffer);
    builtin_delete_process_impl(&mut pm, &buffers, vec![Value::NIL]).expect("delete-process nil");
    buffers.set_current(message_buffer);
    builtin_delete_process_impl(&mut pm, &buffers, vec![Value::symbol("message")])
        .expect("delete-process message");

    assert!(pm.get(object_id).is_none());
    assert!(pm.get(name_id).is_none());
    assert!(pm.get(nil_id).is_none());
    assert!(pm.get(message_id).is_none());
}

#[test]
fn process_manager_find_by_name() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "my-proc".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert_eq!(pm.find_by_name("my-proc"), Some(id));
    assert_eq!(pm.find_by_name("nonexistent"), None);
}

#[test]
fn process_creation_allocates_the_smallest_available_gnu_name() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let (processes)
             (unwind-protect
                 (let* ((first
                         (make-process :name "name-allocation"
                                       :command nil
                                       :noquery t))
                        (second
                         (make-process :name "name-allocation"
                                       :command nil
                                       :noquery t))
                        (third
                         (make-pipe-process :name "name-allocation"
                                            :buffer nil
                                            :noquery t)))
                   (setq processes (list first second third))
                   (let ((initial (mapcar #'process-name processes)))
                     (delete-process second)
                     (let ((reuse-suffix
                            (make-pipe-process :name "name-allocation"
                                               :buffer nil
                                               :noquery t)))
                       (push reuse-suffix processes)
                       (delete-process first)
                       (let ((reuse-base
                              (make-process :name "name-allocation"
                                            :command nil
                                            :noquery t)))
                         (push reuse-base processes)
                         (list initial
                               (process-name reuse-suffix)
                               (process-name reuse-base))))))
               (mapc (lambda (process)
                       (ignore-errors (delete-process process)))
                     processes)))"#,
    );

    assert_eq!(
        result,
        r#"OK (("name-allocation" "name-allocation<1>" "name-allocation<2>") "name-allocation<1>" "name-allocation")"#
    );
}

#[test]
fn make_process_missing_program_reports_gnu_errno_data() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((exec-path nil))
             (condition-case error-data
                 (make-process
                  :name "missing-program"
                  :command '("neomacs-definitely-missing"))
               (file-missing error-data)))"#,
    );

    assert_eq!(
        result,
        r#"OK (file-missing "Searching for program" "No such file or directory" "neomacs-definitely-missing")"#
    );
}

#[cfg(unix)]
#[test]
fn make_process_non_executable_program_reports_gnu_errno_data() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let directory = tmp_dir("make-process-noexec");
    let program = std::path::Path::new(&directory).join("noexecprog");
    std::fs::write(&program, b"#!/bin/sh\n").expect("write non-executable program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644))
        .expect("chmod non-executable program");

    let result = eval_one(&format!(
        r#"(let ((exec-path (list "{directory}")))
             (condition-case error-data
                 (make-process
                  :name "non-executable-program"
                  :command '("noexecprog"))
               (error error-data)))"#
    ));

    assert_eq!(
        result,
        r#"OK (permission-denied "Searching for program" "Permission denied" "noexecprog")"#
    );

    std::fs::remove_dir_all(directory).expect("remove non-executable program fixture");
}

#[test]
fn builtin_process_name_uses_lisp_value_storage() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "my-proc".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    let value = builtin_process_name_impl(&pm, vec![Value::make_process(id)])
        .expect("process-name should succeed");
    let string = value
        .as_lisp_string()
        .expect("process-name should return a string");

    assert_eq!(string.as_bytes(), b"my-proc");
    assert!(string.is_multibyte());
}

#[test]
fn process_type_and_contact_use_stored_lisp_fields() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let buffers = crate::buffer::BufferManager::new();
    let network = pm.create_process_with_kind(
        "net-proc".into(),
        Value::NIL,
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Network,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    {
        let proc = pm.get_mut(network).expect("network process");
        proc.childp = Value::list(vec![
            Value::keyword(":name"),
            proc.name,
            Value::keyword(":host"),
            Value::string("localhost"),
            Value::keyword(":service"),
            Value::fixnum(7777),
            Value::keyword(":server"),
            Value::T,
        ]);
    }

    assert_eq!(
        builtin_process_type_impl(&pm, &buffers, vec![Value::make_process(network)])
            .expect("process-type"),
        Value::symbol("network")
    );
    assert_eq!(
        builtin_process_contact_impl(&pm, vec![Value::make_process(network), Value::NIL])
            .expect("process-contact nil"),
        Value::list(vec![Value::string("localhost"), Value::fixnum(7777)])
    );
    let full = builtin_process_contact_impl(&pm, vec![Value::make_process(network), Value::T])
        .expect("process-contact t");
    assert_eq!(
        builtins::builtin_plist_get(vec![full, Value::keyword(":name")]).expect("plist-get :name"),
        pm.get(network).unwrap().name
    );
    assert_eq!(
        builtins::builtin_plist_get(vec![full, Value::keyword(":server")])
            .expect("plist-get :server"),
        Value::T
    );
    assert_eq!(
        process_public_status_symbol(pm.get(network).unwrap()),
        Value::symbol("listen")
    );
    pm.get_mut(network).expect("network process").status = Value::symbol("open");
    assert_eq!(
        process_public_status_symbol(pm.get(network).unwrap()),
        Value::symbol("open")
    );
    pm.get_mut(network).expect("network process").status = Value::symbol("closed");
    assert_eq!(
        process_public_status_symbol(pm.get(network).unwrap()),
        Value::symbol("closed")
    );
}

#[test]
fn connection_process_mutators_keep_childp_plist_in_sync() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*proc-contact-childp*");
    let mut pm = ProcessManager::new();
    let id = pm.create_process_with_kind(
        "net-proc".into(),
        Value::make_buffer(buffer_id),
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Network,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    {
        let proc = pm.get_mut(id).expect("network process");
        proc.childp = Value::list(vec![
            Value::keyword(":name"),
            proc.name,
            Value::keyword(":server"),
            Value::T,
            Value::keyword(":service"),
            Value::fixnum(7777),
            Value::keyword(":buffer"),
            Value::make_buffer(buffer_id),
            Value::keyword(":filter"),
            Value::symbol("ignore"),
            Value::keyword(":sentinel"),
            Value::symbol("ignore"),
        ]);
    }

    builtin_set_process_buffer_impl(
        &mut pm,
        &mut buffers,
        vec![Value::make_process(id), Value::NIL],
    )
    .expect("set-process-buffer");
    let filter =
        builtin_set_process_filter_impl(&mut pm, vec![Value::make_process(id), Value::NIL])
            .expect("set-process-filter");
    let sentinel =
        builtin_set_process_sentinel_impl(&mut pm, vec![Value::make_process(id), Value::NIL])
            .expect("set-process-sentinel");

    assert_eq!(filter, Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL));
    assert_eq!(sentinel, Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL));

    let contact = builtin_process_contact_impl(&pm, vec![Value::make_process(id), Value::T])
        .expect("process-contact t");
    assert_eq!(
        builtins::builtin_plist_get(vec![contact, Value::keyword(":buffer")])
            .expect("plist-get :buffer"),
        Value::NIL
    );
    assert_eq!(
        builtins::builtin_plist_get(vec![contact, Value::keyword(":filter")])
            .expect("plist-get :filter"),
        Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL)
    );
    assert_eq!(
        builtins::builtin_plist_get(vec![contact, Value::keyword(":sentinel")])
            .expect("plist-get :sentinel"),
        Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL)
    );
}

#[test]
fn make_network_process_server_stores_log_as_lisp_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let process = builtin_make_network_process(
        &mut eval,
        vec![
            Value::keyword(":name"),
            Value::string("neo-log-server"),
            Value::keyword(":server"),
            Value::T,
            Value::keyword(":service"),
            Value::fixnum(0),
            Value::keyword(":log"),
            Value::symbol("ignore"),
        ],
    )
    .expect("make-network-process");
    // make-network-process now returns a first-class process object.
    assert!(
        process.is_process(),
        "expected process object, got {process:?}"
    );
    let id = process.as_process_id().expect("expected process object id");

    let stored = eval.processes.get(id).expect("server process");
    assert_eq!(stored.log, Value::symbol("ignore"));

    let contact =
        builtin_process_contact_impl(&eval.processes, vec![Value::make_process(id), Value::T])
            .expect("process-contact t");
    assert_eq!(
        builtins::builtin_plist_get(vec![contact, Value::keyword(":log")]).expect("plist-get :log"),
        Value::symbol("ignore")
    );
}

#[test]
fn process_buffer_storage_uses_buffer_objects() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("*proc-output*");
    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "my-proc".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    assert_eq!(pm.find_by_buffer_id(buffer_id), Some(id));
    let value =
        builtin_process_buffer_impl(&pm, vec![Value::make_process(id)]).expect("process-buffer");
    assert_eq!(value, Value::make_buffer(buffer_id));

    builtin_set_process_buffer_impl(
        &mut pm,
        &mut buffers,
        vec![Value::make_process(id), Value::NIL],
    )
    .expect("set-process-buffer should accept nil");
    assert!(pm.get(id).expect("process").buffer.is_nil());
}

#[test]
fn process_mark_storage_uses_marker_objects() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let first = buffers.create_buffer("*proc-output-1*");
    let second = buffers.create_buffer("*proc-output-2*");
    let _ = buffers.insert_into_buffer(first, "abc");
    let _ = buffers.insert_into_buffer(second, "z");

    let mut pm = ProcessManager::new();
    let id = pm.create_process(
        "my-proc".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let mark = builtin_process_mark_impl(&pm, &buffers, vec![Value::make_process(id)])
        .expect("process-mark should succeed");
    assert!(mark.is_marker());
    assert!(
        super::super::marker::builtin_marker_buffer_in_buffers(&buffers, vec![mark])
            .expect("marker-buffer")
            .is_nil()
    );

    builtin_set_process_buffer_impl(
        &mut pm,
        &mut buffers,
        vec![Value::make_process(id), Value::make_buffer(first)],
    )
    .expect("attach first process buffer");
    let mark = builtin_process_mark_impl(&pm, &buffers, vec![Value::make_process(id)])
        .expect("process-mark should succeed");
    assert_eq!(
        super::super::marker::builtin_marker_buffer_in_buffers(&buffers, vec![mark])
            .expect("marker-buffer"),
        Value::make_buffer(first)
    );
    assert_eq!(
        super::super::marker::marker_position_as_int_with_buffers(&buffers, &mark)
            .expect("marker-position"),
        4
    );

    builtin_set_process_buffer_impl(
        &mut pm,
        &mut buffers,
        vec![Value::make_process(id), Value::make_buffer(second)],
    )
    .expect("attach second process buffer");
    let mark = builtin_process_mark_impl(&pm, &buffers, vec![Value::make_process(id)])
        .expect("process-mark should succeed");
    assert_eq!(
        super::super::marker::builtin_marker_buffer_in_buffers(&buffers, vec![mark])
            .expect("marker-buffer"),
        Value::make_buffer(second)
    );
    assert_eq!(
        super::super::marker::marker_position_as_int_with_buffers(&buffers, &mark)
            .expect("marker-position"),
        2
    );
}

#[test]
fn internal_default_process_filter_moves_stored_process_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*proc-filter-mark*");
    let pid = ev.processes.create_process(
        "proc-filter-mark".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .sync_process_mark(&mut ev.buffers, pid)
        .expect("sync process mark");
    assert_eq!(
        eval_one_in_context(
            &mut ev,
            r#"(progn
                 (set-buffer "*proc-filter-mark*")
                 (setq process-filter-peer-marker (copy-marker (point-max)))
                 (marker-position process-filter-peer-marker))"#,
        ),
        "OK 1"
    );

    builtin_internal_default_process_filter(
        &mut ev,
        vec![Value::make_process(pid), Value::string("ab")],
    )
    .expect("first insert");
    let mark =
        builtin_process_mark_impl(&ev.processes, &ev.buffers, vec![Value::make_process(pid)])
            .expect("process-mark");
    assert_eq!(
        super::super::marker::marker_position_as_int_with_buffers(&ev.buffers, &mark)
            .expect("marker-position"),
        3
    );
    assert_eq!(
        eval_one_in_context(&mut ev, "(marker-position process-filter-peer-marker)"),
        "OK 3",
        "GNU's default process filter inserts before every marker at the process mark"
    );
    assert_eq!(
        ev.buffers
            .get(buffer_id)
            .expect("buffer")
            .buffer_substring_lisp_string_range(crate::buffer::EmacsByteRange::from_usize(0, 2))
            .as_bytes(),
        b"ab"
    );

    builtin_internal_default_process_filter(
        &mut ev,
        vec![Value::make_process(pid), Value::string("cd")],
    )
    .expect("second insert");
    let mark =
        builtin_process_mark_impl(&ev.processes, &ev.buffers, vec![Value::make_process(pid)])
            .expect("process-mark");
    assert_eq!(
        super::super::marker::marker_position_as_int_with_buffers(&ev.buffers, &mark)
            .expect("marker-position"),
        5
    );
    assert_eq!(
        ev.buffers.get(buffer_id).expect("buffer").buffer_string(),
        "abcd"
    );
}

#[test]
fn internal_default_process_sentinel_inserts_status_at_process_mark() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let original = ev.buffers.current_buffer_id().expect("current buffer");
    let buffer_id = ev.buffers.create_buffer("*proc-sentinel-mark*");
    ev.buffers
        .insert_into_buffer(buffer_id, "before-after")
        .expect("insert process buffer text");
    ev.buffers
        .get_mut(buffer_id)
        .expect("process buffer")
        .goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(6));

    let pid = ev.processes.create_process(
        "sentinel-proc".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .sync_process_mark(&mut ev.buffers, pid)
        .expect("sync process mark");
    let mark =
        builtin_process_mark_impl(&ev.processes, &ev.buffers, vec![Value::make_process(pid)])
            .expect("process-mark before sentinel");
    super::super::marker::builtin_set_marker_in_buffers(
        &mut ev.buffers,
        vec![mark, Value::fixnum(7), Value::make_buffer(buffer_id)],
    )
    .expect("move process mark");
    ev.processes.get_mut(pid).expect("process").status = process_status_exit_value(0);

    builtin_internal_default_process_sentinel(
        &mut ev,
        vec![Value::make_process(pid), Value::string("finished\n")],
    )
    .expect("default sentinel");

    assert_eq!(ev.buffers.current_buffer_id(), Some(original));
    assert_eq!(
        ev.buffers
            .get(buffer_id)
            .expect("process buffer")
            .buffer_string(),
        "before\nProcess sentinel-proc finished\n-after"
    );
    let mark =
        builtin_process_mark_impl(&ev.processes, &ev.buffers, vec![Value::make_process(pid)])
            .expect("process-mark");
    assert_eq!(
        super::super::marker::marker_position_as_int_with_buffers(&ev.buffers, &mark)
            .expect("marker-position"),
        39
    );
}

#[test]
fn process_status_notification_runs_default_sentinel_and_reaps() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*proc-status-notify*");
    ev.buffers
        .insert_into_buffer(buffer_id, "payload")
        .expect("insert process buffer text");
    let pid = ev.processes.create_process(
        "neo-default-sentinel".into(),
        Value::make_buffer(buffer_id),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .sync_process_mark(&mut ev.buffers, pid)
        .expect("sync process mark");
    ev.processes
        .set_child_status_pending(pid, process_status_exit_value(0));

    ev.run_process_status_notification(pid, None)
        .expect("status notification");

    assert_eq!(
        ev.buffers
            .get(buffer_id)
            .expect("process buffer")
            .buffer_string(),
        "payload\nProcess neo-default-sentinel finished\n"
    );
    assert!(ev.processes.get(pid).is_none());
    assert!(ev.processes.get_any(pid).is_some());
}

#[test]
fn builtin_process_tty_name_uses_value_slot() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let real_id = pm.create_process(
        "tty-proc".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let pipe_id = pm.create_process_with_kind(
        "pipe-proc".into(),
        Value::NIL,
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Pipe,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    let tty_value =
        builtin_process_tty_name_impl(&pm, vec![Value::make_process(real_id)]).expect("tty");
    assert!(tty_value.is_string());
    assert!(!tty_value.is_nil());

    let pipe_value =
        builtin_process_tty_name_impl(&pm, vec![Value::make_process(pipe_id)]).expect("tty");
    assert!(pipe_value.is_nil());
}

#[test]
fn make_process_stores_pipe_stderr_process_value() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let process = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("proc-stderr-owner"),
            Value::keyword(":command"),
            Value::list(vec![Value::string("cat")]),
            Value::keyword(":stderr"),
            Value::string("*proc-stderr-buffer*"),
        ],
        true,
    )
    .expect("make-process");
    let id = process.as_process_id().expect("expected process object id");

    let stderrproc = pm.get(id).expect("main process").stderrproc;
    let stderr_id = stderrproc
        .as_process_id()
        .expect("expected stderr pipe process object id");
    let stderr_pipe = pm.get(stderr_id).expect("stderr pipe process");
    assert_eq!(stderr_pipe.kind, ProcessKind::Pipe);
    assert!(stderr_pipe.buffer.as_buffer_id().is_some());

    let stderr_tty =
        builtin_process_tty_name_impl(&pm, vec![Value::make_process(id), Value::symbol("stderr")])
            .expect("process-tty-name stderr");
    assert_eq!(stderr_tty, Value::NIL);

    let stdout_tty =
        builtin_process_tty_name_impl(&pm, vec![Value::make_process(id), Value::symbol("stdout")])
            .expect("process-tty-name stdout");
    assert!(stdout_tty.as_lisp_string().is_some());
}

/// Full `make-process :stderr SEPARATE-BUFFER` lifecycle, GNU byte-exact.
///
/// GNU (`src/process.c`): `:stderr` being a buffer auto-creates an implicit
/// `"<name> stderr"` pipe-process (`Fmake_pipe_process`), `create_process`
/// wires the child's stderr fd to that pipe's read end (`forkerr`, independent
/// of whether stdout uses a PTY), and on child exit the stderr write end closes
/// → the pipe-process EOFs, goes dead, and its default sentinel inserts
/// "\nProcess <name> stderr finished\n" into the stderr buffer.  So:
///   * stdout goes to the main process's buffer (`ob`),
///   * stderr goes to the stderr pipe-process's buffer (`eb`), NOT `ob`,
///   * the `"x stderr"` pipe-process reaches a terminal state (no longer live).
///
/// This is the exact reproduction form that previously diverged: stderr leaked
/// into `ob`, `eb` stayed empty, and the stderr pipe-process never died because
/// the default-PTY spawn path ignored `stderrproc` and merged stderr into the
/// PTY.  Run under the default connection type (PTY for stdout) so it guards the
/// PTY-split-stderr path specifically.
#[test]
fn make_process_stderr_separate_buffer_lifecycle_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let ((p (make-process :name "x"
                                  :command '("{sh}" "-c" "echo OUT; echo ERR 1>&2")
                                  :buffer (get-buffer-create "ob")
                                  :stderr (get-buffer-create "eb"))))
             (while (process-live-p p) (accept-process-output p 1))
             (accept-process-output nil 0.3)
             (list :stderr-proc-live
                   (let ((sp (get-buffer-process "eb")))
                     (and sp (process-live-p sp)))
                   :ob (with-current-buffer "ob" (buffer-string))
                   :eb (with-current-buffer "eb" (buffer-string))))"#
    ));
    assert_eq!(
        result,
        concat!(
            "OK (:stderr-proc-live nil ",
            ":ob \"OUT\n\nProcess x finished\n\" ",
            ":eb \"ERR\n\nProcess x stderr finished\n\")",
        )
    );
}

/// GNU `status_notify` runs only after the wait loop has serviced ready
/// process descriptors.  Consequently a real process's terminal sentinel can
/// inspect every byte already written to its separate stderr pipe, even when
/// the caller targets the real process and the child exits immediately after
/// stdin reaches EOF.  This is the ordering contract asynchronous consumers
/// such as Apheleia use to build their error report inside the sentinel.
#[test]
fn terminal_sentinel_observes_separate_stderr_output_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((seen :pending)
                  (stdout (generate-new-buffer " *sentinel-stdout*"))
                  (stderr (generate-new-buffer " *sentinel-stderr*"))
                  (process
                   (make-process
                    :name "sentinel-stderr-order"
                    :buffer stdout
                    :stderr stderr
                    :command '("{sh}" "-c"
                               "cat >/dev/null; printf ERR >&2; exit 7")
                    :connection-type 'pipe
                    :noquery t
                    :sentinel
                    (lambda (proc _event)
                      (unless (process-live-p proc)
                        (setq seen
                              (list
                               (with-current-buffer stderr (buffer-string))
                               (process-exit-status proc)
                               (process-status proc))))))))
             (set-process-sentinel (get-buffer-process stderr) #'ignore)
             (process-send-eof process)
             (while (eq seen :pending)
               (accept-process-output process 0.1))
             seen)"#
    ));

    assert_eq!(result, r#"OK ("ERR" 7 exit)"#);
}

/// GNU gives a subprocess with no explicit `:stderr` one shared output
/// channel: both stdout and stderr reach the main process filter/buffer, and
/// closing Emacs's copy of the channel cannot SIGPIPE the child.
#[test]
fn make_process_pipe_merges_stderr_into_the_main_output_channel() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((b (generate-new-buffer " *merge-stderr*"))
                  (p (make-process
                      :name "merge-stderr"
                      :connection-type 'pipe
                      :buffer b
                      :command '("{sh}" "-c"
                                 "printf OUT; printf ERR >&2; exit 1")
                      :noquery t)))
             (while (process-live-p p)
               (accept-process-output p 0.1))
             (accept-process-output nil 0.1)
             (list (with-current-buffer b (buffer-string))
                   (process-status p)
                   (process-exit-status p)))"#
    ));
    assert_eq!(
        result,
        concat!(
            "OK (\"OUTERR\n",
            "Process merge-stderr exited abnormally with code 1\n",
            "\" exit 1)",
        )
    );
}

#[test]
fn make_process_stderr_pipe_name_preserves_raw_unibyte_owner_bytes() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let process = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::heap_string(LispString::from_unibyte(vec![0xFF, b'p'])),
            Value::keyword(":command"),
            Value::list(vec![Value::string("cat")]),
            Value::keyword(":stderr"),
            Value::string("*proc-stderr-raw-buffer*"),
        ],
        true,
    )
    .expect("make-process");
    let id = process.as_process_id().expect("expected process object id");

    let stderr_id = pm
        .get(id)
        .expect("main process")
        .stderrproc
        .as_process_id()
        .expect("stderr pipe process object id");
    let stderr_name = pm
        .get(stderr_id)
        .expect("stderr pipe")
        .name
        .as_lisp_string()
        .expect("stderr pipe name");

    assert!(!stderr_name.is_multibyte());
    assert_eq!(
        stderr_name.as_bytes(),
        &[0xFF, b'p', b' ', b's', b't', b'd', b'e', b'r', b'r']
    );
}

#[test]
fn make_process_accepts_existing_pipe_process_for_stderr() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        None,
        crate::emacs_core::process::ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("proc-existing-stderr"),
            Value::keyword(":buffer"),
            Value::string("*proc-existing-stderr-buffer*"),
        ],
    )
    .expect("make-pipe-process");
    let process = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("proc-uses-existing-stderr"),
            Value::keyword(":command"),
            Value::list(vec![Value::string("cat")]),
            Value::keyword(":stderr"),
            stderrproc,
        ],
        true,
    )
    .expect("make-process");
    let id = process.as_process_id().expect("expected process object id");

    assert_eq!(pm.get(id).expect("main process").stderrproc, stderrproc);
}

#[test]
fn make_process_merges_stderr_when_deleted_stderr_pipe_is_stale() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("deleted-stderr"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");
    let stderr_id = stderrproc.as_process_id().expect("stderr pipe process id");
    assert!(pm.delete_process(stderr_id));

    let process = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("stale-stderr-owner"),
            Value::keyword(":command"),
            command_writing_stderr("MERGED"),
            Value::keyword(":stderr"),
            stderrproc,
            Value::keyword(":connection-type"),
            Value::symbol("pipe"),
        ],
        false,
    )
    .expect("stale :stderr should merge stderr into stdout");
    let owner_id = process.as_process_id().expect("owner process id");
    let coding_systems = crate::emacs_core::coding::CodingSystemManager::new();
    let mut output = Vec::new();
    for _ in 0..100 {
        if let Some(read) = pm.read_process_output_without_decoding(
            owner_id,
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        ) {
            output.extend_from_slice(read.undecoded_bytes());
            if output == b"MERGED" {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(output, b"MERGED");
}

#[test]
fn make_process_merges_stderr_when_pipe_writer_was_already_consumed() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("reused-stderr"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");

    let first = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("first-stderr-owner"),
            Value::keyword(":command"),
            command_writing_stderr("FIRST"),
            Value::keyword(":stderr"),
            stderrproc,
            Value::keyword(":connection-type"),
            Value::symbol("pipe"),
        ],
        false,
    )
    .expect("first make-process");
    let first_id = first.as_process_id().expect("first process id");

    let second = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("second-stderr-owner"),
            Value::keyword(":command"),
            command_writing_stderr("SECOND"),
            Value::keyword(":stderr"),
            stderrproc,
            Value::keyword(":connection-type"),
            Value::symbol("pipe"),
        ],
        false,
    )
    .expect("reusing a consumed stderr pipe should merge stderr");
    let second_id = second.as_process_id().expect("second process id");
    let stderr_id = stderrproc.as_process_id().expect("stderr pipe id");
    assert_eq!(
        pm.stderr_pipe_owner(stderr_id),
        Some(first_id),
        "reusing a consumed pipe must not replace its actual writer owner"
    );
    let coding_systems = crate::emacs_core::coding::CodingSystemManager::new();
    let mut output = Vec::new();
    for _ in 0..100 {
        if let Some(read) = pm.read_process_output_without_decoding(
            second_id,
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        ) {
            output.extend_from_slice(read.undecoded_bytes());
            if output == b"SECOND" {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(output, b"SECOND");
}

#[test]
fn stderr_pipe_uses_child_stdout_as_its_live_source() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("child-stdout-stderr"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");
    let stderr_id = stderrproc.as_process_id().expect("stderr pipe process id");
    let process = builtin_make_process_impl(
        &mut pm,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("child-stdout-owner"),
            Value::keyword(":command"),
            command_writing_stderr("ERR"),
            Value::keyword(":stderr"),
            stderrproc,
            Value::keyword(":connection-type"),
            Value::symbol("pipe"),
        ],
        false,
    )
    .expect("make-process");
    let owner_id = process.as_process_id().expect("owner process id");

    assert!(
        pm.open_channel_for_module(stderrproc).is_err(),
        "successful :stderr spawn transfers the pipe writer to the child"
    );
    assert!(pm.get(stderr_id).is_some_and(|proc| {
        proc.kind == ProcessKind::Pipe
            && proc.live_io.child_stdout.is_some()
            && !proc.live_io.child.has_child()
    }));
    assert!(pm.live_process_ids().contains(&stderr_id));
    assert!(pm.live_process_ids().contains(&owner_id));
    assert_eq!(pm.stderr_pipe_owner(stderr_id), Some(owner_id));
}

#[test]
fn stderr_pipe_sentinel_runs_before_live_owner_exits() {
    crate::test_utils::init_test_tracing();
    let closer = if cfg!(windows) {
        find_bin_if_available("python").map(|_| "python".to_string())
    } else {
        find_bin_if_available("python3")
    };
    let Some(closer) = closer else {
        eprintln!("skipping stderr-close lifecycle test: Python is unavailable");
        return;
    };
    let result = eval_one(&format!(
        r#"(let* ((stderr-buffer (generate-new-buffer " *early-stderr*"))
                  (owner-buffer (generate-new-buffer " *early-stderr-owner*"))
                  (pipe-event nil)
                  (stderr (make-pipe-process
                           :name "early-stderr"
                           :buffer stderr-buffer
                           :sentinel (lambda (process _event)
                                       (setq pipe-event
                                             (list (process-status process))))))
                  (owner (make-process
                          :name "early-stderr-owner"
                          :buffer owner-buffer
                          :stderr stderr
                          :connection-type 'pipe
                          :command '("{closer}" "-c"
                                     "import os; os.close(2); print('READY', flush=True); input()"))))
             (let ((deadline (+ (float-time) 1.0)))
               (while (and (null pipe-event)
                           (< (float-time) deadline))
                 (accept-process-output nil 0.01)))
             (let ((before-release
                    (list pipe-event
                          (process-status stderr)
                            (if (process-live-p owner) t nil))))
               (process-send-string owner "release\n")
               (while (process-live-p owner)
                 (accept-process-output owner 0.05))
               (prog1 (list before-release pipe-event (process-status stderr))
                 (kill-buffer stderr-buffer)
                 (kill-buffer owner-buffer))))"#
    ));
    assert_eq!(result, "OK (((closed) closed t) (closed) closed)");
}

#[test]
fn stderr_pipe_wait_notifies_a_pending_owner_stop_first() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let stderr_id = eval.processes.create_process_with_kind(
        "pending-stop-stderr".into(),
        Value::NIL,
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Pipe,
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    let owner_id = eval.processes.create_process(
        "pending-stop-owner".into(),
        Value::NIL,
        "owner".into(),
        vec![],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    {
        let owner = eval.processes.get_mut(owner_id).expect("owner process");
        owner.stderrproc = Value::make_process(stderr_id);
        owner.status = process_status_run_value();
        owner.pending_status = process_status_stop_value(19);
        owner.status_notify_pending = true;
        owner.sentinel = Value::NIL;
    }
    eval.processes
        .get_mut(stderr_id)
        .expect("stderr pipe")
        .live_io
        .stderr_pipe_owner = Some(owner_id);

    let mut outcome = ProcessOutputServiceOutcome::default();
    let pipe_may_notify = eval
        .notify_stderr_pipe_owner_first(stderr_id, None, &mut outcome)
        .expect("notify pending owner status");

    assert!(!pipe_may_notify, "the pipe waits for the newer owner");
    let owner = eval.processes.get(owner_id).expect("owner remains live");
    assert_eq!(owner.status, process_status_stop_value(19));
    assert!(!owner.status_notify_pending);
}

#[cfg(windows)]
#[test]
fn module_pipe_service_polling_is_nonblocking_before_write_and_services_data() {
    crate::test_utils::init_test_tracing();
    let (fd_tx, fd_rx) = mpsc::channel();
    let (idle_tx, idle_rx) = mpsc::channel();
    let (write_tx, write_rx) = mpsc::channel();
    let (data_tx, data_rx) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        let mut ev = Context::new();
        let buffer_id = ev.buffers.create_buffer(" *module-service-idle*");
        let process = builtin_make_pipe_process_impl(
            &mut ev.processes,
            &mut ev.buffers,
            &ev.threads,
            None,
            ConnectionProcessCodingVariables::unbound(),
            vec![
                Value::keyword(":name"),
                Value::string("module-service-idle"),
                Value::keyword(":buffer"),
                Value::make_buffer(buffer_id),
            ],
        )
        .expect("make-pipe-process");
        let id = process.as_process_id().expect("pipe process id");
        let fd = ev
            .processes
            .open_channel_for_module(process)
            .expect("open module channel");
        fd_tx.send(fd).expect("send module fd");

        let request = ProcessOutputServiceRequest::target_only(id);
        let idle = ev
            .poll_process_output_for_service_request(&request)
            .expect("idle service pass");
        idle_tx
            .send(idle.has_target_process_activity())
            .expect("send idle result");

        write_rx.recv().expect("wait for module write");
        let data = ev
            .poll_process_output_for_service_request(&request)
            .expect("data service pass");
        data_tx
            .send((
                data.has_target_process_activity(),
                ev.buffers
                    .get(buffer_id)
                    .expect("module service buffer")
                    .buffer_string(),
            ))
            .expect("send data result");
    });

    let fd = fd_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("module fd should be published");
    assert!(
        !idle_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("idle service pass must return promptly"),
        "idle module pipe must not report output"
    );

    let payload = b"module-service-data";
    unsafe extern "C" {
        fn _write(fd: std::ffi::c_int, buffer: *const u8, count: u32) -> std::ffi::c_int;
        fn _close(fd: std::ffi::c_int) -> std::ffi::c_int;
    }
    assert_eq!(
        unsafe { _write(fd, payload.as_ptr(), payload.len() as u32) },
        payload.len() as std::ffi::c_int
    );
    write_tx.send(()).expect("release data service pass");

    let (activity, output) = data_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("written module data should be serviced");
    assert!(activity);
    assert_eq!(output, "module-service-data");
    assert_eq!(unsafe { _close(fd) }, 0);
    worker.join().expect("service worker");
}

#[test]
fn module_channel_writes_to_make_pipe_process() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut processes = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let process = builtin_make_pipe_process_impl(
        &mut processes,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("module-channel"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");
    let id = process.as_process_id().expect("pipe process id");

    let fd = processes
        .open_channel_for_module(process)
        .expect("open module channel");
    let payload = b"module-event";
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::write(fd, payload.as_ptr().cast(), payload.len()),
            payload.len() as isize
        );
        assert_eq!(libc::close(fd), 0);
    }
    #[cfg(windows)]
    unsafe {
        unsafe extern "C" {
            fn _write(fd: std::ffi::c_int, buffer: *const u8, count: u32) -> std::ffi::c_int;
            fn _close(fd: std::ffi::c_int) -> std::ffi::c_int;
        }

        assert_eq!(
            _write(fd, payload.as_ptr(), payload.len() as u32),
            payload.len() as std::ffi::c_int
        );
        assert_eq!(_close(fd), 0);
    }

    let coding_systems = crate::emacs_core::coding::CodingSystemManager::new();
    let read = processes
        .read_process_output_without_decoding(
            id,
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        )
        .expect("read module event");
    assert_eq!(read.undecoded_bytes(), payload);

    let second_fd = processes
        .open_channel_for_module(process)
        .expect("open second module channel");
    let second_payload = b"module-event-again";
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::write(
                second_fd,
                second_payload.as_ptr().cast(),
                second_payload.len()
            ),
            second_payload.len() as isize
        );
        assert_eq!(libc::close(second_fd), 0);
    }
    #[cfg(windows)]
    unsafe {
        unsafe extern "C" {
            fn _write(fd: std::ffi::c_int, buffer: *const u8, count: u32) -> std::ffi::c_int;
            fn _close(fd: std::ffi::c_int) -> std::ffi::c_int;
        }

        assert_eq!(
            _write(
                second_fd,
                second_payload.as_ptr(),
                second_payload.len() as u32
            ),
            second_payload.len() as std::ffi::c_int
        );
        assert_eq!(_close(second_fd), 0);
    }

    let second_read = processes
        .read_process_output_without_decoding(
            id,
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        )
        .expect("read second module event");
    assert_eq!(second_read.undecoded_bytes(), second_payload);
}

#[test]
fn stderr_pipe_writer_is_restored_after_pipe_spawn_failure() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut processes = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut processes,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("pipe-spawn-failure-stderr"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");
    let result = builtin_make_process_impl(
        &mut processes,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("pipe-spawn-failure-owner"),
            Value::keyword(":command"),
            Value::list(vec![Value::string("neomacs-program-that-does-not-exist")]),
            Value::keyword(":stderr"),
            stderrproc,
            Value::keyword(":connection-type"),
            Value::symbol("pipe"),
        ],
        false,
    );
    assert!(result.is_err());

    let fd = processes
        .open_channel_for_module(stderrproc)
        .expect("stderr pipe writer restored");
    let payload = b"pipe-spawn-failure";
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::write(fd, payload.as_ptr().cast(), payload.len()),
            payload.len() as isize
        );
        assert_eq!(libc::close(fd), 0);
    }
    #[cfg(windows)]
    unsafe {
        unsafe extern "C" {
            fn _write(fd: std::ffi::c_int, buffer: *const u8, count: u32) -> std::ffi::c_int;
            fn _close(fd: std::ffi::c_int) -> std::ffi::c_int;
        }

        assert_eq!(
            _write(fd, payload.as_ptr(), payload.len() as u32),
            payload.len() as std::ffi::c_int
        );
        assert_eq!(_close(fd), 0);
    }
    let coding_systems = crate::emacs_core::coding::CodingSystemManager::new();
    let read = processes
        .read_process_output_without_decoding(
            stderrproc.as_process_id().expect("stderr pipe id"),
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        )
        .expect("read restored stderr pipe");
    assert_eq!(read.undecoded_bytes(), payload);
}

#[cfg(unix)]
#[test]
fn stderr_pipe_writer_is_restored_after_pty_spawn_failure() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let mut processes = ProcessManager::new();
    let threads = crate::emacs_core::threads::ThreadManager::new();
    let stderrproc = builtin_make_pipe_process_impl(
        &mut processes,
        &mut buffers,
        &threads,
        None,
        ConnectionProcessCodingVariables::unbound(),
        vec![
            Value::keyword(":name"),
            Value::string("pty-spawn-failure-stderr"),
            Value::keyword(":buffer"),
            Value::NIL,
        ],
    )
    .expect("make-pipe-process");
    let result = builtin_make_process_impl(
        &mut processes,
        &mut buffers,
        &threads,
        vec![
            Value::keyword(":name"),
            Value::string("pty-spawn-failure-owner"),
            Value::keyword(":command"),
            Value::list(vec![Value::string("neomacs-program-that-does-not-exist")]),
            Value::keyword(":stderr"),
            stderrproc,
        ],
        true,
    );
    assert!(result.is_err());

    let fd = processes
        .open_channel_for_module(stderrproc)
        .expect("stderr pipe writer restored");
    let payload = b"pty-spawn-failure";
    unsafe {
        assert_eq!(
            libc::write(fd, payload.as_ptr().cast(), payload.len()),
            payload.len() as isize
        );
        assert_eq!(libc::close(fd), 0);
    }
    let coding_systems = crate::emacs_core::coding::CodingSystemManager::new();
    let read = processes
        .read_process_output_without_decoding(
            stderrproc.as_process_id().expect("stderr pipe id"),
            ProcessOutputDestination::to_filter(),
            &coding_systems,
        )
        .expect("read restored stderr pipe");
    assert_eq!(read.undecoded_bytes(), payload);
}

#[test]
fn builtin_process_command_uses_value_slot() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let real_id = pm.create_process(
        "cmd-proc".into(),
        Value::NIL,
        "/bin/echo".into(),
        vec!["hello".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let pipe_id = pm.create_process_with_kind(
        "pipe-proc".into(),
        Value::NIL,
        String::new(),
        vec![],
        ProcessKindWithoutDevice::Pipe,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    let command =
        builtin_process_command_impl(&pm, vec![Value::make_process(real_id)]).expect("command");
    assert_eq!(
        command,
        Value::list(vec![Value::string("/bin/echo"), Value::string("hello")])
    );

    let pipe_command =
        builtin_process_command_impl(&pm, vec![Value::make_process(pipe_id)]).expect("command");
    assert!(pipe_command.is_nil());
}

#[test]
fn process_manager_list() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let id1 = pm.create_process(
        "a".into(),
        Value::NIL,
        "p".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let id2 = pm.create_process(
        "b".into(),
        Value::NIL,
        "q".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let ids = pm.list_processes();
    // Newest-first, like GNU's `process-list` (front-insertion `Vprocess_alist`).
    assert_eq!(ids, vec![id2, id1]);
}

#[test]
fn process_manager_env() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    pm.setenv(
        LispString::from_utf8("NEOVM_TEST_VAR"),
        Some(LispString::from_utf8("hello")),
    );
    assert_eq!(
        pm.getenv("NEOVM_TEST_VAR"),
        Some(LispString::from_utf8("hello"))
    );
    pm.setenv(LispString::from_utf8("NEOVM_TEST_VAR"), None);
    assert_eq!(pm.getenv("NEOVM_TEST_VAR"), None);
}

// -- Elisp-level tests --------------------------------------------------

/// `start-process` returns a process object that answers its accessors.
///
/// **The child is long-running on purpose (ledger 180).**  This used to run
/// `echo hello`, whose child is gone before the second form is evaluated, and
/// asserted `run` for its status -- which was only ever true because this port
/// could not see a child exit without waiting.  Measured, `-Q --batch`, six
/// runs of `start-process` + a 2 ms pure-Lisp spin + `process-status`:
///
/// ```text
///                        immediate   after 2 ms
///   GNU 31.0.90            run         exit      (6/6)
///   Neomacs, merge base    run         run       (6/6)
///   Neomacs, now           run         exit      (5/6)
/// ```
///
/// So GNU answers `exit` there and the old expectation was the divergence,
/// not the fix.  A child that is still alive makes `run` the right answer in
/// both editors and takes the race out of the test, which is what this
/// asserts.
#[test]
fn start_process_and_query() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let results = eval_all(&format!(
        r#"(processp (start-process "my-proc" nil "{sh}" "-c" "sleep 30"))
           (process-status (get-process "my-proc"))
           (process-name (get-process "my-proc"))
           (prog1 (process-buffer (get-process "my-proc"))
             (set-process-query-on-exit-flag (get-process "my-proc") nil)
             (delete-process "my-proc"))"#,
    ));
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK run");
    assert_eq!(results[2], r#"OK "my-proc""#);
    assert_eq!(results[3], "OK nil");
}

#[test]
fn start_process_with_buffer() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(start-process "p" "*output*" "{cat}")
           (bufferp (process-buffer (get-process "p")))
           (equal (buffer-name (process-buffer (get-process "p"))) "*output*")"#,
    ));
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
}

#[test]
fn start_process_missing_absolute_program_defers_exit_127() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((log nil))
             (let ((p (start-process
                       "neo-start-missing-absolute"
                       nil
                       "/nonexistent/neomacs-start-process-missing")))
               (set-process-sentinel
                p
                (lambda (proc event)
                  (push (list event
                              (process-status proc)
                              (process-exit-status proc))
                        log)))
               (let ((i 0))
                 (while (and (< i 20) (null log))
                   (accept-process-output nil 0.05)
                   (setq i (1+ i))))
               (list (process-status p)
                     (process-exit-status p)
                     (nreverse log))))"#,
    );
    assert_eq!(
        result,
        "OK (exit 127 ((\"exited abnormally with code 127\n\" exit 127)))"
    );
}

#[test]
fn start_process_buffer_name_program_and_arg_contracts_match_oracle() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (let ((p (start-process "neo-sp-contract-buffer" (current-buffer) "{cat}")))
               (unwind-protect
                   (list (processp p)
                         (null (condition-case err (process-send-eof nil) (error err)))
                         (null (condition-case err (process-running-child-p nil) (error err))))
                 (ignore-errors (delete-process p)))))
           (condition-case err (start-process 'neo-sp-contract-name nil "{cat}") (error err))
           (condition-case err (start-process t nil "{cat}") (error err))
           (condition-case err (start-process nil nil "{cat}") (error err))
           (condition-case err (start-process "neo-sp-contract-buf-symbol" 'x "{cat}") (error err))
           (condition-case err (start-process "neo-sp-contract-buf-t" t "{cat}") (error err))
           (condition-case err (start-process "neo-sp-contract-buf-int" 1 "{cat}") (error err))
           (condition-case err (start-process "neo-sp-contract-prog-symbol" nil 'cat) (error err))
           (condition-case err (start-process "neo-sp-contract-prog-t" nil t) (error err))
           (processp (start-process "neo-sp-contract-prog-nil" nil nil))
           (condition-case err (start-process "neo-sp-contract-arg-symbol" nil "{cat}" 'a) (error err))
           (condition-case err (start-process "neo-sp-contract-arg-t" nil "{cat}" t) (error err))
           (condition-case err (start-process "neo-sp-contract-arg-nil" nil "{cat}" nil) (error err))
           (condition-case err (start-process "neo-sp-contract-arg-int" nil "{cat}" 1) (error err))"#,
    ));
    assert_eq!(results[0], "OK (t t t)");
    assert_eq!(results[1], r#"OK (error ":name value not a string")"#);
    assert_eq!(results[2], r#"OK (error ":name value not a string")"#);
    assert_eq!(results[3], r#"OK (error ":name value not a string")"#);
    assert_eq!(results[4], "OK (wrong-type-argument stringp x)");
    assert_eq!(results[5], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[6], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[7], "OK (wrong-type-argument stringp cat)");
    assert_eq!(results[8], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[9], "OK t");
    assert_eq!(results[10], "OK (wrong-type-argument stringp a)");
    assert_eq!(results[11], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[12], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[13], "OK (wrong-type-argument stringp 1)");
}

#[test]
fn call_process_and_start_file_process_string_contracts_match_oracle() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let results = eval_all(&format!(
        r#"(condition-case err (call-process nil) (error err))
           (condition-case err (call-process t) (error err))
           (condition-case err (call-process 'foo) (error err))
           (condition-case err (call-process "{echo}" nil nil nil 'x) (error err))
           (condition-case err (call-process "{echo}" nil nil nil t) (error err))
           (condition-case err (call-process "{echo}" nil nil nil nil) (error err))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) nil) (error err)))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) t) (error err)))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) 'foo) (error err)))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) "{echo}" nil nil nil 'x) (error err)))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) "{echo}" nil nil nil t) (error err)))
           (with-temp-buffer
             (insert "x")
             (condition-case err (call-process-region (point-min) (point-min) "{echo}" nil nil nil nil) (error err)))
           (condition-case err (start-file-process "neo-sfp-contract-arg-symbol" nil "{echo}" 'x) (error err))
           (condition-case err (start-file-process "neo-sfp-contract-arg-t" nil "{echo}" t) (error err))
           (condition-case err (start-file-process "neo-sfp-contract-arg-nil" nil "{echo}" nil) (error err))
           (condition-case err (start-file-process "neo-sfp-contract-program-symbol" nil 'echo) (error err))
           (condition-case err (start-file-process "neo-sfp-contract-program-t" nil t) (error err))
           (let ((p (start-file-process "neo-sfp-contract-program-nil" nil nil)))
             (unwind-protect (processp p) (ignore-errors (delete-process p))))"#,
    ));

    assert_eq!(results[0], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[1], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[2], "OK (wrong-type-argument stringp foo)");
    assert_eq!(results[3], "OK (wrong-type-argument stringp x)");
    assert_eq!(results[4], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[5], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[6], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[7], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[8], "OK (wrong-type-argument stringp foo)");
    assert_eq!(results[9], "OK (wrong-type-argument stringp x)");
    assert_eq!(results[10], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[11], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[12], "OK (wrong-type-argument stringp x)");
    assert_eq!(results[13], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[14], "OK (wrong-type-argument stringp nil)");
    assert_eq!(results[15], "OK (wrong-type-argument stringp echo)");
    assert_eq!(results[16], "OK (wrong-type-argument stringp t)");
    assert_eq!(results[17], "OK t");
}

/// GNU `call-process` resolves the program via `openp` and, on failure,
/// signals through `report_file_error ("Searching for program", PROG)`
/// (callproc.c:526), which `get_file_errno_data` (fileio.c) turns into the
/// triple `(SYMBOL "Searching for program" STRERROR PROG)`. The SYMBOL and the
/// libc `strerror` middle element are both derived from the errno `openp`
/// recorded: ENOENT -> `file-missing` "No such file or directory", EISDIR ->
/// `file-error` "Is a directory", EACCES -> `permission-denied`
/// "Permission denied". neomacs previously dropped the strerror element and
/// mis-shaped the directory/permission cases (a bare `error`/raw os-error
/// string). Verified against `/usr/bin/emacs --batch`.
#[test]
fn call_process_program_resolution_errors_match_gnu_report_file_error() {
    crate::test_utils::init_test_tracing();

    // EISDIR: an absolute directory. Create our own so the test is hermetic.
    let dir = tmp_dir("call-process-isdir");
    // EACCES: a non-executable regular file looked up via exec-path.
    let noexec_dir = tmp_dir("call-process-noexec");
    let noexec = format!("{noexec_dir}/noexecprog");
    std::fs::write(&noexec, b"#!/bin/sh\n").expect("write noexec file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&noexec, std::fs::Permissions::from_mode(0o644))
            .expect("chmod 0644");
    }

    let results = eval_all(&format!(
        r#"(condition-case e (call-process "/nonexistent/program/xyzzy" nil nil nil) (error e))
           (condition-case e (call-process "{dir}") (error e))
           (condition-case e (call-process "") (error e))
           (let ((exec-path (list "{noexec_dir}")))
             (condition-case e (call-process "noexecprog") (error e)))"#,
    ));

    // ENOENT: missing program — file-missing with the strerror middle element.
    assert_eq!(
        results[0],
        r#"OK (file-missing "Searching for program" "No such file or directory" "/nonexistent/program/xyzzy")"#
    );
    // EISDIR: an absolute directory is not runnable — file-error "Is a directory".
    assert_eq!(
        results[1],
        format!(r#"OK (file-error "Searching for program" "Is a directory" "{dir}")"#)
    );
    // Empty program name expands to default-directory (a directory) -> EISDIR.
    assert_eq!(
        results[2],
        r#"OK (file-error "Searching for program" "Is a directory" "")"#
    );
    // EACCES: a found-but-unrunnable file -> permission-denied "Permission denied".
    assert_eq!(
        results[3],
        r#"OK (permission-denied "Searching for program" "Permission denied" "noexecprog")"#
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&noexec_dir);
}

#[cfg(unix)]
#[test]
fn call_process_lookup_expands_home_relative_program_like_gnu() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let home = tmp_dir("call-process-home-relative-program");
    let bin = format!("{home}/bin");
    std::fs::create_dir_all(&bin).expect("create fake home bin directory");
    let script = format!("{bin}/consult-fd-probe");
    std::fs::write(&script, "#!/bin/sh\nprintf 'home-relative-ok\\n'\n")
        .expect("write home-relative executable");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod executable");

    let result = eval_one(&format!(
        r#"(let ((process-environment (copy-sequence process-environment))
                  (exec-path '("/neomacs-unrelated-exec-path")))
             (setenv "HOME" "{home}")
             (with-temp-buffer
               (let ((status (call-process "~/bin/consult-fd-probe" nil t)))
                 (list status
                       (not (null (string-match-p
                                   "home-relative-ok"
                                   (buffer-string))))))))"#
    ));

    assert_eq!(result, "OK (0 t)");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn delete_process_removes() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let results = eval_all(&format!(
        r#"(start-process "p" nil "{echo}")
           (delete-process (get-process "p"))
           (process-list)"#,
    ));
    assert_eq!(results[2], "OK nil");
}

#[test]
fn process_send_string_test() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(start-process "p" nil "{cat}")
           (process-send-string (get-process "p") "hello")"#,
    ));
    assert_eq!(results[1], "OK nil");
}

#[test]
fn process_send_string_reenters_wait_and_runs_filter_when_write_blocks() {
    crate::test_utils::init_test_tracing();
    let python = find_bin("python3");
    let result = eval_one(&format!(
        r#"(let* ((script (concat
                          "import os, sys\n"
                          "os.write(1, b'O' * 262144)\n"
                          "sys.stdout.flush()\n"
                          "sys.stdin.buffer.read()\n"))
                  (events nil)
                  (p nil))
             (unwind-protect
                 (progn
                   (setq p
                         (make-process
                          :name "send-reentrant-unit"
                          :buffer nil
                          :connection-type 'pipe
                          :command (list "{python}" "-c" script)
                          :filter (lambda (_ string)
                                    (push (length string) events))))
                   (process-send-string p (make-string 262144 ?x))
                   (list (> (apply #'+ events) 0)
                         (apply #'+ events)))
               (when p
                 (ignore-errors
                   (delete-process p)))))"#
    ));
    assert_eq!(result, "OK (t 262144)");
}

#[test]
fn process_exit_status_initial() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let results = eval_all(&format!(
        r#"(start-process "p" nil "{echo}")
           (process-exit-status (get-process "p"))"#,
    ));
    assert_eq!(results[1], "OK 0");
}

#[test]
fn pty_process_output_does_not_translate_lf_to_crlf_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut processes = ProcessManager::new();
    let pid = processes.create_process_lisp(
        LispString::from_utf8("pty-lf"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![
            LispString::from_utf8("-c"),
            LispString::from_utf8("printf 'x\n'"),
        ],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(pid, true).expect("spawn PTY process");

    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // The bytes, not the text: what this test is about is whether the PTY put a
    // CR on the wire, and decoding them would only add a second question.  A
    // `ProcessManager` driven on its own cannot decode anyway -- the decoder is
    // `decode_coding_object`, which evaluates Lisp -- so the read hands back an
    // undecoded run and the fixture says so.
    let mut output: Vec<u8> = Vec::new();
    while std::time::Instant::now() < deadline && !output.contains(&b'\n') {
        if let Some(run) = processes.read_process_output_without_decoding(
            pid,
            ProcessOutputDestination::to_filter(),
            &crate::emacs_core::coding::CodingSystemManager::new(),
        ) {
            output.extend_from_slice(run.undecoded_bytes());
        }
        processes.check_child_status_change(pid);
        std::thread::sleep(Duration::from_millis(10));
    }
    processes.kill_process(pid);

    assert_eq!(output.as_slice(), b"x\n");
}

#[cfg(target_os = "linux")]
fn open_ptmx_descriptor_count() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .expect("read process descriptor table")
        .filter_map(Result::ok)
        .filter(|entry| {
            std::fs::read_link(entry.path())
                .is_ok_and(|target| target == std::path::Path::new("/dev/ptmx"))
        })
        .count()
}

#[cfg(target_os = "linux")]
#[test]
fn reaped_pty_process_releases_live_io_but_preserves_stale_status() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut ev = Context::new();
    let baseline_ptmx = open_ptmx_descriptor_count();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((proc (make-process
                            :name "reaped-pty-resource-owner"
                            :command (list "{sh}" "-c" "exit 0")
                            :connection-type 'pty)))
                 (while (eq (process-status proc) 'run)
                   (accept-process-output proc 0.1))
                 (list (process-status proc)
                       (get-process "reaped-pty-resource-owner")))"#
        ),
    );

    assert_eq!(result, "OK (exit nil)");
    assert_eq!(
        open_ptmx_descriptor_count(),
        baseline_ptmx,
        "reaping must deactivate and release the process's live PTY I/O"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn terminal_pty_process_releases_live_io_when_exit_record_is_retained() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut ev = Context::new();
    let baseline_ptmx = open_ptmx_descriptor_count();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((delete-exited-processes nil)
                     (proc (make-process
                            :name "retained-terminal-pty-resource-owner"
                            :command (list "{sh}" "-c" "exit 0")
                            :connection-type 'pty)))
                 (while (eq (process-status proc) 'run)
                   (accept-process-output proc 0.1))
                 (list (process-status proc)
                       (eq proc
                           (get-process "retained-terminal-pty-resource-owner"))))"#
        ),
    );

    assert_eq!(result, "OK (exit t)");
    assert_eq!(
        open_ptmx_descriptor_count(),
        baseline_ptmx,
        "terminal status must deactivate live PTY I/O even when the Lisp record is retained"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn deleted_pty_process_releases_live_io_and_reaps_child() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut processes = ProcessManager::new();
    let baseline_ptmx = open_ptmx_descriptor_count();
    let id = processes.create_process(
        "deleted-pty-resource-owner".into(),
        Value::NIL,
        sh,
        vec!["-c".into(), "sleep 300".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(id, true).expect("spawn PTY process");
    let os_pid = processes
        .get(id)
        .and_then(|process| process.os_pid)
        .expect("spawned child pid");

    assert!(processes.delete_process(id));
    assert!(processes.get(id).is_none());
    assert_eq!(
        processes.process_status_any(id),
        Some(&process_status_signal_value(signal_kill_number()))
    );
    assert_eq!(
        open_ptmx_descriptor_count(),
        baseline_ptmx,
        "deleting must deactivate and release the process's live PTY I/O"
    );
    assert!(
        !std::path::Path::new(&format!("/proc/{os_pid}")).exists(),
        "delete-process must wait for the killed child instead of retaining a zombie"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn dropping_process_manager_terminates_and_reaps_live_child() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut processes = ProcessManager::new();
    let id = processes.create_process(
        "dropped-process-manager-child".into(),
        Value::NIL,
        sh,
        vec!["-c".into(), "sleep 300".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(id, false).expect("spawn pipe child");
    let os_pid = processes
        .get(id)
        .and_then(|process| process.os_pid)
        .expect("spawned child pid");

    drop(processes);

    let child_survived = std::path::Path::new(&format!("/proc/{os_pid}")).exists();
    if child_survived {
        let _ = sys::send_signal_to_group(os_pid as i64, signal_kill_number());
    }
    assert!(
        !child_survived,
        "dropping the live-resource owner must terminate and reap its child"
    );
}

#[test]
fn process_list_test() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(start-process "a" nil "{echo}")
           (start-process "b" nil "{cat}")
           (process-list)"#,
    ));
    // Process list contains two entries.  Order may vary.  Processes are now
    // first-class objects that print as `#<process NAME>` (matching GNU), so
    // the list shows the two process names rather than bare integer ids.
    let list_str = &results[2];
    assert!(list_str.contains("#<process a>"));
    assert!(list_str.contains("#<process b>"));
}

#[test]
fn call_process_echo() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    // call-process with echo, inserting into current buffer
    let results = eval_all(&format!(
        r#"(get-buffer-create "cp-test")
           (set-buffer "cp-test")
           (call-process "{echo}" nil t nil "hello" "world")
           (buffer-string)"#,
    ));
    // Exit code should be 0.
    assert_eq!(results[2], "OK 0");
    // Buffer should contain "hello world\n".
    assert_eq!(results[3], "OK \"hello world\n\"");
}

#[test]
fn call_process_no_destination() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    // call-process with nil destination discards output
    let results = eval_all(&format!(
        r#"(get-buffer-create "cp-nil")
           (set-buffer "cp-nil")
           (call-process "{echo}" nil nil nil "hello")
           (buffer-string)"#,
    ));
    assert_eq!(results[2], "OK 0");
    assert_eq!(results[3], r#"OK """#);
}

#[test]
fn call_process_display_requests_redisplay_after_buffer_insert() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer("*cp-display*");
    assert!(ev.buffers.switch_current(buf_id));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let current_id = ev.buffers.current_buffer_id().expect("current buffer");
        calls_in_cb.borrow_mut().push(
            ev.buffers
                .get(current_id)
                .expect("current buffer")
                .buffer_string(),
        );
    }));

    crate::emacs_core::callproc::builtin_call_process(
        &mut ev,
        vec![
            Value::string(echo),
            Value::NIL,
            Value::T,
            Value::T,
            Value::string("hello"),
        ],
    )
    .expect("call-process should succeed");

    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("display buffer")
            .buffer_string(),
        "hello\n"
    );
    assert_eq!(*redisplay_calls.borrow(), vec!["hello\n".to_string()]);
}

#[test]
fn call_process_infile_feeds_stdin() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let infile = tmp_file("cp-infile");
    std::fs::write(&infile, "infile-data").expect("write infile");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (list
               (call-process "{cat}" "{infile}" t nil)
               (buffer-string)))"#
    ));
    assert_eq!(results[0], r#"OK (0 "infile-data")"#);
    let _ = std::fs::remove_file(&infile);
}

#[test]
fn call_process_destination_buffer_name_inserts_there() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let results = eval_all(&format!(
        r#"(get-buffer-create "cp-src")
           (get-buffer-create "cp-dst")
           (set-buffer "cp-src")
           (erase-buffer)
           (set-buffer "cp-dst")
           (erase-buffer)
           (set-buffer "cp-src")
           (call-process "{echo}" nil "cp-dst" nil "hello")
           (list
             (with-current-buffer "cp-src" (buffer-string))
             (with-current-buffer "cp-dst" (buffer-string)))"#,
    ));
    assert_eq!(results[7], "OK 0");
    assert_eq!(results[8], "OK (\"\" \"hello\n\")");
}

#[test]
fn call_process_file_destination_collects_stdout_and_stderr() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let out = tmp_file("cp-file");
    let _ = std::fs::remove_file(&out);
    let results = eval_all(&format!(
        r#"(call-process "{sh}" nil '(:file "{out}") nil "-c" "echo out; echo err >&2")
           (with-temp-buffer (insert-file-contents "{out}") (buffer-string))"#
    ));
    assert_eq!(results[0], "OK 0");
    assert!(results[1].contains("out"));
    assert!(results[1].contains("err"));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn call_process_pair_destination_splits_stderr_to_file() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let out = tmp_file("cp-pair-out");
    let err = tmp_file("cp-pair-err");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&err);
    let results = eval_all(&format!(
        r#"(call-process "{sh}" nil '((:file "{out}") "{err}") nil "-c" "echo out; echo err >&2")
           (with-temp-buffer (insert-file-contents "{out}") (buffer-string))
           (with-temp-buffer (insert-file-contents "{err}") (buffer-string))"#
    ));
    assert_eq!(results[0], "OK 0");
    assert!(results[1].contains("out"));
    assert!(!results[1].contains("err"));
    assert!(results[2].contains("err"));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&err);
}

#[test]
fn call_process_dotted_destination_ignores_non_list_stderr_tail_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(with-temp-buffer
             (let ((stderr-buffer (get-buffer-create "*dotted-stderr*")))
               (unwind-protect
                   (list
                    (call-process
                     "{sh}" nil (cons (current-buffer) stderr-buffer) nil
                     "-c" "printf out; printf err >&2")
                    (buffer-string)
                    (with-current-buffer stderr-buffer (buffer-string)))
                 (kill-buffer stderr-buffer))))"#
    ));
    assert_eq!(result, r#"OK (0 "outerr" "")"#);
}

#[test]
fn call_process_integer_destination_returns_nil() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    // Any integer destination behaves like 0: discard and return nil.
    let results = eval_all(&format!(
        r#"(get-buffer-create "cp-int")
           (set-buffer "cp-int")
           (call-process "{echo}" nil 2 nil "hello")
           (buffer-string)"#,
    ));
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], r#"OK """#);
}

#[test]
fn call_process_false() {
    crate::test_utils::init_test_tracing();
    let false_bin = find_bin("false");
    // false exits with code 1
    let result = eval_one(&format!(r#"(call-process "{false_bin}")"#));
    assert_eq!(result, "OK 1");
}

#[test]
fn call_process_shell_command_legacy_args_match_gnu_mapconcat_behavior() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("*call-process-shell-command*");
    let result = builtin_call_process_shell_command(
        &mut eval,
        vec![
            Value::string("printf %s"),
            Value::NIL,
            Value::make_buffer(buffer_id),
            Value::NIL,
            Value::string("a b"),
        ],
    )
    .expect("call-process-shell-command");

    assert_eq!(result.as_fixnum(), Some(0));
    assert_eq!(
        eval.buffers.get(buffer_id).expect("buffer").buffer_string(),
        "ab"
    );
}

#[test]
fn process_file_shell_command_legacy_args_match_gnu_mapconcat_behavior() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("*process-file-shell-command*");
    let result = builtin_process_file_shell_command(
        &mut eval,
        vec![
            Value::string("printf %s"),
            Value::NIL,
            Value::make_buffer(buffer_id),
            Value::NIL,
            Value::string("a b"),
        ],
    )
    .expect("process-file-shell-command");

    assert_eq!(result.as_fixnum(), Some(0));
    assert_eq!(
        eval.buffers.get(buffer_id).expect("buffer").buffer_string(),
        "ab"
    );
}

#[test]
fn call_process_region_test() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(get-buffer-create "cpr-test")
           (set-buffer "cpr-test")
           (insert "hello world")
           (call-process-region 1 12 "{cat}" nil t)
           (buffer-string)"#,
    ));
    // exit code 0
    assert_eq!(results[3], "OK 0");
    // Buffer should contain original text plus piped output
    assert!(results[4].contains("hello world"));
}

#[test]
fn call_process_region_display_requests_redisplay_after_buffer_insert() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer("*cpr-display*");
    assert!(ev.buffers.switch_current(buf_id));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let current_id = ev.buffers.current_buffer_id().expect("current buffer");
        calls_in_cb.borrow_mut().push(
            ev.buffers
                .get(current_id)
                .expect("current buffer")
                .buffer_string(),
        );
    }));

    crate::emacs_core::callproc::builtin_call_process_region(
        &mut ev,
        vec![
            Value::string("xyz"),
            Value::NIL,
            Value::string(cat),
            Value::NIL,
            Value::T,
            Value::T,
        ],
    )
    .expect("call-process-region should succeed");

    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("display buffer")
            .buffer_string(),
        "xyz"
    );
    assert_eq!(*redisplay_calls.borrow(), vec!["xyz".to_string()]);
}

#[test]
fn call_process_region_destination_buffer_name_inserts_there() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(get-buffer-create "cpr-src")
           (get-buffer-create "cpr-dst")
           (with-current-buffer "cpr-src" (erase-buffer) (insert "abc"))
           (with-current-buffer "cpr-dst" (erase-buffer))
           (with-current-buffer "cpr-src"
             (call-process-region (point-min) (point-max) "{cat}" nil "cpr-dst" nil))
           (list
             (with-current-buffer "cpr-src" (buffer-string))
             (with-current-buffer "cpr-dst" (buffer-string)))"#,
    ));
    assert_eq!(results[4], "OK 0");
    assert_eq!(results[5], r#"OK ("abc" "abc")"#);
}

#[test]
fn call_process_region_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = tmp_dir("cpr-cwd");
    let mut eval = Context::new();
    let buf_id = eval.buffers.create_buffer("cpr-cwd");
    assert!(eval.buffers.switch_current(buf_id));
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .set_buffer_local("default-directory", Value::string(format!("{dir}/")));

    let shell = find_bin("sh");
    let result = crate::emacs_core::callproc::builtin_call_process_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(shell),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::string("-c"),
            Value::string("pwd"),
        ],
    )
    .expect("call-process-region should succeed");

    assert_eq!(result.as_fixnum(), Some(0));
    assert_eq!(
        eval.buffers.get(buf_id).expect("buffer").buffer_string(),
        format!("{dir}\n")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_process_region_file_destination_writes_file() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let out = tmp_file("cpr-file");
    let _ = std::fs::remove_file(&out);
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (call-process-region (point-min) (point-max) "{cat}" nil '(:file "{out}") nil))
           (with-temp-buffer (insert-file-contents "{out}") (buffer-string))"#
    ));
    assert_eq!(results[0], "OK 0");
    assert_eq!(results[1], r#"OK "abc""#);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn call_process_region_start_nil_uses_whole_buffer() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (list (call-process-region nil nil "{cat}" nil t nil)
                   (buffer-string)))"#
    ));
    assert_eq!(results[0], r#"OK (0 "abcabc")"#);
}

#[test]
fn call_process_region_start_string_uses_string_input() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (list (call-process-region "xyz" nil "{cat}" nil t nil)
                   (buffer-string)))"#
    ));
    assert_eq!(results[0], r#"OK (0 "abcxyz")"#);
}

#[test]
fn call_process_region_start_string_with_delete_signals_wrong_type() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(condition-case err
               (call-process-region "xyz" nil "{cat}" t t nil)
             (error (car err)))"#
    ));
    assert_eq!(result, "OK wrong-type-argument");
}

#[test]
fn call_process_region_accepts_marker_positions() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abcdef")
             (goto-char 3)
             (let ((m (copy-marker (point))))
               (list (call-process-region m (point-max) "{cat}" nil t nil)
                     (buffer-string))))"#
    ));
    assert_eq!(results[0], r#"OK (0 "abcdefcdef")"#);
}

#[test]
fn call_process_region_reversed_bounds_are_accepted() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (list (call-process-region (point-max) (point-min) "{cat}" nil t nil)
                   (buffer-string)))"#
    ));
    assert_eq!(results[0], r#"OK (0 "abcabc")"#);
}

#[test]
fn call_process_region_reversed_bounds_with_delete_delete_region() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (list (call-process-region (point-max) (point-min) "{cat}" t t nil)
                   (buffer-string)))"#
    ));
    assert_eq!(results[0], r#"OK (0 "abc")"#);
}

#[test]
fn call_process_region_negative_start_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (condition-case err
                 (call-process-region -1 2 "{cat}" nil t nil)
               (error (car err))))"#
    ));
    assert_eq!(result, "OK args-out-of-range");
}

#[test]
fn call_process_region_huge_end_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(with-temp-buffer
             (insert "abc")
             (condition-case err
                 (call-process-region 1 999999 "{cat}" nil t nil)
               (error (car err))))"#
    ));
    assert_eq!(result, "OK args-out-of-range");
}

#[test]
fn call_process_region_integer_destination_returns_nil() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(get-buffer-create "cpr-int")
           (set-buffer "cpr-int")
           (erase-buffer)
           (insert "abc")
           (call-process-region 1 4 "{cat}" nil 3 nil)
           (buffer-string)"#,
    ));
    assert_eq!(results[4], "OK nil");
    assert_eq!(results[5], r#"OK "abc""#);
}

#[test]
fn shell_command_to_string_test() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(shell-command-to-string "echo -n hello")"#);
    assert_eq!(result, r#"OK "hello""#);
}

#[test]
fn shell_command_to_string_with_pipe() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(shell-command-to-string "echo hello | tr a-z A-Z")"#);
    assert_eq!(result, "OK \"HELLO\n\"");
}

#[test]
fn getenv_path() {
    crate::test_utils::init_test_tracing();
    // PATH should always be set — use getenv-internal (C builtin)
    let result = eval_one(r#"(getenv-internal "PATH")"#);
    assert!(result.starts_with("OK \""));
}

#[test]
fn getenv_nonexistent() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(getenv-internal "NEOVM_DEFINITELY_NOT_SET_12345")"#);
    assert_eq!(result, "OK nil");
}

#[test]
fn getenv_missing_non_display_variable_does_not_escape_to_host_environment() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::string("NEOMACS_ENV_POLICY=present")]),
    );
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("PATH=/initial/path")]),
    );

    let result = builtin_getenv_internal(&mut eval, vec![Value::string("PATH")])
        .expect("getenv-internal should succeed");

    assert_eq!(
        result,
        Value::NIL,
        "GNU only consults the native OS environment for its Windows-specific environment repairs"
    );
}

#[test]
fn getenv_display_prefers_selected_frame_then_initial_snapshot() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("display"), Value::string(":frame"));
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::string("NEOMACS_ENV_POLICY=present")]),
    );
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );

    let frame_display = builtin_getenv_internal(&mut eval, vec![Value::string("DISPLAY")])
        .expect("frame DISPLAY lookup");
    assert_eq!(frame_display.as_utf8_str(), Some(":frame"));

    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .remove_parameter(Value::symbol("display"));
    let initial_display = builtin_getenv_internal(&mut eval, vec![Value::string("DISPLAY")])
        .expect("initial DISPLAY lookup");
    assert_eq!(initial_display.as_utf8_str(), Some(":initial"));
}

#[test]
fn getenv_display_on_wayland_uses_initial_x_display_not_native_display() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_display_identity(crate::window::FrameDisplayIdentity::wayland("wayland-7"));
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::string("NEOMACS_ENV_POLICY=present")]),
    );
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );

    let display =
        builtin_getenv_internal(&mut eval, vec![Value::string("DISPLAY")]).expect("DISPLAY lookup");
    assert_eq!(display.as_utf8_str(), Some(":initial"));
    let native_display = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.parameter("display"))
        .expect("native frame display");
    assert_eq!(native_display.as_utf8_str(), Some("wayland-7"));
}

#[test]
fn child_environment_on_wayland_uses_initial_x_display_not_native_display() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_display_identity(crate::window::FrameDisplayIdentity::wayland("wayland-7"));
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );
    let sh = find_bin("sh");

    let result = eval_one_in_context(
        &mut eval,
        &format!(
            r#"(let ((process-environment '("NEOMACS_ENV_POLICY=present")))
                 (with-temp-buffer
                   (call-process "{sh}" nil t nil
                                 "-c" "printf %s \"${{DISPLAY-}}\"")
                   (buffer-string)))"#
        ),
    );

    assert_eq!(result, r#"OK ":initial""#);
}

#[test]
fn getenv_explicit_negative_display_suppresses_adaptive_fallback() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("display"), Value::string(":frame"));
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::string("DISPLAY")]),
    );
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );

    let result = builtin_getenv_internal(&mut eval, vec![Value::string("DISPLAY")])
        .expect("negative DISPLAY lookup");

    assert_eq!(result, Value::NIL);
}

#[test]
fn call_process_materializes_adaptive_display_and_honors_explicit_policy() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("display"), Value::string(":frame"));
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );
    let sh = find_bin("sh");
    let cat = find_bin("cat");

    let result = eval_one_in_context(
        &mut eval,
        &format!(
            r#"(let ((probe
                      (lambda (environment)
                        (let ((process-environment environment))
                          (with-temp-buffer
                            (call-process "{sh}" nil t nil
                                          "-c" "printf %s \"${{DISPLAY-}}\"")
                            (buffer-string))))))
                 (list (funcall probe '("NEOMACS_ENV_POLICY=present"))
                       (funcall probe '("DISPLAY" "NEOMACS_ENV_POLICY=present"))
                       (funcall probe '("DISPLAY=:explicit"
                                        "NEOMACS_ENV_POLICY=present"))
                       (let ((process-environment
                              '("NEOMACS_ENV_POLICY=present")))
                         (with-temp-buffer
                           (insert "input")
                           (call-process-region
                            (point-min) (point-max) "{sh}" t t nil
                            "-c" "\"{cat}\" >/dev/null; printf %s \"${{DISPLAY-}}\"")
                           (buffer-string)))))"#
        ),
    );

    assert_eq!(result, r#"OK (":frame" "" ":explicit" ":frame")"#);
}

fn async_child_display_probe(
    process_connection_type: &str,
    make_process_connection_type: &str,
) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("display"), Value::string(":frame"));
    eval.obarray.set_symbol_value(
        "initial-environment",
        Value::list(vec![Value::string("DISPLAY=:initial")]),
    );
    let sh = find_bin("sh");

    eval_one_in_context(
        &mut eval,
        &format!(
            r#"(let ((process-environment '("NEOMACS_ENV_POLICY=present"))
                     (output "")
                     (process-connection-type {process_connection_type})
                     (process nil))
                 (unwind-protect
                     (progn
                       (setq process
                             (make-process
                              :name "adaptive-display-probe"
                              :buffer nil
                              :connection-type {make_process_connection_type}
                              :command '("{sh}" "-c"
                                         "printf %s \"${{DISPLAY-}}\"")
                              :filter (lambda (_process chunk)
                                        (setq output (concat output chunk)))))
                       (while (process-live-p process)
                         (accept-process-output process 1))
                       (while (accept-process-output process 0))
                       output)
                   (when process
                     (ignore-errors (delete-process process)))))"#
        ),
    )
}

#[test]
fn make_process_pipe_uses_the_canonical_child_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(async_child_display_probe("nil", "'pipe"), r#"OK ":frame""#);
}

#[test]
fn process_output_read_errors_remain_distinct_from_clean_eof() {
    crate::test_utils::init_test_tracing();
    let mut processes = ProcessManager::new();
    let pid = processes.create_process_lisp(
        LispString::from_utf8("read-error-eof"),
        Value::NIL,
        LispString::from_utf8("read-error-eof"),
        Vec::new(),
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    let failed_read: std::io::Result<usize> = Err(std::io::Error::other("broken read source"));
    let result = process_output_read_from_io_result(
        processes.get_mut(pid).expect("created process"),
        &crate::emacs_core::coding::CodingSystemManager::new(),
        ProcessOutputDestination::to_filter(),
        ProcessReadOutcome::from_stream_read(&failed_read),
        &[],
        1,
    );
    assert!(matches!(result, ProcessBytesRead::Failed));

    processes.retire_process_at_read_failure(pid);
    let process = processes.get(pid).expect("failed process remains pending");
    assert_eq!(process.status, process_status_exit_value(256));
    assert_eq!(process.pending_status, process_status_exit_value(256));
    assert!(process.status_notify_pending);

    let mut pty_processes = ProcessManager::new();
    let pty_id = pty_processes.create_process_lisp(
        LispString::from_utf8("closed-pty"),
        Value::NIL,
        LispString::from_utf8("closed-pty"),
        Vec::new(),
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    let pty_result = process_output_read_from_io_result(
        pty_processes.get_mut(pty_id).expect("created pty process"),
        &crate::emacs_core::coding::CodingSystemManager::new(),
        ProcessOutputDestination::to_filter(),
        ProcessReadOutcome::from_pty_read(&Ok(0)),
        &[],
        1,
    );
    assert!(matches!(pty_result, ProcessBytesRead::Eof));
    assert!(
        pty_processes
            .get(pty_id)
            .expect("closed pty remains live until child status")
            .pending_status
            .is_nil()
    );
}

#[cfg(unix)]
#[test]
fn make_process_pty_uses_the_canonical_child_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(async_child_display_probe("t", "'pty"), r#"OK ":frame""#);
}

#[test]
fn getenv_name_must_be_string() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(condition-case err (getenv-internal nil) (error err))"#);
    assert_eq!(result, "OK (wrong-type-argument stringp nil)");
}

#[test]
fn getenv_accepts_optional_nil_env_arg() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err
               (let ((v (getenv-internal "HOME" nil)))
                 (if (stringp v) 'string v))
             (error err))"#,
    );
    assert_eq!(result, "OK string");
}

#[test]
fn getenv_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let result =
        eval_one(r#"(condition-case err (getenv-internal "HOME" nil nil) (error (car err)))"#);
    assert_eq!(result, "OK wrong-number-of-arguments");
}

#[test]
fn setenv_and_getenv() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(setenv "NEOVM_TEST_SETENV" "myvalue")
           (getenv "NEOVM_TEST_SETENV")"#,
    );
    assert_eq!(results[0], r#"OK "myvalue""#);
    assert_eq!(results[1], r#"OK "myvalue""#);
}

#[test]
fn setenv_unset() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(setenv "NEOVM_TEST_UNSET" "val")
           (setenv "NEOVM_TEST_UNSET")
           (getenv "NEOVM_TEST_UNSET")"#,
    );
    assert_eq!(results[2], "OK nil");
}

#[test]
fn setenv_name_must_be_string() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(condition-case err (setenv nil "v") (error err))"#);
    assert_eq!(result, "OK (wrong-type-argument stringp nil)");
}

#[test]
fn setenv_accepts_sequence_value_and_sets_environment() {
    crate::test_utils::init_test_tracing();
    let vector_result = eval_one(
        r#"(let ((old (getenv "NEOVM_TEST_SETENV_SEQ")))
             (unwind-protect
                 (progn
                   (setenv "NEOVM_TEST_SETENV_SEQ" [118 97 108])
                   (getenv "NEOVM_TEST_SETENV_SEQ"))
               (setenv "NEOVM_TEST_SETENV_SEQ" old)))"#,
    );
    assert_eq!(vector_result, r#"OK "val""#);

    let list_result = eval_one(
        r#"(let ((old (getenv "NEOVM_TEST_SETENV_SEQ")))
             (unwind-protect
                 (progn
                   (setenv "NEOVM_TEST_SETENV_SEQ" '(118 97 108))
                   (getenv "NEOVM_TEST_SETENV_SEQ"))
               (setenv "NEOVM_TEST_SETENV_SEQ" old)))"#,
    );
    assert_eq!(list_result, r#"OK "val""#);
}

#[test]
fn setenv_substitute_flag_controls_expansion_and_requires_string() {
    crate::test_utils::init_test_tracing();
    let unsubstituted = eval_one(
        r#"(let ((old (getenv "NEOVM_TEST_SETENV_SEQ")))
             (unwind-protect
                 (progn
                   (setenv "NEOVM_TEST_SETENV_SEQ" "$HOME")
                   (getenv "NEOVM_TEST_SETENV_SEQ"))
               (setenv "NEOVM_TEST_SETENV_SEQ" old)))"#,
    );
    assert_eq!(unsubstituted, r#"OK "$HOME""#);

    let substituted = eval_one(
        r#"(let ((old (getenv "NEOVM_TEST_SETENV_SEQ")))
             (unwind-protect
                 (progn
                   (setenv "NEOVM_TEST_SETENV_SEQ" "$HOME" t)
                   (getenv "NEOVM_TEST_SETENV_SEQ"))
               (setenv "NEOVM_TEST_SETENV_SEQ" old)))"#,
    );
    assert!(substituted.starts_with("OK \""));
    assert_ne!(substituted, r#"OK "$HOME""#);

    let type_err = eval_one(
        r#"(condition-case err (setenv "NEOVM_TEST_SETENV_SEQ" [118 97 108] t) (error err))"#,
    );
    assert_eq!(type_err, "OK (wrong-type-argument stringp [118 97 108])");
}

#[test]
fn setenv_rejects_non_sequence_value() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(condition-case err (setenv "NEOVM_TEST_SETENV_SEQ" 1) (error err))"#);
    assert_eq!(result, "OK (wrong-type-argument sequencep 1)");
}

#[test]
fn setenv_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err (setenv "NEOVM_TEST_SETENV_SEQ" "v" nil nil) (error (car err)))"#,
    );
    assert_eq!(result, "OK wrong-number-of-arguments");
}

#[test]
fn set_binary_mode_stream_contract_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err (set-binary-mode 'stdin t) (error err))
           (condition-case err (set-binary-mode 'stdout nil) (error err))
           (condition-case err (set-binary-mode 'stderr t) (error err))
           (condition-case err (set-binary-mode 'foo t) (error err))
           (condition-case err (set-binary-mode nil t) (error err))
           (condition-case err (set-binary-mode t t) (error err))
           (condition-case err (set-binary-mode 1 t) (error err))"#,
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], r#"OK (error "unsupported stream" foo)"#);
    assert_eq!(results[4], r#"OK (error "unsupported stream" nil)"#);
    assert_eq!(results[5], r#"OK (error "unsupported stream" t)"#);
    assert_eq!(results[6], "OK (wrong-type-argument symbolp 1)");
}

#[test]
fn call_process_bad_program() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(call-process "/nonexistent/program_xyz")"#);
    assert!(result.contains("ERR"));
}

#[test]
fn call_process_bad_program_signals_file_missing() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err (call-process "/nonexistent/program_xyz") (error (car err)))"#,
    );
    assert_eq!(result, "OK file-missing");
}

#[test]
fn call_process_missing_infile_signals_file_missing() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(condition-case err (call-process "{cat}" "/nonexistent/neovm-process-infile") (error (car err)))"#
    ));
    assert_eq!(result, "OK file-missing");
}

#[test]
fn call_process_region_bad_program_signals_file_missing() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err (call-process-region 1 1 "/nonexistent/program_xyz") (error (car err)))"#,
    );
    assert_eq!(result, "OK file-missing");
}

#[test]
fn call_process_symbol_destination_signals_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let result = eval_one(&format!(
        r#"(condition-case err (call-process "{echo}" nil 'foo nil "x") (error err))"#
    ));
    assert_eq!(result, "OK (wrong-type-argument stringp foo)");
}

#[test]
fn call_process_bad_stderr_target_signals_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let result = eval_one(&format!(
        r#"(condition-case err (call-process "{echo}" nil '(t 99) nil "x") (error err))"#
    ));
    assert_eq!(result, "OK (wrong-type-argument stringp 99)");
}

#[test]
fn process_status_wrong_arg_type() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(r#"(process-status 999)"#);
    assert!(result.contains("ERR"));
}

#[test]
fn process_status_accepts_buffer_and_nil_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer "ps-status-buffer"))
                  (p (start-process "ps-buffer-proc" buf "{cat}")))
             (unwind-protect
                 (list
                  (process-status p)
                  (process-status buf)
                  (with-current-buffer buf (process-status nil))
                  (process-status "ps-status-buffer")
                  (condition-case err
                      (let ((empty (generate-new-buffer "ps-empty")))
                        (unwind-protect
                            (process-status empty)
                          (ignore-errors (kill-buffer empty))))
                    (error err)))
               (ignore-errors (delete-process p))
               (ignore-errors (kill-buffer buf))))"#
    ));

    assert_eq!(
        result,
        "OK (run run run nil (error \"Buffer ps-empty has no process\"))"
    );
}

#[test]
fn process_type_accepts_get_process_designators_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer "pt-status-buffer"))
                  (p (start-process "pt-proc" buf "{cat}")))
             (unwind-protect
                 (list
                  (process-type p)
                  (process-type buf)
                  (with-current-buffer buf (process-type nil))
                  (process-type "pt-status-buffer")
                  (condition-case err (process-type "pt-missing") (error err)))
               (ignore-errors (delete-process p))
               (ignore-errors (kill-buffer buf))))"#
    ));

    assert_eq!(
        result,
        "OK (real real real real (error \"Process pt-missing does not exist\"))"
    );
}

#[test]
fn check_process_introspection_rejects_name_strings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-pipe-process :name "strict-proc")))
             (unwind-protect
                 (list
                  (condition-case err (process-name "strict-proc") (error err))
                  (condition-case err (process-buffer "strict-proc") (error err))
                  (condition-case err (process-command "strict-proc") (error err))
                  (condition-case err (process-contact "strict-proc") (error err))
                  (process-name p)
                  (process-command p)
                  (process-contact p))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(
        result,
        "OK ((wrong-type-argument processp \"strict-proc\") (wrong-type-argument processp \"strict-proc\") (wrong-type-argument processp \"strict-proc\") (wrong-type-argument processp \"strict-proc\") \"strict-proc\" nil t)"
    );
}

#[test]
fn check_process_extended_builtins_reject_name_strings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-pipe-process :name "strict-more")))
             (unwind-protect
                 (let* ((bad "strict-more")
                        (forms
                         '((process-id "strict-more")
                           (process-exit-status "strict-more")
                           (process-coding-system "strict-more")
                           (process-datagram-address "strict-more")
                           (process-inherit-coding-system-flag "strict-more")
                           (set-process-buffer "strict-more" nil)
                           (set-process-coding-system "strict-more" nil nil)
                           (set-process-datagram-address "strict-more" nil)
                           (set-process-inherit-coding-system-flag "strict-more" t)
                           (set-process-thread "strict-more" nil)
                           (set-process-window-size "strict-more" 10 20)
                           (process-tty-name "strict-more")
                           (process-mark "strict-more")
                           (process-thread "strict-more")
                           (process-query-on-exit-flag "strict-more")
                           (set-process-query-on-exit-flag "strict-more" nil)
                           (process-filter "strict-more")
                           (set-process-filter "strict-more" nil)
                           (process-sentinel "strict-more")
                           (set-process-sentinel "strict-more" nil)
                           (process-plist "strict-more")
                           (set-process-plist "strict-more" 1)
                           (process-get "strict-more" :k)
                           (process-put "strict-more" :k 1)
                           (clone-process "strict-more")
                           (set-network-process-option "strict-more" :keepalive t))))
                   (mapcar
                    (lambda (form)
                      (condition-case err
                          (progn (eval form) :no-error)
                        (error
                         (and (eq (car err) 'wrong-type-argument)
                              (eq (cadr err) 'processp)
                              (equal (caddr err) bad)))))
                    forms))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(
        result,
        "OK (t t t t t t t t t t t t t t t t t t t t t t t t t t)"
    );
}

#[test]
fn check_process_internal_and_gnutls_builtins_reject_name_strings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-pipe-process :name "strict-tls")))
             (unwind-protect
                 (let* ((bad "strict-tls")
                        (forms
                         '((internal-default-process-filter "strict-tls" 1)
                           (internal-default-process-sentinel "strict-tls" "finished\n")
                           (gnutls-asynchronous-parameters "strict-tls" nil)
                           (gnutls-get-initstage "strict-tls")
                           (gnutls-deinit "strict-tls")
                           (gnutls-peer-status "strict-tls")
                           (gnutls-boot "strict-tls" 1 2)
                           (gnutls-bye "strict-tls" nil))))
                   (mapcar
                    (lambda (form)
                      (condition-case err
                          (progn (eval form) :no-error)
                        (error
                         (and (eq (car err) 'wrong-type-argument)
                              (eq (cadr err) 'processp)
                              (equal (caddr err) bad)))))
                    forms))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(result, "OK (t t t t t t t t)");
}

#[test]
fn start_process_multiple_args() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let results = eval_all(&format!(
        r#"(processp (start-process "echo" nil "{echo}" "a" "b" "c"))
           (process-name (get-process "echo"))"#,
    ));
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], r#"OK "echo""#);
}

#[test]
fn process_runtime_introspection_controls() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(let ((p (start-process "proc-introspect" nil "{cat}")))
             (list
              (processp p)
              (equal (process-live-p p) '(run open listen connect stop))
              (integerp (process-id p))
              (process-contact p t)
              (process-filter p)
              (set-process-filter p nil)
              (set-process-filter p 'ignore)
              (process-filter p)
              (process-sentinel p)
              (set-process-sentinel p nil)
              (set-process-sentinel p 'ignore)
              (process-sentinel p)
              (set-process-plist p '(a 1))
              (process-get p 'a)
              (process-put p 'k 2)
              (process-get p 'k)
              (process-query-on-exit-flag p)
              (set-process-query-on-exit-flag p nil)
              (process-query-on-exit-flag p)
              (delete-process p)
              (process-live-p p)))"#,
    ));
    assert_eq!(
        results[0],
        "OK (t t t t internal-default-process-filter internal-default-process-filter ignore ignore internal-default-process-sentinel internal-default-process-sentinel ignore ignore (a 1 k 2) 1 (a 1 k 2) 2 t nil nil nil nil)"
    );
}

#[test]
fn process_primitive_arities_match_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(mapcar (lambda (s)
                     (list s (subr-arity (symbol-function s))))
                   '(process-filter
                     set-process-filter
                     process-sentinel
                     set-process-sentinel
                     process-coding-system
                     process-datagram-address
                     set-process-datagram-address
                     set-process-thread
                     process-thread
                     process-plist
                     set-process-plist
                     process-mark
                     process-exit-status
                     process-query-on-exit-flag
                     set-process-query-on-exit-flag
                     process-inherit-coding-system-flag
                     set-process-inherit-coding-system-flag))"#,
    );

    assert_eq!(
        result[0],
        "OK ((process-filter (1 . 1)) (set-process-filter (2 . 2)) (process-sentinel (1 . 1)) (set-process-sentinel (2 . 2)) (process-coding-system (1 . 1)) (process-datagram-address (1 . 1)) (set-process-datagram-address (2 . 2)) (set-process-thread (2 . 2)) (process-thread (1 . 1)) (process-plist (1 . 1)) (set-process-plist (2 . 2)) (process-mark (1 . 1)) (process-exit-status (1 . 1)) (process-query-on-exit-flag (1 . 1)) (set-process-query-on-exit-flag (2 . 2)) (process-inherit-coding-system-flag (1 . 1)) (set-process-inherit-coding-system-flag (2 . 2)))"
    );
}

#[test]
fn process_contact_keyword_matrix_for_network_and_pipe() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(list
            (let ((p (make-network-process :name "neo-contact-key-net" :server t :service 0 :log 'ignore)))
              (unwind-protect
                  (let ((port (process-contact p :service))
                        (local (process-contact p :local)))
                    (list
                     (stringp (process-contact p :name))
                     (eq (process-contact p :server) t)
                     (eq (process-contact p :log) 'ignore)
                     (integerp port)
                     (and (vectorp local)
                          (= (length local) 5)
                          (= (aref local 0) 127)
                          (= (aref local 4) port))
                     (null (process-contact p :remote))
                     (null (process-contact p :coding))
                     (null (process-contact p :foo))))
                (ignore-errors (delete-process p))))
            (let ((p (make-pipe-process :name "neo-contact-key-pipe")))
              (unwind-protect
                  (list
                   (stringp (process-contact p :name))
                   (null (process-contact p :server))
                   (null (process-contact p :service))
                   (null (process-contact p :local))
                   (null (process-contact p :remote))
                   (null (process-contact p :coding))
                   (null (process-contact p :foo)))
                (ignore-errors (delete-process p)))))"#,
    );
    assert_eq!(result, "OK ((t t t t t t t t) (t t t t t t t))");
}

#[test]
fn make_pipe_process_gnu_keywords_update_observable_state() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((pl (list :a 1)))
             (let ((p (make-pipe-process
                       :name "neo-pipe-keywords"
                       :buffer nil
                       :noquery t
                       :stop t
                       :filter 'ignore
                       :sentinel 'ignore
                       :plist pl
                       :coding (cons 'raw-text-unix 'utf-8-unix))))
               (setcar (cdr pl) 2)
               (unwind-protect
                   (list (process-status p)
                         (process-query-on-exit-flag p)
                         (process-filter p)
                         (process-sentinel p)
                         (process-coding-system p)
                         (process-plist p)
                         (plist-get (process-contact p t) :plist)
                         (process-contact p)
                         (process-buffer p))
                 (ignore-errors (delete-process p)))))"#,
    );
    assert_eq!(
        result,
        "OK (stop nil ignore ignore (raw-text-unix . utf-8-unix) (:a 1) (:a 2) t nil)"
    );
}

#[cfg(unix)]
#[test]
fn make_serial_process_gnu_keywords_update_observable_state() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((pl (list :a 1)))
             (let ((p (make-serial-process
                       :port "/dev/null"
                       :speed nil
                       :name "neo-serial-keywords"
                       :buffer nil
                       :noquery t
                       :stop t
                       :filter 'ignore
                       :sentinel 'ignore
                       :plist pl
                       :coding (cons 'raw-text-unix 'utf-8-unix))))
               (setcar (cdr pl) 2)
               (unwind-protect
                   (list (process-status p)
                         (process-live-p p)
                         (process-query-on-exit-flag p)
                         (process-filter p)
                         (process-sentinel p)
                         (process-coding-system p)
                         (process-plist p)
                         (plist-get (process-contact p t) :plist)
                         (buffer-name (process-buffer p))
                         (process-contact p))
                 (ignore-errors (delete-process p)))))"#,
    );
    assert_eq!(
        result,
        "OK (stop (stop) nil ignore ignore (raw-text-unix . utf-8-unix) (:a 1) (:a 2) \"neo-serial-keywords\" (\"/dev/null\" nil))"
    );
}

#[cfg(unix)]
#[test]
fn serial_configuration_keywords_update_contact_state() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-serial-process
                    :port "/dev/ptmx"
                    :speed 9600
                    :bytesize nil
                    :parity 'even
                    :stopbits 2
                    :flowcontrol 'hw)))
             (unwind-protect
                 (let ((initial (list (process-contact p :speed)
                                      (process-contact p :bytesize)
                                      (process-contact p :parity)
                                      (process-contact p :stopbits)
                                      (process-contact p :flowcontrol)
                                      (process-contact p :summary))))
                   (serial-process-configure
                    :process p
                    :bytesize 7
                    :parity 'odd
                    :stopbits nil
                    :flowcontrol 'sw)
                   (list initial
                         (list (process-contact p :speed)
                               (process-contact p :bytesize)
                               (process-contact p :parity)
                               (process-contact p :stopbits)
                               (process-contact p :flowcontrol)
                               (process-contact p :summary))
                         (process-contact p)))
               (ignore-errors (delete-process p))))"#,
    );
    assert_eq!(
        result,
        "OK ((9600 8 even 2 hw \"8E2\") (9600 7 odd 1 sw \"7O1\") (\"/dev/ptmx\" 9600))"
    );
}

#[cfg(unix)]
#[test]
fn serial_configuration_validates_option_domains() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-serial-process
                    :port "/dev/ptmx"
                    :speed 9600)))
             (unwind-protect
                 (list
                  (condition-case err
                      (serial-process-configure :process p :bytesize 6)
                    (error err))
                  (condition-case err
                      (serial-process-configure :process p :parity 'mark)
                    (error err))
                  (condition-case err
                      (serial-process-configure :process p :stopbits 3)
                    (error err))
                  (condition-case err
                      (serial-process-configure :process p :flowcontrol 'xon)
                    (error err))
                  (condition-case err
                      (serial-process-configure :process p :speed nil)
                    (error err))
                  (condition-case err
                      (serial-process-configure :process p :speed "fast")
                    (error err)))
               (ignore-errors (delete-process p))))"#,
    );
    assert_eq!(
        result,
        "OK ((error \":bytesize must be nil (8), 7, or 8\") (error \":parity must be nil (no parity), `even', or `odd'\") (error \":stopbits must be nil (1 stopbit), 1, or 2\") (error \":flowcontrol must be nil (no flowcontrol), `hw', or `sw'\") (wrong-type-argument fixnump nil) (wrong-type-argument fixnump \"fast\"))"
    );
}

#[cfg(unix)]
#[test]
fn serial_process_configure_resolves_buffer_and_port_designators() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((by-port (make-serial-process
                           :port "/dev/ptmx"
                           :speed 9600))
                 (by-buffer (make-serial-process
                             :port "/dev/ptmx"
                             :name "neo-serial-by-buffer-process"
                             :buffer "neo-serial-by-buffer"
                             :speed 9600)))
             (unwind-protect
                 (progn
                   (serial-process-configure
                    :port "/dev/ptmx"
                    :bytesize 7)
                   (serial-process-configure
                    :buffer "neo-serial-by-buffer"
                    :parity 'even)
                   (list (process-contact by-port :bytesize)
                         (process-contact by-buffer :parity)))
               (ignore-errors (delete-process by-port))
               (ignore-errors (delete-process by-buffer))))"#,
    );
    assert_eq!(result, "OK (7 even)");
}

#[test]
fn make_process_noquery_and_stop_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((p (make-process :name "neo-make-process-noquery"
                                  :command (list "sh" "-c" "sleep 0.1")
                                  :noquery t)))
             (unwind-protect
                 (list (process-status p) (process-query-on-exit-flag p))
               (ignore-errors (delete-process p))))
           (condition-case err
               (make-process :name "neo-make-process-stop" :command nil :stop "x")
             (error err))"#,
    );

    assert_eq!(results[0], "OK (run nil)");
    assert_eq!(results[1], "OK (wrong-type-argument null \"x\")");
}

#[test]
fn process_constructors_validate_coding_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err
              (make-pipe-process :name "neo-bad-pipe-coding" :coding 1)
            (error err))
           (condition-case err
              (make-network-process :name "neo-bad-network-coding"
                                    :server t :service 0 :coding 1)
            (error err))
           (condition-case err
              (make-process :name "neo-bad-make-process-coding"
                            :command nil :coding 1)
            (error err))"#,
    );

    assert_eq!(results[0], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[2], "OK (wrong-type-argument symbolp 1)");
}

#[test]
fn stopped_network_client_suppresses_open_sentinel_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil))
             (condition-case err
                 (let* ((srv (make-network-process
                              :name "neo-stop-srv" :server t :service t :host 'local))
                        (port (process-contact srv :service))
                        (cli (make-network-process
                              :name "neo-stop-cli"
                              :host 'local
                              :service port
                              :stop t
                              :sentinel (lambda (_p msg) (push msg events)))))
                   (accept-process-output nil 0.1)
                   (prog1
                       (list (process-status cli) (process-live-p cli) events)
                     (delete-process cli)
                     (delete-process srv)))
               (error err)))"#,
    );

    assert_eq!(results[0], "OK (stop (stop) nil)");
}

#[test]
fn process_stale_mutator_matrix_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(let ((p (start-process "proc-stale-mutator" nil "{cat}")))
             (unwind-protect
                 (progn
                   (delete-process p)
                   (list
                    (set-process-filter p 'ignore)
                    (set-process-sentinel p 'ignore)
                    (set-process-plist p '(a 1))
                    (process-put p 'k 2)
                    (set-process-query-on-exit-flag p nil)
                    (set-process-buffer p nil)
                    (set-process-coding-system p 'utf-8-unix)
                    (set-process-inherit-coding-system-flag p t)
                    (set-process-thread p nil)
                    (set-process-window-size p 10 20)
                    (set-process-datagram-address p nil)))
               (ignore-errors (delete-process p))))"#,
    ));
    assert_eq!(
        result,
        "OK (ignore ignore (a 1 k 2) (a 1 k 2) nil nil nil t nil nil nil)"
    );
}

#[test]
fn set_process_window_size_pipe_return_and_bounds_match_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((p (make-pipe-process :name "proc-window-size-pipe")))
             (unwind-protect
                 (list
                  (set-process-window-size p 10 20)
                  (set-process-window-size p 0 0)
                  (set-process-window-size p 65535 65535)
                  (condition-case err (set-process-window-size p "x" 20) (error err))
                  (condition-case err (set-process-window-size p 10 "x") (error err))
                  (condition-case err (set-process-window-size p -1 20) (error err))
                  (condition-case err (set-process-window-size p 10 -1) (error err))
                  (condition-case err (set-process-window-size p 70000 20) (error err))
                  (condition-case err (set-process-window-size p 10 70000) (error err)))
               (ignore-errors (delete-process p))))"#,
    );
    assert_eq!(
        result,
        "OK (nil nil nil (wrong-type-argument integerp \"x\") (wrong-type-argument integerp \"x\") (args-out-of-range -1 0 65535) (args-out-of-range -1 0 65535) (args-out-of-range 70000 0 65535) (args-out-of-range 70000 0 65535))"
    );
}

#[test]
fn process_signal_functions_dispatch_hooks_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(list
            (let ((seen-i nil) (seen-s nil))
              (let ((interrupt-process-functions
                     (list (lambda (proc group)
                             (setq seen-i (list proc group))
                             'custom-interrupt)
                           'internal-default-interrupt-process))
                    (signal-process-functions
                     (list (lambda (proc sig remote)
                             (setq seen-s (list proc sig remote))
                             77)
                           'internal-default-signal-process)))
                (list (interrupt-process 'not-a-real-process 'lambda)
                      seen-i
                      (signal-process "not-a-real-process" 'TERM 'remote-host)
                      seen-s)))
            (let ((interrupt-process-functions nil)
                  (signal-process-functions nil))
              (list (interrupt-process 'ignored)
                    (signal-process "ignored" 1))))"#,
    );

    assert_eq!(
        result,
        "OK ((custom-interrupt (not-a-real-process lambda) 77 (\"not-a-real-process\" TERM remote-host)) (nil nil))"
    );
}

#[test]
fn signal_process_accepts_gnu_signal_name_symbols() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((pid 99999999))
             (list
              (internal-default-signal-process pid 'TERM)
              (internal-default-signal-process pid 'SIGTERM)
              (internal-default-signal-process pid 'term)
              (internal-default-signal-process pid 'sigterm)
              (internal-default-signal-process pid 'EXIT)
              (car (condition-case err
                       (internal-default-signal-process pid 'no-such-signal)
                     (error err)))
              (condition-case err
                  (internal-default-signal-process pid "TERM")
                (error (car err)))))"#,
    );

    assert_eq!(result, "OK (-1 -1 -1 -1 -1 error wrong-type-argument)");
}

/// SIGSTOP, SIGCONT, output, exit -- the four status transitions a real
/// child can make, each recorded and each notified.
///
/// **Each loop waits for the SENTINEL COUNT, not for `process-status`, and
/// that is deliberate (ledger 180).**  `handle_child_signal` records a
/// transition (src/process.c:7745-7747) before `status_notify` reports it
/// (:7885), and only one raw status is held at a time -- so a loop that exits
/// on `(eq (process-status p) 'stop)` can leave the stop sentinel unrun, and
/// the SIGCONT that follows then overwrites the record, making the stop
/// sentinel report `run` when it finally does run.  Waiting on the sentinel
/// is the shape that asks the question the test is named for.
///
/// Re-derived against GNU Emacs 31.0.90 in the shape below, 5 runs of 5:
///
/// ```text
/// (t t exit "resumed\n" ((stop "stopped (signal)\n") (run "run") (exit "finished\n")))
/// ```
#[cfg(unix)]
#[test]
fn signal_process_observes_stop_continue_output_and_exit_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(let* ((buffer (generate-new-buffer " *signal-stop-continue*"))
                  (events nil)
                  (process
                   (make-process
                    :name "signal-stop-continue"
                    :buffer buffer
                    :command (list "{cat}")
                    :connection-type 'pipe
                    :noquery t
                    :sentinel
                    (lambda (proc event)
                      (setq events
                            (cons (list (process-status proc) event)
                                  events))))))
             (unwind-protect
                 (progn
                   (signal-process process 'SIGSTOP)
                   (let ((attempt 0))
                     (while (and (null events) (< attempt 100))
                       (setq attempt (1+ attempt))
                       (accept-process-output nil 0.01)))
                   (let ((stopped (eq (process-status process) 'stop)))
                     (signal-process process 'SIGCONT)
                     (let ((attempt 0))
                       (while (and (< (length events) 2) (< attempt 100))
                         (setq attempt (1+ attempt))
                         (accept-process-output nil 0.01)))
                     (let ((continued (eq (process-status process) 'run)))
                       (when continued
                         (process-send-string process "resumed\n")
                         (process-send-eof process))
                       (let ((attempt 0))
                         (while (and (< (length events) 3)
                                     (< attempt 100))
                           (setq attempt (1+ attempt))
                           (accept-process-output nil 0.01)))
                       (list stopped
                             continued
                             (process-status process)
                             (with-current-buffer buffer (buffer-string))
                             (nreverse events)))))
               (when (process-live-p process)
                 (ignore-errors (delete-process process)))
               (kill-buffer buffer)))"#
    ));

    assert_eq!(
        result,
        "OK (t t exit \"resumed\n\" ((stop \"stopped (signal)\n\") (run \"run\") (exit \"finished\n\")))"
    );
}

#[test]
fn connection_process_signal_controls_match_gnu_errors() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(list
            (let ((p (make-network-process
                      :name "ctrl-net" :server t :service 0)))
              (unwind-protect
                  (list
                   (condition-case err (kill-process p) (error err))
                   (condition-case err (quit-process p) (error err))
                   (condition-case err (interrupt-process p) (error err))
                   (condition-case err (signal-process p 'TERM) (error err))
                   (eq (stop-process p) p)
                   (process-status p)
                   (eq (continue-process p) p)
                   (process-status p))
                (ignore-errors (delete-process p))))
            (let ((p (make-pipe-process :name "ctrl-pipe")))
              (unwind-protect
                  (list
                   (condition-case err (kill-process p) (error err))
                   (condition-case err (quit-process p) (error err))
                   (condition-case err (interrupt-process p) (error err))
                   (condition-case err (signal-process p 'TERM) (error err))
                   (eq (stop-process p) p)
                   (process-status p)
                   (eq (continue-process p) p)
                   (process-status p))
                (ignore-errors (delete-process p)))))"#,
    );

    assert_eq!(
        result,
        "OK (((error \"Process ctrl-net is not a subprocess\") (error \"Process ctrl-net is not a subprocess\") (error \"Process ctrl-net is not a subprocess\") (error \"Cannot signal process ctrl-net\") t stop t listen) ((error \"Process ctrl-pipe is not a subprocess\") (error \"Process ctrl-pipe is not a subprocess\") (error \"Process ctrl-pipe is not a subprocess\") (error \"Cannot signal process ctrl-pipe\") t stop t open))"
    );
}

#[test]
fn process_stale_control_matrix_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(let ((p (start-process "proc-stale-control" nil "{cat}")))
             (unwind-protect
                 (progn
                   (delete-process p)
                   (list
                    (condition-case err (continue-process p) (error (car err)))
                    (condition-case err (interrupt-process p) (error (car err)))
                    (condition-case err (kill-process p) (error (car err)))
                    (condition-case err (stop-process p) (error (car err)))
                    (condition-case err (quit-process p) (error (car err)))
                    (let ((rv (signal-process p 0)))
                      (or (eq rv 0) (eq rv -1)))
                    (set-process-query-on-exit-flag p nil)
                    (process-query-on-exit-flag p)
                    (process-live-p p)
                    (process-status p)
                    (process-exit-status p)))
               (ignore-errors (delete-process p))))"#,
    ));
    assert_eq!(
        result,
        "OK (error error error error error t nil nil nil signal 9)"
    );
}

#[test]
fn process_attributes_runtime_shape_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((attrs (process-attributes (emacs-pid))))
             (list
              (listp attrs)
              (null (assq 'pid attrs))
              (let ((pair (assq 'user attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (let ((pair (assq 'group attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (let ((pair (assq 'euid attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'egid attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'comm attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (let ((pair (assq 'state attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (let ((pair (assq 'ppid attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'pgrp attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'sess attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'tpgid attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'minflt attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'majflt attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'cminflt attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'cmajflt attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'pri attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'nice attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'thcount attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'vsize attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'rss attrs)))
                (and (consp pair) (integerp (cdr pair))))
              (let ((pair (assq 'ttname attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (process-attributes -1)
              (process-attributes -1.0)
              (process-attributes #x7fffffff)
              (let ((float-attrs (process-attributes (float (emacs-pid)))))
                (not (null (assq 'comm float-attrs))))
              (condition-case err (process-attributes 'x) (error err))
              (condition-case err (process-attributes 1.5) (error err))
              (condition-case err (process-attributes #x80000000) (error err))
              (condition-case err (process-attributes #x100000000000000000000) (error err))
              (process-attributes 999999999)))"#,
    );
    assert_eq!(
        result,
        "OK (t t t t t t t t t t t t t t t t t t t t t t nil nil nil t (wrong-type-argument numberp x) (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Not an in-range integer, integral float, or cons of integers\") nil)"
    );
}

#[test]
fn process_attributes_pipe_child_args_ttname_and_running_child_match_oracle() {
    crate::test_utils::init_test_tracing();
    // This raced the child's EXEC, not its exit. /proc/PID/cmdline is empty
    // between fork and execve, and the reader then falls back to the bracketed
    // comm name ("[sh]") exactly as GNU does — a correct reading of a real
    // transient state, but not the one being asserted, so the test failed
    // roughly once in 30 local runs. Wait for the exec to land instead of
    // racing it; the bracketed form is the precise, self-limiting signal that
    // it has not. (Lengthening the child's sleep does NOT fix this, which is
    // why it now sleeps only long enough to outlive the polling window.)
    let result = eval_one(
        r#"(let ((proc (make-process
                        :name "neo-attrs-pipe-child"
                        :command '("/bin/sh" "-c" "sleep 300")
                        :connection-type 'pipe)))
             (unwind-protect
                 (let ((args nil) (ttname nil) (tries 0))
                   (while (progn
                            (let ((attrs (process-attributes (process-id proc))))
                              (setq args (cdr (assq 'args attrs))
                                    ttname (cdr (assq 'ttname attrs))))
                            (and (< tries 200)
                                 (stringp args)
                                 (string-prefix-p "[" args)))
                     (setq tries (1+ tries))
                     (sleep-for 0.01))
                   (and (eq (process-running-child-p proc) t)
                        ;; GNU's Linux implementation reads live /proc/PID/cmdline
                        ;; and shell state can expose either the shell invocation
                        ;; or the final exec'd command.
                        (or (equal args "/bin/sh -c sleep\\ 300")
                            (equal args "sleep 300"))
                        (equal ttname "")))
               (when (process-live-p proc)
                 (delete-process proc))))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn process_attributes_timing_memory_shape_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((attrs (process-attributes (emacs-pid))))
             (list
              (let ((pair (assq 'utime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'stime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'time attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'cutime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'cstime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'ctime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'start attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'etime attrs)))
                (and (consp pair) (consp (cdr pair))))
              (let ((pair (assq 'pcpu attrs)))
                (and (consp pair) (floatp (cdr pair))))
              (let ((pair (assq 'pmem attrs)))
                (and (consp pair) (floatp (cdr pair))))
              (let ((pair (assq 'args attrs)))
                (and (consp pair) (stringp (cdr pair))))
              (null (assq 'pid attrs))))"#,
    );
    assert_eq!(result, "OK (t t t t t t t t t t t t)");
}

#[test]
fn accept_process_output_and_get_process_runtime_surface() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(condition-case err (accept-process-output) (error err))
           (condition-case err (accept-process-output nil 0.01) (error err))
           (condition-case err (accept-process-output 1) (error err))
           (condition-case err (accept-process-output nil "x") (error err))
           (let ((p (start-process "proc-get-probe" nil "{cat}")))
             (list
              (processp (get-process "proc-get-probe"))
              (eq p (get-process "proc-get-probe"))
              (accept-process-output p 0.0)
              (delete-process p)
              (accept-process-output p 0.0)
              (get-process "proc-get-probe")))
           (condition-case err (get-process 'proc-get-probe) (error err))"#,
    ));
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK (wrong-type-argument processp 1)");
    assert_eq!(results[3], r#"OK (wrong-type-argument numberp "x")"#);
    assert_eq!(results[4], "OK (t t nil nil nil nil)");
    assert_eq!(
        results[5],
        "OK (wrong-type-argument stringp proc-get-probe)"
    );
}

#[test]
fn accept_process_output_yields_to_pending_command_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .expect("queue focus event");
    tx.send(crate::keyboard::InputEvent::KeyPress {
        key: crate::keyboard::KeyEvent::char('j'),
        emacs_frame_id: 0,
    })
    .expect("queue key event");

    let result = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.1)])
        .expect("accept-process-output should yield to command input");

    assert_eq!(result, Value::NIL);
    assert_eq!(ev.command_loop.keyboard.pending_input_events.len(), 2);
}

#[test]
fn accept_process_output_drains_ready_output_before_yielding_to_command_input() {
    // Regression: `accept-process-output` must not starve a process that
    // already has readable, undrained output just because command input is
    // queued.  GNU's `wait_reading_process_output` reads readable process fds
    // from the select result and runs their filters before it yields on
    // keyboard input, so the bytes are always drained.  Our split wait
    // abstraction used to early-return on pending command input *before*
    // polling ready process fds, which hung re-entrant `accept-process-output`
    // callers (e.g. Copilot/jsonrpc startup) forever even though the child had
    // written a full response.
    //
    // This can only be reproduced in Rust, not from elisp/batch: an elisp
    // `sleep-for`/`sit-for` pumps the wait loop and drains the bytes early.  A
    // plain Rust `sleep` lets the child's output land in the pipe *without*
    // running the wait loop, so the bytes stay undrained until the
    // `accept-process-output` under test — exactly the starvation window.
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq neo-apo-drain-output "")
             (fset 'neo-apo-drain-filter
                   (lambda (_proc string)
                     (setq neo-apo-drain-output
                           (concat neo-apo-drain-output string)))))"#,
    )
    .expect("install drain filter");

    // Spawn a real child that writes "READY" immediately.
    let pid = ev.processes.create_process(
        "neo-apo-drain-probe".into(),
        Value::NIL,
        printf,
        vec!["READY".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn probe child");
    builtin_set_process_filter(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("neo-apo-drain-filter"),
        ],
    )
    .expect("install ready-output filter");

    // Let the child write its bytes into the pipe WITHOUT running the wait
    // loop.  A Rust sleep does not pump neomacs's wait loop, so the bytes are
    // not drained early (an elisp sleep-for WOULD drain them, which is why this
    // can't be reproduced from elisp/batch).
    std::thread::sleep(Duration::from_millis(200));

    // Inject pending command input.  Without the fix, this makes
    // `service_wait_request_processes` early-return before draining the ready
    // process fd.
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::KeyPress {
        key: crate::keyboard::KeyEvent::char('j'),
        emacs_frame_id: 0,
    })
    .expect("queue key event");

    let result = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.5)],
    )
    .expect("accept-process-output should drain ready output despite pending command input");
    drop(tx);

    // The ready output must have been drained (filter ran) despite the pending
    // command input.
    let drained = ev
        .eval_symbol("neo-apo-drain-output")
        .expect("drain output var should be readable");
    assert_eq!(
        format!("{}", drained),
        r#""READY""#,
        "ready process output must be drained before yielding to command input"
    );

    // GNU's return value reports PROCESS activity, not input: "Return non-nil
    // if we received any output from PROCESS ... before the timeout expired"
    // (process.c:4880-4884).  Output was received, so the call returns t.
    // Pending command input is not part of that contract -- GNU's
    // `Faccept_process_output` passes READ_KBD = 0 and never ends the wait on
    // input (process.c:4957-4959, 5930-5937) -- and the queued keystroke is
    // left for the command loop either way.
    assert_eq!(result, Value::T);
}

#[test]
fn accept_process_output_propagates_throw_from_timer_callback_to_outer_catch() {
    // A non-local `throw` raised from inside a timer callback must propagate
    // out of the `accept-process-output` wait to the matching outer `catch`,
    // matching GNU.  `lisp/emacs-lisp/timer.el` `timer-event-handler` wraps the
    // callback in `condition-case-unless-debug err … (error …)`, which catches
    // `error`-class *signals* only; a `throw` is not an error, so it propagates
    // past the handler to the surrounding `catch`.  Process filters/sentinels in
    // `src/process.c` (`read_process_output`/`exec_sentinel`) likewise never
    // catch throws.  This is the core of `jsonrpc-request`'s continuation
    // protocol (eglot/copilot/lsp): the throw that completes the synchronous
    // request comes from a zero-delay `(run-at-time 0 nil …)`.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // A due GNU timer whose callback throws to 'neo-throw-tag, plus the
    // timer.el-shaped `timer-event-handler` stub (a bare Context has no
    // timer.el): the wait loop reads `timer-list` and dispatches due timers to
    // `timer-event-handler`, whose `condition-case … (error …)` catches error
    // *signals* only — a `throw` sails through it.
    ev.eval_str(
        r#"(fset 'timer-event-handler
                 (lambda (timer)
                   (setq timer-list (delq timer timer-list))
                   (condition-case nil
                       (apply (aref timer 5) (aref timer 6))
                     (error nil))))
"#,
    )
    .expect("install timer-event-handler stub");
    ev.eval_str(
        "(fset 'apio-throwing-timer (lambda () (throw 'neo-throw-tag 'thrown-from-timer)))",
    )
    .expect("build throwing timer callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "apio-throwing-timer",
        )]),
    );

    // The catch surrounds the wait.  The timer fires inside the wait and throws;
    // the throw must reach this catch, yielding 'thrown-from-timer (NOT the
    // post-wait 'no-throw-loop-finished value).
    let result = ev.eval_str(
        r#"(catch 'neo-throw-tag
             (accept-process-output nil 0.2)
             'no-throw-loop-finished)"#,
    );

    assert_eq!(
        format_eval_result(&result),
        "OK thrown-from-timer",
        "a throw from a timer callback must propagate to the outer catch, not be swallowed by the timer wrapper"
    );
}

#[test]
fn timer_service_defers_due_timer_scheduled_by_callback_to_next_pass() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable(
        "neo-second-timer",
        gnu_timer_before(Duration::from_millis(1), "neo-second-timer-callback"),
    );

    ev.eval_str(
        r#"(progn
             (setq neo-timer-batch-log nil)
             (fset 'neo-second-timer-callback
                   (lambda ()
                     (setq neo-timer-batch-log
                           (append neo-timer-batch-log '(second)))))
             (fset 'neo-first-timer-callback
                   (lambda ()
                     (setq neo-timer-batch-log
                           (append neo-timer-batch-log '(first)))
                     (setq timer-list (list neo-second-timer))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install stable timer-batch probe");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "neo-first-timer-callback",
        )]),
    );

    ev.service_pending_timers_with_wait_policy(false)
        .expect("first timer service pass");

    assert_eq!(
        ev.eval_symbol("neo-timer-batch-log")
            .expect("timer batch log"),
        Value::list(vec![Value::symbol("first")]),
        "a timer scheduled by a callback belongs to the next service pass"
    );
    assert_eq!(
        crate::emacs_core::value::list_to_vec(
            &ev.eval_symbol("timer-list").expect("remaining timer list")
        )
        .expect("timer-list should remain a proper list")
        .len(),
        1,
        "the newly scheduled due timer must remain queued"
    );

    ev.service_pending_timers_with_wait_policy(false)
        .expect("second timer service pass");
    assert_eq!(
        ev.eval_symbol("neo-timer-batch-log")
            .expect("timer batch log after second pass"),
        Value::list(vec![Value::symbol("first"), Value::symbol("second")])
    );
}

#[test]
fn kill_emacs_from_a_timer_callback_unwinds_the_service_pass() {
    // GNU's Fkill_emacs never returns (it runs the hooks and calls exit), so a
    // timer callback that kills cannot continue and cannot be caught. Ours must
    // propagate the shutdown out of the callback boundary rather than report it
    // as a callback error, or the process runs on forever with the exit code
    // lost.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq neo-kill-timer-log nil)
             (fset 'neo-kill-timer-callback
                   (lambda ()
                     (setq neo-kill-timer-log (append neo-kill-timer-log '(before)))
                     (kill-emacs 3)
                     (setq neo-kill-timer-log (append neo-kill-timer-log '(after)))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install killing timer callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "neo-kill-timer-callback",
        )]),
    );

    let flow = ev
        .service_pending_timers_with_wait_policy(false)
        .expect_err("kill-emacs must unwind out of the timer service pass");
    assert!(
        matches!(flow, Flow::Shutdown(request) if request.exit_code == 3 && !request.restart),
        "expected a shutdown flow carrying the exit code, got {flow:?}"
    );
    assert_eq!(
        ev.shutdown_request().map(|request| request.exit_code),
        Some(3),
        "the shutdown request must record the exit code kill-emacs was given"
    );
    assert_eq!(
        ev.eval_symbol("neo-kill-timer-log").expect("timer log"),
        Value::list(vec![Value::symbol("before")]),
        "forms after kill-emacs must not run"
    );
}

#[test]
fn kill_emacs_from_a_timer_callback_is_not_catchable_as_an_error() {
    // The shutdown is control flow, not a condition: condition-case must not be
    // able to swallow it (GNU has no signal here to catch).
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq neo-kill-catch-log nil)
             (fset 'neo-kill-catching-callback
                   (lambda ()
                     (condition-case nil
                         (kill-emacs 3)
                       (error (setq neo-kill-catch-log '(caught))))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install catching timer callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "neo-kill-catching-callback",
        )]),
    );

    let flow = ev
        .service_pending_timers_with_wait_policy(false)
        .expect_err("condition-case must not absorb the shutdown");
    assert!(matches!(flow, Flow::Shutdown(_)), "got {flow:?}");
    assert_eq!(
        ev.eval_symbol("neo-kill-catch-log").expect("catch log"),
        Value::NIL,
        "the error handler must not have run"
    );
}

#[test]
fn command_input_preempts_self_rescheduling_zero_idle_timer_between_batches() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let idle_timer = Value::vector(vec![
        Value::NIL,
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::NIL,
        Value::symbol("neo-self-rescheduling-idle-callback"),
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::NIL,
    ]);
    ev.set_variable("neo-self-rescheduling-idle-timer", idle_timer);
    ev.eval_str(
        r#"(progn
             (setq neo-self-rescheduling-idle-count 0)
             (fset 'neo-self-rescheduling-idle-callback
                   (lambda ()
                     (setq neo-self-rescheduling-idle-count
                           (1+ neo-self-rescheduling-idle-count))
                     (when (< neo-self-rescheduling-idle-count 1000)
                       (aset neo-self-rescheduling-idle-timer 0 nil)
                       (setq timer-idle-list
                             (list neo-self-rescheduling-idle-timer)))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-idle-list (delq timer timer-idle-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install self-rescheduling idle timer probe");
    ev.set_variable("timer-idle-list", Value::list(vec![idle_timer]));
    ev.timer_start_idle();

    let (first_batch_tx, first_batch_rx) = std::sync::mpsc::sync_channel(1);
    let (input_sent_tx, input_sent_rx) = std::sync::mpsc::sync_channel(1);
    let mut first_batch_tx = Some(first_batch_tx);
    let mut input_sent_rx = Some(input_sent_rx);
    ev.redisplay_fn = Some(Box::new(move |_| {
        if let Some(tx) = first_batch_tx.take() {
            tx.send(()).expect("announce first serviced timer batch");
            input_sent_rx
                .take()
                .expect("input acknowledgement receiver")
                .recv_timeout(Duration::from_secs(1))
                .expect("input should be queued before the next timer batch");
        }
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let notifier = ev.wait_notifier();
    let sender = std::thread::spawn(move || {
        first_batch_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the idle timer should complete its first batch");
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('f'),
        ))
        .expect("send command input while idle timer reschedules");
        if let Some(notifier) = notifier {
            notifier.notify().expect("wake command-input wait");
        }
        input_sent_tx
            .send(())
            .expect("acknowledge queued command input");
    });

    let outcome = ev
        .wait_for_command_input(Some(std::time::Instant::now() + Duration::from_secs(1)))
        .expect("wait should yield to command input");
    sender.join().expect("input sender should finish");

    assert_eq!(outcome, CommandInputWaitOutcome::InputPending);
    let callback_count = ev
        .eval_symbol("neo-self-rescheduling-idle-count")
        .expect("idle timer callback count should be bound")
        .as_int()
        .expect("idle timer callback count");
    assert!(
        callback_count < 1000,
        "pending command input must win before the rescheduling cap"
    );
}

#[test]
fn stable_timer_batch_rechecks_later_timer_after_earlier_callback_cancels_it() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let canceled_timer = gnu_timer_before(Duration::from_millis(1), "neo-canceled-callback");
    ev.set_variable("neo-canceled-timer", canceled_timer);
    ev.eval_str(
        r#"(progn
             (setq neo-canceled-timer-ran nil)
             (fset 'neo-cancel-later-timer
                   (lambda ()
                     (aset neo-canceled-timer 0 t)
                     (setq timer-list (delq neo-canceled-timer timer-list))))
             (fset 'neo-canceled-callback
                   (lambda () (setq neo-canceled-timer-ran t)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install timer cancellation probe");
    ev.set_variable(
        "timer-list",
        Value::list(vec![
            gnu_timer_before(Duration::from_millis(2), "neo-cancel-later-timer"),
            canceled_timer,
        ]),
    );

    ev.service_pending_timers_with_wait_policy(false)
        .expect("service copied timer list");

    assert_eq!(
        ev.eval_symbol("neo-canceled-timer-ran")
            .expect("canceled timer flag"),
        Value::NIL,
        "the copied list must re-read a shared timer vector after callbacks"
    );
}

#[test]
fn stable_timer_batch_stops_at_first_valid_future_timer_after_callback_mutation() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let moved_timer = gnu_timer_before(Duration::from_millis(1), "neo-moved-callback");
    let future_timer = gnu_timer_after(Duration::from_secs(60), "ignored");
    let future_slots = future_timer
        .as_vector_data()
        .expect("future timer vector")
        .clone();
    for (name, slot) in [
        ("neo-future-high", 1),
        ("neo-future-low", 2),
        ("neo-future-usecs", 3),
        ("neo-future-psecs", 8),
    ] {
        ev.set_variable(name, future_slots[slot]);
    }
    ev.set_variable("neo-moved-timer", moved_timer);
    ev.eval_str(
        r#"(progn
             (setq neo-behind-future-ran nil)
             (fset 'neo-move-next-timer-to-future
                   (lambda ()
                     (aset neo-moved-timer 1 neo-future-high)
                     (aset neo-moved-timer 2 neo-future-low)
                     (aset neo-moved-timer 3 neo-future-usecs)
                     (aset neo-moved-timer 8 neo-future-psecs)))
             (fset 'neo-moved-callback (lambda () (error "moved timer ran")))
             (fset 'neo-behind-future-callback
                   (lambda () (setq neo-behind-future-ran t)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install future-head mutation probe");
    ev.set_variable(
        "timer-list",
        Value::list(vec![
            gnu_timer_before(Duration::from_millis(3), "neo-move-next-timer-to-future"),
            moved_timer,
            gnu_timer_before(Duration::from_millis(1), "neo-behind-future-callback"),
        ]),
    );

    ev.service_pending_timers_with_wait_policy(false)
        .expect("service copied timer list");

    assert_eq!(
        ev.eval_symbol("neo-behind-future-ran")
            .expect("behind-future timer flag"),
        Value::NIL,
        "GNU stops the copied list at its first valid future timer"
    );
}

#[test]
fn accept_process_output_still_catches_error_signal_from_timer_callback() {
    // Guard against over-correcting: an `error` (signal) raised from a timer
    // callback must STILL be caught and logged by the timer wrapper (matching
    // `timer-event-handler`'s `condition-case-unless-debug err … (error …)`),
    // NOT propagated out of the wait.  Only non-local `throw`s propagate.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // timer.el's `timer-event-handler` wraps the callback in
    // `condition-case-unless-debug err … (error …)`; the stub mirrors that, so
    // the error is swallowed inside the handler exactly as with real timer.el.
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (condition-case nil
                         (apply (aref timer 5) (aref timer 6))
                       (error nil))))
             (fset 'apio-erroring-timer (lambda () (error "boom from timer"))))"#,
    )
    .expect("build erroring timer callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "apio-erroring-timer",
        )]),
    );

    // No surrounding catch/condition-case: if the wrapper wrongly propagated the
    // error, this `accept-process-output` would return Err.  GNU's
    // `condition-case (error …)` swallows it, so the wait completes normally.
    let result = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.2)])
        .expect("an error signaled from a timer callback must be caught, not propagated");
    assert_eq!(result, Value::NIL);
}

#[test]
fn accept_process_output_request_uses_gnu_wait_deadlines() {
    let mut processes = ProcessManager::new();

    let poll = parse_accept_process_output_request(&mut processes, &[])
        .expect("parse no-arg accept-process-output")
        .expect("live request");
    assert!(poll.wait_timing_is_poll());
    assert!(poll.completes_on_any_process_activity());

    let timeout =
        parse_accept_process_output_request(&mut processes, &[Value::NIL, Value::make_float(0.25)])
            .expect("parse timed accept-process-output")
            .expect("live request");
    assert!(timeout.wait_timing_is_finite());
    assert!(timeout.completes_on_any_process_activity());

    let id = processes.create_process(
        "target".into(),
        Value::NIL,
        "cat".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let target =
        parse_accept_process_output_request(&mut processes, &[Value::make_process(id), Value::NIL])
            .expect("parse target accept-process-output")
            .expect("live request");
    assert!(target.wait_timing_is_forever());
    assert!(target.completes_on_target_process_activity(id));
    assert!(!target.services_only_target_process_output());
}

#[test]
fn wait_scheduler_can_block_until_command_input_arrives() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let notifier = ev.wait_notifier();

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('z'),
        ))
        .expect("send delayed keypress");
        if let Some(notifier) = notifier {
            notifier.notify().expect("wake command-input wait");
        }
    });

    let completion: CommandInputWaitOutcome = ev
        .wait_for_command_input(Some(std::time::Instant::now() + Duration::from_secs(1)))
        .expect("wait for command input");

    assert_eq!(completion, CommandInputWaitOutcome::InputPending);
    assert_eq!(ev.command_loop.keyboard.pending_input_events.len(), 1);
}

#[test]
fn process_service_accepts_wait_request_boundary() {
    let mut ev = Context::new();
    let request = ProcessOutputServiceRequest::none();

    let poll = ev
        .poll_process_output_for_service_request(&request)
        .expect("poll process output should not throw");
    let ready = ev
        .poll_ready_process_output_for_service_request(
            ProcessWaitEvents::ready_processes(Vec::new()),
            &request,
        )
        .expect("poll ready process output should not throw");
    let _: ProcessOutputServiceOutcome = poll;
    let _: ProcessOutputServiceOutcome = ready;

    assert!(!poll.has_any_process_activity());
    assert!(!ready.has_any_process_activity());
}

#[test]
fn timer_service_intent_exposes_side_effects_only() {
    let mut ev = Context::new();
    let _: () = ev
        .service_timers_without_redisplay()
        .expect("service timer wait intent");
}

#[test]
fn process_wait_events_use_structured_event_shape() {
    let processes = ProcessManager::new();

    let events = processes.wait_for_process_events(Duration::ZERO);
    let _: ProcessWaitEvents = events;

    assert!(!events.has_notification_wakeup());
    assert!(!events.has_ready_processes());
}

#[test]
fn wait_scheduler_remembers_cross_platform_notification_before_wait() {
    crate::test_utils::init_test_tracing();
    let processes = ProcessManager::new();
    assert!(processes.has_wait_notification_backend());
    let notifier = processes
        .wait_notifier()
        .expect("cross-platform wait notifier should be available");

    notifier
        .notify()
        .expect("remember notification issued before poller wait");

    let started = Instant::now();
    let events = processes
        .wait_for_backend_events(
            Duration::from_secs(2),
            ProcessWaitBackendInterest::NotificationsOnly,
        )
        .expect("poller backend should be available");

    assert!(events.has_notification_wakeup());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "a notification issued before wait must be remembered, not time out"
    );
}

#[test]
fn wait_scheduler_does_not_report_timeout_as_notification() {
    crate::test_utils::init_test_tracing();
    let processes = ProcessManager::new();

    let events = processes
        .wait_for_backend_events(
            Duration::from_millis(10),
            ProcessWaitBackendInterest::NotificationsOnly,
        )
        .expect("poller backend should be available");

    assert!(
        !events.has_notification_wakeup(),
        "an elapsed poll timeout is not a cross-thread notification"
    );
}

#[test]
fn accept_process_output_millis_contract_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err (accept-process-output nil 0.1 "x") (error err))
           (condition-case err (accept-process-output nil nil "x") (error err))
           (condition-case err (accept-process-output nil 1 "x") (error err))
           (condition-case err (accept-process-output nil 0.1 nil) (error err))
           (condition-case err (accept-process-output nil 0.1 0) (error err))
           (condition-case err (accept-process-output nil 1 2) (error err))"#,
    );
    assert_eq!(results[0], r#"OK (wrong-type-argument fixnump "x")"#);
    assert_eq!(results[1], r#"OK (wrong-type-argument fixnump "x")"#);
    assert_eq!(results[2], r#"OK (wrong-type-argument fixnump "x")"#);
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK (wrong-type-argument fixnump 0.1)");
    assert_eq!(results[5], "OK nil");
}

#[test]
fn accept_process_output_roots_callbacks_across_gc() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let result = ev.eval_str(&format!(
        r#"(progn
             (fset 'proc-root-filter
                   (lambda (_proc string)
                     (garbage-collect)
                     (setq proc-root-filter-data string)))
             (fset 'proc-root-sentinel
                   (lambda (_proc msg)
                     (setq proc-root-sentinel-data msg)))
             (setq proc-root-filter-data nil
                   proc-root-sentinel-data nil)
             (let ((p (make-process :name "proc-rooting"
                                    :buffer nil
                                    :command (list "{echo}" "out")
                                    :connection-type 'pipe)))
               (unwind-protect
                   (progn
                     (set-process-filter p 'proc-root-filter)
                     (set-process-sentinel p 'proc-root-sentinel)
                     (accept-process-output p 0.1)
                     (accept-process-output p 0.1)
                     (list proc-root-filter-data proc-root-sentinel-data))
                 (condition-case nil
                     (delete-process p)
                   (error nil)))))"#,
    ));
    assert_eq!(
        format_eval_result(&result),
        r#"OK ("out
" "finished
")"#
    );
}

#[test]
fn accept_process_output_waiting_for_target_still_services_other_processes() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((target (make-process :name "apio-target"
                                      :buffer nil
                                      :command (list "{cat}")
                                      :connection-type 'pipe))
                 (other (make-process :name "apio-other"
                                     :buffer nil
                                     :command (list "{echo}" "other")
                                     :connection-type 'pipe))
                 (other-output nil))
             (unwind-protect
                 (progn
                   (set-process-filter other
                                       (lambda (_proc string)
                                         (setq other-output
                                               (cons string other-output))))
                   (list (accept-process-output target 0.1)
                         (nreverse other-output)))
               (condition-case nil (delete-process target) (error nil))
               (condition-case nil (delete-process other) (error nil))))"#,
        ),
    );
    assert_eq!(
        result,
        r#"OK (nil ("other
"))"#
    );
}

/// JUST-THIS-ONE suspends READING FROM another process; it does not suspend
/// the NOTIFICATION of one that has exited.  GNU's wait notifies with
/// `status_notify (NULL, wait_proc)` (src/process.c:5554, :5854), and the NULL
/// first argument makes its `FOR_EACH_PROCESS` visit the whole alist
/// (:7886-7890), reading *"any output that remains"* for each visited process
/// before running its sentinel (:7896-7909).  `just_wait_proc` restricts the
/// SELECT MASK (`compute_read_wait_mask`) and nothing else.
///
/// **This pin asserted `OK (nil nil)`, and ledger 200 measured that that is
/// one arm of a BUILD-DEPENDENT race rather than a contract.**  20 runs of
/// this exact form, `-Q --batch`:
///
/// ```text
///   GNU Emacs 31.0.90                20/20   (nil ("other\n"))
///   target/release/neomacs           20/20   (nil ("other\n"))
///   this suite (debug)                       (nil nil)
/// ```
///
/// So the shipped build agrees with GNU and the debug build does not, which
/// makes the strict assertion a statement about `cargo nextest`'s timing.  It
/// is widened to the two arms, exactly as
/// `accept_process_output_direct_pty_reports_gnu_output_status_invariants` and
/// the two explicit-coding pins beside it are, and the invariant it keeps is
/// the one it is actually for: a just-this-one wait returns `nil` and does not
/// error, hang, or read the TARGET's output.
///
/// The mechanism behind the spread is worth having: this port's whole-alist
/// walk (`record_child_status_changes`) runs only when a delivered SIGCHLD is
/// waiting at one of the wait's two drain points.
/// `a_just_this_one_wait_notifies_another_childs_sentinel_like_gnu` is the
/// shape where that is reliable -- the other child dies DURING the block -- and
/// it pins GNU's answer.  Ledger 200 tried to make the walk unconditional and
/// **that is measured and withdrawn**: it cost three melpa packages, because
/// the walk's notification drains a split `:stderr` pipe before the owner's
/// sentinel where GNU visits the pipe later in the alist (ledger 54's
/// ordering).  See ledger 200 §5.
#[test]
fn accept_process_output_just_this_one_suspends_other_processes() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((target (make-process :name "apio-target-only"
                                      :buffer nil
                                      :command (list "{cat}")
                                      :connection-type 'pipe))
                 (other (make-process :name "apio-other-only"
                                     :buffer nil
                                     :command (list "{echo}" "other")
                                     :connection-type 'pipe))
                 (other-output nil))
             (unwind-protect
                 (progn
                   (set-process-filter other
                                       (lambda (_proc string)
                                         (setq other-output
                                               (cons string other-output))))
                   (list (accept-process-output target 0.1 nil t)
                         (nreverse other-output)))
               (condition-case nil (delete-process target) (error nil))
               (condition-case nil (delete-process other) (error nil))))"#,
        ),
    );
    assert!(
        result == "OK (nil nil)"
            || result
                == r#"OK (nil ("other
"))"#,
        "a just-this-one wait must return nil without erroring; the two arms \
         are the docstring's measurement (GNU 20/20 and target/release/neomacs \
         20/20 answer the second, this suite answers the first).  Got: {result}"
    );
}

#[test]
fn accept_process_output_integer_just_this_one_suppresses_timers() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6))))
             (fset 'apio-wait-timer-callback
                   (lambda () (setq apio-wait-timer-fired t)))
             (setq apio-wait-timer-fired nil))"#,
    )
    .expect("install timer callback");

    let pid = ev.processes.create_process(
        "apio-wait-target".into(),
        Value::NIL,
        cat,
        Vec::new(),
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn target child");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "apio-wait-timer-callback",
        )]),
    );

    let first = builtin_accept_process_output(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::make_float(0.0),
            Value::NIL,
            Value::fixnum(1),
        ],
    )
    .expect("accept-process-output with integer just-this-one");
    let after_first = ev
        .eval_symbol("apio-wait-timer-fired")
        .expect("timer flag after timer-suppressed wait");
    let second = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.0)])
        .expect("accept-process-output should service timers without target restriction");
    let after_second = ev
        .eval_symbol("apio-wait-timer-fired")
        .expect("timer flag after unrestricted wait");

    assert_eq!(first, Value::NIL);
    assert_eq!(after_first, Value::NIL);
    assert_eq!(second, Value::NIL);
    assert_eq!(after_second, Value::T);
}

#[test]
fn accept_process_output_timer_preserves_deactivate_mark_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6))))
             (fset 'apio-timer-deactivate
                   (lambda () (setq deactivate-mark nil)))
             (setq deactivate-mark 'keep))"#,
    )
    .expect("install timer deactivate setup");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "apio-timer-deactivate",
        )]),
    );

    builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.05)])
        .expect("accept-process-output should service timer");

    assert_eq!(
        ev.eval_symbol("deactivate-mark")
            .expect("deactivate-mark after timer callback"),
        Value::symbol("keep")
    );
}

#[test]
fn accept_process_output_runs_timer_before_filter_and_sentinel_like_gnu() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq apio-order-events nil)
             (fset 'apio-order-timer
                   (lambda ()
                     (setq apio-order-events
                           (append apio-order-events '(timer)))))
             (fset 'apio-order-filter
                   (lambda (_proc string)
                     (setq apio-order-events
                           (append apio-order-events
                                   (list (list 'filter string))))))
             (fset 'apio-order-sentinel
                   (lambda (_proc msg)
                     (setq apio-order-events
                           (append apio-order-events
                                   (list (list 'sentinel msg)))))))"#,
    )
    .expect("install timer/filter/sentinel order setup");
    ev.eval_str(
        r#"(fset 'timer-event-handler
                 (lambda (timer)
                   (setq timer-list (delq timer timer-list))
                   (apply (aref timer 5) (aref timer 6))))"#,
    )
    .expect("install timer-event-handler stub");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "apio-order-timer",
        )]),
    );

    let pid = ev.processes.create_process(
        "apio-order".into(),
        Value::NIL,
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn ordering process");
    builtin_set_process_filter(
        &mut ev,
        vec![Value::make_process(pid), Value::symbol("apio-order-filter")],
    )
    .expect("install ordering filter");
    builtin_set_process_sentinel(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("apio-order-sentinel"),
        ],
    )
    .expect("install ordering sentinel");

    let first = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("first accept-process-output");
    let events_after_first = ev
        .eval_symbol("apio-order-events")
        .expect("ordering event list after first wait");
    assert_eq!(first, Value::T);
    let after_first = format!("{}", events_after_first);
    let after_filter = r#"(timer (filter "out
"))"#;
    let after_sentinel = r#"(timer (filter "out
") (sentinel "finished
"))"#;
    assert!(
        after_first == after_filter || after_first == after_sentinel,
        "unexpected timer/process order after first accept: {after_first}"
    );

    let second = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("second accept-process-output");
    let events_after_second = ev
        .eval_symbol("apio-order-events")
        .expect("ordering event list");

    assert_eq!(second, Value::NIL);
    assert_eq!(format!("{}", events_after_second), after_sentinel);
}

#[test]
fn accept_process_output_runs_gnu_timer_then_internal_timer_before_process_callbacks() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq apio-full-order nil)
             (fset 'apio-gnu-order-callback
                   (lambda ()
                     (setq apio-full-order
                           (append apio-full-order '(gnu)))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (funcall (aref timer 5))))
             (fset 'apio-rust-order-callback
                   (lambda ()
                     (setq apio-full-order
                           (append apio-full-order '(rust)))))
             (fset 'apio-full-order-filter
                   (lambda (_proc string)
                     (setq apio-full-order
                           (append apio-full-order
                                   (list (list 'filter string))))))
             (fset 'apio-full-order-sentinel
                   (lambda (_proc msg)
                     (setq apio-full-order
                           (append apio-full-order
                                   (list (list 'sentinel msg)))))))"#,
    )
    .expect("install mixed timer ordering setup");

    // Two due GNU timers in sorted order (timer.el's `timer--activate` keeps
    // `timer-list` sorted by trigger time, so the list head is the soonest);
    // due timers fire in list order, so the 2ms-overdue callback runs before
    // the 1ms-overdue one.
    ev.set_variable(
        "timer-list",
        Value::list(vec![
            gnu_timer_before(Duration::from_millis(2), "apio-gnu-order-callback"),
            gnu_timer_before(Duration::from_millis(1), "apio-rust-order-callback"),
        ]),
    );

    let pid = ev.processes.create_process(
        "apio-full-order".into(),
        Value::NIL,
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn mixed ordering process");
    builtin_set_process_filter(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("apio-full-order-filter"),
        ],
    )
    .expect("install mixed ordering filter");
    builtin_set_process_sentinel(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("apio-full-order-sentinel"),
        ],
    )
    .expect("install mixed ordering sentinel");

    let first = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("accept-process-output with mixed timer sources");
    let events_after_first = ev
        .eval_symbol("apio-full-order")
        .expect("mixed ordering event list");

    assert_eq!(first, Value::T);
    let after_first = format!("{}", events_after_first);
    let after_filter = r#"(gnu rust (filter "out
"))"#;
    let after_sentinel = r#"(gnu rust (filter "out
") (sentinel "finished
"))"#;
    assert!(
        after_first == after_filter || after_first == after_sentinel,
        "unexpected mixed timer/process order after first accept: {after_first}"
    );

    let second = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("second accept-process-output with mixed timer sources");
    let events_after_second = ev
        .eval_symbol("apio-full-order")
        .expect("mixed ordering event list after second wait");

    assert_eq!(second, Value::NIL);
    assert_eq!(format!("{}", events_after_second), after_sentinel);
}

#[test]
fn accept_process_output_runs_default_process_filter() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let _ = ev.buffers.create_buffer("*apio-default-filter*");
    let pid = ev.processes.create_process(
        "apio-default-filter".into(),
        Value::make_buffer(
            ev.buffers
                .find_buffer_by_name("*apio-default-filter*")
                .expect("process buffer should exist"),
        ),
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn output process");

    assert_eq!(
        builtin_process_filter(&mut ev, vec![Value::make_process(pid)]).expect("process-filter"),
        Value::symbol("internal-default-process-filter")
    );

    let first = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("first accept-process-output");
    let second = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("second accept-process-output");
    let buf_id = ev
        .buffers
        .find_buffer_by_name("*apio-default-filter*")
        .expect("default filter should create process buffer");
    let text = ev
        .buffers
        .get(buf_id)
        .expect("process buffer")
        .buffer_string();

    assert_eq!(first, Value::T);
    assert!(
        second == Value::T || second == Value::NIL,
        "second wait should either observe terminal status or find no remaining activity"
    );
    assert_eq!(text, "out\n\nProcess apio-default-filter finished\n");
}

#[test]
fn accept_process_output_discards_output_when_filter_is_t() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*apio-discard-filter*");
    let pid = ev.processes.create_process(
        "apio-discard-filter".into(),
        Value::make_buffer(buffer_id),
        echo,
        vec!["discarded".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    builtin_set_process_filter(&mut ev, vec![Value::make_process(pid), Value::T])
        .expect("set t process filter");
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn output process");

    while ev
        .processes
        .get(pid)
        .is_some_and(|process| process.status.is_symbol_named("run"))
    {
        builtin_accept_process_output(
            &mut ev,
            vec![Value::make_process(pid), Value::make_float(0.1)],
        )
        .expect("a t filter must discard output without being called");
    }

    assert_eq!(
        builtin_process_filter(&mut ev, vec![Value::make_process(pid)]).expect("process-filter"),
        Value::T
    );
    assert_eq!(
        ev.buffers
            .get(buffer_id)
            .expect("process buffer")
            .buffer_string(),
        "\nProcess apio-discard-filter finished\n",
        "a t process filter must discard process output but retain GNU's status message"
    );
}

#[test]
fn process_filter_t_suspends_output_until_filter_is_resumed() {
    crate::test_utils::init_test_tracing();
    let shell = find_bin("sh");
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*filter-resume*");
    let pid = ev.processes.create_process(
        "filter-resume".into(),
        Value::make_buffer(buffer_id),
        shell,
        vec!["-c".into(), "printf held; sleep 0.3; printf later".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    builtin_set_process_filter(&mut ev, vec![Value::make_process(pid), Value::T])
        .expect("suspend process output");
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn suspended output process");

    std::thread::sleep(Duration::from_millis(50));
    builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.05)],
    )
    .expect("waiting while output is suspended");
    builtin_set_process_filter(&mut ev, vec![Value::make_process(pid), Value::NIL])
        .expect("resume the default process filter");

    while ev
        .processes
        .get(pid)
        .is_some_and(|process| process.status.is_symbol_named("run"))
    {
        builtin_accept_process_output(
            &mut ev,
            vec![Value::make_process(pid), Value::make_float(0.5)],
        )
        .expect("accept resumed process output");
    }

    assert_eq!(
        ev.buffers
            .get(buffer_id)
            .expect("process buffer")
            .buffer_string(),
        "heldlater\nProcess filter-resume finished\n",
        "GNU leaves bytes unread while filter t suspends process output"
    );
}

#[test]
fn accept_process_output_target_terminated_reports_exit_after_ready_output() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let spawned = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(progn
                 (setq apio-ready-exit-buffer
                       (get-buffer-create "*apio-ready-exit*"))
                 (setq apio-ready-exit-process
                       (make-process :name "apio-ready-exit"
                                     :buffer apio-ready-exit-buffer
                                     :command (list "{echo}" "out")
                                     :connection-type 'pipe))
                 'spawned)"#
        ),
    );
    assert_eq!(spawned, "OK spawned");

    std::thread::sleep(Duration::from_millis(200));

    let result = eval_one_in_context(
        &mut ev,
        r#"(let ((accepted (accept-process-output apio-ready-exit-process 0.5)))
             (list accepted (process-status apio-ready-exit-process)))"#,
    );
    let buffer_id = ev
        .buffers
        .find_buffer_by_name("*apio-ready-exit*")
        .expect("process buffer");
    let text = ev
        .buffers
        .get(buffer_id)
        .expect("process buffer")
        .buffer_string();
    let killed = eval_one_in_context(&mut ev, "(kill-buffer apio-ready-exit-buffer)");

    // GNU contract for THIS scenario (child exited during a raw thread sleep,
    // so nothing was serviced before the wait): the wait drains the pending
    // output -- the drained bytes count as got_some_output (process.c:5588),
    // so `accept-process-output` returns t -- and the terminated target then
    // ends the wait; `process-status` decodes the reaped status (`exit`) at
    // observation (GNU Fprocess_status runs update_status). Note: when a
    // Lisp-visible wait (e.g. `sleep-for`) runs BEFORE `accept-process-output`
    // it services output + EOF + sentinel first, and the later targeted wait
    // returns nil with the "Process NAME finished" line already inserted --
    // verified byte-identical against emacs --batch for that variant.
    assert_eq!(result, "OK (t exit)");
    assert_eq!(killed, "OK t");
    // The default sentinel's "Process NAME finished" line may or may not have
    // landed by the time the buffer is read, and GNU emits it with either one
    // or two newlines depending on bolp state at insertion (both shapes
    // observed in emacs --batch probes: "out\nProcess x finished\n" and
    // "hello\n\nProcess neo-cx423-eof finished\n").
    assert!(
        text == "out\n"
            || text == "out\nProcess apio-ready-exit finished\n"
            || text == "out\n\nProcess apio-ready-exit finished\n",
        "unexpected buffer content: {text:?}"
    );
}

#[test]
fn accept_process_output_runs_pty_status_notification_after_output() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-pty-finish*")))
                 (set-buffer buf)
                 (insert "preexisting")
                 (let ((proc (make-process :name "apio-pty-finish"
                                           :buffer buf
                                           :command (list "{echo}" "process-output"))))
                   (accept-process-output proc 1)
                   (let ((first-status (process-status proc)))
                     (accept-process-output proc 1)
                     (list (if (memq first-status '(run exit signal)) t)
                           (process-status proc)
                           (buffer-string)
                           (kill-buffer buf)))))"#
        ),
    );

    assert_eq!(
        result,
        "OK (t exit \"preexistingprocess-output\n\nProcess apio-pty-finish finished\n\" t)"
    );
}

#[test]
fn accept_process_output_defers_pty_status_after_explicit_coding() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-pty-coding*")))
                 (set-buffer buf)
                 (let ((proc (make-process :name "apio-pty-coding"
                                           :buffer buf
                                           :command (list "{echo}" "hello"))))
                   (set-process-query-on-exit-flag proc nil)
                   (set-process-coding-system proc 'utf-8-unix 'utf-8-unix)
                   (accept-process-output proc 1)
                   (prog1 (list (process-status proc)
                                (equal (substring (buffer-string) 0 6) "hello\n"))
                     (kill-buffer buf))))"#
        ),
    );

    // Whether the child's exit has been observed by the time the wait returns
    // is a race in GNU too (measured: `emacs --batch` gives `run` in ~2/3 of
    // runs and `exit` -- with the default sentinel's "finished" line -- in
    // ~1/3). The invariants are: the explicitly-set coding system decoded the
    // output ("hello\n" reached the buffer first) and the status is one of
    // run/exit. Pinning `run` here would pin the pre-observation-decode
    // masking that GNU does not have.
    assert!(
        result == "OK (run t)" || result == "OK (exit t)",
        "unexpected pty status/output after explicit coding: {result}"
    );
}

#[test]
fn accept_process_output_pipe_reports_gnu_output_status_invariants() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let* ((buf (get-buffer-create " *apio-pipe-reap*"))
                      (proc (make-process :name "apio-pipe-reap"
                                          :command (list "{echo}" "x")
                                          :buffer buf
                                          :connection-type 'pipe
                                          :sentinel (lambda (&rest _) nil))))
                 (set-process-query-on-exit-flag proc nil)
                 (accept-process-output proc 1)
                 (prog1 (list (process-type proc)
                              (process-status proc)
                              (memq proc (process-list))
                              (save-current-buffer
                                (set-buffer buf)
                                (buffer-string)))
                   (kill-buffer buf)))"#
        ),
    );

    assert!(
        result == "OK (real exit nil \"x\n\")"
            || result == "OK (real run (#<process apio-pipe-reap>) \"x\n\")",
        "unexpected pipe status/output after accept-process-output: {result}"
    );
}

#[test]
fn accept_process_output_direct_pty_reports_gnu_output_status_invariants() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-direct-pty*")))
                 (save-current-buffer
                   (set-buffer buf)
                   (insert "preexisting"))
                 (let ((proc (make-process :name "apio-direct-pty"
                                           :command (list "{echo}" "process-output")
                                           :buffer buf)))
                   (accept-process-output proc 1)
                   (prog1 (list (process-type proc)
                                (process-status proc)
                                (if (process-tty-name proc) t)
                                (memq proc (process-list))
                                (save-current-buffer
                                  (set-buffer buf)
                                  (buffer-string)))
                     (kill-buffer buf))))"#
        ),
    );

    // GNU's MINIMUM pass after reading PTY output is non-blocking
    // (`wait_reading_process_output`, process.c).  The child can therefore
    // still be `run`, or its exit and default sentinel can already have been
    // observed.  Both states occurred repeatedly with the same GNU build;
    // the invariant is that the complete child output is present first.
    assert!(
        result == "OK (real run t (#<process apio-direct-pty>) \"preexistingprocess-output\n\")"
            || result
                == "OK (real exit t nil \"preexistingprocess-output\n\nProcess apio-direct-pty finished\n\")",
        "unexpected direct PTY status/output after accept-process-output: {result}"
    );
}

#[test]
fn accept_process_output_decodes_multibyte_before_explicit_coding_status() {
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-pty-coding-mb*")))
                 (set-buffer buf)
                 (let ((proc (make-process :name "apio-pty-coding-mb"
                                           :buffer buf
                                           :command (list "{printf}" "%s" "café世界"))))
                   (set-process-query-on-exit-flag proc nil)
                   (set-process-coding-system proc 'utf-8-unix 'utf-8-unix)
                   (accept-process-output proc 1)
                   (prog1 (list (buffer-string)
                                (string-bytes (buffer-string))
                                (length (buffer-string)))
                     (kill-buffer buf))))"#
        ),
    );

    // GNU's own answer here is NOT determinate, and the neighbouring
    // `accept_process_output_direct_pty_*` pin already says so for the same
    // reason: the MINIMUM pass after reading PTY output is non-blocking, so
    // the child's exit and its default sentinel may or may not have been
    // observed by the time the single `accept-process-output` returns.
    // Measured, `-Q --batch`, GNU Emacs 31.0.90, 10 runs of this exact form:
    //
    //     9/10   "café世界\nProcess apio-pty-coding-mb finished\n"  48  43
    //     1/10   "café世界"                                          11   6
    //
    // The invariant is the one this pin is actually for: the complete decoded
    // multibyte output is present, and it is present BEFORE any status
    // message.  Ledger 200 widened the assertion after making the whole-alist
    // walk unconditional moved this port from the 1/10 arm to the 9/10 arm.
    assert!(
        result == "OK (\"café世界\" 11 6)"
            || result == "OK (\"café世界\nProcess apio-pty-coding-mb finished\n\" 48 43)",
        "unexpected multibyte output/status after accept-process-output: {result}"
    );
}

#[test]
fn accept_process_output_with_temp_buffer_defers_explicit_coding_status() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-pty-coding-temp*")))
                 (set-buffer buf)
                 (let ((proc (make-process :name "apio-pty-coding-temp"
                                           :buffer (current-buffer)
                                           :command (list "{echo}" "hello"))))
                   (set-process-query-on-exit-flag proc nil)
                   (set-process-coding-system proc 'utf-8-unix 'utf-8-unix)
                   (accept-process-output proc 1))
                 (prog1 (buffer-string)
                   (kill-buffer buf)))"#
        ),
    );

    // As above: GNU is not determinate here.  Measured, `-Q --batch`, GNU
    // Emacs 31.0.90, 10 runs of this exact form:
    //
    //     6/10   "hello\n"
    //     4/10   "hello\n\nProcess apio-pty-coding-temp finished\n"
    //
    // The invariant is that the child's output is complete and comes first.
    assert!(
        result == "OK \"hello\n\""
            || result == "OK \"hello\n\nProcess apio-pty-coding-temp finished\n\"",
        "unexpected output/status after accept-process-output: {result}"
    );
}

#[test]
fn second_accept_process_output_publishes_deferred_explicit_coding_status() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-pty-coding-second*")))
                 (set-buffer buf)
                 (let ((proc (make-process :name "apio-pty-coding-second"
                                           :buffer buf
                                           :command (list "{echo}" "hello"))))
                   (set-process-query-on-exit-flag proc nil)
                   (set-process-coding-system proc 'utf-8-unix 'utf-8-unix)
                   (accept-process-output proc 1)
                   (accept-process-output proc 1)
                   (prog1 (list (process-status proc) (buffer-string))
                     (kill-buffer buf))))"#
        ),
    );

    assert_eq!(
        result,
        "OK (exit \"hello\n\nProcess apio-pty-coding-second finished\n\")"
    );
}

#[test]
fn process_live_p_loop_runs_pending_default_sentinel() {
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-live-loop*")))
                 (fset 'apio-live-p
                       (lambda (process)
                         (and (processp process)
                              (memq (process-status process)
                                    '(run open listen connect stop)))))
                 ;; `start-process' is Lisp in GNU (lisp/subr.el:3466) and has no
                 ;; Rust subr since DIVERGENCES.md 149; a bare `Context' is GNU
                 ;; before `loadup.el', so the launcher here is the C primitive
                 ;; the Lisp one calls (src/process.c:1767).
                 (let ((proc (make-process
                              :name "apio-live-loop" :buffer buf
                              :command (list "{printf}" "X%sY" "MID"))))
                   (set-process-query-on-exit-flag proc nil)
                   (while (apio-live-p proc)
                     (accept-process-output proc 1))
                   (prog1 (progn (set-buffer buf) (buffer-string))
                     (fmakunbound 'apio-live-p)
                     (kill-buffer buf))))"#
        ),
    );

    assert_eq!(result, "OK \"XMIDY\nProcess apio-live-loop finished\n\"");
}

/// `kill-buffer` hangs up an attached real process, and its sentinel sees
/// `signal` with a dead buffer.
///
/// **The loop waits for the SENTINEL, not for `process-status`, and that is
/// deliberate (ledger 180).**  GNU records a child's death from the SIGCHLD
/// handler (`handle_child_signal`, src/process.c:7691) and runs the sentinel
/// only later, in `status_notify` -- so a loop spinning on
/// `(memq (process-status p) '(run ...))` can exit before the sentinel it is
/// waiting for has run.  That is GNU's own behaviour, measured: after
/// `kill-buffer`, a PURE-Lisp spin (no `accept-process-output` anywhere) sees
/// GNU's `process-status` become `signal` after 86-215 iterations, about
/// 0.1 ms, with the sentinel list still `nil`.  The original shape passed in
/// GNU only because SIGCHLD delivery lags the first `process-status` call by
/// those ~50 us; it is a race that GNU happens to win, not a guarantee.
///
/// Re-derived against GNU Emacs 31.0.90 in the shape below, 5 runs of 5:
///
/// ```text
/// (signal nil t nil nil (("hangup\n" signal nil)))
/// ```
#[test]
fn kill_buffer_hangups_attached_real_process() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut ev = Context::new();

    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(let ((buf (get-buffer-create " *apio-kill-buffer-hup*"))
                     (log nil))
                 (let ((proc (make-process
                              :name "apio-kill-buffer-hup"
                              :buffer buf
                              :command (list "{sh}" "-c" "read line")
                              :connection-type 'pipe
                              :sentinel
                              (lambda (p e)
                                (setq log
                                      (cons (list e
                                                  (process-status p)
                                                  (buffer-live-p (process-buffer p)))
                                            log))))))
                   (set-process-query-on-exit-flag proc nil)
                   (kill-buffer buf)
                   (let ((i 0))
                     (while (and (null log) (< i 100))
                       (accept-process-output proc 0.05)
                       (setq i (1+ i))))
                   (prog1 (list (process-status proc)
                                (memq (process-status proc) '(run open listen connect stop))
                                (bufferp (process-buffer proc))
                                (buffer-live-p (process-buffer proc))
                                (marker-buffer (process-mark proc))
                                log)
                     (if (memq (process-status proc) '(run open listen connect stop))
                         (delete-process proc)
                       nil))))"#
        ),
    );

    assert_eq!(
        result,
        "OK (signal nil t nil nil ((\"hangup\n\" signal nil)))"
    );
}

#[test]
fn accept_process_output_restores_current_buffer_and_match_data() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(fset 'apio-restore-filter
                  (lambda (_proc _string)
                    (set-buffer (get-buffer-create "*apio-restore-other*"))
                    (string-match "bb" "abba")))"#,
    )
    .expect("install restore filter");

    let home_id = ev.buffers.create_buffer("*apio-restore-home*");
    assert!(ev.buffers.switch_current(home_id));
    let _ = eval_one_in_context(&mut ev, r#"(string-match "yz" "xyz")"#);
    let before_match_data = ev
        .eval_str("(match-data)")
        .expect("capture match-data before callback");
    let before_buffer = ev.buffers.current_buffer_id();

    let pid = ev.processes.create_process(
        "apio-restore".into(),
        Value::NIL,
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn restore process");
    builtin_set_process_filter(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("apio-restore-filter"),
        ],
    )
    .expect("install process filter");

    let result = builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("accept-process-output with restoring filter");
    let after_match_data = ev
        .eval_str("(match-data)")
        .expect("capture match-data after callback");

    assert_eq!(result, Value::T);
    assert_eq!(ev.buffers.current_buffer_id(), before_buffer);
    assert_eq!(after_match_data, before_match_data);
}

#[test]
fn accept_process_output_preserves_process_callback_runtime_state() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(progn
                 (fset 'apio-state-filter
                       (lambda (_proc string)
                         (setq apio-state-filter-observed
                               (list (current-buffer)
                                     (match-data)
                                     deactivate-mark
                                     last-nonmenu-event))
                         (set-buffer (get-buffer-create "*apio-state-other*"))
                         (string-match "bb" "abba")
                         (setq deactivate-mark nil)
                         (setq last-nonmenu-event 'changed)
                         (setq apio-state-filter-string string)))
                 (fset 'apio-state-sentinel
                       (lambda (_proc msg)
                         (setq apio-state-sentinel-observed
                               (list (current-buffer)
                                     (match-data)
                                     deactivate-mark
                                     last-nonmenu-event))
                         (set-buffer (get-buffer-create "*apio-state-other*"))
                         (string-match "cc" "acca")
                         (setq deactivate-mark nil)
                         (setq last-nonmenu-event 'changed)
                         (setq apio-state-sentinel-msg msg)))
                 (setq apio-state-filter-observed nil
                       apio-state-sentinel-observed nil
                       apio-state-filter-string nil
                       apio-state-sentinel-msg nil
                       last-nonmenu-event 'before
                       deactivate-mark 'keep)
                 (let ((home (get-buffer-create "*apio-state-home*")))
                   (set-buffer home)
                   (string-match "yz" "xyz")
                   (let ((before-buffer (current-buffer))
                         (before-match (match-data))
                         (p (make-process :name "apio-state"
                                          :buffer nil
                                          :command (list "{echo}" "out")
                                          :connection-type 'pipe)))
                     (unwind-protect
                         (progn
                           (set-process-filter p 'apio-state-filter)
                           (set-process-sentinel p 'apio-state-sentinel)
                           (accept-process-output p 0.1)
                           (accept-process-output p 0.1)
                           (list apio-state-filter-string
                                 apio-state-sentinel-msg
                                 (eq (current-buffer) before-buffer)
                                 (equal (match-data) before-match)
                                 deactivate-mark
                                 last-nonmenu-event
                                 (eq (nth 0 apio-state-filter-observed) before-buffer)
                                 (equal (nth 1 apio-state-filter-observed) before-match)
                                 (nth 2 apio-state-filter-observed)
                                 (nth 3 apio-state-filter-observed)
                                 (eq (nth 0 apio-state-sentinel-observed) before-buffer)
                                 (equal (nth 1 apio-state-sentinel-observed) before-match)
                                 (nth 2 apio-state-sentinel-observed)
                                 (nth 3 apio-state-sentinel-observed)))
                       (condition-case nil
                           (delete-process p)
                         (error nil))))))"#,
        ),
    );
    assert_eq!(
        result,
        r#"OK ("out
" "finished
" t t nil before t t nil t t t nil t)"#
    );
}

#[test]
fn network_delete_process_sentinel_uses_shared_callback_runtime_state() {
    crate::test_utils::init_test_tracing();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("listener local addr").port();
    let accept_thread = std::thread::spawn(move || {
        let _ = listener.accept();
    });
    let mut ev = Context::new();
    let result = eval_one_in_context(
        &mut ev,
        &format!(
            r#"(progn
             (fset 'apio-net-open-sentinel
                   (lambda (_proc msg)
                     (setq apio-net-open-state
                           (list msg
                                 (eq (current-buffer) apio-net-before-buffer)
                                 (equal (match-data) apio-net-before-match)
                                 deactivate-mark
                                 last-nonmenu-event))
                     (set-buffer (get-buffer-create "*apio-net-other*"))
                     (string-match "bb" "abba")
                     (setq deactivate-mark nil)
                     (setq last-nonmenu-event 'changed)))
             (setq last-nonmenu-event 'before
                   deactivate-mark 'keep
                   apio-net-open-state nil)
             (let ((home (get-buffer-create "*apio-net-home*")))
               (set-buffer home)
               (string-match "yz" "xyz")
               (setq apio-net-before-buffer (current-buffer)
                     apio-net-before-match (match-data))
               (let* ((p (make-network-process :name "apio-net-open"
                                                :host "127.0.0.1"
                                                :service {port}
                                                :sentinel 'apio-net-open-sentinel))
                      ;; GNU fires NO sentinel for a synchronous (non-:nowait)
                      ;; connect (`connect_network_socket` never calls
                      ;; `exec_sentinel`), so the state must still be nil here.
                      (state-after-create apio-net-open-state))
                 ;; `delete-process` on a network connection runs the sentinel
                 ;; synchronously with "deleted\n" (Fdelete_process ->
                 ;; status_notify), under the callback runtime-state rules.
                 (delete-process p)
                 (list state-after-create
                       (car apio-net-open-state)
                       (nth 1 apio-net-open-state)
                       (nth 2 apio-net-open-state)
                       (nth 3 apio-net-open-state)
                       (nth 4 apio-net-open-state)
                       (eq (current-buffer) apio-net-before-buffer)
                       (equal (match-data) apio-net-before-match)
                       deactivate-mark
                       last-nonmenu-event))))"#,
        ),
    );
    let _ = accept_thread.join();
    assert_eq!(
        result,
        r#"OK (nil "deleted
" t t nil t t t nil before)"#
    );
}

#[test]
fn sleep_for_uses_shared_wait_request_for_process_output_and_timers() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'sleep-shared-filter
                   (lambda (_proc string) (setq sleep-shared-output string)))
             (fset 'sleep-shared-timer
                   (lambda () (setq sleep-shared-timer-fired 'done)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6))))
             (setq sleep-shared-output nil
                   sleep-shared-timer-fired nil))"#,
    )
    .expect("install sleep-for callback setup");

    let pid = ev.processes.create_process(
        "sleep-shared".into(),
        Value::NIL,
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn sleep-for process");
    builtin_set_process_filter(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("sleep-shared-filter"),
        ],
    )
    .expect("install sleep-for process filter");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "sleep-shared-timer",
        )]),
    );

    crate::emacs_core::timer::builtin_sleep_for(&mut ev, vec![Value::make_float(0.05)])
        .expect("sleep-for should use the shared wait request");

    assert_eq!(
        ev.eval_symbol("sleep-shared-output")
            .expect("sleep-for process output variable"),
        Value::string("out\n")
    );
    assert_eq!(
        ev.eval_symbol("sleep-shared-timer-fired")
            .expect("sleep-for timer variable"),
        Value::symbol("done")
    );
}

#[test]
fn accept_process_output_services_pending_resize_from_shared_wait_request() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames
        .get_mut(fid)
        .expect("frame should exist")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .expect("queue focus event");
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .expect("queue resize event");

    let result = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.01)])
        .expect("accept-process-output should service wait-request special input");
    drop(tx);

    let width = crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![])
        .expect("frame-native-width should succeed");
    let height = crate::emacs_core::frame::builtin_frame_native_height(&mut ev, vec![])
        .expect("frame-native-height should succeed");

    assert_eq!(result, Value::NIL);
    assert_eq!(width, Value::fixnum(700));
    assert_eq!(height, Value::fixnum(800));
}

#[test]
fn accept_process_output_services_resize_arriving_during_wait() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames
        .get_mut(fid)
        .expect("frame should exist")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let sender = tx.clone();
    let resize_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        sender
            .send(crate::keyboard::InputEvent::Resize {
                width: 710,
                height: 820,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            })
            .expect("queue resize event during wait");
    });

    let result = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.05)])
        .expect("accept-process-output should service resize arriving during wait");
    resize_thread.join().expect("resize sender thread");
    drop(tx);

    let width = crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![])
        .expect("frame-native-width should succeed");
    let height = crate::emacs_core::frame::builtin_frame_native_height(&mut ev, vec![])
        .expect("frame-native-height should succeed");

    assert_eq!(result, Value::NIL);
    assert_eq!(width, Value::fixnum(710));
    assert_eq!(height, Value::fixnum(820));
}

#[test]
fn accept_process_output_window_close_uses_special_event_map_handler_when_loaded() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffer_manager_mut().create_buffer("*scratch*");
    ev.buffer_manager_mut().set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    install_minimal_special_event_command_runtime(&mut ev);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose {
        emacs_frame_id: frame.0,
    })
    .expect("queue window close");
    ev.input_rx = Some(rx);
    ev.command_loop.running = true;

    let result = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.0)])
        .expect("accept-process-output should consume handled window close");
    drop(tx);

    assert_eq!(result, Value::NIL);
    let logged = ev
        .eval_symbol("neo-last-delete-frame-event")
        .expect("delete-frame event should be logged");
    assert_eq!(
        logged,
        Value::list(vec![
            Value::symbol("delete-frame"),
            Value::list(vec![Value::make_frame(frame.0)]),
        ]),
    );
}

#[test]
fn accept_process_output_window_close_quits_without_special_handler() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose { emacs_frame_id: 0 })
        .expect("queue window close");
    ev.input_rx = Some(rx);

    let flow = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.0)])
        .expect_err("unhandled window close should still quit");
    drop(tx);

    assert!(matches!(flow, Flow::Signal(ref sig) if sig.symbol_name() == "quit"));
}

#[test]
fn accept_process_output_window_close_honors_throw_on_input_before_quit() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose { emacs_frame_id: 0 })
        .expect("queue window close");
    ev.input_rx = Some(rx);
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let flow = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.0)])
        .expect_err("throw-on-input should interrupt accept-process-output");
    assert!(matches!(
        flow,
        Flow::Throw(ref thrown)
            if thrown.tag == Value::symbol("tag") && thrown.value == Value::T
    ));

    ev.obarray.set_symbol_value("throw-on-input", Value::NIL);
    let flow = builtin_accept_process_output(&mut ev, vec![Value::NIL, Value::make_float(0.0)])
        .expect_err("window close should still quit afterwards");
    drop(tx);

    assert!(matches!(flow, Flow::Signal(ref sig) if sig.symbol_name() == "quit"));
}

/// GNU `wait_reading_process_output` runs `maybe_quit` at the top of every
/// `while(1)` iteration when `read_kbd >= 0` (process.c:5399-5400).  `Fsleep_for`
/// passes `read_kbd = 0`, so a pending C-g promotes to a `quit` signal within one
/// iteration instead of running for the full deadline.  Before the fix our wait
/// loop omitted `maybe_quit`, so `(sleep-for N)` ignored C-g for its full
/// duration.  Here we set the cross-thread `quit_requested` atomic (as the input
/// bridge does) and assert a 5s wait returns a `quit` signal PROMPTLY.
#[test]
fn wait_until_honors_pending_quit_request_promptly() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // Simulate the input-bridge thread flagging a pending C-g while the
    // evaluator is blocked in a wait.
    ev.quit_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let start = std::time::Instant::now();
    let flow = ev
        .wait_until(deadline)
        .expect_err("a pending quit must interrupt the wait with a quit signal");
    let elapsed = start.elapsed();

    assert!(
        matches!(flow, Flow::Signal(ref sig) if sig.symbol_name() == "quit"),
        "expected a `quit' signal, got {flow:?}"
    );
    // Must return WELL under the 5s deadline — within one wait iteration.
    assert!(
        elapsed < Duration::from_secs(1),
        "wait should honor the pending quit promptly, took {elapsed:?}"
    );
    // The atomic must be drained so a subsequent poll doesn't re-fire.
    assert!(
        !ev.quit_requested.load(std::sync::atomic::Ordering::Relaxed),
        "quit_requested should be cleared after the wait drains it"
    );
}

/// GNU `maybe_quit` returns without signaling when `inhibit-quit` is non-nil;
/// `accept-process-output` is documented to block with quit inhibited.  A pending
/// quit under `inhibit-quit` must therefore NOT interrupt the wait — it must run
/// to its deadline.  Guards that the per-iteration `maybe_quit` stays
/// inhibit-quit-safe.
#[test]
fn wait_until_respects_inhibit_quit() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // Set inhibit-quit through the normal setq path so the cached runtime
    // field stays in sync (GNU's specbind of Qinhibit_quit).
    ev.eval_str("(setq inhibit-quit t)")
        .expect("bind inhibit-quit");
    ev.quit_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let deadline = std::time::Instant::now() + Duration::from_millis(50);
    ev.wait_until(deadline)
        .expect("inhibit-quit must suppress the pending quit, letting the wait elapse");
}

#[test]
fn process_mark_type_thread_send_and_running_child_runtime_surface() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        r#"(let ((p (start-process "proc-mark-type-thread-send" nil "{cat}")))
             (unwind-protect
                 (list
                  (processp p)
                  (eq (process-type p) 'real)
                  (not (processp (process-thread p)))
                  (markerp (process-mark p))
                  (marker-buffer (process-mark p))
                  (marker-position (process-mark p))
                  (process-running-child-p p)
                  (processp (process-send-eof p))
                  (with-temp-buffer
                    (insert "abc")
                    (process-send-region p (point-min) (point-max)))
                  (delete-process p)
                  (process-live-p p))
               (ignore-errors (delete-process p))))
           (condition-case err (process-send-eof) (error (car err)))
           (condition-case err (process-running-child-p) (error (car err)))
           (condition-case err (process-mark 'x) (error err))
           (condition-case err (process-type 'x) (error err))
           (condition-case err (process-thread 'x) (error err))
           (condition-case err (process-send-region 'x 1 1) (error err))
           (condition-case err (process-send-eof 'x) (error err))
           (condition-case err (process-running-child-p 'x) (error err))
           (condition-case err (process-send-eof nil nil) (error (car err)))
           (condition-case err (process-running-child-p nil nil) (error (car err)))"#,
    ));
    assert_eq!(results[0], "OK (t t t t nil nil nil t nil nil nil)");
    assert_eq!(results[1], "OK error");
    assert_eq!(results[2], "OK error");
    assert_eq!(results[3], "OK (wrong-type-argument processp x)");
    assert_eq!(results[4], "OK (wrong-type-argument processp x)");
    assert_eq!(results[5], "OK (wrong-type-argument processp x)");
    assert_eq!(results[6], "OK (wrong-type-argument processp x)");
    assert_eq!(results[7], "OK (wrong-type-argument processp x)");
    assert_eq!(results[8], "OK (wrong-type-argument processp x)");
    assert_eq!(results[9], "OK wrong-number-of-arguments");
    assert_eq!(results[10], "OK wrong-number-of-arguments");
}

#[test]
fn pipe_process_send_after_eof_discards_input_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let buffers = crate::buffer::BufferManager::new();
    let mut pm = ProcessManager::new();
    let id = pm.create_process_with_kind_lisp(
        LispString::from_utf8("send-after-eof-pipe"),
        Value::NIL,
        LispString::from_utf8(&cat),
        vec![],
        ProcessKindWithoutDevice::Real,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    pm.spawn_child(id, false).expect("spawn pipe child");

    assert_eq!(
        builtin_process_send_eof_impl(&mut pm, &buffers, vec![Value::make_process(id)])
            .expect("process-send-eof"),
        Value::make_process(id)
    );
    assert!(
        pm.get(id)
            .is_some_and(|proc| proc.input_disposition == ProcessInputDisposition::Discard),
        "pipe EOF should install the GNU-style discard sink"
    );
    assert!(
        pm.get(id).is_some_and(|proc| proc.eof_sent_to_process),
        "explicit EOF should be recorded for GNU-style same-wait status delivery"
    );
    builtin_process_send_string_impl(
        &mut pm,
        &buffers,
        vec![Value::make_process(id), Value::string("after-eof")],
    )
    .expect("post-EOF process-send-string should succeed");
    assert_eq!(pm.get(id).map(|proc| proc.write_queue), Some(Value::NIL));

    pm.delete_process(id);
}

#[test]
fn process_send_eof_delivers_pty_eot_after_queued_input() {
    crate::test_utils::init_test_tracing();
    let shell = find_bin("sh");
    let results = eval_all(&format!(
        r##"(let* ((process-connection-type t)
                   (output (generate-new-buffer " *pty-eof-output*"))
                   (stderr (make-pipe-process
                            :name "pty-eof-stderr"
                            :noquery t
                            :filter #'ignore))
                   (process (make-process
                             :name "pty-eof"
                             :buffer output
                             :command '("{shell}" "-c" "tr '[:lower:]' '[:upper:]'")
                             :stderr stderr
                             :sentinel #'ignore
                             :noquery t))
                   (deadline (+ (float-time) 1.0)))
              (unwind-protect
                  (progn
                    (process-send-string process "one two\nthree\n")
                    (process-send-eof process)
                    (while (and (process-live-p process)
                                (< (float-time) deadline))
                      (accept-process-output process 0.05))
                    (list
                     (with-current-buffer output (buffer-string))
                     (process-status process)
                     (stringp (process-tty-name process 'stdin))))
                (ignore-errors (delete-process process))
                (ignore-errors (delete-process stderr))
                (kill-buffer output)))"##,
    ));
    assert_eq!(
        results,
        [r#"OK ("ONE TWO
THREE
" exit t)"#]
    );
}

#[test]
fn process_coding_tty_and_kill_buffer_query_runtime_surface() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let results = eval_all(&format!(
        // Bind `default-process-coding-system` to its real-startup value
        // (mule-cmds.el sets it to (utf-8-unix . utf-8-unix); the minimal unit
        // bootstrap leaves it at (undecided-unix . utf-8-unix)). A buffer-less
        // network process with no :coding derives its coding from this
        // variable, exactly like GNU `set_network_socket_coding_system`.
        //
        // `let*`, not `let`: plain `let` evaluates every init form before any
        // binding takes effect, so `start-process` would run under the ambient
        // value and never see the one this form exists to establish.  That was
        // invisible while `make-process` ignored the variable outright
        // (DIVERGENCES.md entry 131) and the process coding was a Rust literal.
        r#"(let* ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                  (p (start-process "proc-coding-tty-query" nil "{cat}")))
             (unwind-protect
                 (list
                  (equal (process-coding-system p) '(utf-8-unix . utf-8-unix))
                  (process-datagram-address p)
                  (process-inherit-coding-system-flag p)
                  (process-kill-buffer-query-function)
                  (stringp (process-tty-name p))
                  (stringp (process-tty-name p 'stdin))
                  (stringp (process-tty-name p 'stdout))
                  (stringp (process-tty-name p 'stderr))
                  (condition-case err (process-tty-name p 0) (error err))
                  (let ((pp (make-pipe-process :name "proc-coding-tty-query-pipe")))
                    (unwind-protect
                        (list
                         (null (process-tty-name pp))
                         (null (process-tty-name pp nil))
                         (null (process-tty-name pp 'stdin))
                         (null (process-tty-name pp 'stdout))
                         (null (process-tty-name pp 'stderr)))
                      (ignore-errors (delete-process pp))))
                  (let ((np (make-network-process :name "proc-coding-tty-query-network" :server t :service 0)))
                    (unwind-protect
                        (list
                         ;; GNU `set_network_socket_coding_system` defaults a
                         ;; buffer-less network process (with no :coding and a
                         ;; multibyte default buffer) to the car/cdr of
                         ;; `default-process-coding-system` (utf-8-unix), NOT
                         ;; binary; verified against the real GNU 31 binary.
                         (equal (process-coding-system np) '(utf-8-unix . utf-8-unix))
                         (null (process-tty-name np))
                         (null (process-tty-name np nil))
                         (null (process-tty-name np 'stdin))
                         (null (process-tty-name np 'stdout))
                         (null (process-tty-name np 'stderr)))
                      (ignore-errors (delete-process np))))
                  (delete-process p)
                  (process-live-p p))
               (ignore-errors (delete-process p))))
           (condition-case err (process-coding-system 'x) (error err))
           (condition-case err (process-datagram-address 'x) (error err))
           (condition-case err (process-inherit-coding-system-flag 'x) (error err))
           (condition-case err (process-tty-name 'x) (error err))
           (condition-case err (process-tty-name nil) (error err))
           (condition-case err (process-tty-name 'x t) (error err))
           (condition-case err (process-kill-buffer-query-function nil) (error (car err)))
           (condition-case err (process-coding-system) (error (car err)))
           (condition-case err (process-datagram-address) (error (car err)))
           (condition-case err (process-inherit-coding-system-flag) (error (car err)))
           (condition-case err (process-tty-name) (error (car err)))"#,
    ));
    assert_eq!(
        results[0],
        "OK (t nil nil t t t t t (error \"Unknown stream\" 0) (t t t t t) (t t t t t t) nil nil)"
    );
    assert_eq!(results[1], "OK (wrong-type-argument processp x)");
    assert_eq!(results[2], "OK (wrong-type-argument processp x)");
    assert_eq!(results[3], "OK (wrong-type-argument processp x)");
    assert_eq!(results[4], "OK (wrong-type-argument processp x)");
    assert_eq!(results[5], "OK (wrong-type-argument processp nil)");
    assert_eq!(results[6], "OK (wrong-type-argument processp x)");
    assert_eq!(results[7], "OK wrong-number-of-arguments");
    assert_eq!(results[8], "OK wrong-number-of-arguments");
    assert_eq!(results[9], "OK wrong-number-of-arguments");
    assert_eq!(results[10], "OK wrong-number-of-arguments");
    assert_eq!(results[11], "OK wrong-number-of-arguments");
}

#[test]
fn make_network_process_honors_dynamic_coding_system_for_read_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                  (coding-system-for-read 'binary))
             (let ((process
                    (make-network-process
                     :name "network-dynamic-read-coding"
                     :server t
                     :service 0)))
               (unwind-protect
                   (process-coding-system process)
                 (delete-process process))))"#,
    );

    assert_eq!(results, ["OK (binary . utf-8-unix)"]);
}

#[cfg(unix)]
#[test]
fn synchronous_network_refusal_reports_bare_errno_and_contact_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let* ((server
                   (make-network-process
                    :name "network-refusal-port"
                    :server t
                    :service 0))
                  (port (process-contact server :service)))
             (delete-process server)
             (condition-case error-data
                 (make-network-process
                  :name "network-refusal-client"
                  :host "127.0.0.1"
                  :service port)
               (file-error
                (list
                 (car error-data)
                 (nth 1 error-data)
                 (nth 2 error-data)
                 (nth 3 error-data)
                 (nth 4 error-data)
                 (nth 5 error-data)
                 (nth 6 error-data)
                 (nth 7 error-data)
                 (equal port (nth 8 error-data))))))"#,
    );

    assert_eq!(
        results,
        [
            "OK (file-error \"make client process failed\" \"Connection refused\" :name \"network-refusal-client\" :host \"127.0.0.1\" :service t)"
        ]
    );
}

#[test]
fn process_list_network_serial_runtime_surface() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(mapcar (lambda (s)
                     (list s
                           (fboundp s)
                           (subrp (symbol-function s))
                           (subr-arity (symbol-function s))
                           (commandp s)))
                   '(list-system-processes
                     num-processors
                     make-network-process
                     make-pipe-process
                     make-serial-process
                     serial-process-configure
                     set-network-process-option))
           (let ((n0 (num-processors))
                 (n1 (num-processors t)))
             (list
              (listp (list-system-processes))
              (integerp (car (list-system-processes)))
              (not (null (member (emacs-pid) (list-system-processes))))
              (condition-case err (list-system-processes nil) (error (car err)))
              (integerp n0)
              (integerp n1)
              (> n0 0)
              (= n0 n1)
              (condition-case err (num-processors 1 2) (error (car err)))
              (list-processes)
              (list-processes nil)
              (list-processes t)
              (list-processes nil nil)
              (list-processes nil t)
              (condition-case err (list-processes nil nil nil) (error (car err)))
              (listp (list-processes--refresh))
              (equal (car (list-processes--refresh)) "")
              (condition-case err (list-processes--refresh nil) (error (car err)))))
           (list
            (make-network-process)
            (condition-case err (make-network-process :name "np") (error err))
            (condition-case err (make-network-process :name 1) (error err))
            (condition-case err (make-network-process :service 80) (error err))
            (let ((p (make-network-process :name "np-server" :server t :service 0)))
              (unwind-protect
                  (processp p)
                (ignore-errors (delete-process p))))
            (make-pipe-process)
            (let ((p (make-pipe-process :name "pp")))
              (unwind-protect
                  (processp p)
                (ignore-errors (delete-process p))))
            (condition-case err (make-pipe-process :name 1) (error err))
            (make-serial-process)
            (condition-case err (make-serial-process :name "sp" :port t :speed 9600) (error err))
            (condition-case err (make-serial-process :name "sp" :port 1 :speed 9600) (error err))
            (condition-case err (make-serial-process :name "sp") (error err))
            (condition-case err (make-serial-process :name "sp" :port "/tmp/no-port") (error err))
            (with-temp-buffer
              (condition-case err (serial-process-configure) (error (car err))))
            (with-temp-buffer
              (let ((p (start-process "serial-cfg-proc" nil "cat")))
                (unwind-protect
                    (condition-case err (serial-process-configure p) (error (car err)))
                  (ignore-errors (delete-process p)))))
            (condition-case err (set-network-process-option) (error (car err)))
            (condition-case err (set-network-process-option 1 :foo 1) (error err))
            (let ((p (start-process "netopt-real" nil "cat")))
              (unwind-protect
                  (condition-case err (set-network-process-option p :foo 1) (error err))
                (ignore-errors (delete-process p))))
            (let ((p (make-network-process :name "netopt-network" :server t :service 0)))
              (unwind-protect
                  (condition-case err (set-network-process-option p :foo 1) (error err))
                (ignore-errors (delete-process p)))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((list-system-processes t t (0 . 0) nil) (num-processors t t (0 . 1) nil) (make-network-process t t (0 . many) nil) (make-pipe-process t t (0 . many) nil) (make-serial-process t t (0 . many) nil) (serial-process-configure t t (0 . many) nil) (set-network-process-option t t (3 . 4) nil))"
    );
    assert_eq!(
        results[1],
        "OK (t t t wrong-number-of-arguments t t t t wrong-number-of-arguments nil nil nil nil nil wrong-number-of-arguments t t wrong-number-of-arguments)"
    );
    assert_eq!(
        results[2],
        "OK (nil (wrong-type-argument stringp nil) (error \":name value not a string\") (error \"Missing :name keyword parameter\") t nil t (error \":name value not a string\") nil (wrong-type-argument stringp t) (wrong-type-argument stringp 1) (error \"No port specified\") (error \":speed not specified\") error malformed-keyword-arg-list wrong-number-of-arguments (wrong-type-argument processp 1) (error \"Process is not a network process\") (error \"Unknown or unsupported option\"))"
    );
}

#[test]
fn process_keyword_arg_lists_match_gnu_malformed_pair_handling() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(list
            (condition-case err (make-network-process :name "odd-net" :server t :service)
              (error (car err)))
            (condition-case err (make-process :name)
              (error (car err)))
            (condition-case err (make-pipe-process :name)
              (error (car err)))
            (condition-case err (make-serial-process :name)
              (error (car err)))
            (condition-case err (serial-process-configure :process)
              (error (car err)))
            (let ((p (make-network-process
                      :name "even-unknown-net" :server t :service 0 :ignored nil)))
              (unwind-protect
                  (list (processp p) (process-contact p :ignored))
                (ignore-errors (delete-process p)))))"#,
    );

    assert_eq!(
        result,
        "OK (malformed-keyword-arg-list malformed-keyword-arg-list malformed-keyword-arg-list malformed-keyword-arg-list malformed-keyword-arg-list (t nil))"
    );
}

#[cfg(unix)]
#[test]
fn process_constructors_duplicate_keywords_use_first_value_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(list
            (let ((p (make-process
                      :name "neo-dup-make"
                      :name 1
                      :buffer nil
                      :buffer "bad-buffer"
                      :command nil
                      :command (list "sh" "-c" "printf bad")
                      :noquery t
                      :noquery nil
                      :stop nil
                      :stop t)))
              (unwind-protect
                  (list (process-name p)
                        (process-buffer p)
                        (process-command p)
                        (process-query-on-exit-flag p))
                (ignore-errors (delete-process p))))
            (let ((p (make-pipe-process
                      :name "neo-dup-pipe"
                      :name 1
                      :buffer nil
                      :buffer 1
                      :noquery t
                      :noquery nil
                      :stop t
                      :stop nil
                      :filter 'ignore
                      :filter nil
                      :plist (list :a 1)
                      :plist (list :a 2))))
              (unwind-protect
                  (list (process-name p)
                        (process-buffer p)
                        (process-query-on-exit-flag p)
                        (process-status p)
                        (process-filter p)
                        (process-plist p))
                (ignore-errors (delete-process p))))
            (let ((p (make-serial-process
                      :port "/dev/ptmx"
                      :port 1
                      :speed 9600
                      :speed "bad"
                      :bytesize 7
                      :bytesize 6
                      :parity 'even
                      :parity 'mark
                      :stopbits 2
                      :stopbits 3
                      :flowcontrol 'hw
                      :flowcontrol 'bad)))
              (unwind-protect
                  (list (process-contact p)
                        (process-contact p :bytesize)
                        (process-contact p :parity)
                        (process-contact p :stopbits)
                        (process-contact p :flowcontrol)
                        (process-contact p :summary))
                (ignore-errors (delete-process p))))
            (let ((p (make-network-process
                      :name "neo-dup-network"
                      :name 1
                      :server t
                      :server nil
                      :service 0
                      :service "bad"
                      :noquery t
                      :noquery nil
                      :stop t
                      :stop nil
                      :log 'ignore
                      :log nil
                      :plist (list :n 1)
                      :plist (list :n 2))))
              (unwind-protect
                  (list (process-name p)
                        (process-query-on-exit-flag p)
                        (process-status p)
                        (process-contact p :server)
                        (integerp (process-contact p :service))
                        (process-contact p :log)
                        (process-plist p))
                (ignore-errors (delete-process p))))
            (condition-case err
                (make-pipe-process :name 1 :name "neo-dup-pipe-late")
              (error (car err)))
            (condition-case err
                (make-process :name 1 :name "neo-dup-make-late" :command nil)
              (error (car err)))
            (condition-case err
                (make-network-process
                 :name 1 :name "neo-dup-network-late" :server t :service 0)
              (error (car err)))
            (condition-case err
                (make-serial-process
                 :port 1 :port "/dev/ptmx" :speed 9600)
              (error (car err))))"#,
    );
    assert_eq!(
        result,
        "OK ((\"neo-dup-make\" nil nil nil) (\"neo-dup-pipe\" nil nil stop ignore (:a 1)) ((\"/dev/ptmx\" 9600) 7 even 2 hw \"7E2\") (\"neo-dup-network\" nil stop t t ignore (:n 1)) error error error wrong-type-argument)"
    );
}

#[test]
fn make_process_file_handler_dispatches_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar neo-make-process-handler-calls nil)
             (defun neo-make-process-handler (operation &rest args)
               (setq neo-make-process-handler-calls (list operation args))
               (list :handled operation args))
             (let ((default-directory "/mock:/tmp/")
                   (file-name-handler-alist
                    '(("\\`/mock:" . neo-make-process-handler))))
               (list
                (make-process :name "fh" :command nil :file-handler t)
                neo-make-process-handler-calls)))"#,
    );

    assert_eq!(
        result,
        "OK ((:handled make-process (:name \"fh\" :command nil :file-handler t)) (make-process (:name \"fh\" :command nil :file-handler t)))"
    );
}

#[test]
fn start_file_process_file_handler_dispatches_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar neo-start-file-process-handler-calls nil)
             (defun neo-start-file-process-handler (operation &rest args)
               (setq neo-start-file-process-handler-calls (list operation args))
               (list :handled operation args))
             (let ((default-directory "/mock:/tmp/")
                   (file-name-handler-alist
                    '(("\\`/mock:" . neo-start-file-process-handler))))
               (list
                (start-file-process "sfp" nil "prog" "arg")
                neo-start-file-process-handler-calls)))"#,
    );

    assert_eq!(
        result,
        "OK ((:handled start-file-process (\"sfp\" nil \"prog\" \"arg\")) (start-file-process (\"sfp\" nil \"prog\" \"arg\")))"
    );
}

#[test]
fn system_process_file_handlers_dispatch_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar neo-system-process-handler-calls nil)
             (defun neo-system-process-handler (operation &rest args)
               (setq neo-system-process-handler-calls
                     (cons (list operation args)
                           neo-system-process-handler-calls))
               (cond
                ((eq operation 'list-system-processes)
                 (list 10 20))
                ((eq operation 'process-attributes)
                 (list (cons 'pid (car args))))))
             (let ((default-directory "/mock:/tmp/")
                   (file-name-handler-alist
                    '(("\\`/mock:" . neo-system-process-handler))))
               (list
                (list-system-processes)
                (process-attributes "remote-pid")
                (nreverse neo-system-process-handler-calls))))"#,
    );

    assert_eq!(
        result,
        "OK ((10 20) ((pid . \"remote-pid\")) ((list-system-processes nil) (process-attributes (\"remote-pid\"))))"
    );
}

#[test]
fn make_network_process_feature_advertisement_is_conservative() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list
            (featurep 'make-network-process)
            (featurep 'make-network-process '(:family local))
            (featurep 'make-network-process '(:family ipv4))
            (featurep 'make-network-process '(:family ipv6))
            (featurep 'make-network-process '(:service t))
            (featurep 'make-network-process '(:server t))
            (featurep 'make-network-process '(:nowait t))
            (featurep 'make-network-process '(:type datagram))
            (featurep 'make-network-process '(:type seqpacket))
            (featurep 'make-network-process :reuseaddr)
            (featurep 'make-network-process :keepalive)
            (featurep 'make-network-process :bindtodevice)
            (featurep 'make-network-process :nodelay)
            (featurep 'make-network-process :priority)
            (featurep 'make-network-process :oobinline)
            (featurep 'make-network-process :linger)
            (featurep 'make-network-process :dontroute)
            (featurep 'make-network-process :broadcast))
           (get 'make-network-process 'subfeatures)"#,
    );

    let expected_featurep = cfg_select! {
        any(target_os = "linux", target_os = "android") => {
            "OK (t t t t t t t t t t t t t t t t t t)"
        }
        _ => {
            "OK (t t t t t t t t t t t nil t nil t t t t)"
        }
    };
    let expected_subfeatures = cfg_select! {
        any(target_os = "linux", target_os = "android") => {
            "OK (:nodelay :reuseaddr :priority :oobinline :linger :keepalive :dontroute :broadcast :bindtodevice (:server t) (:service t) (:family ipv6) (:family ipv4) (:family local) (:type seqpacket) (:type datagram) (:nowait t))"
        }
        _ => {
            "OK (:nodelay :reuseaddr :oobinline :linger :keepalive :dontroute :broadcast (:server t) (:service t) (:family ipv6) (:family ipv4) (:family local) (:type seqpacket) (:type datagram) (:nowait t))"
        }
    };
    assert_eq!(results[0], expected_featurep);
    assert_eq!(results[1], expected_subfeatures);
}

#[test]
#[cfg(any(target_os = "linux", target_os = "android"))]
fn set_network_process_option_applies_known_options_and_updates_contact_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((p (make-network-process :name "netopt-known" :server t :service 0)))
             (unwind-protect
                 (list
                  (set-network-process-option p :reuseaddr nil)
                  (set-network-process-option p :nodelay t)
                  (set-network-process-option p :priority 0)
                  (set-network-process-option p :linger 0)
                  (set-network-process-option p :bindtodevice nil)
                  (not (null (memq :reuseaddr (process-contact p t))))
                  (process-contact p :reuseaddr)
                  (process-contact p :nodelay)
                  (process-contact p :priority)
                  (process-contact p :linger)
                  (not (null (memq :bindtodevice (process-contact p t))))
                  (process-contact p :bindtodevice))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(results[0], "OK (t t t t t t nil t 0 0 t nil)");
}

#[test]
#[cfg(any(target_os = "linux", target_os = "android"))]
fn make_network_process_constructor_socket_options_are_applied_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((p (make-network-process
                    :name "netopt-ctor" :server 2 :service 0
                    :reuseaddr nil :nodelay t :priority 0 :linger 0
                    :bindtodevice nil)))
             (unwind-protect
                 (list
                  (process-status p)
                  (process-contact p :server)
                  (integerp (process-contact p :service))
                  (not (null (memq :reuseaddr (process-contact p t))))
                  (process-contact p :reuseaddr)
                  (process-contact p :nodelay)
                  (process-contact p :priority)
                  (process-contact p :linger)
                  (not (null (memq :bindtodevice (process-contact p t))))
                  (process-contact p :bindtodevice))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(results[0], "OK (listen 2 t t nil t 0 0 t nil)");
}

#[test]
#[cfg(any(target_os = "linux", target_os = "android"))]
fn set_network_process_option_rejects_bad_values_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((p (make-network-process :name "netopt-bad" :server t :service 0)))
             (unwind-protect
                 (list
                  (condition-case err
                      (set-network-process-option p :priority t)
                    (error err))
                  (condition-case err
                      (set-network-process-option p :bindtodevice 1)
                    (error err))
                  (set-network-process-option p :bogus 1 t)
                  (condition-case err
                      (set-network-process-option p :bogus 1)
                    (error err)))
               (ignore-errors (delete-process p))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((error \"Bad option value for :priority\") (error \"Bad option value for :bindtodevice\") nil (error \"Unknown or unsupported option\"))"
    );
}

#[test]
fn make_network_process_validates_gnu_keyword_domains() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err
              (make-network-process :name "np-nowait" :server t :nowait t :service 0)
            (error err))
           (condition-case err
              (make-network-process :name "np-type" :server t :service 0 :type 'bogus)
             (error err))
           (condition-case err
              (let ((p (make-network-process
                        :name "np-datagram" :server t :service 0 :type 'datagram)))
                (prog1 (process-status p)
                  (delete-process p)))
             (error err))
           (condition-case err
              (let ((path (make-temp-file "neomacs-seqpacket-domain-"))
                    (p nil))
                (delete-file path)
                (unwind-protect
                    (progn
                      (setq p (make-network-process
                               :name "np-seqpacket" :server t :service path
                               :type 'seqpacket :family 'local))
                      (process-status p))
                  (when p (delete-process p))
                  (ignore-errors (delete-file path))))
             (error err))
           (condition-case err
              (let* ((srv (make-network-process
                           :name "np-nowait-srv" :server t :service 0 :host 'local))
                     (port (process-contact srv :service))
                     (cli (make-network-process
                           :name "np-nowait-client" :host 'local
                           :service port :nowait t)))
                (prog1 (process-status cli)
                  (delete-process cli)
                  (delete-process srv)))
             (error err))
           (condition-case err
              (make-network-process :name "np-family" :server t :service 0 :family 'bogus)
             (error err))"#,
    );

    assert_eq!(
        results[0],
        "OK (error \"`:server' is incompatible with `:nowait'\")"
    );
    assert_eq!(results[1], "OK (error \"Unsupported connection type\")");
    assert_eq!(results[2], "OK open");
    assert_eq!(results[3], "OK listen");
    assert_eq!(results[4], "OK connect");
    assert_eq!(results[5], "OK (error \"Unknown address family\")");
}

#[cfg(unix)]
#[test]
fn make_network_process_numeric_family_constants_match_gnu() {
    crate::test_utils::init_test_tracing();
    let af_inet6 = libc::AF_INET6;
    let af_unix = libc::AF_UNIX;
    let result = eval_one(&format!(
        r#"(list
            (let ((p (make-network-process
                      :name "np-family6" :server t :service 0 :family {af_inet6})))
              (unwind-protect
                  (let ((local (process-contact p :local)))
                    (list (process-status p)
                          (vectorp local)
                          (= (length local) 9)
                          (= (aref local 8) (process-contact p :service))
                          (= (process-contact p :family) {af_inet6})))
                (delete-process p)))
            (condition-case err
                (make-network-process
                 :name "np-family-local-int" :server t :service 0 :family {af_unix})
              (error (car err)))
            (condition-case err
                (make-network-process
                 :name "np-family-bad-int" :server t :service 0 :family 424242)
              (error err)))"#
    ));

    assert_eq!(
        result,
        "OK ((listen t t t t) wrong-type-argument (error \"127.0.0.1/0 ai_family not supported\"))"
    );
}

#[test]
fn make_network_process_nowait_tcp_loopback_opens_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil) (srv nil) (cli nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "nowait-srv" :server t :service 0
                              :host 'local :noquery t))
                   (setq cli (make-network-process
                              :name "nowait-cli" :host 'local
                              :service (process-contact srv :service)
                              :nowait t :noquery t
                              :sentinel (lambda (p e)
                                          (push (list :cli (substring e 0 -1)
                                                      (process-status p))
                                                events))))
                   (let ((initial (list (process-status cli)
                                        (process-live-p cli)
                                        (vectorp (process-contact cli :remote))
                                        (vectorp (process-contact cli :local)))))
                     (dotimes (_ 20)
                       (accept-process-output nil 0.05))
                     (list initial
                           (process-status cli)
                           (process-live-p cli)
                           events
                           (vectorp (process-contact cli :remote))
                           (vectorp (process-contact cli :local)))))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((connect (connect stop) t t) open (open listen connect stop) ((:cli \"open\" open)) t t)"
    );
}

#[test]
fn make_network_process_nowait_retains_tls_parameters_until_connect_completes() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("listener address").port();
    let tls_parameters = Value::list(vec![
        Value::symbol("gnutls-x509pki"),
        Value::keyword(":hostname"),
        Value::string("localhost"),
    ]);
    let mut eval = Context::new();

    let process = builtin_make_network_process(
        &mut eval,
        vec![
            Value::keyword(":name"),
            Value::string("nowait-tls-parameters"),
            Value::keyword(":host"),
            Value::string("127.0.0.1"),
            Value::keyword(":service"),
            Value::fixnum(i64::from(port)),
            Value::keyword(":family"),
            Value::symbol("ipv4"),
            Value::keyword(":nowait"),
            Value::T,
            Value::keyword(":tls-parameters"),
            tls_parameters,
        ],
    )
    .expect("start deferred TLS connection");
    let id = process.as_process_id().expect("network process id");

    assert_eq!(
        eval.processes
            .get(id)
            .expect("network process")
            .gnutls_boot_parameters,
        tls_parameters,
        "the TCP completion path must still know that TLS negotiation is pending"
    );
}

#[test]
fn make_network_process_nowait_hostname_dns_failure_is_async_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let id = eval.processes.create_process_with_kind_lisp(
        LispString::from_utf8("nowait-dns-fail"),
        Value::NIL,
        LispString::from_utf8("network"),
        Vec::new(),
        ProcessKindWithoutDevice::Network,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    sender
        .send(Err("Name or service not known".to_string()))
        .expect("seed dns failure");
    let proc = eval.processes.get_mut(id).expect("network process");
    proc.status = process_status_connect_value();
    proc.childp = Value::list(vec![
        ProcessKeyword::Host.value(),
        Value::string("-bad.example"),
        ProcessKeyword::Service.value(),
        Value::fixnum(9),
        ProcessKeyword::Nowait.value(),
        Value::T,
    ]);
    proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Dns(PendingDnsRequest {
        host: "-bad.example".to_string(),
        receiver,
        ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        socket_options: Vec::new(),
    }));

    let outcome = eval
        .wait_for_process_output(ProcessOutputWaitRequest::new(
            ProcessOutputWaitTiming::For(Duration::from_secs(1)),
            Some(id),
            false,
            true,
        ))
        .expect("wait for dns failure");

    assert_eq!(outcome, ProcessOutputWaitOutcome::NoProcessActivity);
    assert_eq!(
        builtin_process_status_impl(
            &mut eval.processes,
            &eval.buffers,
            vec![Value::make_process(id)]
        )
        .expect("status"),
        Value::symbol("failed")
    );
    assert_eq!(
        builtin_process_live_p_impl(&mut eval.processes, vec![Value::make_process(id)])
            .expect("live-p"),
        Value::NIL
    );
}

#[test]
fn process_send_string_waits_for_nowait_tcp_connect_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil) (recv nil) (srv nil) (cli nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "nowait-send-srv" :server t :service 0
                              :host 'local :noquery t
                              :filter (lambda (_p s) (setq recv s))))
                   (setq cli (make-network-process
                              :name "nowait-send-cli" :host 'local
                              :service (process-contact srv :service)
                              :nowait t :noquery t
                              :sentinel (lambda (p e)
                                          (push (list :cli (substring e 0 -1)
                                                      (process-status p))
                                                events))))
                   (process-send-string cli "ping-nowait")
                   (dotimes (_ 20)
                     (accept-process-output nil 0.05))
                   (list (process-status cli) recv events))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK (open \"ping-nowait\" ((:cli \"open\" open)))"
    );
}

#[cfg(unix)]
#[test]
fn process_send_string_waits_for_nowait_local_stream_connect_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil) (recv nil) (srv nil) (cli nil)
                 (path (make-temp-file "neomacs-nowait-local-")))
             (delete-file path)
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "nowait-local-srv" :server t
                              :family 'local :service path :noquery t
                              :filter (lambda (_p s) (setq recv s))))
                   (setq cli (make-network-process
                              :name "nowait-local-cli"
                              :family 'local :service path
                              :nowait t :noquery t
                              :sentinel (lambda (p e)
                                          (push (list :cli (substring e 0 -1)
                                                      (process-status p))
                                                events))))
                   (let ((initial (list (process-status cli)
                                        (process-live-p cli)
                                        (stringp (process-contact cli :remote))
                                        (equal (process-contact cli :local) ""))))
                     (process-send-string cli "ping-local-nowait")
                     (dotimes (_ 20)
                       (accept-process-output nil 0.05))
                     (list initial
                           (process-status cli)
                           (process-live-p cli)
                           recv
                           events
                           (stringp (process-contact cli :remote))
                           (equal (process-contact cli :local) ""))))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((connect (connect stop) t t) open (open listen connect stop) \"ping-local-nowait\" ((:cli \"open\" open)) t t)"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn make_network_process_nowait_tcp_refusal_fails_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil) (srv nil) (cli nil) (port nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "nowait-fail-srv" :server t :service 0
                              :host 'local :noquery t))
                   (setq port (process-contact srv :service))
                   (delete-process srv)
                   (setq srv nil)
                   (setq cli (make-network-process
                              :name "nowait-fail-cli" :host 'local
                              :service port :nowait t :noquery t
                              :sentinel (lambda (p e)
                                          (push (list :cli (substring e 0 -1)
                                                      (process-status p)
                                                      (process-exit-status p))
                                                events))))
                   (let ((initial (list (process-status cli)
                                        (process-live-p cli))))
                     (dotimes (_ 20)
                       (accept-process-output cli 0.05))
                     (list initial
                           (process-status cli)
                           (process-live-p cli)
                           (process-exit-status cli)
                           events)))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((connect (connect stop)) failed nil 111 ((:cli \"failed with code 111\" failed 111)))"
    );
}

#[test]
fn process_contact_no_block_returns_nil_for_pending_nowait_network_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let id = eval.processes.create_process_with_kind_lisp(
        LispString::from_utf8("pending-contact"),
        Value::NIL,
        LispString::from_utf8("network"),
        Vec::new(),
        ProcessKindWithoutDevice::Network,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let proc = eval.processes.get_mut(id).expect("network process");
    proc.status = process_status_connect_value();
    proc.childp = Value::list(vec![
        ProcessKeyword::Host.value(),
        Value::string("192.0.2.1"),
        ProcessKeyword::Service.value(),
        Value::fixnum(9),
        ProcessKeyword::Remote.value(),
        Value::vector(vec![
            Value::fixnum(192),
            Value::fixnum(0),
            Value::fixnum(2),
            Value::fixnum(1),
            Value::fixnum(9),
        ]),
    ]);
    proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Tcp {
        remaining_addrs: Vec::new(),
        socket_options: Vec::new(),
    });
    let process = Value::make_process(id);

    assert_eq!(
        builtin_process_contact(&mut eval, vec![process, Value::NIL, Value::T])
            .expect("process-contact"),
        Value::NIL
    );
    assert_eq!(
        builtin_process_contact(
            &mut eval,
            vec![process, ProcessKeyword::Remote.value(), Value::T],
        )
        .expect("process-contact :remote"),
        Value::NIL
    );
    assert_eq!(
        builtin_process_contact(&mut eval, vec![process, Value::T, Value::T])
            .expect("process-contact t"),
        Value::NIL
    );
}

#[test]
fn network_accessors_wait_for_pending_nowait_connects_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((srv nil) (cli1 nil) (cli2 nil) (cli3 nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "wait-accessor-srv" :server t
                              :service 0 :host 'local :noquery t))
                   (let ((port (process-contact srv :service)))
                     (setq cli1 (make-network-process
                                 :name "wait-accessor-cli1"
                                 :host 'local :service port
                                 :nowait t :noquery t))
                     (let ((part1 (list (process-status cli1)
                                        (process-datagram-address cli1)
                                        (process-status cli1))))
                       (setq cli2 (make-network-process
                                   :name "wait-accessor-cli2"
                                   :host 'local :service port
                                   :nowait t :noquery t))
                       (let ((part2 (list (process-status cli2)
                                          (set-network-process-option
                                           cli2 :nodelay t)
                                          (process-status cli2))))
                         (setq cli3 (make-network-process
                                     :name "wait-accessor-cli3"
                                     :host 'local :service port
                                     :nowait t :noquery t))
                         (let ((part3 (list (process-status cli3)
                                            (set-process-datagram-address
                                             cli3 [127 0 0 1 9])
                                            (process-status cli3))))
                           (list part1 part2 part3))))))
               (when cli1 (delete-process cli1))
               (when cli2 (delete-process cli2))
               (when cli3 (delete-process cli3))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((connect nil open) (connect t open) (connect nil open))"
    );
}

#[test]
fn process_send_eof_half_closes_network_stream_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((srv nil) (cli nil) (accepted nil) (events nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "eof-srv" :server t :service 0
                              :host 'local :noquery t
                              :log (lambda (_server client _msg)
                                     (setq accepted client)
                                     (set-process-sentinel
                                      client
                                      (lambda (p e)
                                        (push (list (process-status p)
                                                    (substring e 0 -1))
                                              events))))))
                   (setq cli (make-network-process
                              :name "eof-cli" :host 'local
                              :service (process-contact srv :service)
                              :nowait t :noquery t))
                   (let ((before (process-status cli))
                         (ret (process-send-eof cli)))
                     (dotimes (_ 30)
                       (accept-process-output nil 0.05))
                     (list before
                           (processp ret)
                           (process-status cli)
                           (and accepted (process-status accepted))
                           events)))
               (when cli (delete-process cli))
               (when accepted (delete-process accepted))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK (connect t closed closed ((closed \"connection broken by remote peer\") (open \"open from 127.0.0.1\")))"
    );
}

#[test]
fn make_network_process_stop_server_defers_accept_until_continue_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((recv nil) (srv nil) (cli nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "stop-srv" :server t :service 0
                              :host 'local :stop t :noquery t
                              :filter (lambda (_p s) (setq recv s))))
                   (setq cli (make-network-process
                              :name "stop-cli" :host 'local
                              :service (process-contact srv :service)
                              :noquery t))
                   (process-send-string cli "hello-stop-server")
                   (accept-process-output nil 0.2)
                   (let ((before (list (process-status srv)
                                       (process-live-p srv)
                                       recv)))
                     (continue-process srv)
                     (dotimes (_ 20)
                       (accept-process-output nil 0.05))
                     (list before
                           (process-status srv)
                           (process-live-p srv)
                           recv)))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((stop (stop) nil) listen (listen connect stop) \"hello-stop-server\")"
    );
}

#[test]
fn process_send_string_rejects_network_server_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let buffers = crate::buffer::BufferManager::new();
    let id = pm.create_process_with_kind(
        "send-listener-unit".into(),
        Value::NIL,
        "network".into(),
        vec![],
        ProcessKindWithoutDevice::Network,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    pm.get_mut(id).expect("server process").childp = Value::list(vec![
        ProcessKeyword::Server.value(),
        Value::T,
        ProcessKeyword::Service.value(),
        Value::fixnum(0),
    ]);

    match builtin_process_send_string_impl(
        &mut pm,
        &buffers,
        vec![Value::make_process(id), Value::string("x")],
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|value| value.as_utf8_str()),
                Some("Process send-listener-unit not running: listen")
            );
        }
        other => panic!("expected listener send error, got {other:?}"),
    }
}

#[test]
fn make_network_process_stop_client_still_allows_send_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((recv nil) (srv nil) (cli nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "stop-client-srv" :server t :service 0
                              :host 'local :noquery t
                              :filter (lambda (_p s) (setq recv s))))
                   (setq cli (make-network-process
                              :name "stop-client" :host 'local
                              :service (process-contact srv :service)
                              :stop t :noquery t))
                   (process-send-string cli "hello-stop-client")
                   (dotimes (_ 20)
                     (accept-process-output nil 0.05))
                   (let ((before (list (process-status cli)
                                       (process-live-p cli)
                                       recv)))
                     (continue-process cli)
                     (list before
                           (process-status cli)
                           (process-live-p cli)
                           recv)))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((stop (stop) \"hello-stop-client\") open (open listen connect stop) \"hello-stop-client\")"
    );
}

#[test]
fn make_network_process_datagram_udp_loopback_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((srv nil) (cli nil) (recv nil))
             (unwind-protect
                 (progn
                   (setq srv
                         (make-network-process
                          :name "udp-srv" :type 'datagram :server t
                          :host 'local :service t :family 'ipv4 :noquery t
                          :filter (lambda (_p s) (setq recv s))))
                   (let* ((local (process-contact srv :local))
                          (port (aref local (1- (length local)))))
                     (setq cli
                           (make-network-process
                            :name "udp-cli" :type 'datagram
                            :host 'local :service port :family 'ipv4 :noquery t))
                     (process-send-string cli "ping-udp")
                     (let ((k 0))
                       (while (and (null recv) (< k 100))
                         (accept-process-output nil 0.02)
                         (setq k (1+ k))))
                     (let ((new [127 0 0 1 9])
                           (v6 [0 0 0 0 0 0 0 0 9]))
                       (list recv
                             (process-status srv)
                             (process-status cli)
                             (vectorp (process-datagram-address srv))
                             (vectorp (process-datagram-address cli))
                             (equal (set-process-datagram-address cli new) new)
                             (equal (process-datagram-address cli) new)
                             (equal (process-contact cli :remote) new)
                             (equal (plist-get (process-contact cli t) :remote) new)
                             (null (set-process-datagram-address cli v6))
                             (equal (process-datagram-address cli) new)))))
               (when cli (delete-process cli))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(results[0], "OK (\"ping-udp\" open open t t t t t t t t)");
}

#[test]
fn make_network_process_service_name_strings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((p nil))
             (unwind-protect
                 (progn
                   (setq p (make-network-process
                            :name "svc-udp" :type 'datagram
                            :host 'local :service "domain"
                            :family 'ipv4 :noquery t))
                   (list (process-status p)
                         (aref (process-contact p :remote) 4)))
               (when p (delete-process p))))"#,
    );

    assert_eq!(results[0], "OK (open 53)");
}

#[test]
fn make_network_process_empty_service_string_means_port_zero_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((srv nil) (udp nil))
             (unwind-protect
                 (progn
                   (setq srv (make-network-process
                              :name "empty-svc-server" :server t
                              :host 'local :service "" :family 'ipv4
                              :noquery t))
                   (setq udp (make-network-process
                              :name "empty-svc-udp" :type 'datagram
                              :host 'local :service "" :family 'ipv4
                              :noquery t))
                   (list (process-status srv)
                         (integerp (process-contact srv :service))
                         (= (process-contact srv :service)
                            (aref (process-contact srv :local) 4))
                         (process-status udp)
                         (aref (process-contact udp :remote) 4)))
               (when udp (delete-process udp))
               (when srv (delete-process srv))))"#,
    );

    assert_eq!(results[0], "OK (listen t t open 0)");
}

#[test]
fn make_network_process_numeric_service_wraps_like_gnu_getaddrinfo() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((a nil) (b nil) (c nil))
             (unwind-protect
                 (progn
                   (setq a (make-network-process
                            :name "svc-fixnum-wrap" :type 'datagram
                            :host 'local :service 70000 :family 'ipv4
                            :noquery t))
                   (setq b (make-network-process
                            :name "svc-string-wrap" :type 'datagram
                            :host 'local :service " 70000" :family 'ipv4
                            :noquery t))
                   (setq c (make-network-process
                            :name "svc-zero-wrap" :type 'datagram
                            :host 'local :service 65536 :family 'ipv4
                            :noquery t))
                   (list (aref (process-contact a :remote) 4)
                         (aref (process-contact b :remote) 4)
                         (aref (process-contact c :remote) 4)))
               (when a (delete-process a))
               (when b (delete-process b))
               (when c (delete-process c))))"#,
    );

    assert_eq!(results[0], "OK (4464 4464 0)");
}

#[cfg(unix)]
#[test]
fn make_network_process_local_datagram_loopback_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-dgram-"))
                 (srv nil) (cli nil) (recv nil))
             (delete-file path)
             (unwind-protect
                 (progn
                   (setq srv
                         (make-network-process
                          :name "uds-srv" :type 'datagram :family 'local
                          :server t :service path :noquery t
                          :filter (lambda (_p s) (setq recv s))))
                   (setq cli
                         (make-network-process
                          :name "uds-cli" :type 'datagram :family 'local
                          :service path :noquery t))
                   (process-send-string cli "ping-local")
                   (let ((k 0))
                     (while (and (null recv) (< k 100))
                       (accept-process-output nil 0.02)
                       (setq k (1+ k))))
                   (list recv
                         (process-status srv)
                         (process-status cli)
                         (equal (process-contact cli :local) "")
                         (equal (process-contact cli :remote) path)
                         (equal (process-datagram-address cli) path)
                         (consp (process-datagram-address srv))
                         (equal (set-process-datagram-address cli path) path)))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (\"ping-local\" open open t t t t t)");
}

#[cfg(unix)]
#[test]
fn make_network_process_local_seqpacket_loopback_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-seqpacket-"))
                 (srv nil) (cli nil) (recv nil) (events nil))
             (delete-file path)
             (unwind-protect
                 (progn
                   (setq srv
                         (make-network-process
                          :name "seq-local-srv" :type 'seqpacket :family 'local
                          :server t :service path :noquery t
                          :log (lambda (_server client msg)
                                 (push (list (process-name client) msg) events)
                                 (set-process-filter
                                  client
                                  (lambda (_p s) (setq recv s))))))
                   (setq cli
                         (make-network-process
                          :name "seq-local-cli" :type 'seqpacket :family 'local
                          :service path :noquery t))
                   (accept-process-output nil 0.2)
                   (process-send-string cli "ping-seq-local")
                   (let ((k 0))
                     (while (and (null recv) (< k 100))
                       (accept-process-output nil 0.02)
                       (setq k (1+ k))))
                   (list recv
                         (process-status srv)
                         (process-status cli)
                         (length events)
                         (equal (process-contact cli :local) "")
                         (equal (process-contact cli :remote) path)
                         (null (process-datagram-address srv))
                         (null (process-datagram-address cli))))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (\"ping-seq-local\" listen open 1 t t t t)");
}

#[test]
fn make_network_process_stream_server_accepts_client_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((events nil)
                 (accepted nil))
             (condition-case err
                 (let* ((srv (make-network-process
                              :name "srv" :server t :service t :host 'local :noquery t
                              :log (lambda (server client msg)
                                     (setq accepted client)
                                     (push (list (process-name client) msg) events))))
                        (port (process-contact srv :service))
                        (cli (make-network-process :name "cli" :host 'local :service port)))
                   (accept-process-output nil 0.2)
                   (prog1
                       (list (process-status srv)
                             (integerp port)
                             (> port 0)
                             (process-status cli)
                             (length events)
                             (not (process-query-on-exit-flag srv))
                             (process-query-on-exit-flag accepted))
                     (delete-process cli)
                     (delete-process srv)
                     (delete-process accepted)))
               (error err)))"#,
    );

    assert_eq!(results[0], "OK (listen t t open 1 t t)");
}

#[test]
fn make_network_process_explicit_inet_address_skips_host_service_family_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((srv nil)
                 (cli nil))
             (condition-case err
                 (unwind-protect
                     (progn
                       (setq srv (make-network-process
                                  :name "srv" :server t
                                  :local [127 0 0 1 0]
                                  :host 42
                                  :service 1
                                  :family 'bogus))
                       (setq cli (make-network-process
                                  :name "cli"
                                  :remote (process-contact srv :local)
                                  :host 42))
                       (accept-process-output nil 0.2)
                       (list (process-status srv)
                             (vectorp (process-contact srv :local))
                             (integerp (process-contact srv :service))
                             (= (aref (process-contact srv :local) 4)
                                (process-contact srv :service))
                             (process-contact srv :host)
                             (process-contact srv :family)
                             (process-status cli)
                             (vectorp (process-contact cli :remote))
                             (vectorp (process-contact cli :local))
                             (process-contact cli :host)
                             (process-contact cli :service)))
                   (when cli (delete-process cli))
                   (when srv (delete-process srv)))
               (error err)))"#,
    );

    assert_eq!(results[0], "OK (listen t t t 42 bogus open t t 42 nil)");
}

#[cfg(unix)]
#[test]
fn make_network_process_local_stream_server_accepts_client_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-sock-"))
                 (events nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (let* ((srv (make-network-process
                                  :name "srv" :server t :family 'local :service path :noquery t
                                  :log (lambda (server client msg)
                                         (push (list (process-name client)
                                                     msg
                                                     (process-contact client :remote)
                                                     (process-contact client :local)
                                                     (process-query-on-exit-flag client))
                                               events))))
                            (cli (make-network-process
                                  :name "cli" :family 'local :service path)))
                       (accept-process-output nil 0.2)
                       (prog1
                           (list (process-status srv)
                                 (not (process-query-on-exit-flag srv))
                                 (equal (process-contact srv :local) path)
                                 (equal (process-contact srv :service) path)
                                 (process-status cli)
                                 (equal (process-contact cli :remote) path)
                                 (equal (process-contact cli :local) "")
                                 (length events)
                                 (and events
                                      (not (null (string-match-p "^srv <[0-9]+>$"
                                                                 (caar events)))))
                                 (and events (equal (cadar events) "accept from -\n"))
                                 (and events (equal (nth 2 (car events)) ""))
                                 (and events (equal (nth 3 (car events)) path))
                                 (and events (nth 4 (car events))))
                         (delete-process cli)
                         (delete-process srv)))
                   (error err))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (listen t t t open t t 1 t t t t t)");
}

#[cfg(unix)]
#[test]
fn make_network_process_server_plist_is_inherited_by_accepted_local_client() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-plist-"))
                 (events nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (let* ((srv (make-network-process
                                  :name "srv" :server t :family 'local :service path
                                  :plist '(:authenticated t :foo bar)
                                  :log (lambda (server client msg)
                                         (push (list (process-get server :authenticated)
                                                     (process-get client :authenticated)
                                                     (process-get client :foo))
                                               events))))
                            (cli (make-network-process
                                  :name "cli" :family 'local :service path)))
                       (accept-process-output nil 0.2)
                       (prog1
                           (list (process-get srv :authenticated)
                                 (process-get srv :foo)
                                 (length events)
                                 (car events))
                         (delete-process cli)
                         (delete-process srv)))
                   (error err))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (t bar 1 (t t bar))");
}

#[cfg(unix)]
#[test]
fn accepted_local_client_shallow_copies_server_plist_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-plist-copy-"))
                 (accepted nil)
                 (srv nil)
                 (cli nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (progn
                       (setq srv (make-network-process
                                  :name "srv" :server t :family 'local :service path
                                  :plist '(:same old)
                                  :log (lambda (_server client _msg)
                                         (setq accepted client))))
                       (setq cli (make-network-process
                                  :name "cli" :family 'local :service path))
                       (accept-process-output nil 0.2)
                       (let ((same-before
                              (eq (process-plist srv) (process-plist accepted))))
                         (process-put accepted :same 'new)
                         (list same-before
                               (process-get srv :same)
                               (process-get accepted :same))))
                   (error err))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (when accepted (delete-process accepted))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (nil old new)");
}

#[cfg(unix)]
#[test]
fn accepted_local_client_with_custom_filter_has_no_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-client-buffer-"))
                 (accepted nil)
                 (srv nil)
                 (cli nil)
                 (buf nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (progn
                       (setq buf (get-buffer-create "neomacs-server-buffer"))
                       (setq srv (make-network-process
                                  :name "srv" :server t :family 'local :service path
                                  :buffer buf
                                  :filter (lambda (&rest _args))
                                  :log (lambda (_server client _msg)
                                         (setq accepted client))))
                       (set-process-inherit-coding-system-flag srv t)
                       (setq cli (make-network-process
                                  :name "cli" :family 'local :service path))
                       (accept-process-output nil 0.2)
                       (list (process-inherit-coding-system-flag srv)
                             (process-buffer accepted)
                             (process-inherit-coding-system-flag accepted)))
                   (error err))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (when accepted (delete-process accepted))
               (when (buffer-live-p buf) (kill-buffer buf))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (t nil nil)");
}

#[cfg(unix)]
#[test]
fn accepted_local_client_with_default_filter_gets_distinct_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-client-default-buffer-"))
                 (accepted nil)
                 (accepted-buffer nil)
                 (srv nil)
                 (cli nil)
                 (buf nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (progn
                       (setq buf (get-buffer-create "neomacs-server-buffer"))
                       (setq srv (make-network-process
                                  :name "srv" :server t :family 'local :service path
                                  :buffer buf
                                  :log (lambda (_server client _msg)
                                         (setq accepted client))))
                       (set-process-inherit-coding-system-flag srv t)
                       (setq cli (make-network-process
                                  :name "cli" :family 'local :service path))
                       (accept-process-output nil 0.2)
                       (setq accepted-buffer (process-buffer accepted))
                       (list (eq buf accepted-buffer)
                             (buffer-live-p accepted-buffer)
                             (and (buffer-name accepted-buffer)
                                  (not (null
                                        (string-match-p
                                         "^neomacs-server-buffer <[0-9]+>$"
                                         (buffer-name accepted-buffer)))))
                             (process-inherit-coding-system-flag accepted)))
                   (error err))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (when accepted (delete-process accepted))
               (when (buffer-live-p accepted-buffer) (kill-buffer accepted-buffer))
               (when (buffer-live-p buf) (kill-buffer buf))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (nil t t t)");
}

#[cfg(unix)]
#[test]
fn accepted_local_client_completes_the_pinentry_initial_handshake() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-pinentry-"))
                 (accepted nil)
                 (greeting nil)
                 (srv nil)
                 (cli nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (progn
                       (setq srv
                             (make-network-process
                              :name "pinentry" :server t :noquery t
                              :family 'local :service path
                              :filter (lambda (&rest _args))
                              :log (lambda (_server client _msg)
                                     (setq accepted client))
                              :sentinel
                              (lambda (process _status)
                                ;; Historical GNU pinentry.el initializes a
                                ;; newly accepted client through exactly this
                                ;; query-flag distinction.
                                (when (and (eq (process-status process) 'open)
                                           (process-query-on-exit-flag process))
                                  (set-process-query-on-exit-flag process nil)
                                  (process-send-string
                                   process "OK Your orders please\n")))))
                       (setq cli
                             (make-network-process
                              :name "pinentry-client" :family 'local :service path
                              :filter (lambda (_process output)
                                        (setq greeting
                                              (concat (or greeting "") output)))))
                       (let ((attempts 0))
                         (while (and (null greeting) (< attempts 20))
                           (accept-process-output nil 0.02)
                           (setq attempts (1+ attempts))))
                       (list greeting
                             (not (process-query-on-exit-flag srv))
                             (not (process-query-on-exit-flag accepted))))
                   (error err))
               (when cli (delete-process cli))
               (when accepted (delete-process accepted))
               (when srv (delete-process srv))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(results[0], "OK (\"OK Your orders please\n\" t t)");
}

#[cfg(unix)]
#[test]
fn make_network_process_explicit_local_address_skips_family_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((path (make-temp-file "neomacs-local-address-"))
                 (srv nil)
                 (cli nil))
             (delete-file path)
             (unwind-protect
                 (condition-case err
                     (progn
                       (setq srv (make-network-process
                                  :name "srv" :server t
                                  :local path
                                  :host "ignored"
                                  :service 1))
                       (setq cli (make-network-process
                                  :name "cli"
                                  :remote (process-contact srv :local)
                                  :host "bad.invalid"
                                  :service 1))
                       (accept-process-output nil 0.2)
                       (list (process-status srv)
                             (equal (process-contact srv :local) path)
                             (process-contact srv :host)
                             (process-contact srv :service)
                             (process-status cli)
                             (equal (process-contact cli :remote) path)
                             (equal (process-contact cli :local) "")
                             (process-contact cli :host)
                             (process-contact cli :service)))
                   (error err))
               (when cli (delete-process cli))
               (when srv (delete-process srv))
               (ignore-errors (delete-file path))))"#,
    );

    assert_eq!(
        results[0],
        "OK (listen t \"ignored\" 1 open t t \"bad.invalid\" 1)"
    );
}

#[test]
fn num_processors_openmp_parser_matches_gnu_rules() {
    assert_eq!(parse_openmp_threads(b"3"), Some(3));
    assert_eq!(parse_openmp_threads(b" 4,8"), Some(4));
    assert_eq!(parse_openmp_threads(b"5 "), Some(5));
    assert_eq!(parse_openmp_threads(b"0"), Some(0));
    assert_eq!(parse_openmp_threads(b""), None);
    assert_eq!(parse_openmp_threads(b"threads=4"), None);
    assert_eq!(parse_openmp_threads(b"4x"), None);

    assert_eq!(
        current_processors_count_overridable_with_env(Some(b"3"), None, 32),
        3
    );
    assert_eq!(
        current_processors_count_overridable_with_env(Some(b"3"), Some(b"2"), 32),
        2
    );
    assert_eq!(
        current_processors_count_overridable_with_env(Some(b" 4,8"), Some(b"0"), 32),
        4
    );
    assert_eq!(
        current_processors_count_overridable_with_env(None, Some(b"1"), 32),
        1
    );
    assert_eq!(
        current_processors_count_overridable_with_env(Some(b"0"), Some(b"5"), 32),
        5
    );
}

#[test]
fn list_processes_refresh_returns_propertized_spacer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(r#"(list-processes--refresh)"#);
    assert_eq!(
        result,
        r##"OK ("" header-line-indent #(" " 0 1 (display (space :align-to (+ header-line-indent-width 0)))))"##
    );
}

#[test]
fn minibuffer_sort_preprocess_history_sequence_contract() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(minibuffer--sort-preprocess-history nil)
           (minibuffer--sort-preprocess-history "")
           (minibuffer--sort-preprocess-history [97])
           (minibuffer--sort-preprocess-history '(97))
           (condition-case err (minibuffer--sort-preprocess-history 1) (error err))
           (condition-case err (minibuffer--sort-preprocess-history) (error err))"#,
    );

    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK (wrong-type-argument sequencep 1)");
    // GNU verified: arity errors on Lisp-defined functions carry the
    // (MIN . MAX) arity tuple, not the function symbol.
    assert_eq!(results[5], "OK (wrong-number-of-arguments (1 . 1) 0)");
}

#[test]
fn window_adjust_process_window_size_requires_list_window() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err (window-adjust-process-window-size 1 2) (error err))
           (condition-case err (window-adjust-process-window-size-largest 1 2) (error err))
           (condition-case err (window-adjust-process-window-size-smallest 1 2) (error err))
           (window-adjust-process-window-size nil nil)
           (window-adjust-process-window-size-largest nil nil)
           (window-adjust-process-window-size-smallest nil nil)"#,
    );

    assert_eq!(results[0], "OK (wrong-type-argument listp 2)");
    assert_eq!(results[1], "OK (wrong-type-argument listp 2)");
    assert_eq!(results[2], "OK (wrong-type-argument listp 2)");
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK nil");
    assert_eq!(results[5], "OK nil");
}

#[test]
fn network_interface_broadcast_derivation_helpers() {
    crate::test_utils::init_test_tracing();
    let ipv4_address = int_vector(&[192, 168, 1, 30, 0]);
    let ipv4_netmask = int_vector(&[255, 255, 255, 0, 0]);
    let ipv4_raw = int_vector(&[0, 0, 0, 0, 0]);
    assert_eq!(
        derive_network_interface_list_broadcast(
            NetworkAddressFamily::Ipv4,
            &ipv4_address,
            &ipv4_netmask,
            &ipv4_raw,
        ),
        int_vector(&[192, 168, 1, 255, 0])
    );
    assert_eq!(
        derive_network_interface_info_broadcast(
            NetworkAddressFamily::Ipv4,
            &ipv4_address,
            &ipv4_address,
        ),
        int_vector(&[0, 0, 0, 0, 0])
    );
    let ipv4_nontrivial_raw = int_vector(&[172, 17, 255, 255, 0]);
    assert_eq!(
        derive_network_interface_info_broadcast(
            NetworkAddressFamily::Ipv4,
            &int_vector(&[172, 17, 0, 1, 0]),
            &ipv4_nontrivial_raw,
        ),
        ipv4_nontrivial_raw
    );

    let ipv6_address = int_vector(&[9224, 33287, 9568, 22592, 60060, 9727, 65190, 14566, 0]);
    let ipv6_netmask = int_vector(&[65535, 65535, 65535, 65535, 0, 0, 0, 0, 0]);
    assert_eq!(
        derive_network_interface_list_broadcast(
            NetworkAddressFamily::Ipv6,
            &ipv6_address,
            &ipv6_netmask,
            &int_vector(&[0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ),
        int_vector(&[9224, 33287, 9568, 22592, 65535, 65535, 65535, 65535, 0])
    );
}

#[cfg(target_os = "linux")]
#[test]
fn network_interface_info_loopback_matches_gnu_linux_ioctl_metadata() {
    crate::test_utils::init_test_tracing();
    let info = builtin_network_interface_info_impl(vec![Value::string("lo")]).unwrap();

    assert_eq!(
        format!("{info}"),
        "([127 0 0 1 0] [0 0 0 0 0] [255 0 0 0 0] (772 . [0 0 0 0 0 0]) (running loopback up))"
    );
}

#[test]
fn network_lookup_literal_family_filtering_helpers() {
    crate::test_utils::init_test_tracing();
    let loopback_v4 = int_vector(&[127, 0, 0, 1, 0]);
    let loopback_v6 = int_vector(&[0, 0, 0, 0, 0, 0, 0, 1, 0]);

    let v4_any = resolve_network_lookup_addresses("127.0.0.1", None, None);
    let v4_only =
        resolve_network_lookup_addresses("127.0.0.1", Some(NetworkAddressFamily::Ipv4), None);
    let v4_rejected =
        resolve_network_lookup_addresses("127.0.0.1", Some(NetworkAddressFamily::Ipv6), None);
    assert!(!v4_any.is_empty());
    assert_eq!(v4_any, v4_only);
    assert_eq!(v4_any[0], loopback_v4);
    assert!(v4_rejected.is_empty());

    let v6_any = resolve_network_lookup_addresses("::1", None, None);
    let v6_only = resolve_network_lookup_addresses("::1", Some(NetworkAddressFamily::Ipv6), None);
    let v6_rejected =
        resolve_network_lookup_addresses("::1", Some(NetworkAddressFamily::Ipv4), None);
    assert_eq!(v6_any, v6_only);
    if let Some(first) = v6_any.first() {
        assert_eq!(first, &loopback_v6);
    }
    assert!(v6_rejected.is_empty());

    let numeric_v4 = resolve_network_lookup_addresses(
        "127.0.0.1",
        Some(NetworkAddressFamily::Ipv4),
        Some(NetworkLookupHint::Numeric),
    );
    assert_eq!(numeric_v4, vec![loopback_v4]);
    assert!(
        resolve_network_lookup_addresses(
            "localhost",
            Some(NetworkAddressFamily::Ipv4),
            Some(NetworkLookupHint::Numeric)
        )
        .is_empty()
    );
}

#[test]
fn network_lookup_embedded_nul_normalizes_like_c_strings() {
    crate::test_utils::init_test_tracing();
    let plain = resolve_network_lookup_addresses("abc", None, None);
    let embedded_nul = resolve_network_lookup_addresses("abc\0def", None, None);
    assert_eq!(embedded_nul, plain);

    let empty = resolve_network_lookup_addresses("", None, None);
    let nul_only = resolve_network_lookup_addresses("\0", None, None);
    assert_eq!(nul_only, empty);
}

#[test]
fn process_network_interface_and_signal_runtime_surface() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(mapcar (lambda (s)
                     (let ((fn (and (fboundp s) (symbol-function s))))
                       (list s
                             (fboundp s)
                             (and fn (subrp fn))
                             (and fn (subr-arity fn))
                             (commandp s))))
                   '(process-connection
                     format-network-address
                     network-interface-list
                     network-interface-info
                     network-lookup-address-info
                     signal-names))
           (let* ((ifname (or (and (fboundp 'network-interface-list)
                                   (stringp (car (car (network-interface-list))))
                                   (car (car (network-interface-list))))
                              "lo")))
             (list
              (format-network-address [127 0 0 1 80])
              (format-network-address [127 0 0 1 80] t)
              (format-network-address [0 0 0 0 0 0 0 1 80])
              (format-network-address [0 0 0 0 0 0 0 1 80] t)
              (format-network-address "x")
              (format-network-address nil)
              (format-network-address [1])
              (format-network-address [127 0 0 1 65536])
              (format-network-address [0 0 0 0 0 0 0 1 65536])
              (condition-case err (format-network-address) (error err))
              (listp (network-interface-list))
              (consp (car (network-interface-list)))
              (stringp (car (car (network-interface-list))))
              (vectorp (cdr (car (network-interface-list))))
              (listp (network-interface-list nil))
              (let ((entry (car (network-interface-list t))))
                (and (listp entry)
                     (= (length entry) 4)
                     (vectorp (nth 1 entry))
                     (vectorp (nth 2 entry))
                     (vectorp (nth 3 entry))))
              (let* ((entries (network-interface-list t))
                     (ok t))
                (while (and ok entries)
                  (let* ((entry (car entries))
                         (addr (nth 1 entry))
                         (bc (nth 2 entry))
                         (mask (nth 3 entry))
                         (len (length addr))
                         (limit (if (= len 5) 4 8))
                         (bits-mask (if (= len 5) #xff #xffff))
                         (idx 0)
                         (vals nil))
                    (while (< idx limit)
                      (setq vals
                            (append vals
                                    (list (logand bits-mask
                                                  (logior (aref addr idx)
                                                          (lognot (aref mask idx)))))))
                      (setq idx (1+ idx)))
                    (setq vals (append vals '(0)))
                    (setq ok (equal bc (apply #'vector vals))))
                  (setq entries (cdr entries)))
                ok)
              (condition-case err (network-interface-list nil nil nil) (error err))
              (condition-case err (network-interface-list nil t) (error err))
              (let* ((entries (network-interface-list t 'ipv4))
                     (ok t))
                (while (and ok entries)
                  (let* ((entry (car entries))
                         (addr (nth 1 entry)))
                    (setq ok (and (vectorp addr) (= (length addr) 5))))
                  (setq entries (cdr entries)))
                ok)
              (let* ((entries (network-interface-list t 'ipv6))
                     (ok t))
                (while (and ok entries)
                  (let* ((entry (car entries))
                         (addr (nth 1 entry)))
                    (setq ok (and (vectorp addr) (= (length addr) 9))))
                  (setq entries (cdr entries)))
                ok)
              (let* ((entries (network-interface-list nil 'ipv4))
                     (ok t))
                (while (and ok entries)
                  (let* ((entry (car entries))
                         (addr (cdr entry)))
                    (setq ok (and (vectorp addr) (= (length addr) 5))))
                  (setq entries (cdr entries)))
                ok)
              (let* ((entries (network-interface-list nil 'ipv6))
                     (ok t))
                (while (and ok entries)
                  (let* ((entry (car entries))
                         (addr (cdr entry)))
                    (setq ok (and (vectorp addr) (= (length addr) 9))))
                  (setq entries (cdr entries)))
                ok)
              (let ((info (network-interface-info ifname)))
                (and (listp info)
                     (= (length info) 5)
                     (vectorp (car info))
                     (vectorp (nth 1 info))
                     (vectorp (nth 2 info))
                     (or (null (nth 3 info))
                         (consp (nth 3 info)))
                     (listp (nth 4 info))))
              (let ((lo-info (network-interface-info "lo")))
                (and (listp lo-info)
                     (= (length lo-info) 5)
                     (vectorp (car lo-info))
                     (vectorp (nth 1 lo-info))
                     (vectorp (nth 2 lo-info))))
              (let* ((ifname (car (car (network-interface-list nil 'ipv4))))
                     (info (and ifname (network-interface-info ifname)))
                     (entries (network-interface-list nil 'ipv4))
                     (found nil))
                (while entries
                  (let ((entry (car entries)))
                    (if (and (equal (car entry) ifname)
                             (equal (cdr entry) (car info)))
                        (setq found t)))
                  (setq entries (cdr entries)))
                (or (null ifname) found))
              (let* ((info (network-interface-info ifname))
                     (addr (car info))
                     (bc (nth 1 info))
                     (mask (nth 2 info))
                     (len (length addr)))
                (and (or (= len 5) (= len 9))
                     (= (length bc) len)
                     (= (length mask) len)))
              (let* ((lo-info (network-interface-info "lo"))
                     (addr (car lo-info))
                     (bc (nth 1 lo-info))
                     (mask (nth 2 lo-info)))
                (and (= (length addr) (length bc))
                     (= (length addr) (length mask))))
              (equal (network-interface-info (concat "lo" (string 0) "x"))
                     (network-interface-info "lo"))
              (condition-case err (network-interface-info nil) (error err))
              (condition-case err (network-interface-info "abcdefghijklmnop") (error err))
              (condition-case err (network-interface-info (concat "abcdefghijklmnop" (string 0))) (error err))
              (condition-case err (network-interface-info (concat "aaaaaaaaaaaaaa" (string 233))) (error err))
              (null (network-interface-info (concat "aaaaaaaaaaaaa" (string 233))))
              (listp (network-lookup-address-info "localhost"))
              (vectorp (car (network-lookup-address-info "localhost")))
              (listp (network-lookup-address-info "localhost" 'ipv4))
              (vectorp (car (network-lookup-address-info "localhost" 'ipv6)))
              (let* ((v4-any (network-lookup-address-info "127.0.0.1"))
                     (v4-only (network-lookup-address-info "127.0.0.1" 'ipv4)))
                (and (equal v4-any v4-only)
                     (consp v4-only)
                     (equal (car v4-only) [127 0 0 1 0])))
              (null (network-lookup-address-info "127.0.0.1" 'ipv6))
              (let* ((v6-any (network-lookup-address-info "::1"))
                     (v6-only (network-lookup-address-info "::1" 'ipv6)))
                (and (equal v6-any v6-only)
                     (or (null v6-only)
                         (equal (car v6-only) [0 0 0 0 0 0 0 1 0]))))
              (null (network-lookup-address-info "::1" 'ipv4))
              (let* ((entries (network-lookup-address-info "localhost" 'ipv4))
                     (ok t))
                (while (and ok entries)
                  (setq ok (= (length (car entries)) 5))
                  (setq entries (cdr entries)))
                ok)
              (let* ((entries (network-lookup-address-info "localhost" 'ipv6))
                     (ok t))
                (while (and ok entries)
                  (setq ok (= (length (car entries)) 9))
                  (setq entries (cdr entries)))
                ok)
              (equal (network-lookup-address-info (concat "abc" (string 0) "def"))
                     (network-lookup-address-info "abc"))
              (equal (network-lookup-address-info (string 0))
                     (network-lookup-address-info ""))
              (equal (network-lookup-address-info (string-to-multibyte "abc"))
                     (network-lookup-address-info "abc"))
              (let ((err (condition-case err
                             (network-lookup-address-info "é")
                           (error err))))
                (and (consp err)
                     (eq (car err) 'error)
                     (stringp (cadr err))
                     (numberp (string-match-p "Non-ASCII hostname .*puny-encode-domain"
                                              (cadr err)))))
              (condition-case err (network-lookup-address-info "localhost" t) (error err))
              (equal (network-lookup-address-info "127.0.0.1" 'ipv4 'numeric)
                     '([127 0 0 1 0]))
              (null (network-lookup-address-info "localhost" 'ipv4 'numeric))
              (condition-case err (network-lookup-address-info "localhost" 'ipv4 t) (error err))
              (condition-case err (network-lookup-address-info 1) (error err))
              (listp (signal-names))
              (stringp (car (signal-names)))
              (not (null (member "KILL" (signal-names))))
              (condition-case err (signal-names nil) (error err))
              (condition-case err (process-connection nil) (error err))))"#,
    );

    assert_eq!(
        results[0],
        "OK ((process-connection nil nil nil nil) (format-network-address t t (1 . 2) nil) (network-interface-list t t (0 . 2) nil) (network-interface-info t t (1 . 1) nil) (network-lookup-address-info t t (1 . 3) nil) (signal-names t t (0 . 0) nil))"
    );
    assert_eq!(
        results[1],
        "OK (\"127.0.0.1:80\" \"127.0.0.1\" \"[0:0:0:0:0:0:0:1]:80\" \"0:0:0:0:0:0:0:1\" \"x\" nil nil nil nil (wrong-number-of-arguments format-network-address 0) t t t t t t t (wrong-number-of-arguments network-interface-list 3) (error \"Unsupported address family\") t t t t t t t t t t (wrong-type-argument stringp nil) (error \"interface name too long\") (error \"interface name too long\") (error \"interface name too long\") t t t t t t t t t t t t t t t (error \"Unsupported family\") t t (error \"Unsupported hints value\") (wrong-type-argument stringp 1) t t t (wrong-number-of-arguments signal-names 1) (void-function process-connection))"
    );
}

#[test]
fn format_network_address_cons_family_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(list
            (format-network-address (cons 17 [1 2 3]))
            (format-network-address (cons -1 [1 2 3]))
            (car (condition-case err
                     (format-network-address (cons 'x [1 2 3]))
                   (error err))))"#,
    );

    assert_eq!(results[0], "OK (\"<Family 17>\" \"<Family -1>\" error)");
}

#[test]
fn gnutls_log_level_is_defined_for_tls_negotiation() {
    // GNU `gnutls.c` DEFVAR_INTs `gnutls-log-level` (default 0); `gnutls.el`
    // only forward-declares it (`(defvar gnutls-log-level)  ; gnutls.c`).
    // Without the C-side definition the variable is void and
    // `gnutls-negotiate` errors on `:loglevel ,gnutls-log-level` before it can
    // call `gnutls-boot`, so every TLS package download fails and
    // `use-package` is unusable.  https://github.com/eval-exec/neomacs/issues/121
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one("(list (boundp 'gnutls-log-level) gnutls-log-level)");
    assert_eq!(result, "OK (t 0)");
}

#[test]
fn libgnutls_version_is_defined_for_nsm_tls_checks() {
    // GNU `gnutls.c` DEFVAR_LISPs `libgnutls-version` even without GnuTLS,
    // where its documented value is -1.  `nsm.el` reads it during TLS package
    // refresh, so it must be bound even though Neomacs uses a Rust TLS backend.
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one("(list (boundp 'libgnutls-version) libgnutls-version)");
    assert_eq!(result, "OK (t -1)");
}

#[test]
fn process_adaptive_read_buffering_is_a_bound_nil_variable() {
    // GNU `process.c` `syms_of_process` DEFVAR_LISPs
    // `process-adaptive-read-buffering` (default nil).  It is a *variable*,
    // not a function: GNU has no `process-adaptive-read-buffering-p` nor
    // `set-process-adaptive-read-buffering`.  The variable must be bound to
    // nil so `(boundp 'process-adaptive-read-buffering)` is t and reading it
    // (e.g. tramp-sh.el's `(let ((process-adaptive-read-buffering nil)) ...)`)
    // does not raise `void-variable`.
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list (boundp 'process-adaptive-read-buffering)
               process-adaptive-read-buffering
               (default-boundp 'process-adaptive-read-buffering)
               (fboundp 'process-adaptive-read-buffering-p)
               (fboundp 'set-process-adaptive-read-buffering))",
    );
    assert_eq!(result, "OK (t nil t nil nil)");
}

#[test]
fn process_defvars_match_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        r#"(list
           delete-exited-processes
           process-prioritize-lower-fds
           interrupt-process-functions
           signal-process-functions
           internal--daemon-sockname
           read-process-output-max
           fast-read-process-output
           process-error-pause-time
           (default-boundp 'interrupt-process-functions)
           (default-boundp 'signal-process-functions)
           (default-boundp 'fast-read-process-output))"#,
    );

    assert_eq!(
        result,
        "OK (t nil (internal-default-interrupt-process) (internal-default-signal-process) nil 65536 t 1 t t t)"
    );
}

#[test]
fn read_process_output_max_limits_filter_chunks_and_snapshots_at_creation() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        r#"(let ((chunks nil)
                 (p nil))
             (unwind-protect
                 (progn
                   (let ((read-process-output-max 5)
                         (process-connection-type nil))
                     (setq p
                           (make-process
                            :name "readmax-unit"
                            :buffer nil
                            :connection-type 'pipe
                            :command (list "/bin/sh" "-c" "printf 0123456789abcdef")
                            :filter (lambda (_ string)
                                      (push (length string) chunks)))))
                   (setq read-process-output-max 1000)
                   (while (process-live-p p)
                     (accept-process-output p 1))
                   (while (accept-process-output p 0))
                   (nreverse chunks))
               (when p
                 (ignore-errors
                   (delete-process p)))))"#,
    );

    assert_eq!(result, "OK (5 5 5 1)");
}

#[test]
fn read_process_output_carries_split_decode_sequences_between_chunks() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        r#"(let ((process-connection-type nil))
             (let ((probe
                    (lambda (script coding)
                      (let ((chunks nil)
                            (p nil))
                        (unwind-protect
                            (progn
                              (let ((read-process-output-max 1))
                                (setq p
                                      (make-process
                                       :name "readmax-utf8-unit"
                                       :buffer nil
                                       :connection-type 'pipe
                                       :coding coding
                                       :command (list "/bin/sh" "-c" script)
                                       :filter (lambda (_ string)
                                                 (push (list (length string)
                                                             (string-to-list string))
                                                       chunks)))))
                              (while (process-live-p p)
                                (accept-process-output p 1))
                              (while (accept-process-output p 0))
                              (nreverse chunks))
                          (when p
                            (ignore-errors
                              (delete-process p))))))))
               (list (funcall probe "printf '\\303\\251X'" 'utf-8-unix)
                     (funcall probe "printf '\\303'" 'utf-8-unix)
                     (funcall probe "printf '\\r\\nX'" 'utf-8-dos))))"#,
    );

    assert_eq!(
        result,
        "OK (((1 (233)) (1 (88))) ((1 (4194243))) ((1 (10)) (1 (88))))"
    );
}

#[test]
fn adaptive_read_buffering_updates_delay_with_gnu_thresholds() {
    let mut processes = ProcessManager::new();
    processes.set_default_read_config(ProcessReadConfig {
        readmax: 1024,
        adaptive_read_buffering: 1,
    });
    let id = processes.create_process(
        "adaptive-read".into(),
        Value::NIL,
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let process = processes.get_mut(id).expect("process");

    update_process_adaptive_read_buffering(process, 42, false);
    assert_eq!(process.read_output_delay, Duration::from_millis(20));
    assert!(process.read_output_skip);

    process.read_output_skip = false;
    update_process_adaptive_read_buffering(process, 1024, true);
    assert_eq!(process.read_output_delay, Duration::from_millis(10));
    assert!(process.read_output_skip);

    update_process_adaptive_read_buffering(process, 1024, true);
    assert_eq!(process.read_output_delay, Duration::ZERO);
    assert!(!process.read_output_skip);
}

#[test]
fn internal_default_process_filter_inserts_while_another_buffer_is_read_only() {
    // neomacs#192 (magit-blame shows nothing): magit's blame filter calls
    // `internal-default-process-filter' while the BLAMED buffer is current and
    // read-only (`magit-blame-mode' makes it so), and the blame output belongs to
    // magit's own process buffer. GNU inserts it: `read_process_output_before_insert'
    // (src/process.c) does `Fset_buffer (p->buffer)' FIRST, so the read-only barf
    // in `prepare_to_modify_buffer' tests the process buffer, not whatever buffer
    // happened to be current. neomacs tested the current buffer and signalled
    // `buffer-read-only', once per chunk, so no blame chunk was ever parsed.
    //
    // Verified against GNU 31 in batch: the insert succeeds and returns "chunk".
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let process_buffer = ev.buffers.create_buffer("*proc-filter-ro*");
    let pid = ev.processes.create_process(
        "proc-filter-ro".into(),
        Value::make_buffer(process_buffer),
        "prog".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .sync_process_mark(&mut ev.buffers, pid)
        .expect("sync process mark");

    // A different, read-only buffer is current -- the blamed source file.
    let blamed = ev.buffers.create_buffer("blamed.c");
    ev.buffers.set_current(blamed);
    if let Some(buffer) = ev.buffers.get_mut(blamed) {
        buffer.set_read_only_value(true);
    }

    builtin_internal_default_process_filter(
        &mut ev,
        vec![Value::make_process(pid), Value::string("chunk")],
    )
    .expect("process output must be inserted despite the read-only current buffer");

    assert_eq!(
        ev.buffers
            .get(process_buffer)
            .expect("process buffer")
            .buffer_substring_lisp_string_range(crate::buffer::EmacsByteRange::from_usize(0, 5))
            .as_bytes(),
        b"chunk"
    );
    // GNU leaves the process buffer current (only `read_process_output''s unwind
    // restores it), which is what a Lisp caller observes.
    assert_eq!(
        ev.buffers.current_buffer().map(|buffer| buffer.id),
        Some(process_buffer)
    );
    // The read-only buffer is untouched, and still read-only.
    assert!(
        ev.buffers
            .get(blamed)
            .expect("blamed buffer")
            .get_read_only()
    );
}

#[test]
fn internal_default_process_filter_uses_a_reaped_process_buffer() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let* ((target (get-buffer-create " *reaped-filter-target*"))
                  (other (get-buffer-create " *reaped-filter-other*"))
                  (process
                   (make-pipe-process
                    :name "reaped-filter"
                    :buffer target)))
             (delete-process process)
             (with-current-buffer other
               (setq buffer-read-only t)
               (internal-default-process-filter process "chunk")
               (list
                (with-current-buffer target
                  (and (string-suffix-p "chunk" (buffer-string)) t))
                (eq (current-buffer) target))))"#,
    );

    assert_eq!(result, "OK (t t)");
}

/// CONTRACT: a process reaching a terminal status dirties chrome.
///
/// GNU's `status_notify` calls `bset_update_mode_line` on the process's buffer
/// when its status changed (process.c:7940), because `mode-line-process`
/// renders that status. This is the one chrome trigger whose staleness is
/// invisible to the editing user — nothing else is going to repaint, so the
/// exited process would keep showing "Run" until some unrelated edit happened
/// to force a chrome walk.
///
/// This pin exists because a mutation run found the trigger UNCOVERED: the
/// commit that added it claimed a pin per defect, but removing this call site
/// reddened nothing. The layout-engine harness cannot stage a live child, so
/// the case belongs here, next to the other process tests.
#[test]
fn process_reaching_terminal_status_dirties_chrome() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    // Reach a steady state first: whatever the startup did, acknowledge it, so
    // the assertion below is about the process and nothing else.
    ctx.note_chrome_generated(crate::window::WindowId(1));
    while ctx.chrome_dirty().is_any_dirty() {
        ctx.note_chrome_generated(crate::window::WindowId(1));
        break;
    }

    let before = ctx.chrome_dirty().is_dirty(crate::window::WindowId(1));
    let result = ctx.eval_str(
        // Only core builtins here: this Context has no subr.el, so
        // `process-live-p` / `ignore-errors` / `generate-new-buffer` are all
        // void -- and so is `start-process', which is `subr.el:3466' Lisp over
        // `make-process' (DIVERGENCES.md 149). The bounded loop also keeps a
        // wedged child from hanging CI.
        r#"(let* ((buf (get-buffer-create "p52-proc-chrome"))
                  (p (make-process :name "p52-proc" :buffer buf
                                   :command (list "true")))
                  (n 0))
             (while (and (< n 100) (eq (process-status p) 'run))
               (accept-process-output p 0.1)
               (setq n (+ n 1)))
             (symbol-name (process-status p)))"#,
    );
    assert!(result.is_ok(), "the child must run: {result:?}");

    assert!(
        !before || ctx.chrome_dirty().is_dirty(crate::window::WindowId(1)),
        "precondition sanity"
    );
    assert!(
        ctx.chrome_dirty().is_any_dirty(),
        "a process reaching a terminal status must dirty chrome, so \
         mode-line-process stops showing a stale status (GNU process.c:7940)"
    );
}

/// DIVERGENCES.md #18: an error signaled inside a process FILTER is reported,
/// not swallowed. GNU routes it through `cmd_error_internal (error_val, "error
/// in process filter: ")` (process.c:6208), whose default reporter writes the
/// diagnostic to stderr and calls `Fkill_emacs (-1)` when noninteractive
/// (keyboard.c:1078-1083) -- so batch dies with status 255 and the form after
/// the error never runs.
#[test]
fn a_process_filter_error_is_reported_and_kills_batch_like_gnu() {
    let mut eval = Context::new();
    eval.assign("noninteractive", Value::T);

    let flow = eval.finish_callback_flow(
        Err(signal("error", vec![Value::string("filter boom")])),
        AsyncCallbackKind::ProcessFilter,
    );

    match flow {
        Err(Flow::Shutdown(request)) => {
            assert_eq!(
                request.exit_code, -1,
                "GNU's Fkill_emacs (-1) is exit status 255"
            );
        }
        other => panic!("a filter error must not be swallowed, got {other:?}"),
    }
    assert_eq!(
        eval.shutdown_request().map(|r| r.exit_code),
        Some(-1),
        "the exit must be recorded, so the run cannot continue past it"
    );
}

/// Same for SENTINELS: GNU `exec_sentinel_error_handler` (process.c:7791) uses
/// the identical path with the "error in process sentinel: " context.
#[test]
fn a_process_sentinel_error_is_reported_and_kills_batch_like_gnu() {
    let mut eval = Context::new();
    eval.assign("noninteractive", Value::T);

    let flow = eval.finish_callback_flow(
        Err(signal("error", vec![Value::string("sentinel boom")])),
        AsyncCallbackKind::ProcessSentinel,
    );

    assert!(
        matches!(flow, Err(Flow::Shutdown(request)) if request.exit_code == -1),
        "a sentinel error must not be swallowed, got {flow:?}"
    );
}

/// A TIMER error is NOT fatal, and this is the boundary the fix must not cross.
/// GNU runs timers through timer.el `timer-event-handler`, which wraps the call
/// in `condition-case-unless-debug` and merely `message`s the error
/// (timer.el:332-338) -- it never reaches `cmd_error_internal`, so batch
/// survives a failing timer.
#[test]
fn a_timer_error_is_logged_but_never_fatal_like_gnu() {
    let mut eval = Context::new();
    eval.assign("noninteractive", Value::T);

    let flow = eval.finish_callback_flow(
        Err(signal("error", vec![Value::string("timer boom")])),
        AsyncCallbackKind::Timer,
    );

    assert!(flow.is_ok(), "a timer error must not end the process");
    assert_eq!(eval.shutdown_request(), None);
}

/// Interactive sessions report the same error WITHOUT exiting: GNU's default
/// reporter only writes-and-dies on the noninteractive branch.
#[test]
fn an_interactive_filter_error_is_reported_without_exiting_like_gnu() {
    let mut eval = Context::new();
    eval.assign("noninteractive", Value::NIL);

    let flow = eval.finish_callback_flow(
        Err(signal("error", vec![Value::string("filter boom")])),
        AsyncCallbackKind::ProcessFilter,
    );

    assert!(flow.is_ok(), "an interactive filter error must not exit");
    assert_eq!(eval.shutdown_request(), None);
}

/// Ledger entry 54: the implicit `:stderr` pipe is `closed`, and still
/// attached, when the owner's sentinel runs.
///
/// GNU splits this across two phases. The fd-scan loop retires a pipe
/// connection the moment its read returns 0 -- `tick++`, `deactivate_process`,
/// and status `(exit 0)` if it was still running (src/process.c:6072-6080),
/// which `process-status` reports as `closed` for a pipe
/// (src/process.c:1193) -- while the SENTINEL runs later, from `status_notify`
/// (src/process.c:7873), which the fd loop calls only after it finishes.
/// `status_notify` walks the process alist newest-first, so the owner, created
/// after its stderr pipe, is notified first and sees the pipe closed but not
/// yet removed.
///
/// The consequence is Lisp-visible: `process-kill-buffer-query-function`
/// (lisp/subr.el:3542) prompts only for a process whose status is one of
/// `run stop open listen`, so a sentinel that kills the stderr buffer -- what
/// Magit's blame sentinel does -- prompts on `open` and is silent on `closed`.
/// In batch that prompt reads EOF from stdin and kills the session.
///
/// Twelve iterations, both empty and non-empty stderr payloads, because the
/// divergence was timing-dependent: GNU answers `closed` 12/12; before this
/// fix neomacs answered `GONE` 10/12 (the pipe reaped out of the alist ahead
/// of the owner's sentinel) and `open` 2/12 (EOF discarded entirely).
#[test]
fn the_stderr_pipe_is_closed_and_attached_when_the_owner_sentinel_runs() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let ((seen nil))
             (dolist (payload '("" "boom"))
               (dotimes (_ 6)
                 (let* ((out (generate-new-buffer " *p14-out*"))
                        (err (generate-new-buffer " *p14-err*"))
                        (done nil))
                   (let ((p (make-process
                             :name "p14"
                             :buffer out
                             :stderr err
                             :command (list "{sh}" "-c"
                                            (format "printf '%%s' '%%s' 1>&2; printf 'x'"
                                                    payload)))))
                     (set-process-sentinel
                      p (lambda (pr _e)
                          (when (memq (process-status pr) '(exit signal))
                            (let ((sp (get-buffer-process err)))
                              (push (if sp (process-status sp) 'GONE) seen))
                            (setq done t))))
                     (while (not done) (accept-process-output nil 0.05))))))
             (delete-dups (nreverse seen)))"#
    ));

    assert_eq!(result, "OK (closed)");
}

/// The deferred half of the same split: the pipe's own sentinel still runs,
/// after the owner's. GNU's `status_notify` reaches the older stderr pipe on
/// the same pass, so retiring it early in the fd loop must not cost it its
/// notification.
#[test]
fn the_stderr_pipe_sentinel_still_runs_after_the_owners() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((order nil)
                  (out (generate-new-buffer " *p14b-out*"))
                  (err (generate-new-buffer " *p14b-err*"))
                  (done nil)
                  (p (make-process
                      :name "p14b"
                      :buffer out
                      :stderr err
                      :command (list "{sh}" "-c" "printf 'e' 1>&2; printf 'x'"))))
             (set-process-sentinel (get-buffer-process err)
                                   (lambda (&rest _) (push 'stderr order)))
             (set-process-sentinel
              p (lambda (pr _e)
                  (when (memq (process-status pr) '(exit signal))
                    (push 'owner order)
                    (setq done t))))
             (while (not done) (accept-process-output nil 0.05))
             (dotimes (_ 4) (accept-process-output nil 0.05))
             (nreverse order))"#
    ));

    assert_eq!(result, "OK (owner stderr)");
}

/// GNU `Faccept_process_output` passes READ_KBD = 0 to
/// `wait_reading_process_output` (process.c:4957-4959), and with READ_KBD = 0
/// pending input never ends the wait -- the loop only calls `swallow_events`,
/// its `break` is `#if 0`-ed out under "Exiting when read_kbd doesn't request
/// that seems wrong, though" (process.c:5930-5937) -- and the docstring
/// promises the call "should not be expected to return before the timeout
/// expires".
///
/// The not-yet-executed events of a running keyboard macro are pending input,
/// so a wait started from a command inside a macro is the case that exposes a
/// yield-on-input policy: the first of two macro commands returned instantly
/// while the second, with the macro exhausted, waited the full time.  Measured
/// on GNU Emacs -Q --batch (tmp/p97-probe10.el): both commands wait.
#[test]
fn accept_process_output_inside_a_kbd_macro_waits_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(
            r#"(progn
                 (setq neo-accept-wait-log nil)
                 (defun neo-accept-wait-command ()
                   (interactive)
                   (let ((start (float-time)))
                     (accept-process-output nil 0.05)
                     (push (- (float-time) start) neo-accept-wait-log)))
                 (let ((map (make-sparse-keymap)))
                   (define-key map "a" #'neo-accept-wait-command)
                   (use-local-map map)
                   (execute-kbd-macro "aa"))
                 (mapcar (lambda (elapsed) (>= elapsed 0.04))
                         neo-accept-wait-log))"#,
        )
        .expect("keyboard macro should execute");

    assert_eq!(format!("{result}"), "(t t)");
}

/// GNU `Fmake_process` resolves the coding system its child's output is
/// DECODED with through a four-step chain (src/process.c:1950-1976): the
/// `:coding` keyword's car, then `coding-system-for-read`, then the car of
/// `(find-operation-coding-system 'start-process NAME BUFFER COMMAND...)` --
/// i.e. `process-coding-system-alist` matched against the PROGRAM, because
/// `start-process`'s `target-idx` is 2 (src/coding.c:11784) -- and finally the
/// car of `default-process-coding-system`.
///
/// Every expected value below was measured by running tmp/pw131/pin2.el under
/// GNU Emacs 31.0.90; none was derived.
#[test]
fn make_process_decode_coding_follows_gnu_precedence_chain() {
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw131-chars (setup)
               (let ((buf (generate-new-buffer " *pw131*")))
                 (unwind-protect
                     (let ((p (funcall setup buf)))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (append (with-current-buffer buf (buffer-string)) nil))
                   (kill-buffer buf))))
             (defun pw131-spawn (buf &rest keys)
               (apply #'make-process :name "pw131" :buffer buf :sentinel #'ignore
                      :command (list "{printf}" "caf\\303\\251") keys))
             (list
              ;; `coding-system-for-read' wins outright, and a byte-faithful
              ;; coding leaves the child's bytes as eight-bit characters.
              (pw131-chars (lambda (b) (let ((coding-system-for-read 'binary))
                                         (pw131-spawn b))))
              ;; ... including a coding that really converts.
              (pw131-chars (lambda (b) (let ((coding-system-for-read 'latin-1))
                                         (pw131-spawn b))))
              ;; `process-coding-system-alist', matched against the PROGRAM.
              (pw131-chars (lambda (b)
                             (let ((process-coding-system-alist
                                    '(("printf" binary . binary))))
                               (pw131-spawn b))))
              ;; `default-process-coding-system'.
              (pw131-chars (lambda (b)
                             (let ((default-process-coding-system '(binary . binary)))
                               (pw131-spawn b))))
              ;; An explicit `:coding' beats `coding-system-for-read'.
              (pw131-chars (lambda (b) (let ((coding-system-for-read 'binary))
                                         (pw131-spawn b :coding 'utf-8))))
              ;; `:coding (nil . X)' is still a PRESENT `:coding', so the nil car
              ;; does NOT fall back to `coding-system-for-read' -- GNU's `else'
              ;; branch at src/process.c:1957 is skipped and the chain resumes at
              ;; the alist (src/process.c:1959-1976).
              (pw131-chars (lambda (b) (let ((coding-system-for-read 'binary))
                                         (pw131-spawn b :coding '(nil . latin-1)))))
              ;; An undefined coding system signals rather than falling back.
              (condition-case e
                  (pw131-chars (lambda (b)
                                 (let ((coding-system-for-read 'no-such-coding-xyz))
                                   (pw131-spawn b))))
                (error (list (car e) (cadr e))))
              ;; With no PROGRAM there is nothing to match, so GNU never calls
              ;; `find-operation-coding-system' at all (src/process.c:1970) and
              ;; the chain lands on `default-process-coding-system', which is
              ;; bound here rather than inherited so the pin does not encode the
              ;; locale the test happens to run under.
              (let* ((process-coding-system-alist '(("." binary . binary)))
                     (default-process-coding-system '(utf-8-unix . utf-8-unix))
                     (p (make-process :name "printf" :buffer nil :command nil)))
                (prog1 (process-coding-system p) (delete-process p)))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(99 97 102 4194243 4194217) ",
            "(99 97 102 195 169) ",
            "(99 97 102 4194243 4194217) ",
            "(99 97 102 4194243 4194217) ",
            "(99 97 102 233) ",
            "(99 97 102 233) ",
            "(coding-system-error no-such-coding-xyz) ",
            "(utf-8-unix . utf-8-unix))",
        )
    );
}

/// GNU decides an asynchronous process's decoder in TWO stages.  The chain
/// above stores a coding system on the process; `setup_process_coding_systems`
/// (src/process.c:8380-8409) then turns that into the decoder the bytes really
/// go through, and it is there -- not in any of the five creation-time
/// resolvers -- that a UNIBYTE destination drops character-code conversion
/// while KEEPING end-of-line conversion (`raw_text_coding_system`,
/// src/process.c:8398-8399).  The rule applies only when the process's filter
/// is the internal default one and its buffer is a live unibyte buffer, so a
/// Lisp filter never sees it.
///
/// Every expected value below was measured under GNU Emacs 31.0.90.
#[test]
fn make_process_unibyte_buffer_drops_character_conversion_but_keeps_eol() {
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw131-crlf (unibyte coding &optional filter)
               (let ((buf (generate-new-buffer " *pw131*"))
                     (seen nil))
                 (unwind-protect
                     (progn
                       (when unibyte
                         (with-current-buffer buf (set-buffer-multibyte nil)))
                       (let ((p (apply #'make-process
                                       :name "pw131" :buffer buf :sentinel #'ignore
                                       :coding coding
                                       :command (list "{printf}" "caf\\303\\251\\r\\n")
                                       (when filter
                                         (list :filter
                                               (lambda (_p s)
                                                 (setq seen (concat seen s))))))))
                         (while (accept-process-output p 1))
                         (while (process-live-p p) (accept-process-output p 0.05))
                         (append (or seen (with-current-buffer buf (buffer-string)))
                                 nil)))
                   (kill-buffer buf))))
             (list
              ;; unibyte destination, DOS eol: no character conversion, but the
              ;; CR is still eaten.
              (pw131-crlf t 'utf-8-dos)
              ;; unibyte destination, UNIX eol: no character conversion AND the
              ;; CR survives -- the two halves disagree, so neither is guessable.
              (pw131-crlf t 'utf-8-unix)
              ;; multibyte destination: both halves apply.
              (pw131-crlf nil 'utf-8-dos)
              ;; a Lisp filter is handed decoded text, so the downgrade does not
              ;; apply even though the process buffer is unibyte.
              (pw131-crlf t 'utf-8-dos t)
              ;; the rule also applies to the coding the CHAIN produced, not
              ;; just to an explicit `:coding'.
              (let ((buf (generate-new-buffer " *pw131*")))
                (unwind-protect
                    (progn
                      (with-current-buffer buf (set-buffer-multibyte nil))
                      (let ((p (make-process :name "pw131" :buffer buf
                                             :sentinel #'ignore
                                             :command (list "{printf}" "caf\\303\\251"))))
                        (while (accept-process-output p 1))
                        (while (process-live-p p) (accept-process-output p 0.05))
                        (append (with-current-buffer buf (buffer-string)) nil)))
                  (kill-buffer buf)))
              ;; and it is re-decided against the process's CURRENT buffer.
              ;; This process was created against a multibyte buffer and handed
              ;; a unibyte one afterwards, which is why GNU re-runs
              ;; `setup_process_coding_systems' from `set-process-buffer'
              ;; (src/process.c:1312) rather than freezing the answer.
              (let ((mb (generate-new-buffer " *pw131-mb*"))
                    (ub (generate-new-buffer " *pw131-ub*")))
                (unwind-protect
                    (progn
                      (with-current-buffer ub (set-buffer-multibyte nil))
                      (let ((p (make-process
                                :name "pw131" :buffer mb :sentinel #'ignore
                                :coding 'utf-8-dos
                                :command (list "{sh}" "-c"
                                               "sleep 0.3; printf 'caf\\303\\251\\r\\n'"))))
                        (set-process-buffer p ub)
                        (while (accept-process-output p 1))
                        (while (process-live-p p) (accept-process-output p 0.05))
                        (append (with-current-buffer ub (buffer-string)) nil)))
                  (kill-buffer mb)
                  (kill-buffer ub)))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(99 97 102 195 169 10) ",
            "(99 97 102 195 169 13 10) ",
            "(99 97 102 233 10) ",
            "(99 97 102 233 10) ",
            "(99 97 102 195 169) ",
            "(99 97 102 195 169 10))",
        )
    );
}

/// The ENCODE half of the same C block (src/process.c:1979-2007) runs the
/// mirror-image chain -- `:coding`'s cdr, `coding-system-for-write`, the cdr of
/// the `find-operation-coding-system` answer, the cdr of
/// `default-process-coding-system` -- and `process-coding-system` reports both
/// halves.
///
/// Every expected value below was measured under GNU Emacs 31.0.90.
#[test]
fn make_process_encode_coding_follows_gnu_precedence_chain() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw131-coding (thunk)
               (let ((p (funcall thunk)))
                 (prog1 (process-coding-system p) (delete-process p))))
             (list
              ;; The decode half falls through to
              ;; `default-process-coding-system', bound here so the pin does not
              ;; encode the locale the test happens to run under.
              (pw131-coding (lambda ()
                              (let ((coding-system-for-write 'latin-1)
                                    (default-process-coding-system
                                     '(utf-8-unix . utf-8-unix)))
                                (make-process :name "pw131" :buffer nil
                                              :sentinel #'ignore
                                              :command (list "{cat}")))))
              (pw131-coding (lambda ()
                              (let ((default-process-coding-system
                                     '(latin-1 . koi8-r)))
                                (make-process :name "pw131" :buffer nil
                                              :sentinel #'ignore
                                              :command (list "{cat}")))))
              (pw131-coding (lambda ()
                              (let ((process-coding-system-alist
                                     '(("cat" latin-1 . koi8-r))))
                                (make-process :name "pw131" :buffer nil
                                              :sentinel #'ignore
                                              :command (list "{cat}")))))))"#
    ));

    assert_eq!(
        result,
        "OK ((utf-8-unix . latin-1) (latin-1 . koi8-r) (latin-1 . koi8-r))"
    );
}

/// GNU `Fmake_pipe_process` has its own coding resolver
/// (src/process.c:2517-2570), and it is NOT `Fmake_process`'s.  It cannot reach
/// `process-coding-system-alist` at all -- `coding_systems` is initialised to
/// `Qt` at :2520 and never assigned, so the `CONSP (coding_systems)` arm is
/// dead code -- and it short-circuits to nil when a buffer is unibyte, asking
/// the PROCESS buffer for the decode half (:2533-2534) and `current_buffer`
/// for the encode half (:2559-2560).
///
/// Every expected value below was measured by running tmp/pw137/pin.el under
/// GNU Emacs 31.0.90; none was derived.  `default-process-coding-system` is
/// bound explicitly wherever it is the answer, because the unit bootstrap
/// leaves it at `(undecided-unix . utf-8-unix)` while both shipped editors
/// hold `(utf-8-unix . utf-8-unix)`; a pin that inherited it would be recording
/// the runtime rather than the behaviour.
#[test]
fn make_pipe_process_coding_follows_gnus_pipe_chain() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar pw137-n 0)
             (defun pw137-pipe-cs (&rest args)
               (setq pw137-n (1+ pw137-n))
               (let ((p (apply #'make-pipe-process :name (format "pw137p-%d" pw137-n)
                               :noquery t :sentinel #'ignore args)))
                 (prog1 (process-coding-system p) (delete-process p))))
             (list
              ;; Nothing bound: both halves land on `default-process-coding-system'.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              ;; The two dynamic overrides, one half each.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (coding-system-for-read 'binary))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (coding-system-for-write 'latin-1))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              ;; A SUPPLIED `:coding' ENDS the chain, even for the half whose
              ;; value is nil: GNU writes the connection primitives as one
              ;; `else if' chain, so a non-nil `tem' skips every later arm.
              ;; `make-process' is written with a separate `if (NILP (val))'
              ;; for its tail and therefore answers `utf-8-unix' for this same
              ;; form -- see `make_process_decode_coding_follows_gnu_precedence_chain'.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (coding-system-for-read 'binary))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")
                               :coding '(nil . latin-1)))
              (pw137-pipe-cs :buffer (generate-new-buffer " *mb*") :coding 'utf-8-dos)
              ;; A unibyte PROCESS buffer short-circuits the DECODE half only.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (b (generate-new-buffer " *ub*")))
                (with-current-buffer b (set-buffer-multibyte nil))
                (pw137-pipe-cs :buffer b))
              ;; ... and the override still beats it.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (coding-system-for-read 'latin-1)
                    (b (generate-new-buffer " *ub*")))
                (with-current-buffer b (set-buffer-multibyte nil))
                (pw137-pipe-cs :buffer b))
              ;; A unibyte CURRENT buffer short-circuits the ENCODE half, and
              ;; the two halves therefore answer for DIFFERENT buffers.  This
              ;; row is the one a single "the buffer is unibyte" flag cannot
              ;; express.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (cur (generate-new-buffer " *cur*"))
                    (pb (generate-new-buffer " *mb*")))
                (with-current-buffer cur
                  (set-buffer-multibyte nil)
                  (pw137-pipe-cs :buffer pb)))
              ;; The tail really is `default-process-coding-system', both halves.
              (let ((default-process-coding-system '(latin-1 . koi8-r)))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              (let ((default-process-coding-system nil))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              ;; Neither alist is reachable from a pipe process.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (process-coding-system-alist '(("pw137p" binary . binary))))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                    (network-coding-system-alist '(("pw137p" binary . binary))))
                (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
              ;; GNU checks the value the CHAIN produced, not the `:coding'
              ;; keyword, because the check is `setup_coding_system' reached
              ;; through `setup_process_coding_systems' (src/process.c:2573).
              (condition-case e
                  (pw137-pipe-cs :buffer (generate-new-buffer " *mb*") :coding 'no-such-xyz)
                (error (list (car e) (cadr e))))
              (condition-case e
                  (let ((coding-system-for-read 'no-such-xyz))
                    (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
                (error (list (car e) (cadr e))))
              (condition-case e
                  (let ((coding-system-for-write 'no-such-xyz))
                    (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
                (error (list (car e) (cadr e))))
              (condition-case e
                  (let ((default-process-coding-system '(no-such-xyz . no-such-xyz)))
                    (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")))
                (error (list (car e) (cadr e))))
              ;; Nothing is left behind when it fires.
              (let ((before (length (process-list))))
                (ignore-errors
                  (pw137-pipe-cs :buffer (generate-new-buffer " *mb*")
                                 :coding 'no-such-xyz))
                (- (length (process-list)) before))))"#,
    );

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(utf-8-unix . utf-8-unix) ",
            "(binary . utf-8-unix) ",
            "(utf-8-unix . latin-1) ",
            "(nil . latin-1) ",
            "(utf-8-dos . utf-8-dos) ",
            "(nil . utf-8-unix) ",
            "(latin-1 . utf-8-unix) ",
            "(utf-8-unix) ",
            "(latin-1 . koi8-r) ",
            "(nil) ",
            "(utf-8-unix . utf-8-unix) ",
            "(utf-8-unix . utf-8-unix) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "0)",
        )
    );
}

/// The pipe chain's answer is what the BYTES go through, not just what
/// `process-coding-system` reports.
///
/// A pipe process is fed by handing it to `make-process` as `:stderr` -- which
/// is also how GNU itself builds one when `:stderr` names a buffer
/// (`CALLN (Fmake_pipe_process, ...)`, src/process.c:1883) -- so the child's
/// stderr is decoded by the PIPE's resolver, under whatever was bound when the
/// pipe was created rather than when the child was spawned.
///
/// The last three rows are the shared second stage on top of it:
/// `setup_process_coding_systems` drops character-code conversion for a unibyte
/// process buffer while KEEPING end-of-line conversion (src/process.c:8395-8399,
/// entry 131), and it applies to the coding the chain produced -- nil, i.e.
/// `raw_text_coding_system (Qnil)` = bare `raw-text`, which DETECTS the line
/// endings (entry 134).
///
/// Every expected value below was measured under GNU Emacs 31.0.90.
#[test]
fn make_pipe_process_stderr_bytes_go_through_the_pipes_own_chain() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defvar pw137-n 0)
             (defun pw137-stderr (bind-fn coding unibyte)
               (setq pw137-n (1+ pw137-n))
               (let* ((buf (generate-new-buffer " *err*"))
                      (_ (when unibyte
                           (with-current-buffer buf (set-buffer-multibyte nil))))
                      (pipe (funcall bind-fn
                                     (lambda ()
                                       (apply #'make-pipe-process
                                              :name (format "pw137e-%d" pw137-n)
                                              :noquery t :sentinel #'ignore :buffer buf
                                              (if coding (list :coding coding) nil)))))
                      (p (make-process :name (format "pw137c-%d" pw137-n) :noquery t
                                       :buffer nil :sentinel #'ignore :stderr pipe
                                       :command (list "{sh}" "-c"
                                                      "printf 'caf\\303\\251\\r\\nx\\r\\n' >&2"))))
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (dotimes (_ 20) (accept-process-output pipe 0.05))
                 (prog1 (append (with-current-buffer buf (buffer-string)) nil)
                   (delete-process pipe))))
             (list
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                (pw137-stderr #'funcall nil nil))
              (pw137-stderr (lambda (f) (let ((coding-system-for-read 'binary)) (funcall f)))
                            nil nil)
              (pw137-stderr (lambda (f) (let ((coding-system-for-read 'raw-text)) (funcall f)))
                            nil nil)
              (pw137-stderr (lambda (f) (let ((coding-system-for-read 'latin-1)) (funcall f)))
                            nil nil)
              (pw137-stderr (lambda (f)
                              (let ((default-process-coding-system '(binary . binary)))
                                (funcall f)))
                            nil nil)
              (pw137-stderr #'funcall 'utf-8-dos nil)
              (pw137-stderr #'funcall 'utf-8-dos t)
              (pw137-stderr #'funcall 'utf-8-unix t)
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                (pw137-stderr #'funcall nil t))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(99 97 102 233 13 10 120 13 10) ",
            "(99 97 102 4194243 4194217 13 10 120 13 10) ",
            "(99 97 102 4194243 4194217 10 120 10) ",
            "(99 97 102 195 169 10 120 10) ",
            "(99 97 102 4194243 4194217 13 10 120 13 10) ",
            "(99 97 102 233 10 120 10) ",
            "(99 97 102 195 169 10 120 10) ",
            "(99 97 102 195 169 13 10 120 13 10) ",
            "(99 97 102 195 169 10 120 10))",
        )
    );
}

/// GNU `Fmake_serial_process`'s coding chain (src/process.c:3247-3275) is the
/// shortest of the five: the `:coding` keyword, then
/// `coding-system-for-read`/`-write`, and then NOTHING.  `val` is left at the
/// `Qnil` it was initialised to, so `(nil . nil)` is a serial process's normal
/// answer rather than an omission -- `setup_coding_system` reads nil as
/// `undecided`, which detects (src/coding.c:5675-5676).
///
/// It reaches neither `process-coding-system-alist` (there is not even a
/// `coding_systems` variable in the function) nor
/// `default-process-coding-system` (there is no arm that reads it), and its
/// unibyte-buffer short circuit cannot change an answer that is already nil.
///
/// `/dev/ptmx` is the port because it is a real character device that
/// `serial_open` + `tcgetattr` accept on any Linux, which is what GNU needs to
/// get as far as the coding chain: `/dev/null` fails `tcgetattr` and a
/// nonexistent path signals `file-missing`.  Every expected value below was
/// measured on that port under GNU Emacs 31.0.90.
#[cfg(unix)]
#[test]
fn make_serial_process_coding_is_the_overrides_and_nothing_else() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar pw137-n 0)
             (defun pw137-serial-cs (&rest args)
               (setq pw137-n (1+ pw137-n))
               (let ((p (apply #'make-serial-process :port "/dev/ptmx" :speed 9600
                               :name (format "pw137s-%d" pw137-n) :noquery t
                               :sentinel #'ignore args)))
                 (prog1 (process-coding-system p) (delete-process p))))
             (list
              ;; Nothing bound: nil, which means DETECT.
              (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
              (let ((coding-system-for-read 'binary))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
              (let ((coding-system-for-write 'latin-1))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
              (let ((coding-system-for-read 'binary))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")
                                 :coding '(nil . latin-1)))
              (pw137-serial-cs :buffer (generate-new-buffer " *mb*") :coding 'utf-8-dos)
              ;; A unibyte process buffer cannot change an answer already nil.
              (let ((b (generate-new-buffer " *ub*")))
                (with-current-buffer b (set-buffer-multibyte nil))
                (pw137-serial-cs :buffer b))
              ;; Neither tail exists.
              (let ((default-process-coding-system '(latin-1 . koi8-r)))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
              (let ((process-coding-system-alist '(("pw137s" binary . binary))))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
              ;; The check is on what the chain produced.
              (condition-case e
                  (pw137-serial-cs :buffer (generate-new-buffer " *mb*") :coding 'no-such-xyz)
                (error (list (car e) (cadr e))))
              (condition-case e
                  (let ((coding-system-for-read 'no-such-xyz))
                    (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))
                (error (list (car e) (cadr e))))
              ;; ... and an undefined `default-process-coding-system' is not an
              ;; error here, because a serial process never looks at it.
              (let ((default-process-coding-system '(no-such-xyz . no-such-xyz)))
                (pw137-serial-cs :buffer (generate-new-buffer " *mb*")))))"#,
    );

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(nil) ",
            "(binary) ",
            "(nil . latin-1) ",
            "(nil . latin-1) ",
            "(utf-8-dos . utf-8-dos) ",
            "(nil) ",
            "(nil) ",
            "(nil) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "(nil))",
        )
    );
}

/// `set_network_socket_coding_system` (src/process.c:3291-3367) is the one of
/// the three connection resolvers that was already right, and this pins the
/// columns that distinguish it so it stays that way: it DOES reach
/// `find-operation-coding-system`, with `open-network-stream`, whose
/// `target-idx` is 3 (src/coding.c:11788) -- so `network-coding-system-alist`
/// is matched against the SERVICE, not the process name, and
/// `process-coding-system-alist` is the wrong alist entirely.
///
/// The fifth row is the asymmetry: a unibyte process buffer short-circuits the
/// DECODE half past the alist while the ENCODE half, which asks
/// `current_buffer` (:3347-3348), still reaches it.
///
/// What was NOT right is the check.  GNU validates the value the chain
/// produced, so a bad `coding-system-for-read`, a bad
/// `default-process-coding-system` and a bad alist entry all signal
/// `coding-system-error`; Neomacs checked only the `:coding` keyword and
/// installed the undefined name for the other three.
///
/// Every expected value below was measured under GNU Emacs 31.0.90 against a
/// real loopback listener, which is also how this test gets a port.
#[cfg(unix)]
#[test]
fn make_network_process_coding_reaches_the_service_alist_per_half() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defvar pw137-n 0)
             (defvar pw137-srv
               (make-network-process :name "pw137-srv" :server t :host "127.0.0.1"
                                     :service t :family 'ipv4 :noquery t :buffer nil
                                     :filter (lambda (_p _s) nil)))
             (defvar pw137-port (plist-get (process-contact pw137-srv t) :service))
             (defun pw137-net-cs (&rest args)
               (setq pw137-n (1+ pw137-n))
               (let ((p (apply #'make-network-process :name (format "pw137n-%d" pw137-n)
                               :host "127.0.0.1" :service pw137-port :family 'ipv4
                               :noquery t :sentinel #'ignore args)))
                 (prog1 (process-coding-system p)
                   (delete-process p)
                   (dotimes (_ 4) (accept-process-output nil 0.02))
                   (dolist (q (process-list))
                     (when (string-prefix-p "pw137-srv <" (process-name q))
                       (delete-process q))))))
             (unwind-protect
                 (list
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                    (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                        (b (generate-new-buffer " *ub*")))
                    (with-current-buffer b (set-buffer-multibyte nil))
                    (pw137-net-cs :buffer b))
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                        (cur (generate-new-buffer " *cur*"))
                        (pb (generate-new-buffer " *mb*")))
                    (with-current-buffer cur
                      (set-buffer-multibyte nil)
                      (pw137-net-cs :buffer pb)))
                  ;; The alist is keyed on the SERVICE, and a fixnum service
                  ;; matches by `BASE_EQ' (src/coding.c:10851).
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                        (network-coding-system-alist
                         (list (cons pw137-port (cons 'binary 'koi8-r)))))
                    (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                  ;; The unibyte short circuit takes the DECODE half past the
                  ;; alist; the ENCODE half asks `current_buffer' and still
                  ;; reaches it.
                  (let* ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                         (network-coding-system-alist
                          (list (cons pw137-port (cons 'binary 'koi8-r))))
                         (b (generate-new-buffer " *ub*")))
                    (with-current-buffer b (set-buffer-multibyte nil))
                    (pw137-net-cs :buffer b))
                  ;; Keyed on the NAME it does not fire, and
                  ;; `process-coding-system-alist' is the wrong alist.
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                        (network-coding-system-alist '(("pw137n" binary . koi8-r))))
                    (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                  (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                        (process-coding-system-alist
                         (list (cons pw137-port (cons 'binary 'koi8-r)))))
                    (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                  ;; The check is on the value the chain produced.
                  (condition-case e
                      (let ((coding-system-for-read 'no-such-xyz))
                        (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                    (error (list (car e) (cadr e))))
                  (condition-case e
                      (let ((default-process-coding-system '(no-such-xyz . no-such-xyz)))
                        (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                    (error (list (car e) (cadr e))))
                  (condition-case e
                      (let ((network-coding-system-alist
                             (list (cons pw137-port (cons 'no-such-xyz 'no-such-xyz)))))
                        (pw137-net-cs :buffer (generate-new-buffer " *mb*")))
                    (error (list (car e) (cadr e))))
                  ;; ... and it happens where GNU's happens: after the socket
                  ;; exists.  `connect_network_socket' calls
                  ;; `setup_process_coding_systems' only once the connect has
                  ;; succeeded (src/process.c:3761), so a refused port beats an
                  ;; undefined coding system to the signal.
                  (let ((dead (let ((s (make-network-process
                                        :name "pw137-dead" :server t
                                        :host "127.0.0.1" :service t
                                        :family 'ipv4 :noquery t)))
                                (prog1 (plist-get (process-contact s t) :service)
                                  (delete-process s)))))
                    (condition-case e
                        (let ((coding-system-for-read 'no-such-xyz))
                          (make-network-process :name "pw137-refused"
                                                :host "127.0.0.1" :service dead
                                                :family 'ipv4 :noquery t
                                                :sentinel #'ignore))
                      (error (car e))))
                  ;; A listening process runs the same chain.
                  (let* ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                         (coding-system-for-read 'binary)
                         (s (make-network-process :name "pw137-srvB" :server t
                                                  :host "127.0.0.1" :service t
                                                  :family 'ipv4 :noquery t
                                                  :buffer (generate-new-buffer " *mb*"))))
                    (prog1 (process-coding-system s) (delete-process s)))
                  ;; An accepted connection does NOT re-run it: it copies the
                  ;; server's pair, "as the coding system of the new process
                  ;; should reflect the settings at the time the server socket
                  ;; was opened" (src/process.c:5152-5158).
                  (let* ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                         (srv (let ((coding-system-for-read 'koi8-r))
                                (make-network-process :name "pw137-srvC" :server t
                                                      :host "127.0.0.1" :service t
                                                      :family 'ipv4 :noquery t :buffer nil
                                                      :filter (lambda (_p _s) nil))))
                         (port (plist-get (process-contact srv t) :service))
                         (accepted nil))
                    (make-network-process :name "pw137-probe" :host "127.0.0.1"
                                          :service port :family 'ipv4 :noquery t
                                          :buffer nil :coding 'binary :sentinel #'ignore)
                    (dotimes (_ 20) (accept-process-output nil 0.05))
                    (dolist (q (process-list))
                      (when (string-prefix-p "pw137-srvC <" (process-name q))
                        (setq accepted (process-coding-system q))))
                    (prog1 accepted (delete-process srv))))
               (delete-process pw137-srv)))"#,
    );

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(utf-8-unix . utf-8-unix) ",
            "(nil . utf-8-unix) ",
            "(utf-8-unix) ",
            "(binary . koi8-r) ",
            "(nil . koi8-r) ",
            "(utf-8-unix . utf-8-unix) ",
            "(utf-8-unix . utf-8-unix) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "(coding-system-error no-such-xyz) ",
            "file-error ",
            "(binary . utf-8-unix) ",
            "(koi8-r . utf-8-unix))",
        )
    );
}

/// DIVERGENCES.md entry 139: GNU writes the coding system a run of process
/// output was ACTUALLY decoded with back onto the process and into
/// `last-coding-system-used` (`read_process_output_set_last_coding_system`,
/// src/process.c:6417-6446).
///
/// Every expected value below was measured by running `tmp/pw46/final.el` under
/// GNU Emacs 31.0.90.  All of them use `:connection-type 'pipe`: on a PTY, GNU's
/// EOF read returns -1 and `read_process_output` bails at src/process.c:6316
/// before the last block is decoded, which silently drops the decoder's
/// carryover -- a separate divergence that would otherwise be measured here
/// instead of the coding chain.
#[test]
fn process_output_write_back_reports_the_coding_actually_used() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw139-run (script setup)
               (let ((buf (generate-new-buffer " *pw139*")))
                 (unwind-protect
                     (let ((p (funcall setup buf script)))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (defun pw139-spawn (buf script &rest keys)
               (apply #'make-process :name "pw139" :buffer buf :sentinel #'ignore
                      :connection-type 'pipe
                      :command (list "{sh}" "-c" script) keys))
             (let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
               (list
                ;; An undecided end of line DETECTS, and the answer is written
                ;; back: the slot and the variable both become the subsidiary.
                (pw139-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'utf-8))
                                           (pw139-spawn b s))))
                (pw139-run "printf 'a\\rb\\r'"
                           (lambda (b s) (let ((coding-system-for-read 'utf-8))
                                           (pw139-spawn b s))))
                ;; No terminator at all is GNU's EOL_SEEN_NONE: `decode_eol'
                ;; skips `adjust_coding_eol_type' (src/coding.c:6805), so the
                ;; name does NOT grow a suffix.
                (pw139-run "printf 'abc'"
                           (lambda (b s) (let ((coding-system-for-read 'utf-8))
                                           (pw139-spawn b s))))
                ;; nil is `undecided', and with a nil ENCODE half the write-back
                ;; completes it too (`coding_inherit_eol_type', :6442-6444).
                (pw139-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((default-process-coding-system nil))
                                           (pw139-spawn b s))))
                (pw139-run "printf 'abc'"
                           (lambda (b s) (let ((default-process-coding-system nil))
                                           (pw139-spawn b s))))
                ;; A concrete eol type is not adjusted, so nothing moves.
                (pw139-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'utf-8-unix))
                                           (pw139-spawn b s))))
                ;; An ALIAS reports its base's subsidiary, because the eol
                ;; vector in the shared spec holds canonical names.
                (pw139-run "printf 'a\\rb\\r'"
                           (lambda (b s) (let ((coding-system-for-read 'latin-1))
                                           (pw139-spawn b s))))
                ;; `binary' converts nothing and adjusts nothing, but still
                ;; reports (GNU sets the variable for every decoded run).
                (pw139-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'binary))
                                           (pw139-spawn b s))))
                ;; `raw-text' drops the character half and DETECTS the eol half.
                (pw139-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'raw-text))
                                           (pw139-spawn b s)))))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10 98 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 10 98 10) (utf-8-mac . utf-8-unix) utf-8-mac) ",
            "((97 98 99) (utf-8 . utf-8-unix) utf-8) ",
            "((97 10 98 10) (undecided-dos . undecided-dos) undecided-dos) ",
            "((97 98 99) (undecided . undecided-unix) undecided) ",
            "((97 13 10 98 13 10) (utf-8-unix . utf-8-unix) utf-8-unix) ",
            "((97 10 98 10) (iso-latin-1-mac . utf-8-unix) iso-latin-1-mac) ",
            "((97 13 10 98 13 10) (binary . utf-8-unix) binary) ",
            "((97 10 98 10) (raw-text-dos . utf-8-unix) raw-text-dos))",
        )
    );
}

/// DIVERGENCES.md entry 139: a subprocess is decoded through ONE
/// `struct coding_system` in GNU, so an end-of-line type detected by one chunk
/// is the type the NEXT chunk decodes with.
///
/// The diagnostic shape is not the obvious one.  A first chunk of CR LF
/// followed by a chunk of bare LF reads the same either way, because DOS
/// decoding leaves a lone LF alone; what separates sticky from per-chunk is a
/// later chunk of bare CRs, which a dos process keeps and a freshly-detecting
/// one would call `mac' and convert.  Measured under GNU 31.0.90.
#[test]
fn process_eol_detection_is_sticky_for_the_process_not_the_chunk() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw139s-run (script &optional unibyte)
               (let ((buf (generate-new-buffer " *pw139s*")))
                 (unwind-protect
                     (progn
                       (when unibyte (with-current-buffer buf (set-buffer-multibyte nil)))
                       (let ((p (make-process :name "pw139s" :buffer buf :sentinel #'ignore
                                              :connection-type 'pipe
                                              :command (list "{sh}" "-c" script))))
                         (while (accept-process-output p 1))
                         (while (process-live-p p) (accept-process-output p 0.05))
                         (list (append (with-current-buffer buf (buffer-string)) nil)
                               (car (process-coding-system p)))))
                   (kill-buffer buf))))
             (let ((default-process-coding-system '(utf-8-unix . utf-8-unix))
                   (coding-system-for-read 'utf-8))
               (list
                ;; dos first, then bare CRs: the CRs SURVIVE.
                (pw139s-run "printf 'a\\r\\nb\\r\\n'; sleep 0.6; printf 'x\\ry\\r'")
                ;; mac first, then bare LFs: nothing left to convert.
                (pw139s-run "printf 'a\\rb\\r'; sleep 0.6; printf 'x\\ny\\n'")
                ;; dos first, then bare LFs -- the shape that is NOT diagnostic.
                (pw139s-run "printf 'a\\r\\nb\\r\\n'; sleep 0.6; printf 'x\\ny\\n'")
                ;; A first chunk with no terminator decides nothing, so the
                ;; second chunk still detects (GNU's EOL_SEEN_NONE).
                (pw139s-run "printf 'abc'; sleep 0.6; printf 'x\\ry\\r'")
                ;; A CR LF split ACROSS the two reads.  GNU holds a trailing CR
                ;; back only once the eol type is concretely dos, so here the
                ;; first chunk detects `mac' on its own and the second chunk
                ;; inherits it -- CR LF becomes two newlines.
                (pw139s-run "printf 'a\\r'; sleep 0.6; printf '\\nb\\r\\n'")
                ;; The same split under a CONCRETE dos coding, where GNU's
                ;; `eol_dos' does hold the CR back (src/coding.c:1348).
                (let ((coding-system-for-read 'utf-8-dos))
                  (pw139s-run "printf 'a\\r'; sleep 0.6; printf '\\nb\\r\\n'"))
                ;; A unibyte process buffer: the raw-text downgrade detects and
                ;; sticks in exactly the same way.
                (pw139s-run "printf 'caf\\303\\251\\r\\n'; sleep 0.6; printf 'x\\ry\\r'" t))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10 98 10 120 13 121 13) utf-8-dos) ",
            "((97 10 98 10 120 10 121 10) utf-8-mac) ",
            "((97 10 98 10 120 10 121 10) utf-8-dos) ",
            "((97 98 99 120 10 121 10) utf-8-mac) ",
            "((97 10 10 98 10 10) utf-8-mac) ",
            "((97 10 98 10) utf-8-dos) ",
            "((99 97 102 195 169 10 120 13 121 13) raw-text-dos))",
        )
    );
}

/// DIVERGENCES.md entry 139: `call-process` reports too
/// (`Vlast_coding_system_used = CODING_ID_NAME (process_coding.id)`,
/// src/callproc.c:913) -- and only when it read the child's output into a
/// buffer, because that assignment sits inside the branch guarded by an open
/// `fd0`.  Measured under GNU Emacs 31.0.90.
#[test]
fn call_process_reports_the_coding_its_output_was_decoded_with() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw139c-run (coding script &optional unibyte)
               (with-temp-buffer
                 (when unibyte (set-buffer-multibyte nil))
                 (let ((coding-system-for-read coding))
                   (call-process "{sh}" nil t nil "-c" script))
                 (list (append (buffer-string) nil) last-coding-system-used)))
             (list
              (pw139c-run 'utf-8 "printf 'a\\r\\nb\\r\\n'")
              (pw139c-run 'utf-8 "printf 'abc'")
              (pw139c-run 'latin-1 "printf 'a\\rb\\r'")
              (pw139c-run 'binary "printf 'a\\r\\nb\\r\\n'")
              (pw139c-run 'utf-8 "printf 'caf\\303\\251\\r\\n'" t)
              ;; DESTINATION nil: GNU never opens fd0, so the variable keeps
              ;; whatever it held.
              (progn (setq last-coding-system-used 'pw139-untouched)
                     (call-process "{sh}" nil nil nil "-c" "printf 'a\\r\\n'")
                     last-coding-system-used)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10 98 10) utf-8-dos) ",
            "((97 98 99) utf-8) ",
            "((97 10 98 10) iso-latin-1-mac) ",
            "((97 13 10 98 13 10) binary) ",
            "((99 97 102 195 169 10) raw-text-dos) ",
            "pw139-untouched)",
        )
    );
}

/// DIVERGENCES.md entry 151: `read_process_output_set_last_coding_system`
/// reports `CODING_ID_NAME (coding->id)` (src/process.c:6417-6425), and by the
/// time it reads that id BOTH of GNU's rewrites have had their turn at it --
/// `setup_coding_system (found, coding)` inside `detect_coding`
/// (src/coding.c:6751) for the character code, and `adjust_coding_eol_type`
/// (:6805) for the end of line.  Entry 139 moved the second and left the first.
///
/// The last three rows are the negative controls, and they are what makes the
/// other eight mean something.  `detect_coding`'s whole body is guarded by
/// `null_byte_found || eight_bit_found || coding->head_ascii < coding->src_bytes
/// || detect_info.found` (:6596-6599), so a pure-ASCII child settles the end of
/// line and NOT the character code; a child with no terminator at all settles
/// neither; and a unibyte process buffer is downgraded to `raw-text` first,
/// which does not detect at all, because the undecided half of `raw-text` is
/// the end of line rather than the character code.
///
/// Every row is `:connection-type 'pipe` for entry 139's reason: on a pty GNU's
/// EOF read returns -1 and drops the decoder's carryover (src/process.c:6316).
/// Measured under GNU Emacs 31.0.90 running this test's own program
/// (`tmp/pw151/pin.el`, `tmp/pw151/pin-gnu.txt`).
#[test]
fn process_output_write_back_reports_the_character_code_detection_chose() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw151-run (script setup)
               (let ((buf (generate-new-buffer " *pw151*")))
                 (unwind-protect
                     (let ((p (funcall setup buf script)))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (defun pw151-spawn (buf script &rest keys)
               (apply #'make-process :name "pw151" :buffer buf :sentinel #'ignore
                      :connection-type 'pipe
                      :command (list "{sh}" "-c" script) keys))
             (let ((default-process-coding-system '(undecided . utf-8-unix))
                   (cafe "printf 'caf\\303\\251\\r\\nx\\r\\n'"))
               (list
                ;; `undecided' detects on BOTH axes and one name carries both.
                (pw151-run cafe (lambda (b s) (let ((coding-system-for-read 'undecided))
                                                (pw151-spawn b s))))
                ;; A concrete eol on an undecided BASE still detects the
                ;; character code; `detect_coding' re-applies the specified eol
                ;; after the re-base (src/coding.c:6752-6753), so the CRs
                ;; survive here and the name is `utf-8-unix'.
                (pw151-run cafe (lambda (b s) (let ((coding-system-for-read 'undecided-unix))
                                                (pw151-spawn b s))))
                (pw151-run cafe (lambda (b s) (let ((coding-system-for-read 'undecided-dos))
                                                (pw151-spawn b s))))
                ;; `prefer-utf-8' is the same category with UTF-8 raised.
                (pw151-run cafe (lambda (b s) (let ((coding-system-for-read 'prefer-utf-8))
                                                (pw151-spawn b s))))
                ;; A nil chain is `undecided', and the write-back completes the
                ;; still-nil ENCODE half from the DETECTED name (:6442-6444).
                (pw151-run cafe (lambda (b s) (let ((default-process-coding-system nil))
                                                (pw151-spawn b s))))
                ;; An encode half that is not nil does not move.
                (pw151-run cafe (lambda (b s)
                                  (let ((default-process-coding-system '(undecided . latin-1)))
                                    (pw151-spawn b s))))
                ;; The other two detection outcomes: a null byte is
                ;; `no-conversion' (:6688), whose eol type is concrete `unix',
                ;; so its CR LF survives; bytes that are not valid UTF-8 fall to
                ;; the category priority list and land on `iso-latin-1'.
                (pw151-run "printf 'a\\0b\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'undecided))
                                           (pw151-spawn b s))))
                (pw151-run "printf 'caf\\351\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'undecided))
                                           (pw151-spawn b s))))
                ;; The negative controls.
                (pw151-run "printf 'a\\r\\nb\\r\\n'"
                           (lambda (b s) (let ((coding-system-for-read 'undecided))
                                           (pw151-spawn b s))))
                (pw151-run "printf 'abc'"
                           (lambda (b s) (let ((coding-system-for-read 'undecided))
                                           (pw151-spawn b s))))
                (pw151-run cafe (lambda (b s)
                                  (with-current-buffer b (set-buffer-multibyte nil))
                                  (let ((default-process-coding-system nil))
                                    (pw151-spawn b s)))))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 233 13 10 120 13 10) (utf-8-unix . utf-8-unix) utf-8-unix) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-dos) utf-8-dos) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . latin-1) utf-8-dos) ",
            "((97 0 98 13 10) (no-conversion . utf-8-unix) no-conversion) ",
            "((99 97 102 233 10) (iso-latin-1-dos . utf-8-unix) iso-latin-1-dos) ",
            "((97 10 98 10) (undecided-dos . utf-8-unix) undecided-dos) ",
            "((97 98 99) (undecided . utf-8-unix) undecided) ",
            "((99 97 102 195 169 10 120 10) (raw-text-dos . raw-text-dos) raw-text-dos))",
        )
    );
}

/// DIVERGENCES.md entry 151: the character code goes sticky the same way the
/// end of line does, and INDEPENDENTLY of it.
///
/// `undecided-dos` is still type `Qundecided`, so `setup_coding_system` leaves
/// `CODING_REQUIRE_DETECTION` raised for it (src/coding.c:5713): a chunk that
/// has settled the end of line has not settled the character code, and the next
/// chunk still detects.  The first three rows are that; the last one is a
/// process whose whole life is ASCII and which therefore never detects at all.
///
/// Rows four and five are the ones that prove stickiness rather than merely
/// reporting it, and they need no observation of an intermediate read: a second
/// chunk of latin-1 bytes decoded by a process that is `utf-8` by then leaves a
/// raw eight-bit character (4194281), and a second chunk of UTF-8 bytes decoded
/// by a process that is `iso-latin-1` by then leaves TWO characters (195 169).
/// Re-detecting per chunk would answer the opposite of each.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn process_charset_detection_is_sticky_for_the_process_not_the_chunk() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw151s-run (script)
               (let ((buf (generate-new-buffer " *pw151s*")))
                 (unwind-protect
                     (let ((p (make-process :name "pw151s" :buffer buf :sentinel #'ignore
                                            :connection-type 'pipe
                                            :command (list "{sh}" "-c" script))))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (let ((default-process-coding-system '(undecided . utf-8-unix))
                   (coding-system-for-read 'undecided))
               (list
                (pw151s-run "printf 'a\\r\\nb\\r\\n'; sleep 0.7; printf 'caf\\303\\251\\r\\n'")
                (pw151s-run "printf 'a\\nb\\n'; sleep 0.7; printf 'caf\\303\\251\\n'")
                (pw151s-run "printf 'abc'; sleep 0.7; printf 'caf\\303\\251\\r\\n'")
                (pw151s-run "printf 'caf\\303\\251\\r\\n'; sleep 0.7; printf 'caf\\351\\r\\n'")
                (pw151s-run "printf 'caf\\351\\r\\n'; sleep 0.7; printf 'caf\\303\\251\\r\\n'")
                (pw151s-run "printf 'a\\r\\nb\\r\\n'; sleep 0.7; printf 'x\\r\\ny\\r\\n'"))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10 98 10 99 97 102 233 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 10 98 10 99 97 102 233 10) (utf-8-unix . utf-8-unix) utf-8-unix) ",
            "((97 98 99 99 97 102 233 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 233 10 99 97 102 4194281 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 233 10 99 97 102 195 169 10) (iso-latin-1-dos . utf-8-unix) iso-latin-1-dos) ",
            "((97 10 98 10 120 10 121 10) (undecided-dos . utf-8-unix) undecided-dos))",
        )
    );
}

/// DIVERGENCES.md entry 151: detection is told whether more bytes may follow,
/// and it changes the answer.
///
/// Four of GNU's detectors end on
/// `if (src_base < src && coding->mode & CODING_MODE_LAST_BLOCK)` -- UTF-8 at
/// src/coding.c:1215, `emacs-mule` at :1910, Shift-JIS at :4620, Big5 at :4667
/// -- and `read_process_output` raises that flag only at EOF
/// (src/process.c:6321).  So a chunk that stops in the middle of a character is
/// still UTF-8 and the partial character is carryover, where the very same
/// bytes handed to `decode-coding-string` -- a complete source, GNU sets the
/// flag in `code_convert_string` (src/coding.c:9606) -- are NOT UTF-8 and fall
/// to `iso-latin-1`.  The last row is that disagreement, and it is the point:
/// the two doors differ because the flag differs, not because the detectors do.
///
/// Without the flag this fix would have regressed the most ordinary case there
/// is -- a process emitting UTF-8 in chunks the kernel split mid-character --
/// from carryover to two mojibake characters.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn process_detection_treats_a_partial_trailing_character_as_carryover() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw151b-run (script)
               (let ((buf (generate-new-buffer " *pw151b*")))
                 (unwind-protect
                     (let ((p (make-process :name "pw151b" :buffer buf :sentinel #'ignore
                                            :connection-type 'pipe
                                            :command (list "{sh}" "-c" script))))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (let ((default-process-coding-system '(undecided . utf-8-unix))
                   (coding-system-for-read 'undecided))
               (list
                ;; A two-byte character split across the read boundary.
                (pw151b-run "printf 'caf\\303'; sleep 0.7; printf '\\251\\r\\n'")
                ;; A three-byte one.
                (pw151b-run "printf 'a\\344\\270'; sleep 0.7; printf '\\255\\r\\n'")
                ;; Truncated with nothing following: the EOF read IS the last
                ;; block, but detection answered `utf-8' on the first read and
                ;; is sticky, so the orphan byte lands as an eight-bit
                ;; character rather than re-deciding the coding system.
                (pw151b-run "printf 'caf\\303'")
                ;; The string door on the same bytes IS a complete source.
                (list (append (decode-coding-string "caf\303" 'undecided) nil)
                      last-coding-system-used))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((99 97 102 233 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 20013 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((99 97 102 4194243) (utf-8 . utf-8-unix) utf-8) ",
            "((99 97 102 195) iso-latin-1))",
        )
    );
}

/// DIVERGENCES.md entry 151: `Fcall_process` decodes through
/// `decode_coding_c_string` too, so `detect_coding` re-bases its coding system
/// before the decoder runs and `Vlast_coding_system_used = CODING_ID_NAME
/// (process_coding.id)` (src/callproc.c:913) reports the re-based name.
///
/// Entry 139 pinned this door for the end of line; these are the character-code
/// rows, negative controls included.  Measured under GNU Emacs 31.0.90 running
/// this test's own program.
#[test]
fn call_process_reports_the_character_code_detection_chose() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw151c-run (coding script)
               (with-temp-buffer
                 (let ((coding-system-for-read coding))
                   (call-process "{sh}" nil t nil "-c" script))
                 (list (append (buffer-string) nil) last-coding-system-used)))
             (list
              (pw151c-run 'undecided "printf 'caf\\303\\251\\r\\nx\\r\\n'")
              (pw151c-run 'undecided "printf 'a\\r\\nb\\r\\n'")
              (pw151c-run 'undecided "printf 'abc'")
              (pw151c-run 'undecided "printf 'caf\\351\\r\\n'")
              (pw151c-run 'undecided "printf 'a\\0b\\r\\n'")
              (pw151c-run 'undecided-dos "printf 'caf\\303\\251\\r\\nx\\r\\n'")
              (pw151c-run 'prefer-utf-8 "printf 'caf\\303\\251\\r\\nx\\r\\n'")))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((99 97 102 233 10 120 10) utf-8-dos) ",
            "((97 10 98 10) undecided-dos) ",
            "((97 98 99) undecided) ",
            "((99 97 102 233 10) iso-latin-1-dos) ",
            "((97 0 98 13 10) no-conversion) ",
            "((99 97 102 233 10 120 10) utf-8-dos) ",
            "((99 97 102 233 10 120 10) utf-8-dos))",
        )
    );
}

/// DIVERGENCES.md entry 143: `inhibit-eol-conversion` is read at CONVERSION
/// time, not at the time the coding system is resolved.
///
/// This is the measurement that decides the shape of the fix, and a subprocess
/// is the only place it is observable, because it is the only conversion whose
/// two halves can be put in different dynamic extents: the coding system is
/// resolved once at `make-process`, and the bytes arrive later.
///
/// GNU re-reads its C global at every decision -- `decode_eol`
/// (src/coding.c:6767), `decode_coding` (:7481), the eight decoders' `eol_dos`
/// (:1250-1251 and seven copies) -- and `setup_coding_system`'s read (:5681)
/// only tunes `common_flags`.  So a process created inside the binding and read
/// outside it CONVERTS, and one created outside and read inside does NOT.  A
/// fix that stored the flag on the process at creation would have the first two
/// rows exactly backwards.
///
/// The write-back moves with it: with the flag set, `adjust_coding_eol_type`
/// never fires, so `(process-coding-system P)` keeps `utf-8` where it would
/// otherwise become `utf-8-dos` (entry 139).
///
/// Every row uses `:connection-type 'pipe` for entry 139's reason: on a pty,
/// GNU's EOF read returns -1 and drops the decoder's carryover
/// (src/process.c:6316).  Measured under GNU Emacs 31.0.90
/// (`tmp/pw49/gnu.txt`).
#[test]
fn inhibit_eol_conversion_is_read_when_process_output_arrives_not_when_it_is_resolved() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw143p (coding bind-create bind-read)
               (let ((buf (generate-new-buffer " *pw143*")))
                 (unwind-protect
                     (let ((p (let ((coding-system-for-read coding)
                                    (inhibit-eol-conversion bind-create))
                                (make-process :name "pw143" :buffer buf
                                              :sentinel #'ignore
                                              :connection-type 'pipe
                                              :command (list "{sh}" "-c"
                                                             "printf 'a\\r\\nb\\r\\n'")))))
                       (let ((inhibit-eol-conversion bind-read))
                         (while (accept-process-output p 1))
                         (while (process-live-p p) (accept-process-output p 0.05)))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (defun pw143s (bind-send)
               (let ((buf (generate-new-buffer " *pw143s*")))
                 (unwind-protect
                     (let ((p (let ((coding-system-for-read 'binary)
                                    (coding-system-for-write 'utf-8-dos))
                                (make-process :name "pw143s" :buffer buf
                                              :sentinel #'ignore
                                              :connection-type 'pipe
                                              :command (list "{sh}" "-c" "cat")))))
                       (let ((inhibit-eol-conversion bind-send))
                         (process-send-string p "a\nb\n"))
                       (process-send-eof p)
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (append (with-current-buffer buf (buffer-string)) nil))
                   (kill-buffer buf))))
             (list
              ;; A CONCRETE `-dos' coding: bound at creation only -> converts;
              ;; bound at read only -> does not.
              (pw143p 'utf-8-dos t nil)
              (pw143p 'utf-8-dos nil t)
              (pw143p 'utf-8-dos t t)
              (pw143p 'utf-8-dos nil nil)
              ;; An UNDECIDED eol: the flag also suppresses entry 139's
              ;; write-back, because there is no adjustment to write back.
              (pw143p 'utf-8 t t)
              (pw143p 'utf-8 nil nil)
              ;; The ENCODE half, at send time (`send_process').
              (pw143s t)
              (pw143s nil)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10 98 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 13 10 98 13 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 13 10 98 13 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 10 98 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "((97 13 10 98 13 10) (utf-8 . utf-8-unix) utf-8) ",
            "((97 10 98 10) (utf-8-dos . utf-8-unix) utf-8-dos) ",
            "(97 10 98 10) ",
            "(97 13 10 98 13 10))",
        )
    );
}

// ---------------------------------------------------------------------------
// DIVERGENCES.md entry 147: `make-serial-process` opens its port.
// ---------------------------------------------------------------------------

/// One pty pair, held open from the Rust side, so a serial process can be
/// created on the SLAVE and fed real bytes through the MASTER.
///
/// This is the only way to give `make-serial-process` a device that carries
/// traffic without real hardware, and it is the fixture DIVERGENCES.md entry
/// 137 built and then deliberately refused to pin against, because our
/// `make-serial-process` never opened the port and every row measured an empty
/// buffer.  The master is put into raw mode BEFORE anything is written, so the
/// bytes queued for the slave are not rewritten by the line discipline on the
/// way in (`ICRNL` would eat the CRs this fixture exists to carry) -- and it is
/// therefore deterministic: the payload is already in the pty's input queue
/// before the serial process opens the slave, so no row depends on timing.
///
/// The pair is a tty, which is what a serial port IS; what it is NOT is a pty
/// whose master has CLOSED, so none of these rows measure the EOF carryover
/// quirk DIVERGENCES.md entry 139 found.  The master stays open for the whole
/// test and is closed by `Drop`.
#[cfg(unix)]
struct SerialTestPty {
    master: std::os::fd::OwnedFd,
    slave_path: String,
}

#[cfg(unix)]
impl SerialTestPty {
    fn open() -> Self {
        use std::os::fd::FromRawFd;
        // SAFETY: each call takes only the descriptor it was handed, and
        // `ptsname`'s result is copied out before any other libc call runs.
        unsafe {
            let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(
                master >= 0,
                "posix_openpt: {}",
                std::io::Error::last_os_error()
            );
            assert_eq!(libc::grantpt(master), 0, "grantpt");
            assert_eq!(libc::unlockpt(master), 0, "unlockpt");
            let name = libc::ptsname(master);
            assert!(!name.is_null(), "ptsname");
            let slave_path = std::ffi::CStr::from_ptr(name)
                .to_str()
                .expect("pty slave path is ASCII")
                .to_owned();
            // Raw BEFORE the first write: the line discipline processes input
            // as it is written to the master, so `ICRNL` would turn this
            // fixture's CR LF into LF LF before any coding system sees it.
            let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(
                libc::tcgetattr(master, attributes.as_mut_ptr()),
                0,
                "tcgetattr"
            );
            let mut attributes = attributes.assume_init();
            libc::cfmakeraw(&raw mut attributes);
            assert_eq!(
                libc::tcsetattr(master, libc::TCSANOW, &raw const attributes),
                0,
                "tcsetattr"
            );
            Self {
                master: std::os::fd::OwnedFd::from_raw_fd(master),
                slave_path,
            }
        }
    }

    fn write(&self, bytes: &[u8]) {
        use std::os::fd::AsRawFd;
        // SAFETY: writes `bytes.len()` bytes from a live slice to an owned fd.
        let written = unsafe {
            libc::write(
                self.master.as_raw_fd(),
                bytes.as_ptr().cast::<libc::c_void>(),
                bytes.len(),
            )
        };
        assert_eq!(written, bytes.len() as isize, "short write to pty master");
    }

    /// Everything the slave side has written back, waiting up to two seconds
    /// for the first byte.  Used to prove the WRITE half: `process-send-string`
    /// on a serial process has to reach the device.
    fn read_available(&self) -> Vec<u8> {
        use std::os::fd::AsRawFd;
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let mut buf = [0u8; 256];
            // SAFETY: reads at most `buf.len()` bytes into a live buffer.
            let count = unsafe {
                libc::read(
                    self.master.as_raw_fd(),
                    buf.as_mut_ptr().cast::<libc::c_void>(),
                    buf.len(),
                )
            };
            if count > 0 {
                #[allow(clippy::cast_sign_loss)]
                return buf[..count as usize].to_vec();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Vec::new()
    }
}

/// GNU `Fmake_serial_process` opens the port at src/process.c:3212 -- BEFORE
/// the process buffer (:3220), before the coding chain (:3246-3277) and before
/// `Fserial_process_configure` (:3284), and under a
/// `record_unwind_protect (remove_process, proc)` (:3207) that removes the
/// record again if any of those fails.
///
/// So the errors are ordered, and every row below says which one wins.  All of
/// them were measured against GNU Emacs 31.0.90; before this fix neomacs
/// returned a live process for the first four, because it never opened
/// anything.
#[cfg(unix)]
#[test]
fn make_serial_process_open_failures_beat_everything_downstream() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defun pw147-try (thunk) (condition-case err (funcall thunk) (error err)))
             (list
              ;; `serial_open' -> `report_file_error ("Opening serial port", port)'
              ;; (src/sysdep.c:2982-2984): the errno classification is the same
              ;; one every other failed open in Emacs gets.
              (pw147-try (lambda () (make-serial-process :port "/nonexistent/pw147-tty"
                                                   :speed 9600 :name "a" :noquery t)))
              (pw147-try (lambda () (make-serial-process :port "/dev" :speed 9600
                                                   :name "b" :noquery t)))
              (pw147-try (lambda () (make-serial-process :port "/proc/1/mem" :speed 9600
                                                   :name "c" :noquery t)))
              ;; `:speed nil' means "do not configure", so an open that succeeds
              ;; on a device that is not a tty succeeds -- and the same port with
              ;; a speed reports the `tcgetattr' the configuration needs.
              (pw147-try (lambda () (make-serial-process :port "/dev/null" :speed 9600
                                                   :name "d" :noquery t)))
              (let ((p (make-serial-process :port "/dev/null" :speed nil
                                            :name "e" :noquery t)))
                (prog1 (list (process-status p) (process-contact p t))
                  (delete-process p)))
              ;; The open beats the coding chain ...
              (pw147-try (lambda ()
                     (let ((coding-system-for-read 'pw147-no-such))
                       (make-serial-process :port "/nonexistent/pw147-tty" :speed 9600
                                            :name "f" :noquery t))))
              ;; ... and it beats the configuration.
              (pw147-try (lambda () (make-serial-process :port "/nonexistent/pw147-tty"
                                                   :speed 9600 :bytesize 5
                                                   :name "g" :noquery t)))
              ;; The coding chain beats the configuration, on a port that opens
              ;; but is not a tty and on one that is.
              (pw147-try (lambda ()
                     (let ((coding-system-for-read 'pw147-no-such))
                       (make-serial-process :port "/dev/null" :speed 9600 :name "h"
                                            :noquery t
                                            :buffer (generate-new-buffer " *h*")))))
              (pw147-try (lambda ()
                     (let ((coding-system-for-read 'pw147-no-such))
                       (make-serial-process :port "/dev/ptmx" :speed 9600 :bytesize 5
                                            :name "i" :noquery t
                                            :buffer (generate-new-buffer " *i*")))))
              ;; `tcgetattr' beats every keyword domain check, because GNU reads
              ;; the attributes before it validates any of them
              ;; (src/sysdep.c:3162 vs :3193).
              (pw147-try (lambda () (make-serial-process :port "/dev/null" :speed 9600
                                                   :bytesize 5 :name "j" :noquery t)))
              (pw147-try (lambda () (make-serial-process :port "/dev/null" :speed 9600
                                                   :parity 'mark :name "k" :noquery t)))
              ;; ... but the PORT checks beat the `:speed' check, which is why a
              ;; bad speed cannot be reported for a call with no port at all
              ;; (src/process.c:3193-3200).
              (pw147-try (lambda () (make-serial-process :speed "x" :name "l" :noquery t)))
              (pw147-try (lambda () (make-serial-process :port 1 :speed "x"
                                                  :name "m" :noquery t)))
              (pw147-try (lambda () (make-serial-process :port "/nonexistent/pw147-tty"
                                                   :speed "x" :name "n" :noquery t)))))"#,
    );

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(file-missing \"Opening serial port\" \"No such file or directory\" \"/nonexistent/pw147-tty\") ",
            "(file-error \"Opening serial port\" \"Is a directory\" \"/dev\") ",
            "(permission-denied \"Opening serial port\" \"Permission denied\" \"/proc/1/mem\") ",
            "(file-error \"Failed tcgetattr\" \"Inappropriate ioctl for device\") ",
            "(open (:port \"/dev/null\" :speed nil :name \"e\" :noquery t)) ",
            "(file-missing \"Opening serial port\" \"No such file or directory\" \"/nonexistent/pw147-tty\") ",
            "(file-missing \"Opening serial port\" \"No such file or directory\" \"/nonexistent/pw147-tty\") ",
            "(coding-system-error pw147-no-such) ",
            "(coding-system-error pw147-no-such) ",
            "(file-error \"Failed tcgetattr\" \"Inappropriate ioctl for device\") ",
            "(file-error \"Failed tcgetattr\" \"Inappropriate ioctl for device\") ",
            "(error \"No port specified\") ",
            "(wrong-type-argument stringp 1) ",
            "(wrong-type-argument fixnump \"x\"))",
        )
    );
}

/// What a FAILED `make-serial-process` leaves behind, which is a statement
/// about WHERE the failure happened.
///
/// GNU unwinds the process record in every case (`record_unwind_protect
/// (remove_process, proc)`, src/process.c:3207), so no failure ever leaks a
/// process.  The BUFFER is different: `Fget_buffer_create` runs at :3220, after
/// the open and before the coding chain, so an open failure leaves no buffer
/// and every later failure leaves one.  Measured under GNU 31.0.90.
#[cfg(unix)]
#[test]
fn a_failed_make_serial_process_leaks_nothing_but_its_buffer() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defun pw147-aftermath (tag thunk)
               (let ((before (length (process-list))))
                 (list (car (condition-case err (funcall thunk) (error err)))
                       (and (get-buffer tag) t)
                       (- (length (process-list)) before)
                       (and (get-process tag) t))))
             (list
              (pw147-aftermath "pw147a"
                         (lambda () (make-serial-process :port "/nonexistent/pw147-tty"
                                                         :speed 9600 :name "pw147a"
                                                         :noquery t)))
              (pw147-aftermath "pw147b"
                         (lambda () (make-serial-process :port "/dev/ptmx" :speed 9600
                                                         :bytesize 5 :name "pw147b"
                                                         :noquery t)))
              (pw147-aftermath "pw147c"
                         (lambda ()
                           (let ((coding-system-for-read 'pw147-no-such))
                             (make-serial-process :port "/dev/ptmx" :speed 9600
                                                  :name "pw147c" :noquery t))))
              (pw147-aftermath "pw147d"
                         (lambda () (make-serial-process :port "/dev/null" :speed 9600
                                                         :name "pw147d" :noquery t)))
              ;; An explicit `:buffer' is created at the same moment, so an open
              ;; failure does not create it either.
              (progn (ignore-errors
                       (make-serial-process :port "/nonexistent/pw147-tty" :speed 9600
                                            :name "pw147e" :buffer "pw147-buf" :noquery t))
                     (and (get-buffer "pw147-buf") t))))"#,
    );

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(file-missing nil 0 nil) ",
            "(error t 0 nil) ",
            "(coding-system-error t 0 nil) ",
            "(file-error t 0 nil) ",
            "nil)",
        )
    );
}

/// The payload DIVERGENCES.md entry 137 measured under GNU and refused to pin,
/// because `make-serial-process` here never opened the port and every row came
/// back with an empty buffer.  It opens now, so these are real.
///
/// Each row gets its OWN pty pair with the bytes `c a f <c3> <a9> CR LF x CR LF`
/// already queued, and reads the process-coding-system slot AFTER the output,
/// where GNU's `read_process_output_set_last_coding_system`
/// (src/process.c:6417-6425) has replaced it with the coding actually used --
/// the write-back DIVERGENCES.md entry 139 implemented.  Entry 137's own pins
/// read the slot BEFORE any output for the opposite reason; between them the
/// chain and the write-back are measured without overlapping.
///
/// The last two rows are the serial chain's missing tail on real bytes rather
/// than on a reporting slot: `default-process-coding-system` and
/// `process-coding-system-alist` are bound to something that would be plainly
/// visible, and are invisible.
///
/// Re-measured under GNU 31.0.90 on real pty pairs by `tmp/pw151/serial_probe.py`
/// (`tmp/pw151/serial-gnu.txt`) when entry 151 promoted the three nil-chain rows
/// from a bytes-only pin to a full one.
#[cfg(unix)]
#[test]
fn a_serial_process_decodes_the_bytes_its_port_delivers() {
    crate::test_utils::init_test_tracing();
    const PAYLOAD: &[u8] = b"caf\xc3\xa9\r\nx\r\n";
    let ptys: Vec<SerialTestPty> = (0..7).map(|_| SerialTestPty::open()).collect();
    for pty in &ptys {
        pty.write(PAYLOAD);
    }
    let port = |index: usize| ptys[index].slave_path.as_str();

    let result = eval_one(&format!(
        r#"(progn
             (defvar pw147-n 0)
             (defun pw147-serial-bytes (port want &rest args)
               (let* ((b (generate-new-buffer " *pw147*"))
                      (p (apply #'make-serial-process :port port :speed 9600
                                :name (format "pw147p-%d" (setq pw147-n (1+ pw147-n)))
                                :noquery t :buffer b args))
                      (rounds 60))
                 (while (and (< (buffer-size b) want) (> rounds 0))
                   (accept-process-output p 0.05)
                   (setq rounds (1- rounds)))
                 (prog1 (list (with-current-buffer b (append (buffer-string) nil))
                              (process-coding-system p))
                   (delete-process p))))
             (list
              ;; The three rows whose chain answers nil pin their coding slot
              ;; as well as their bytes.  Entry 147 pinned only the bytes here,
              ;; because an `undecided' decode reported its own name; entry 151
              ;; carried the `CodingSystemManager' into the decoder, so
              ;; `detect_coding''s re-base (src/coding.c:6751) reaches the slot
              ;; and all three answer `utf-8-dos' as GNU does.
              (pw147-serial-bytes "{p0}" 7)
              (let ((coding-system-for-read 'binary))
                (pw147-serial-bytes "{p1}" 10))
              (let ((coding-system-for-read 'raw-text))
                (pw147-serial-bytes "{p2}" 8))
              (let ((coding-system-for-read 'latin-1))
                (pw147-serial-bytes "{p3}" 8))
              (pw147-serial-bytes "{p4}" 8 :coding 'latin-1)
              (let ((default-process-coding-system '(binary . binary)))
                (pw147-serial-bytes "{p5}" 7))
              (let ((process-coding-system-alist '(("pw147p" binary . binary))))
                (pw147-serial-bytes "{p6}" 7))))"#,
        p0 = port(0),
        p1 = port(1),
        p2 = port(2),
        p3 = port(3),
        p4 = port(4),
        p5 = port(5),
        p6 = port(6),
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-dos)) ",
            "((99 97 102 4194243 4194217 13 10 120 13 10) (binary)) ",
            "((99 97 102 4194243 4194217 10 120 10) (raw-text-dos . raw-text-dos)) ",
            "((99 97 102 195 169 10 120 10) (iso-latin-1-dos . iso-latin-1-dos)) ",
            "((99 97 102 195 169 10 120 10) (iso-latin-1-dos . latin-1)) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-dos)) ",
            "((99 97 102 233 10 120 10) (utf-8-dos . utf-8-dos)))",
        )
    );
}

/// The other direction: GNU gives a serial process ONE descriptor for both
/// (`p->infd = p->outfd = fd`, src/process.c:3214-3215), so
/// `process-send-string` has to arrive on the wire.
#[cfg(unix)]
#[test]
fn process_send_string_reaches_a_serial_port() {
    crate::test_utils::init_test_tracing();
    let pty = SerialTestPty::open();

    let result = eval_one(&format!(
        r#"(let ((p (make-serial-process :port "{port}" :speed 9600
                                         :name "pw147w" :noquery t)))
             (process-send-string p "pw147-ping")
             (prog1 (list (process-status p) (process-live-p p))
               (delete-process p)))"#,
        port = pty.slave_path,
    ));
    assert_eq!(result, "OK (open (open listen connect stop))");
    assert_eq!(pty.read_available(), b"pw147-ping");
}

#[cfg(unix)]
impl SerialTestPty {
    /// The line settings currently in force on the pair, read from the master.
    fn attributes(&self) -> libc::termios {
        use std::os::fd::AsRawFd;
        // SAFETY: `tcgetattr` initialises the whole struct on success.
        unsafe {
            let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(
                libc::tcgetattr(self.master.as_raw_fd(), attributes.as_mut_ptr()),
                0,
                "tcgetattr on pty master"
            );
            attributes.assume_init()
        }
    }
}

/// `serial-process-configure` has to configure the DEVICE, not just the
/// contact plist -- the distinction this entry exists for.
///
/// Every assertion below reads the pty's real `termios` back from the master
/// side, so it fails if the settings only ever reached
/// `(process-contact P t)`.  Each row is one of GNU's five `serial_configure`
/// arms (src/sysdep.c:3175-3300); `:speed` goes through GNU's `convert_speed`
/// (:3131-3143, bug#49524), which is why a plain 9600 has to come back as the
/// `B9600` constant rather than as the number.
#[cfg(unix)]
#[test]
fn serial_configuration_reaches_the_device() {
    crate::test_utils::init_test_tracing();
    let pty = SerialTestPty::open();

    let result = eval_one(&format!(
        r#"(let ((p (make-serial-process :port "{port}" :speed 9600 :bytesize 7
                                         :parity 'odd :stopbits 2 :flowcontrol 'hw
                                         :name "pw147cfg" :noquery t)))
             (prog1 (plist-get (process-contact p t) :summary)
               (delete-process p)))"#,
        port = pty.slave_path,
    ));
    assert_eq!(result, "OK \"7O2\"");

    let attributes = pty.attributes();
    // SAFETY: `cfgetospeed`/`cfgetispeed` only read through the pointer.
    let (ospeed, ispeed) = unsafe {
        (
            libc::cfgetospeed(&raw const attributes),
            libc::cfgetispeed(&raw const attributes),
        )
    };
    assert_eq!(ospeed, libc::B9600, "GNU convert_speed (9600) is B9600");
    assert_eq!(ispeed, libc::B9600);
    // `:bytesize` and `:parity` are deliberately NOT asserted, and the reason
    // is the fixture rather than the code: Linux's pty driver ends every
    // termios change with `c_cflag &= ~(CSIZE | PARENB); c_cflag |= CS8 | CREAD`
    // (drivers/tty/pty.c `pty_set_termios`), so a pty cannot hold CS7 or a
    // parity bit no matter who writes it -- measured, the CS7 this call sets
    // reads back as CS8.  Their arms are covered by the `:summary` above and by
    // the domain pins; asserting them here would pin the pty driver.
    assert_eq!(
        attributes.c_cflag & libc::CSTOPB,
        libc::CSTOPB,
        ":stopbits 2"
    );
    assert_eq!(
        attributes.c_cflag & libc::CRTSCTS,
        libc::CRTSCTS,
        ":flowcontrol hw"
    );
    // `cfmakeraw` ran first, and GNU adds CLOCAL|CREAD on top (src/sysdep.c:3166-3172).
    assert_eq!(attributes.c_lflag & libc::ICANON, 0, "cfmakeraw");
    assert_eq!(attributes.c_iflag & libc::ICRNL, 0, "cfmakeraw");
    assert_eq!(attributes.c_oflag & libc::OPOST, 0, "cfmakeraw");
    assert_eq!(
        attributes.c_cflag & (libc::CLOCAL | libc::CREAD),
        libc::CLOCAL | libc::CREAD
    );
}

/// The other half of the same statement: a `:speed nil` port is opened and
/// then LEFT ALONE.
///
/// GNU's `Fserial_process_configure` returns before `serial_configure` when the
/// contact's `:speed` is nil (src/process.c:3098-3099, documented at :3042-3045
/// as "the serial port is not configured any further"), so nothing is written
/// to the device -- the pty keeps the canonical-mode settings it was created
/// with, and `serial-process-configure` on it stays a no-op.
#[cfg(unix)]
#[test]
fn a_speed_nil_serial_port_is_opened_and_not_configured() {
    crate::test_utils::init_test_tracing();
    let pty = SerialTestPty::open();
    // Undo the fixture's raw mode so "left alone" is visible as a difference.
    // SAFETY: `tcsetattr` only reads through the provided pointer.
    unsafe {
        use std::os::fd::AsRawFd;
        let mut attributes = pty.attributes();
        attributes.c_lflag |= libc::ICANON;
        attributes.c_iflag |= libc::ICRNL;
        assert_eq!(
            libc::tcsetattr(pty.master.as_raw_fd(), libc::TCSANOW, &raw const attributes),
            0
        );
    }

    let result = eval_one(&format!(
        r#"(let ((p (make-serial-process :port "{port}" :speed nil
                                         :name "pw147nil" :noquery t)))
             (prog1 (list (process-status p)
                          (process-contact p t)
                          (serial-process-configure :process p :bytesize 7)
                          (process-contact p t))
               (delete-process p)))"#,
        port = pty.slave_path,
    ));
    assert_eq!(
        result,
        concat!(
            "OK (open ",
            "(:port \"",
            "{PORT}",
            "\" :speed nil :name \"pw147nil\" :noquery t) ",
            "nil ",
            "(:port \"",
            "{PORT}",
            "\" :speed nil :name \"pw147nil\" :noquery t))",
        )
        .replace("{PORT}", &pty.slave_path)
    );

    let attributes = pty.attributes();
    assert_ne!(attributes.c_lflag & libc::ICANON, 0, "left alone");
    assert_ne!(attributes.c_iflag & libc::ICRNL, 0, "left alone");
}

/// DIVERGENCES.md entry 156: a subprocess reads through GNU's `detect_coding`
/// too, so a UTF-16 signature survives the null bytes that surround it.
///
/// `read_and_insert_process_output` and the filter branch both reach
/// `decode_coding_object` through `decode_coding_c_string` (src/process.c:6502,
/// :6562), and that is where `CODING_REQUIRE_DETECTION` runs `detect_coding`
/// (src/coding.c:8128-8129).  The null byte NARROWS the category walk to UTF-16
/// (:6614-6618) instead of closing it, so rows one and two report a concrete
/// UTF-16 coding system where entry 151 left the process door reporting
/// `no-conversion` -- the answer the reporting detector gives, which the process
/// door had started sharing.
///
/// Rows three and four are the narrowed walk finding nothing and the null byte's
/// FALLBACK standing (:6683-6684).
///
/// Row five is `detect_coding_utf_16`'s own LAST_BLOCK conjunct
/// (:1505-1511), which `read_process_output` leaves clear until EOF
/// (src/process.c:6321): five bytes are an odd count, and an odd count refutes
/// UTF-16 only for a complete source.  `decode-coding-string` on the very same
/// five bytes answers `no-conversion`; a pipe answers
/// `utf-16le-with-signature-mac`.
///
/// Row six is the signature split across the read boundary.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program
/// (`tmp/pw156/pin2.el`, `tmp/pw156/pin2-gnu.txt`).
#[test]
fn a_utf_16_signature_survives_its_own_null_bytes_in_a_process_read() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw156p-run (script)
               (let ((buf (generate-new-buffer " *pw156p*")))
                 (unwind-protect
                     (let ((p (make-process :name "pw156p" :buffer buf :sentinel #'ignore
                                            :connection-type 'pipe
                                            :command (list "{sh}" "-c" script))))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             (process-coding-system p)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (let ((default-process-coding-system '(undecided . utf-8-unix))
                   (coding-system-for-read 'undecided))
               (list
                (pw156p-run "printf '\\377\\376a\\0\\r\\0\\n\\0'")
                (pw156p-run "printf '\\376\\377\\0a\\0\\r\\0\\n'")
                (pw156p-run "printf 'a\\0b\\0c\\0d\\0'")
                (pw156p-run "printf 'a\\0b\\r\\n'")
                (pw156p-run "printf '\\377\\376a\\0\\r'")
                (pw156p-run "printf '\\377\\376a\\0'; sleep 0.7; printf '\\r\\0\\n\\0'"))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "((97 10) (utf-16le-with-signature-dos . utf-8-unix) utf-16le-with-signature-dos) ",
            "((97 10) (utf-16be-with-signature-dos . utf-8-unix) utf-16be-with-signature-dos) ",
            "((97 0 98 0 99 0 100 0) (no-conversion . utf-8-unix) no-conversion) ",
            "((97 0 98 13 10) (no-conversion . utf-8-unix) no-conversion) ",
            "((97 10) (utf-16le-with-signature-mac . utf-8-unix) utf-16le-with-signature-mac) ",
            "((97 10) (utf-16le-with-signature-dos . utf-8-unix) utf-16le-with-signature-dos))",
        )
    );
}

/// GNU has ONE decoder, and a subprocess reaches it.
///
/// `read_and_insert_process_output` decodes with `decode_coding_c_string
/// (process_coding, buf, nread, curbuf)` (src/process.c:6502); the filter
/// branch with `decode_coding_c_string (coding, chars, nbytes, Qt)` (:6562);
/// `Fcall_process` with `decode_coding_c_string (&process_coding, buf, nread,
/// curbuf)` (src/callproc.c:856).  `decode_coding_c_string` is a macro whose
/// body is `decode_coding_object (coding, Qnil, 0, 0, bytes, bytes,
/// dst_object)` (src/coding.h:750-755) -- the same C function
/// `decode-coding-string` reaches through `code_convert_string`.  They do not
/// rhyme; they are one call.
///
/// So the five coding systems whose decoders live in the evaluator -- ISO-2022,
/// `emacs-mule`, Shift-JIS, GBK and the charset codings -- decode from a
/// subprocess exactly as they decode from a string.  Every row here names its
/// coding system EXPLICITLY, so nothing in it depends on detection: this is the
/// decoder, and only the decoder.
///
/// Each row is `(STRING PROCESS-BUFFER PROCESS-FILTER CALL-PROCESS)`, and the
/// point of the row is that the four are the same list.  Measured under GNU
/// Emacs 31.0.90 running this test's own program.
#[test]
fn a_subprocess_is_decoded_by_the_decoder_a_string_is() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw159-buf (script coding)
               (let ((buf (generate-new-buffer " *pw159*")))
                 (unwind-protect
                     (let ((p (make-process :name "pw159" :buffer buf :sentinel #'ignore
                                            :connection-type 'pipe
                                            :coding (cons coding 'utf-8-unix)
                                            :command (list "{sh}" "-c" script))))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (defun pw159-filter (script coding)
               (let ((acc nil))
                 (let ((p (make-process :name "pw159f" :buffer nil :sentinel #'ignore
                                        :connection-type 'pipe
                                        :coding (cons coding 'utf-8-unix)
                                        :filter (lambda (_p s) (push s acc))
                                        :command (list "{sh}" "-c" script))))
                   (while (accept-process-output p 1))
                   (while (process-live-p p) (accept-process-output p 0.05))
                   (list (append (apply #'concat (nreverse acc)) nil)
                         last-coding-system-used))))
             (defun pw159-callproc (script coding)
               (with-temp-buffer
                 (let ((coding-system-for-read coding))
                   (call-process "{sh}" nil t nil "-c" script))
                 (list (append (buffer-string) nil) last-coding-system-used)))
             (defun pw159-string (bytes coding)
               (let ((d (decode-coding-string bytes coding)))
                 (list (append d nil) last-coding-system-used)))
             (defun pw159-row (script bytes coding)
               (list (pw159-string bytes coding)
                     (pw159-buf script coding)
                     (pw159-filter script coding)
                     (pw159-callproc script coding)))
             (list
              ;; a ESC $ B $ " ESC ( B LF  -- an ISO-2022 designation and one
              ;; JIS X 0208 character.  Every byte of it is below 0x80.
              (pw159-row "printf 'a\\033$B$\\042\\033(B\\n'"
                         "a\033$B$\042\033(B\n" 'iso-2022-7bit)
              ;; emacs-mule: 0x92 is japanese-jisx0208's leading code.
              (pw159-row "printf 'a\\222\\260\\241\\n'" "a\222\260\241\n" 'emacs-mule)
              ;; shift_jis 82 A0
              (pw159-row "printf 'a\\202\\240\\n'" "a\202\240\n" 'japanese-shift-jis)
              ;; gbk B0 A1
              (pw159-row "printf 'a\\260\\241\\n'" "a\260\241\n" 'chinese-gbk)
              ;; cp437 81 -> U+00FC, a charset coding system
              (pw159-row "printf 'a\\201\\n'" "a\201\n" 'cp437)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            // iso-2022-7bit: U+3042, not the escape bytes.
            "(((97 12354 10) iso-2022-7bit-unix) ((97 12354 10) iso-2022-7bit-unix) ",
            "((97 12354 10) iso-2022-7bit-unix) ((97 12354 10) iso-2022-7bit-unix)) ",
            // emacs-mule: U+4E9C.
            "(((97 20124 10) emacs-mule-unix) ((97 20124 10) emacs-mule-unix) ",
            "((97 20124 10) emacs-mule-unix) ((97 20124 10) emacs-mule-unix)) ",
            // japanese-shift-jis: U+3042.
            "(((97 12354 10) japanese-shift-jis-unix) ((97 12354 10) japanese-shift-jis-unix) ",
            "((97 12354 10) japanese-shift-jis-unix) ((97 12354 10) japanese-shift-jis-unix)) ",
            // chinese-gbk: U+554A.
            "(((97 21834 10) chinese-gbk-unix) ((97 21834 10) chinese-gbk-unix) ",
            "((97 21834 10) chinese-gbk-unix) ((97 21834 10) chinese-gbk-unix)) ",
            // cp437: U+00FC.
            "(((97 252 10) cp437-unix) ((97 252 10) cp437-unix) ",
            "((97 252 10) cp437-unix) ((97 252 10) cp437-unix)))",
        )
    );
}

/// `decode_coding_object` runs the coding system's `:post-read-conversion`
/// (src/coding.c:8180-8194), and a subprocess read IS a `decode_coding_object`
/// call -- so process output runs the hook, on the buffer branch and on the
/// filter branch alike.
///
/// `vietnamese-viqr` is the measurement rather than a probe-defined coding
/// system because its ENTIRE conversion is that hook: its `:coding-type` is
/// `utf-8` and the ASCII mnemonic translation happens in elisp.  So the text
/// answers the question by itself, with no counter to trust.
///
/// The row that must NOT move is the first.  `code_convert_string`'s identity
/// fast path (src/coding.c:9609-9628) returns a pure-ASCII source unconverted
/// and therefore never reaches `decode_coding_object` at all, so
/// `decode-coding-string` does not run the hook where every other door does --
/// and reports the coding system's plain name where every other door reports
/// the end-of-line subsidiary `decode_eol` chose.  The pin carries GNU's
/// disagreement with itself rather than one side of it.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn a_process_read_runs_the_coding_systems_post_read_conversion() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defvar pw159h-src "Vie^.t Nam a` e^`\n")
             (defvar pw159h-script "printf 'Vie^.t Nam a` e^`\\n'")
             (defun pw159h-buf ()
               (let ((buf (generate-new-buffer " *pw159h*")))
                 (unwind-protect
                     (let ((p (make-process :name "pw159h" :buffer buf :sentinel #'ignore
                                            :connection-type 'pipe
                                            :coding (cons 'vietnamese-viqr 'utf-8-unix)
                                            :command (list "{sh}" "-c" pw159h-script))))
                       (while (accept-process-output p 1))
                       (while (process-live-p p) (accept-process-output p 0.05))
                       (list (append (with-current-buffer buf (buffer-string)) nil)
                             last-coding-system-used))
                   (kill-buffer buf))))
             (defun pw159h-filter ()
               (let ((acc nil))
                 (let ((p (make-process :name "pw159hf" :buffer nil :sentinel #'ignore
                                        :connection-type 'pipe
                                        :coding (cons 'vietnamese-viqr 'utf-8-unix)
                                        :filter (lambda (_p s) (push s acc))
                                        :command (list "{sh}" "-c" pw159h-script))))
                   (while (accept-process-output p 1))
                   (while (process-live-p p) (accept-process-output p 0.05))
                   (list (append (apply #'concat (nreverse acc)) nil)
                         last-coding-system-used))))
             (list
              ;; The identity fast path: no conversion, no hook, no subsidiary.
              (let ((d (decode-coding-string pw159h-src 'vietnamese-viqr)))
                (list (append d nil) last-coding-system-used))
              ;; call-process, src/callproc.c:856.
              (with-temp-buffer
                (let ((coding-system-for-read 'vietnamese-viqr))
                  (call-process "{sh}" nil t nil "-c" pw159h-script))
                (list (append (buffer-string) nil) last-coding-system-used))
              ;; make-process, buffer branch, src/process.c:6502.
              (pw159h-buf)
              ;; make-process, filter branch, src/process.c:6562.
              (pw159h-filter)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            // The source verbatim: V i e ^ . t SPC N a m SPC a ` SPC e ^ ` LF
            "((86 105 101 94 46 116 32 78 97 109 32 97 96 32 101 94 96 10) vietnamese-viqr) ",
            // Việt Nam à ề -- the hook ran.
            "((86 105 7879 116 32 78 97 109 32 224 32 7873 10) vietnamese-viqr-unix) ",
            "((86 105 7879 116 32 78 97 109 32 224 32 7873 10) vietnamese-viqr-unix) ",
            "((86 105 7879 116 32 78 97 109 32 224 32 7873 10) vietnamese-viqr-unix))",
        )
    );
}

/// GNU's EOF read is a zero-byte `decode_coding_object` call, and it happens on
/// the FILTER branch of a PIPE and nowhere else.
///
/// `read_process_output` raises `CODING_MODE_LAST_BLOCK` the first time
/// `emacs_read` returns nothing and falls THROUGH to the decode; the second
/// time -- and on any read ERROR -- the flag is already up (or `nbytes < 0`)
/// and it returns without decoding anything (src/process.c:6313-6321).  So the
/// filter branch runs the coding system's `:post-read-conversion` once more
/// than there were chunks, with `produced_char` zero, and does not call the
/// filter for it (`SBYTES (text) > 0`, :6567).
///
/// Two things narrow it, and both are rows here because both were measured
/// under GNU Emacs 31.0.90 running this test's own program:
///
/// * `read_and_insert_process_output` -- the branch `fast-read-process-output'
///   and the default filter take -- `return`s on `!nread` BEFORE deciding
///   anything (:6464), so the buffer branch has no zero-byte decode at all.
/// * A pty is not a pipe.  When the child on the far end of a pty exits, Linux
///   answers the master with `EIO` rather than with a zero-byte read, so
///   `nbytes < 0` takes the early return and the flag is never raised.  Entry
///   159 sized this residual as "3 hook calls for 2 chunks" without naming the
///   connection type; on a pty GNU runs the hook twice, exactly as this port
///   already did.
///
/// Each row is `(FILTER-CHUNKS HOOK-CALLS LAST-CODING-SYSTEM-USED)`.
#[test]
fn an_eof_read_decodes_a_zero_byte_last_block_on_the_filter_branch() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defvar pw166-hooks 0)
             (defun pw166-prc (len) (setq pw166-hooks (1+ pw166-hooks)) len)
             (define-coding-system 'pw166-hook-utf8
               "utf-8 whose :post-read-conversion counts the decodes it is run for"
               :mnemonic ?U :coding-type 'utf-8 :charset-list '(unicode)
               :post-read-conversion #'pw166-prc)
             (defun pw166-run (conn filterp script)
               (setq pw166-hooks 0 last-coding-system-used nil)
               (let* ((buf (unless filterp (generate-new-buffer " *pw166*")))
                      (acc nil)
                      (p (make-process :name "pw166" :buffer buf :sentinel #'ignore
                                       :connection-type conn
                                       :coding (cons 'pw166-hook-utf8 'utf-8-unix)
                                       :filter (when filterp (lambda (_p s) (push s acc)))
                                       :command (list "{sh}" "-c" script))))
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (prog1 (list (length acc) pw166-hooks last-coding-system-used)
                   (when buf (kill-buffer buf)))))
             (list
              ;; filter branch, pipe, nothing written: the hook still runs once,
              ;; on zero characters, and no chunk reaches the filter.
              (pw166-run 'pipe t "true")
              ;; filter branch, pipe, one chunk: one hook call for the chunk and
              ;; one for the last block.
              (pw166-run 'pipe t "printf abc")
              ;; the same on a pty: EIO, not a zero-byte read, so no last block.
              (pw166-run 'pty t "true")
              ;; the buffer branch has no zero-byte decode either way.
              (pw166-run 'pipe nil "true")
              (pw166-run 'pipe nil "printf abc")))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(0 1 pw166-hook-utf8) ",
            "(1 2 pw166-hook-utf8) ",
            "(0 0 nil) ",
            "(0 0 nil) ",
            "(0 1 pw166-hook-utf8))",
        )
    );
}

/// A subprocess's decoder keeps its state across a read boundary, and the
/// boundary is the DECODER's answer rather than a rule about its name.
///
/// GNU decodes a process through ONE `struct coding_system` for the process's
/// whole life (`proc_decode_coding_system[channel]`, src/process.c:6242), and
/// each decoder ends by reporting how far it got:
///
/// ```c
///  no_more_source:
///   coding->consumed_char += consumed_chars_base;
///   coding->consumed = src_base - coding->source;
/// ```
///
/// (src/coding.c:1421-1423 for UTF-8, and the same two lines at :1696, :2541,
/// :3982, :4791, :4886 and :5591.)  `decode_coding` then turns the unconsumed
/// tail into `coding->carryover` when the flag is clear (:7466-7474), and
/// `read_process_output_set_last_coding_system` copies it onto the process
/// (:6448-6457).  Two consequences, and both are rows here:
///
/// * a character split across a read boundary is completed by the next read,
///   for EVERY decoder and not just the UTF-8 family;
/// * an ISO-2022 designation set in one read is still in force in the next,
///   because the designation lives in `coding->spec.iso_2022` and the struct
///   outlives the read.
///
/// The split is forced by a HANDSHAKE and not by a sleep: the child writes,
/// blocks on `read`, and only this test can unblock it, so `2` chunks is a
/// measurement rather than a hope.  Each row is
/// `(CHUNKS CONCATENATED-CHARACTERS LAST-CODING-SYSTEM-USED)`, and the
/// characters are the concatenation precisely so that a row which failed to
/// split would still be WRONG rather than accidentally right.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn a_process_decoder_carries_its_state_and_its_carryover_across_a_read() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw166b-run (coding cmd)
               (let* ((acc nil)
                      (p (make-process :name "pw166b" :buffer nil :sentinel #'ignore
                                       :connection-type 'pipe
                                       :coding (cons coding 'binary)
                                       :filter (lambda (_p s) (setq acc (append acc (list s))))
                                       :command (list "{sh}" "-c" cmd))))
                 (while (null acc) (accept-process-output p 1))
                 (process-send-string p "go\n")
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (list (length acc) (append (apply #'concat acc) nil)
                       last-coding-system-used)))
             (list
              ;; the UTF-8 family, which the deleted name table already covered
              (pw166b-run 'utf-8 "printf 'a\\303'; read x; printf '\\251\\n'")
              (pw166b-run 'utf-8 "printf 'a\\343\\201'; read x; printf '\\202\\n'")
              ;; an ISO-2022 designation that ends read 1: state, not carryover
              (pw166b-run 'iso-2022-7bit "printf 'a\\033$B'; read x; printf '$\\042\\033(B\\n'")
              ;; the same designation, split inside the character it introduces
              (pw166b-run 'iso-2022-7bit "printf 'a\\033$B$'; read x; printf '\\042\\033(B\\n'")
              (pw166b-run 'japanese-shift-jis "printf 'a\\202'; read x; printf '\\240\\n'")
              (pw166b-run 'chinese-gbk "printf 'a\\260'; read x; printf '\\241\\n'")
              (pw166b-run 'chinese-big5 "printf 'a\\244'; read x; printf '\\100\\n'")
              (pw166b-run 'japanese-iso-8bit "printf 'a\\244'; read x; printf '\\242\\n'")
              (pw166b-run 'emacs-mule "printf 'a\\222'; read x; printf '\\260\\241\\n'")
              (pw166b-run 'utf-16le "printf 'a\\000\\102'; read x; printf '\\000\\n\\000'")
              (pw166b-run 'utf-16le-with-signature "printf '\\377\\376a\\000'; read x; printf '\\n\\000'")
              ;; the control: every byte of iso-latin-1 is a character, so
              ;; nothing is ever held back and both chunks decode whole
              (pw166b-run 'iso-latin-1 "printf 'a\\351'; read x; printf '\\350\\n'")))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(2 (97 233 10) utf-8-unix) ",
            "(2 (97 12354 10) utf-8-unix) ",
            "(2 (97 12354 10) iso-2022-7bit-unix) ",
            "(2 (97 12354 10) iso-2022-7bit-unix) ",
            "(2 (97 12354 10) japanese-shift-jis-unix) ",
            "(2 (97 21834 10) chinese-gbk-unix) ",
            "(2 (97 19968 10) chinese-big5-unix) ",
            "(2 (97 12354 10) japanese-iso-8bit-unix) ",
            "(2 (97 20124 10) emacs-mule-unix) ",
            "(2 (97 66 10) utf-16le-unix) ",
            "(2 (97 10) utf-16le-with-signature-unix) ",
            "(2 (97 233 232 10) iso-latin-1-unix))",
        )
    );
}

// ---------------------------------------------------------------------------
// Ledger 186: a charset decoder refuses a leading byte before it reads another
// ---------------------------------------------------------------------------

/// GNU's `decode_coding_charset` looks the leading byte up in
/// `AREF (valids, c)` BEFORE it reads any more of the source
/// (src/coding.c:5526-5528), so a byte that no charset in the coding system's
/// `:charset-list` can START goes to `invalid_code:` in the read it arrived
/// in -- even when a longer charset in the same list would have needed bytes
/// this read does not have.  Ledger 166 found this and left it open; the text
/// is the same either way, but a Lisp filter sees the run boundaries.
///
/// The split is forced by a HANDSHAKE and not by a sleep: the child writes,
/// blocks on `read', and only this test can unblock it.  Rows 1-5 are the
/// divergence, rows 6-8 are the controls that stop the fix from becoming
/// "never hold anything back": a VALID leading byte that really is incomplete
/// must still wait for the next read, a complete two-byte character must not,
/// and every byte of `iso-latin-1' is a character so nothing is ever held.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program
/// (`tmp/pw186/probe1.el`, `tmp/pw186/probe1-gnu.txt`).  Before this entry the
/// first five rows read
/// `(2 ((97) (4194176 98 10)))`, `(2 ((97) (4194303 98 10)))`,
/// `(1 ((4194176 98 10)))` -- the third one being a read whose filter never
/// ran at all -- `(2 ((97) (4194176 98 10)))` and `(2 ((97) (4194176 98 10)))`.
#[test]
fn a_charset_decoder_refuses_a_leading_byte_no_charset_can_start() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw186-run (coding cmd)
               (let* ((acc nil)
                      (p (make-process :name "pw186" :buffer nil :sentinel #'ignore
                                       :connection-type 'pipe
                                       :coding (cons coding 'binary)
                                       :filter (lambda (_p s) (setq acc (append acc (list s))))
                                       :command (list "{sh}" "-c" cmd))))
                 ;; Bounded: a port that holds the whole of read 1 back never
                 ;; calls the filter, and an unbounded wait would then deadlock
                 ;; against a child blocked on `read'.
                 (let ((tries 0))
                   (while (and (null acc) (< tries 10))
                     (setq tries (1+ tries))
                     (accept-process-output p 1)))
                 (process-send-string p "go\n")
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (list (length acc)
                       (mapcar (lambda (s) (append s nil)) acc)
                       last-coding-system-used)))
             (list
              ;; 0x80 and 0xFF begin no charset of `chinese-gbk': its list is
              ;; (ascii chinese-gbk), whose leading byte ranges are 0-127 and
              ;; 0x81-0xFE.
              (pw186-run 'chinese-gbk "printf 'a\\200'; read x; printf 'b\\n'")
              (pw186-run 'chinese-gbk "printf 'a\\377'; read x; printf 'b\\n'")
              ;; the same byte as the WHOLE of read 1: GNU decodes it and runs
              ;; the filter, so there are two chunks and not one
              (pw186-run 'chinese-gbk "printf '\\200'; read x; printf 'b\\n'")
              (pw186-run 'cp936 "printf 'a\\200'; read x; printf 'b\\n'")
              ;; gb18030 has 2-byte AND 4-byte charsets in one list
              (pw186-run 'chinese-gb18030 "printf 'a\\200'; read x; printf 'b\\n'")
              ;; controls
              (pw186-run 'chinese-gb18030 "printf 'a\\201\\100'; read x; printf 'b\\n'")
              (pw186-run 'chinese-gbk "printf 'a\\260'; read x; printf '\\241b\\n'")
              (pw186-run 'iso-latin-1 "printf 'a\\351'; read x; printf '\\350\\n'")))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(2 ((97 4194176) (98 10)) chinese-gbk-unix) ",
            "(2 ((97 4194303) (98 10)) chinese-gbk-unix) ",
            "(2 ((4194176) (98 10)) chinese-gbk-unix) ",
            "(2 ((97 4194176) (98 10)) chinese-gbk-unix) ",
            "(2 ((97 4194176) (98 10)) chinese-gb18030-unix) ",
            "(2 ((97 19970) (98 10)) chinese-gb18030-unix) ",
            "(2 ((97) (21834 98 10)) chinese-gbk-unix) ",
            "(2 ((97 233) (232 10)) iso-latin-1-unix))",
        )
    );
}

// ---------------------------------------------------------------------------
// Ledger 169: the removal decision precedes the sentinel (GNU `status_notify`)
// ---------------------------------------------------------------------------

/// GNU's `status_notify` settles the process's presence in `Vprocess_alist`
/// BEFORE it runs the sentinel: it applies the pending status
/// (src/process.c:7914-7915), builds the message (:7916), then takes the
/// removal decision -- `remove_process' when `delete-exited-processes' is
/// non-nil, `deactivate_process' otherwise (:7926-7929) -- and only then calls
/// `exec_sentinel' (:7937).  `get-buffer-process' (:8425-8427),
/// `get-process' and `process-list' all walk `Vprocess_alist', so an exit
/// sentinel in GNU sees its own process already gone.
///
/// Measured, `emacs -Q --batch`, GNU Emacs 31.0.90:
///
/// ```text
/// PW169-CHILD-SENTINEL: (:event "finished" :get-buffer-process nil
///                        :get-process nil :in-process-list nil
///                        :process-status exit :process-live-p nil ...)
/// ```
#[test]
fn exit_sentinel_sees_its_own_process_already_removed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169-child*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-child"
                         :buffer buf
                         :command '("{sh}" "-c" "printf hi")
                         :connection-type 'pipe
                         :noquery t
                         :sentinel
                         (lambda (p event)
                           (when (string-prefix-p "finished" event)
                             (setq seen
                                   (list :get-buffer-process
                                         (and (get-buffer-process buf) t)
                                         :get-process
                                         (and (get-process "pw169-child") t)
                                         :in-process-list
                                         (and (memq p (process-list)) t)
                                         :process-status (process-status p)
                                         :buffer-text
                                         (with-current-buffer buf
                                           (buffer-substring-no-properties
                                            (point-min) (point-max))))))))))
             (while (eq seen :pending) (accept-process-output proc 0.1))
             seen)"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:get-buffer-process nil :get-process nil :in-process-list nil ",
            ":process-status exit :buffer-text \"hi\")",
        )
    );
}

/// The same removal decision on a pty child.  GNU takes it in `status_notify`
/// regardless of `connection-type`, because the decision reads only
/// `p->status` (src/process.c:7919-7929).
///
/// Measured, GNU Emacs 31.0.90:
/// `PW169-PTY-SENTINEL: (:event "finished" :get-buffer-process nil
///  :get-process nil :in-process-list nil :process-status exit)`
#[test]
fn exit_sentinel_of_a_pty_child_sees_its_process_removed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169-pty*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-pty"
                         :buffer buf
                         :command '("{sh}" "-c" "printf hi")
                         :connection-type 'pty
                         :noquery t
                         :sentinel
                         (lambda (p event)
                           (when (string-prefix-p "finished" event)
                             (setq seen
                                   (list :get-buffer-process
                                         (and (get-buffer-process buf) t)
                                         :get-process
                                         (and (get-process "pw169-pty") t)
                                         :in-process-list
                                         (and (memq p (process-list)) t)
                                         :process-status
                                         (process-status p))))))))
             (while (eq seen :pending) (accept-process-output proc 0.1))
             seen)"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:get-buffer-process nil :get-process nil ",
            ":in-process-list nil :process-status exit)",
        )
    );
}

/// The removal is `delete-exited-processes'-gated, and this is what makes the
/// ordering observable rather than merely early: GNU calls `remove_process'
/// only under the flag and `deactivate_process' otherwise
/// (src/process.c:7926-7929), and `deactivate_process' (:4812) does not touch
/// `Vprocess_alist'.  So with the flag nil GNU's exit sentinel DOES see its own
/// process -- the opposite answer from the default -- which no "reap earlier"
/// change may flatten.
///
/// Measured, GNU Emacs 31.0.90:
/// `PW169-KEEP-SENTINEL: (:event "finished" :get-buffer-process t
///  :get-process t :in-process-list t :process-status exit)`
#[test]
fn exit_sentinel_still_sees_its_process_when_delete_exited_processes_is_nil() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((delete-exited-processes nil)
                  (buf (generate-new-buffer " *pw169-keep*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-keep"
                         :buffer buf
                         :command '("{sh}" "-c" "printf hi")
                         :noquery t
                         :sentinel
                         (lambda (p event)
                           (when (string-prefix-p "finished" event)
                             (setq seen
                                   (list :get-buffer-process
                                         (and (get-buffer-process buf) t)
                                         :get-process
                                         (and (get-process "pw169-keep") t)
                                         :in-process-list
                                         (and (memq p (process-list)) t)
                                         :process-status
                                         (process-status p))))))))
             (while (eq seen :pending) (accept-process-output proc 0.1))
             seen)"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:get-buffer-process t :get-process t ",
            ":in-process-list t :process-status exit)",
        )
    );
}

/// A signalled child takes the same path: `status_notify' compares the status
/// SYMBOL against `Qsignal', `Qexit' and `Qclosed' (src/process.c:7923-7924),
/// so `(signal . 15)' is removed exactly like `(exit . 0)'.
///
/// Measured, GNU Emacs 31.0.90:
/// `PW169-SIGNAL-SENTINEL: (:event "terminated" :get-process nil
///  :in-process-list nil :process-status signal :exit-status 15)`
#[test]
fn signalled_child_sentinel_sees_its_process_removed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169-sig*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-sig"
                         :buffer buf
                         :command '("{sh}" "-c" "kill -TERM $$; sleep 5")
                         :noquery t
                         :sentinel
                         (lambda (p _event)
                           (unless (process-live-p p)
                             (setq seen
                                   (list :get-process
                                         (and (get-process "pw169-sig") t)
                                         :in-process-list
                                         (and (memq p (process-list)) t)
                                         :process-status (process-status p)
                                         :exit-status
                                         (process-exit-status p))))))))
             (while (eq seen :pending) (accept-process-output proc 0.1))
             seen)"#
    ));

    assert_eq!(
        result,
        "OK (:get-process nil :in-process-list nil :process-status signal :exit-status 15)"
    );
}

/// The retirement is strictly per-process, not a batch at the end of the
/// notification pass: GNU's `FOR_EACH_PROCESS' body (src/process.c:7887)
/// retires and then notifies ONE process before moving to the next.  With two
/// children exiting together, the sentinel that runs first therefore sees the
/// other still listed and itself already gone, and the second sees neither.
///
/// Deliberately blind to WHICH of the two runs first.  GNU's alist is
/// newest-first (`create_process' conses onto the front, src/process.c:953) so
/// GNU reports `pw169-b' first; this port's notification walk is driven by
/// poller readiness and reported `pw169-a' first.  That ordering difference is
/// a separate divergence, recorded in ledger 169 as found and not fixed.
/// Pinning it here would make this test fail for the wrong reason.
///
/// Measured, GNU Emacs 31.0.90:
/// `PW169-TWO-SENTINELS: (("pw169-b" :live ("pw169-a" ...))
///                        ("pw169-a" :live (...)))`
/// -- the first sentinel sees one other `pw169-' process and never itself; the
/// second sees none.
#[test]
fn each_sentinel_sees_only_the_processes_not_yet_retired_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((seen nil)
                  (n 0)
                  (mk (lambda (name)
                        (make-process
                         :name name
                         :buffer (generate-new-buffer (concat " *" name "*"))
                         :command '("{sh}" "-c" "printf x")
                         :noquery t
                         :sentinel
                         (lambda (p event)
                           (when (string-prefix-p "finished" event)
                             (setq n (1+ n))
                             (let ((mine 0) (others 0))
                               (dolist (q (process-list))
                                 (when (string-prefix-p "pw169-" (process-name q))
                                   (if (eq q p)
                                       (setq mine (1+ mine))
                                     (setq others (1+ others)))))
                               (push (list :self mine :others others) seen))))))))
             (funcall mk "pw169-a")
             (funcall mk "pw169-b")
             (while (< n 2) (accept-process-output nil 0.1))
             (nreverse seen))"#
    ));

    assert_eq!(result, "OK ((:self 0 :others 1) (:self 0 :others 0))");
}

/// The identity a sentinel receives outlives the removal, because removal is
/// deregistration from a directory and not destruction of the object: GNU's
/// `remove_process' (src/process.c:957-966) only rewrites `Vprocess_alist',
/// and `exec_sentinel' hands the sentinel the very `Lisp_Object proc' the
/// notification loop already held (:7845-7846), never a re-lookup.
///
/// Measured, GNU Emacs 31.0.90, on the value captured inside the sentinel:
/// `PW169-REAPED-VALUE: (:eq-to-original t :processp t :name "pw169-val"
///  :status exit :exit 0 :buffer t :sentinel t
///  :filter internal-default-process-filter :command ("sh" "-c" "printf hi")
///  :type real :contact t :query-on-exit t :plist nil :tty "/dev/pts/31")`
///
/// (The pin below reads `:query-on-exit nil` only because the test passes
/// `:noquery t`, which the probe did not.)
#[test]
fn a_retired_process_value_still_answers_every_accessor_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169-val*"))
                  (kept nil)
                  (done nil)
                  (proc (make-process
                         :name "pw169-val"
                         :buffer buf
                         :command '("{sh}" "-c" "printf hi")
                         :noquery t
                         :sentinel (lambda (p event)
                                     (when (string-prefix-p "finished" event)
                                       (setq kept p done t))))))
             (while (not done) (accept-process-output proc 0.1))
             (list :eq-to-original (eq kept proc)
                   :processp (processp kept)
                   :name (process-name kept)
                   :status (process-status kept)
                   :exit (process-exit-status kept)
                   :buffer (and (bufferp (process-buffer kept)) t)
                   :sentinel (and (process-sentinel kept) t)
                   :filter (process-filter kept)
                   :command (process-command kept)
                   :type (process-type kept)
                   :contact (process-contact kept)
                   :query-on-exit (process-query-on-exit-flag kept)
                   :plist (process-plist kept)))"#
    ));

    assert_eq!(
        result,
        format!(
            "OK (:eq-to-original t :processp t :name \"pw169-val\" \
             :status exit :exit 0 :buffer t :sentinel t \
             :filter internal-default-process-filter \
             :command (\"{sh}\" \"-c\" \"printf hi\") \
             :type real :contact t :query-on-exit nil :plist nil)"
        )
    );
}

/// `delete-process` reaches the same retirement through the same code: GNU's
/// `Fdelete_process` stamps the terminal status and calls `status_notify`
/// (src/process.c:1128 for a network/pipe/serial process, :1148 for a child),
/// so the sentinel it runs sees the `delete-exited-processes' decision rather
/// than an unconditional removal.  `Fdelete_process`'s own trailing
/// `remove_process' (:1155) is what makes the deletion unconditional, and it
/// runs after the sentinel has returned.
///
/// So with the flag nil the deleted process is still listed inside its sentinel
/// and gone immediately after -- the one place where "before" and "after" the
/// sentinel differ.  Measured, GNU Emacs 31.0.90:
///
/// ```text
/// PW169-DELETE-KEEP-SENTINEL: (:event "killed" :get-buffer-process t
///                              :get-process t :in-process-list t)
/// PW169-DELETE-KEEP-AFTER:    (:get-process nil :in-process-list nil)
/// ```
#[test]
fn delete_process_sentinel_honours_delete_exited_processes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((delete-exited-processes nil)
                  (buf (generate-new-buffer " *pw169-del2*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-del2"
                         :buffer buf
                         :command '("{sh}" "-c" "sleep 30")
                         :noquery t
                         :sentinel
                         (lambda (p _event)
                           (setq seen
                                 (list :get-buffer-process
                                       (and (get-buffer-process buf) t)
                                       :get-process
                                       (and (get-process "pw169-del2") t)
                                       :in-process-list
                                       (and (memq p (process-list)) t)))))))
             (delete-process proc)
             (list :in-sentinel seen
                   :after (list :get-process
                                (and (get-process "pw169-del2") t)
                                :in-process-list
                                (and (memq proc (process-list)) t))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:in-sentinel (:get-buffer-process t :get-process t :in-process-list t) ",
            ":after (:get-process nil :in-process-list nil))",
        )
    );
}

/// The default setting is unchanged by the above: `delete-exited-processes' is
/// `t', so `status_notify' removes (src/process.c:7926) and the sentinel sees
/// nothing.  Measured, GNU Emacs 31.0.90:
/// `PW169-DELETE-SENTINEL: (:event "killed" :get-buffer-process nil
///  :get-process nil :in-process-list nil :process-status signal)`
#[test]
fn delete_process_sentinel_sees_its_process_removed_by_default_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169-del*"))
                  (seen :pending)
                  (proc (make-process
                         :name "pw169-del"
                         :buffer buf
                         :command '("{sh}" "-c" "sleep 30")
                         :noquery t
                         :sentinel
                         (lambda (p event)
                           (setq seen
                                 (list :event (string-trim event)
                                       :get-buffer-process
                                       (and (get-buffer-process buf) t)
                                       :get-process
                                       (and (get-process "pw169-del") t)
                                       :in-process-list
                                       (and (memq p (process-list)) t)
                                       :process-status (process-status p)))))))
             (delete-process proc)
             seen)"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:event \"killed\" :get-buffer-process nil :get-process nil ",
            ":in-process-list nil :process-status signal)",
        )
    );
}

/// The neighbour audit of ledger 169, as one value: every Lisp entry point an
/// exit sentinel can reach about its own just-retired process, run inside that
/// sentinel.  35 entry points, one assert, so a future change to any of them
/// has to come past this pin.
///
/// Measured, `emacs -Q --batch`, GNU Emacs 31.0.90 (probe
/// `tmp/pw169/audit-list.el`).  All 35 rows match GNU; the one row whose text
/// differs, `coding`, differs only under this unit-test runtime and is
/// annotated at the pin.
///
/// Six of these rows moved with ledger 169 and none of them is about the
/// process list: `running-child-p`, `interrupt`, `kill`, `continue` and `stop`
/// all gate on GNU's `p->infd < 0`, which becomes true inside
/// `deactivate_process` -- the function `remove_process` calls.  Retiring too
/// late kept the gate open, so five subrs silently succeeded on a dead process
/// and `stop-process` re-entered the sentinel.  That is the argument for
/// auditing the neighbours rather than pinning the reported symptom.
#[test]
fn exit_sentinel_neighbour_audit() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((buf (generate-new-buffer " *pw169audit*"))
                  (depth 0)
                  (result :pending)
                  (try (lambda (thunk)
                         (condition-case e (funcall thunk)
                           (error (list 'error (cadr e))))))
                  (proc nil))
             (setq proc
                   (make-process
                    :name "pw169audit"
                    :buffer buf
                    :command '("{sh}" "-c" "printf hi")
                    :connection-type 'pipe
                    :noquery t
                    :sentinel
                    (lambda (p event)
                      (when (and (= depth 0) (string-prefix-p "finished" event))
                        (setq depth 1)
                        (setq result
                              (list
                               (cons 'processp    (funcall try (lambda () (and (processp p) t))))
                               (cons 'status      (funcall try (lambda () (process-status p))))
                               (cons 'live-p      (funcall try (lambda () (and (process-live-p p) t))))
                               (cons 'exit-status (funcall try (lambda () (process-exit-status p))))
                               (cons 'id-nonnil   (funcall try (lambda () (and (process-id p) t))))
                               (cons 'name        (funcall try (lambda () (process-name p))))
                               (cons 'buffer-live (funcall try (lambda () (and (buffer-live-p (process-buffer p)) t))))
                               (cons 'mark-set    (funcall try (lambda () (and (marker-buffer (process-mark p)) t))))
                               (cons 'type        (funcall try (lambda () (process-type p))))
                               (cons 'contact     (funcall try (lambda () (process-contact p))))
                               (cons 'filter      (funcall try (lambda () (process-filter p))))
                               (cons 'sentinel-set (funcall try (lambda () (and (process-sentinel p) t))))
                               (cons 'plist       (funcall try (lambda () (process-plist p))))
                               (cons 'query-on-exit (funcall try (lambda () (process-query-on-exit-flag p))))
                               (cons 'coding      (funcall try (lambda () (process-coding-system p))))
                               (cons 'inherit-coding (funcall try (lambda () (process-inherit-coding-system-flag p))))
                               (cons 'tty-name-string (funcall try (lambda () (and (stringp (process-tty-name p)) t))))
                               (cons 'thread-nonnil (funcall try (lambda () (and (process-thread p) t))))
                               (cons 'running-child-p (funcall try (lambda () (process-running-child-p p))))
                               (cons 'get-process (funcall try (lambda () (and (get-process "pw169audit") t))))
                               (cons 'get-buffer-process (funcall try (lambda () (and (get-buffer-process buf) t))))
                               (cons 'in-process-list (funcall try (lambda () (and (memq p (process-list)) t))))
                               (cons 'set-plist   (funcall try (lambda () (progn (set-process-plist p '(:pw169 t)) (process-plist p)))))
                               (cons 'set-filter  (funcall try (lambda () (progn (set-process-filter p #'ignore) (process-filter p)))))
                               (cons 'send-string (funcall try (lambda () (progn (process-send-string p "x") 'ok))))
                               (cons 'send-eof    (funcall try (lambda () (progn (process-send-eof p) 'ok))))
                               (cons 'interrupt   (funcall try (lambda () (progn (interrupt-process p) 'ok))))
                               (cons 'kill        (funcall try (lambda () (progn (kill-process p) 'ok))))
                               (cons 'signal-0    (funcall try (lambda () (progn (signal-process p 0) 'ok))))
                               (cons 'continue    (funcall try (lambda () (progn (continue-process p) 'ok))))
                               (cons 'stop        (funcall try (lambda () (progn (stop-process p) 'ok))))
                               (cons 'accept-output (funcall try (lambda () (accept-process-output p 0))))
                               (cons 'delete      (funcall try (lambda () (progn (delete-process p) 'ok))))
                               (cons 'status-after-delete (funcall try (lambda () (process-status p))))
                               (cons 'in-list-after-delete (funcall try (lambda () (and (memq p (process-list)) t))))))))))
             (while (eq result :pending) (accept-process-output proc 0.1))
             result)"#
    ));

    assert_eq!(result, PW169_NEIGHBOUR_AUDIT);
}

/// GNU Emacs 31.0.90's answer, verbatim, with this port's four known
/// deviations substituted and labelled.  Keeping the two in one string is
/// deliberate: a reader comparing them sees exactly which rows are still open.
const PW169_NEIGHBOUR_AUDIT: &str = concat!(
    "OK ((processp . t) (status . exit) (live-p) (exit-status . 0) ",
    "(id-nonnil . t) (name . \"pw169audit\") (buffer-live . t) (mark-set . t) ",
    "(type . real) (contact . t) (filter . internal-default-process-filter) ",
    "(sentinel-set . t) (plist) (query-on-exit) ",
    // HARNESS, not a divergence: `emacs -Q` and `./target/release/neomacs -Q`
    // both answer `(utf-8-unix . utf-8-unix)` here (probe
    // `tmp/pw169/audit-list.el`, measured on both).  This unit-test runtime
    // starts without the locale-derived coding priority, so the DECODE half is
    // still `undecided` when the sentinel reads it.
    "(coding undecided-unix . utf-8-unix) (inherit-coding) (tty-name-string) ",
    "(thread-nonnil . t) ",
    // GNU raises `Process NAME is not active` here, from the `p->infd < 0`
    // gate at src/process.c:7045-7047 -- and `p->infd` goes to -1 in
    // `deactivate_process` (:4845-4847), which `remove_process` calls (:965).
    // So GNU's gate closes at the retirement, and this port's live-table
    // lookup is the same gate: before ledger 169 it answered 0 here, because
    // the retirement had not happened yet.
    "(running-child-p error \"Process pw169audit is not active\") ",
    "(get-process) (get-buffer-process) (in-process-list) ",
    "(set-plist :pw169 t) (set-filter . ignore) ",
    "(send-string error \"Process pw169audit not running: finished\n\") ",
    "(send-eof error \"Process pw169audit not running: finished\n\") ",
    // The same gate, through `process_send_signal` (src/process.c:7087-7089).
    // All four answered `ok` before ledger 169, and `stop-process` on a
    // still-live retiring process re-entered the sentinel, so the audit probe
    // looped: 4716 rows instead of 123.
    "(interrupt error \"Process pw169audit is not active\") ",
    "(kill error \"Process pw169audit is not active\") ",
    "(signal-0 . ok) ",
    "(continue error \"Process pw169audit is not active\") ",
    "(stop error \"Process pw169audit is not active\") ",
    "(accept-output) (delete . ok) (status-after-delete . exit) ",
    "(in-list-after-delete))",
);

/// The same audit on the `:stderr` pipe process, whose sentinel runs after the
/// pipe has been retired too -- and which answers a DIFFERENT set of errors,
/// because GNU tests the process TYPE before it tests `p->infd < 0`.
///
/// `process_send_signal` raises "is not a subprocess" at src/process.c:7084-7086
/// and only then "is not active" at :7087-7089; `Fprocess_running_child_p` has
/// the same pair at :7042-7047; `internal-default-signal-process` raises
/// "Cannot signal process" from `p->pid <= 0` after a bare `CHECK_PROCESS`
/// (:7379-7382); and `Fstop_process` / `Fcontinue_process` never reach any of
/// them, because they handle a network, serial or pipe process first and
/// return the process (:7267-7278, :7294-7315).
///
/// A pipe is never `Qreal` and its pid is 0, so all five answers are
/// independent of whether it is still listed.  This port answered them from a
/// live-table lookup placed AHEAD of the type check, which was invisible while
/// the pipe was still listed inside its own sentinel and became six wrong rows
/// the moment ledger 169 retired it on time.
///
/// Measured, `emacs -Q --batch`, GNU Emacs 31.0.90 (`tmp/pw169/audit.el`).
#[test]
fn stderr_pipe_sentinel_neighbour_audit() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((obuf (generate-new-buffer " *pw169pipe-out*"))
                  (ebuf (generate-new-buffer " *pw169pipe-err*"))
                  (depth 0)
                  (owner-done nil)
                  (result :pending)
                  (try (lambda (thunk)
                         (condition-case e (funcall thunk)
                           (error (list 'error (cadr e))))))
                  (proc (make-process
                         :name "pw169pipe"
                         :buffer obuf
                         :command '("{sh}" "-c" "printf out; printf err 1>&2")
                         :stderr ebuf
                         :noquery t
                         :sentinel (lambda (_p _e) (setq owner-done t)))))
             (set-process-sentinel
              (get-buffer-process ebuf)
              (lambda (p event)
                (when (and (= depth 0) (string-prefix-p "finished" event))
                  (setq depth 1)
                  (setq result
                        (list
                         (cons 'status (funcall try (lambda () (process-status p))))
                         (cons 'type (funcall try (lambda () (process-type p))))
                         (cons 'get-buffer-process
                               (funcall try (lambda () (and (get-buffer-process ebuf) t))))
                         (cons 'in-process-list
                               (funcall try (lambda () (and (memq p (process-list)) t))))
                         (cons 'running-child-p
                               (funcall try (lambda () (process-running-child-p p))))
                         (cons 'interrupt
                               (funcall try (lambda () (progn (interrupt-process p) 'ok))))
                         (cons 'kill
                               (funcall try (lambda () (progn (kill-process p) 'ok))))
                         (cons 'signal-0
                               (funcall try (lambda () (progn (signal-process p 0) 'ok))))
                         (cons 'continue
                               (funcall try (lambda () (progn (continue-process p) 'ok))))
                         (cons 'stop
                               (funcall try (lambda () (progn (stop-process p) 'ok))))
                         (cons 'send-eof
                               (funcall try (lambda () (progn (process-send-eof p) 'ok)))))))))
             (while (not owner-done) (accept-process-output nil 0.1))
             (let ((n 0))
               (while (and (eq result :pending) (< n 50))
                 (setq n (1+ n))
                 (accept-process-output nil 0.05)))
             result)"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK ((status . closed) (type . pipe) ",
            "(get-buffer-process) (in-process-list) ",
            "(running-child-p error \"Process pw169pipe stderr is not a subprocess\") ",
            "(interrupt error \"Process pw169pipe stderr is not a subprocess\") ",
            "(kill error \"Process pw169pipe stderr is not a subprocess\") ",
            "(signal-0 error \"Cannot signal process pw169pipe stderr\") ",
            "(continue . ok) (stop . ok) ",
            "(send-eof error \"Process pw169pipe stderr not running: finished\n\"))",
        )
    );
}

/// A bare integer argument to `signal-process` is an OS pid, and GNU never
/// looks it up: `internal-default-signal-process` calls `get_process` only for
/// a NON-number (src/process.c:7369-7370), and a number goes straight to
/// `CONS_TO_INTEGER (process, pid_t, pid)` (:7375-7376).  The docstring says so
/// too (:7405-7407).
///
/// This port consulted the live process table first, so a small integer
/// answered for whichever process happened to hold that internal `ProcessId`.
/// Measured, `-Q --batch`, with exactly one live child:
///
/// ```text
///                          GNU 31.0.90   Neomacs, before
/// (signal-process 1 0)     -1            0      <- this port's process #1
/// (signal-process 2 0)     -1            -1
/// (signal-process 3 0)     -1            -1
/// ```
///
/// `-1` is `kill (1, 0)` failing with EPERM against init.  Signal 0 only, so
/// nothing is actually signalled.  Found by ledger 169's neighbour audit, not
/// by the bug it set out to fix.
#[test]
fn signal_process_reads_an_integer_as_an_os_pid_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((p (make-process
                      :name "pw169-sigfix"
                      :buffer (generate-new-buffer " *pw169sig*")
                      :command '("{sh}" "-c" "sleep 30")
                      :noquery t
                      :sentinel #'ignore))
                  (answers (mapcar (lambda (n)
                                     (condition-case e (signal-process n 0)
                                       (error (list 'error (cadr e)))))
                                   '(1 2 3)))
                  (own (condition-case e (signal-process (process-id p) 0)
                         (error (list 'error (cadr e))))))
             (prog1 (list :small answers
                          :own own
                          :own-pid-is-large (and (> (process-id p) 100) t)
                          :still-live (and (process-live-p p) t))
               (delete-process p)))"#
    ));

    assert_eq!(
        result,
        "OK (:small (-1 -1 -1) :own 0 :own-pid-is-large t :still-live t)"
    );
}

/// GNU's PROCESS argument to `internal-default-signal-process` is a NUMBER
/// domain, not a non-negative one.  `get_process` is called only for a
/// non-number (src/process.c:7369-7370); every number goes to
/// `CONS_TO_INTEGER (process, pid_t, pid)` (:7375-7376) and then to
/// `kill (pid, signo)` (:7397).  `CONS_TO_INTEGER` is `cons_to_signed` over
/// `pid_t`'s full signed range (src/lisp.h:4188-4191), so it accepts a
/// fixnum, a bignum and an INTEGRAL float, negative ones included -- and a
/// negative pid is a POSIX process GROUP, which is why GNU does not
/// range-check it.
///
/// This port guarded the arm `pid >= 0`, so a negative integer fell through to
/// the designator resolver and raised.  Measured, `-Q --batch`
/// (`tmp/pw175/signal-negative.el`), GNU Emacs 31.0.90 against this port
/// before the fix:
///
/// ```text
///                                   GNU 31.0.90   Neomacs, before
/// (signal-process -99999 0)         -1            (wrong-type-argument processp -99999)
/// (signal-process (- child-pid) 0)  0             (wrong-type-argument processp -682594)
/// (signal-process 99999999.0 0)     -1            (wrong-type-argument processp 99999999.0)
/// (signal-process -99999999.9 0)    error         (wrong-type-argument processp -99999999.9)
/// ```
///
/// `-1` is `kill` failing with ESRCH; the child's own group exists because
/// every child is `setsid`-ed into one (`isolate_child_command`), which is
/// also GNU's arrangement.  Signal 0 throughout, so nothing is signalled.
/// Ledger 169 residual 5, ledger 175.
#[test]
fn signal_process_takes_a_negative_integer_as_a_process_group_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((p (make-process
                      :name "pw175-siggroup"
                      :buffer (generate-new-buffer " *pw175sig*")
                      :command '("{sh}" "-c" "sleep 30")
                      :noquery t
                      :sentinel #'ignore))
                  (try (lambda (thunk)
                         (condition-case e (funcall thunk)
                           (error (list 'error (car e) (cadr e)))))))
             (prog1
                 (list :absent-group (funcall try (lambda () (signal-process -99999 0)))
                       :own-group    (funcall try (lambda ()
                                                    (signal-process (- (process-id p)) 0)))
                       :own-pid      (funcall try (lambda () (signal-process (process-id p) 0)))
                       :integral-float (funcall try (lambda () (signal-process 99999999.0 0)))
                       :fractional-float (funcall try (lambda () (signal-process -99999999.9 0)))
                       :huge-float   (funcall try (lambda () (signal-process 1e300 0)))
                       :bignum       (funcall try
                                              (lambda ()
                                                (signal-process 99999999999999999999999999 0)))
                       :cons         (funcall try (lambda () (signal-process '(1 . 2) 0)))
                       :still-live   (and (process-live-p p) t))
               (delete-process p)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (:absent-group -1 :own-group 0 :own-pid 0 :integral-float -1",
            " :fractional-float (error error \"Not an in-range integer, integral float,",
            " or cons of integers\")",
            " :huge-float (error error \"Not an in-range integer, integral float,",
            " or cons of integers\")",
            " :bignum (error error \"Not an in-range integer, integral float,",
            " or cons of integers\")",
            " :cons (error wrong-type-argument processp) :still-live t)"
        )
    );
}

/// GNU's status-notification walk is NEWEST-FIRST, and it is observable.
///
/// `status_notify` iterates `FOR_EACH_PROCESS` (src/process.c:7885), which is
/// `FOR_EACH_ALIST_VALUE (Vprocess_alist, ...)` (:343), and `make_process`
/// conses each new process onto the FRONT of that alist (:953).  So when one
/// pass finds two processes whose status has changed, the one created LAST
/// gets its sentinel first.  `process-list` is the same list
/// (`Fmapcar (Qcdr, Vprocess_alist)`, :1749), which this port already
/// reproduces by sorting on descending `ProcessId` (`list_processes`).
///
/// This port took the poller's ready list instead, which is a `HashMap`
/// iteration order -- so the order was not merely oldest-first, it was
/// RANDOM.  Measured, `-Q --batch`, twelve runs of
/// `tmp/pw175/notify-order5.el` (two children that have already exited when
/// the first notification pass runs):
///
/// ```text
///                             b-then-a    a-then-b
/// GNU Emacs 31.0.90              12           0
/// Neomacs, before                 5           7
/// ```
///
/// And on the exact shape below (`tmp/pw175/notify-order6.el`, three runs of
/// six on each editor, against the merge-base binary built in the main tree):
///
/// ```text
///                             b-then-a    a-then-b
/// GNU Emacs 31.0.90              18           0
/// Neomacs at the merge base       7          11
/// ```
///
/// Six runs are pinned rather than one because the defect is a coin flip:
/// one run reproduces it only 58% of the time.  Each child touches a marker
/// file immediately before exiting, and the spin waits for BOTH markers rather
/// than for the clock, so a loaded machine makes this test slower instead of
/// splitting the two statuses across two passes -- which is the one condition
/// under which GNU's own answer is not `b a` either.  Ledger 169 residual 1,
/// ledger 175.
#[test]
fn two_processes_notified_in_one_pass_run_their_sentinels_newest_first_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let ((orders nil)
                 (dir (file-name-as-directory (make-temp-file "pw175order" t))))
             (dotimes (i 6)
               (let* ((order nil)
                      (marker (lambda (tag) (expand-file-name (format "%s-%d" tag i) dir)))
                      (sentinel (lambda (p _m)
                                  (when (memq (process-status p) '(exit signal))
                                    (push (substring (process-name p) 6 7) order))))
                      (spawn (lambda (tag)
                               (make-process
                                :name (format "pw175-%s-%d" tag i) :buffer nil :noquery t
                                :command (list "{sh}" "-c"
                                               (format ": > %s; exit 0"
                                                       (funcall marker tag)))
                                :sentinel sentinel)))
                      (a (funcall spawn "a"))
                      (b (funcall spawn "b")))
                 ;; A pure-Lisp spin runs no notification pass, so no sentinel
                 ;; can fire while it runs.  Spin until each child has said it
                 ;; is about to exit, so a loaded machine delays the spin rather
                 ;; than splitting the pass, then give both the exit itself.
                 (let ((deadline (+ (float-time) 20)))
                   (while (and (< (float-time) deadline)
                               (not (and (file-exists-p (funcall marker "a"))
                                         (file-exists-p (funcall marker "b")))))
                     nil)
                   (setq deadline (+ (float-time) 0.4))
                   (while (< (float-time) deadline) nil))
                 (accept-process-output nil 1)
                 (accept-process-output nil 1)
                 (ignore-errors (delete-process a))
                 (ignore-errors (delete-process b))
                 (push (mapconcat #'identity (nreverse order) "") orders)))
             (delete-directory dir t)
             (nreverse orders))"#
    ));

    assert_eq!(result, r#"OK ("ba" "ba" "ba" "ba" "ba" "ba")"#);
}

/// The readiness-wake path takes the poller's list, not the process list, so
/// its notification order is reconciled by permuting ONLY the entries that
/// have a status to report -- newest-first among themselves, every other
/// position left exactly where the poller put it.  That is GNU's split: the
/// output/filter walk in `wait_reading_process_output` is in fd order while
/// `status_notify` walks the alist (src/process.c:7885, :953).
#[test]
fn the_notification_walk_permutes_only_the_pending_entries() {
    crate::test_utils::init_test_tracing();
    let mut pm = ProcessManager::new();
    let ids: Vec<ProcessId> = (0..4)
        .map(|i| {
            pm.create_process_with_kind(
                format!("pw175-order-{i}").into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Pipe,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            )
        })
        .collect();
    for index in [0usize, 2] {
        pm.get_mut(ids[index])
            .expect("pipe process")
            .status_notify_pending = true;
    }

    let mut walk = ids.clone();
    super::order_pending_status_notifications_newest_first(&pm, &mut walk);

    assert_eq!(
        walk,
        vec![ids[2], ids[1], ids[0], ids[3]],
        "the two pending entries swap into newest-first order; the other two do not move"
    );

    // One pending process is already in GNU's order, so nothing moves.
    pm.get_mut(ids[2])
        .expect("pipe process")
        .status_notify_pending = false;
    let mut walk = ids.clone();
    super::order_pending_status_notifications_newest_first(&pm, &mut walk);
    assert_eq!(walk, ids);
}

/// `process-status`'s connection remapping is an `else if` chain, and its
/// FIRST arm is `exit -> closed` (src/process.c:1195-1196); the
/// `p->command == t` stop is the second (:1197-1198) and `run -> open` the
/// third (:1199-1200).  So a connection that has finished reports `closed`
/// however many times `stop-process` was called on it.
///
/// This port asked `command == t` first, which is invisible until something
/// sets `p->command' on a connection that has already closed -- which is
/// exactly what `stop-process` does, and what ledger 169 made this port start
/// doing on a retired connection the way GNU does (`Fstop_process` :7267-7278
/// has no liveness test at all).  It was the last divergent row of the
/// three-kind neighbour sweep.
///
/// Measured, GNU Emacs 31.0.90: `closed`.
#[test]
fn a_stopped_but_finished_connection_reports_closed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((obuf (generate-new-buffer " *pw169stop-out*"))
                  (ebuf (generate-new-buffer " *pw169stop-err*"))
                  (owner-done nil)
                  (proc (make-process
                         :name "pw169stop"
                         :buffer obuf
                         :command '("{sh}" "-c" "printf out; printf err 1>&2")
                         :stderr ebuf
                         :noquery t
                         :sentinel (lambda (_p _e) (setq owner-done t))))
                  (epipe (get-buffer-process ebuf)))
             (set-process-sentinel epipe #'ignore)
             (while (not owner-done) (accept-process-output nil 0.1))
             (dotimes (_ 10) (accept-process-output nil 0.05))
             (list :before (process-status epipe)
                   :after-stop (progn (stop-process epipe) (process-status epipe))
                   :after-continue (progn (continue-process epipe)
                                          (process-status epipe))))"#
    ));

    assert_eq!(
        result,
        "OK (:before closed :after-stop closed :after-continue closed)"
    );
}

/// GNU decodes NOTHING for a default filter with no live buffer, and the
/// skipped decode takes `last-coding-system-used', the process's own sticky
/// coding system and the `:post-read-conversion' hook with it.
///
/// `read_and_dispose_of_process_output` chooses between two branches
/// (src/process.c:6557-6559):
///
/// ```c
///   if (fast_read_process_output
///       && EQ (p->filter, Qinternal_default_process_filter))
///     read_and_insert_process_output (p, chars, nbytes, coding);
///   else
///     { decode_coding_c_string (...); ... }
/// ```
///
/// and `read_and_insert_process_output`'s first statement is
///
/// ```c
///   if (!nread || NILP (p->buffer) || !BUFFER_LIVE_P (XBUFFER (p->buffer)))
///     return;
/// ```
///
/// (:6464-6465).  Three disjuncts, one `if`, and it stands BEFORE
/// `decode_coding_c_string` (:6502) and before
/// `read_process_output_set_last_coding_system` (:6506).  Entry 166 closed the
/// `!nread` disjunct; the other two are this entry's, and they are not a
/// nicety: `read_process_output_set_last_coding_system` is the only writer of
/// `Vlast_coding_system_used` on this path (:6421) and the only writer of
/// `p->decode_coding_system` (:6425), so a decode that never runs cannot
/// report a coding system and cannot make one sticky.  That is why GNU loses
/// no `last-coding-system-used` semantics by skipping the decode: the variable
/// names the coding system the last CONVERSION used, and no conversion
/// happened.
///
/// The last five rows detach the buffer HALF WAY THROUGH, which is where the
/// two disjuncts differ from a process that never had one: the decoder is
/// already live, so a run that decoded anyway would report a coding system for
/// a process GNU leaves alone.  The child traps `SIGHUP` because `kill-buffer`
/// signals the buffer's process (`kill_buffer_processes`, src/buffer.c), and
/// these rows are about the READ rather than about the signal.  The reset of
/// `last-coding-system-used` deliberately comes AFTER `process-send-string`,
/// because the encode side writes it too and GNU's row would otherwise carry
/// the send's answer rather than the read's.
///
/// Each row is `(HOOK-CALLS LAST-CODING-SYSTEM-USED PROCESS-DECODE-CODING)`,
/// with the filter chunk count appended for the mid-stream rows, and
/// `last-coding-system-used` is pre-set to `pw171-untouched' so that "GNU
/// wrote nothing" is a value rather than the absence of one.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn a_default_filter_with_no_live_buffer_decodes_nothing_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defvar pw171-hooks 0)
             (defun pw171-prc (len) (setq pw171-hooks (1+ pw171-hooks)) len)
             (define-coding-system 'pw171-hook-utf8
               "utf-8 whose :post-read-conversion counts the decodes it is run for"
               :mnemonic ?U :coding-type 'utf-8 :charset-list '(unicode)
               :post-read-conversion #'pw171-prc)
             (defun pw171-run (buffer filterp conn coding script)
               (setq pw171-hooks 0 last-coding-system-used 'pw171-untouched)
               (let* ((acc nil)
                      (p (make-process :name "pw171" :buffer buffer :sentinel #'ignore
                                       :connection-type conn :noquery t
                                       :coding (cons coding 'utf-8-unix)
                                       :filter (when filterp (lambda (_p s) (push s acc)))
                                       :command (list "{sh}" "-c" script))))
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (prog1 (list pw171-hooks last-coding-system-used
                              (car (process-coding-system p)))
                   (when (buffer-live-p buffer) (kill-buffer buffer)))))
             ;; DETACH says how the process stops having a live buffer half way
             ;; through: nil is `(set-process-buffer p nil)', GNU's
             ;; `NILP (p->buffer)'; 'kill is `(kill-buffer BUF)', GNU's
             ;; `!BUFFER_LIVE_P (XBUFFER (p->buffer))'.
             (defun pw171-mid (detach filterp coding)
               (let* ((acc nil) (seen nil)
                      (buf (generate-new-buffer " *pw171m*"))
                      (p (make-process :name "pw171m" :buffer buf :sentinel #'ignore
                                       :connection-type 'pipe :noquery t
                                       :coding (cons coding 'utf-8-unix)
                                       :filter (when filterp
                                                 (lambda (_p s) (push s acc) (setq seen t)))
                                       :command
                                       (list "{sh}" "-c"
                                             "trap '' HUP; printf abc; read x; printf 'd\\303\\251f'"))))
                 (while (if filterp (not seen)
                          (zerop (with-current-buffer buf (buffer-size))))
                   (accept-process-output p 1))
                 (if (eq detach 'kill) (kill-buffer buf) (set-process-buffer p nil))
                 (process-send-string p "go\n")
                 (setq pw171-hooks 0 last-coding-system-used 'pw171-untouched acc nil)
                 (while (accept-process-output p 1))
                 (while (process-live-p p) (accept-process-output p 0.05))
                 (list pw171-hooks last-coding-system-used
                       (car (process-coding-system p)) (length acc))))
             (list
              ;; `NILP (p->buffer)': the default filter with no buffer at all.
              (pw171-run nil nil 'pipe 'pw171-hook-utf8 "printf abc")
              ;; `!BUFFER_LIVE_P' from the start.  `Fget_buffer_create' hands a
              ;; dead buffer object straight back -- "even if it is dead",
              ;; src/buffer.c:581-582 -- so `make-process' accepts it and this
              ;; row is a read rather than a signal.
              (pw171-run (let ((b (generate-new-buffer " *pw171d*"))) (kill-buffer b) b)
                         nil 'pipe 'pw171-hook-utf8 "printf abc")
              ;; the same on a pty, where there is no last block to confuse it
              (pw171-run nil nil 'pty 'pw171-hook-utf8 "printf abc")
              ;; control: a LIVE buffer on the same branch decodes the data
              ;; read -- and only that one, because entry 166's `!nread'
              ;; disjunct still holds for the zero-byte last block.
              (pw171-run (generate-new-buffer " *pw171a*") nil 'pipe 'pw171-hook-utf8 "printf abc")
              ;; control: a Lisp filter has no buffer either and decodes BOTH
              ;; reads, because it is the other branch entirely.
              (pw171-run nil t 'pipe 'pw171-hook-utf8 "printf abc")
              ;; `fast-read-process-output' nil sends the DEFAULT filter down
              ;; the filter branch, so the very same process decodes.  Both
              ;; conjuncts of :6557-6558, not just the filter one.
              (let ((fast-read-process-output nil))
                (pw171-run nil nil 'pipe 'pw171-hook-utf8 "printf abc"))
              ;; the sticky rewrite the skipped decode also skips: `undecided'
              ;; over UTF-8 bytes resolves to `utf-8-unix' on the filter branch
              ;; and stays `undecided' when nothing decoded it.
              (pw171-run nil nil 'pipe 'undecided "printf 'a\\303\\251\\n'")
              (pw171-run nil t 'pipe 'undecided "printf 'a\\303\\251\\n'")
              ;; the same two disjuncts reached MID-STREAM, with a decoder that
              ;; has already run once and a character split across the read.
              (pw171-mid nil nil 'pw171-hook-utf8)
              (pw171-mid 'kill nil 'pw171-hook-utf8)
              (pw171-mid nil t 'pw171-hook-utf8)
              (pw171-mid nil nil 'undecided)
              (pw171-mid nil t 'undecided)))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(0 pw171-untouched pw171-hook-utf8) ",
            "(0 pw171-untouched pw171-hook-utf8) ",
            "(0 pw171-untouched pw171-hook-utf8) ",
            "(1 pw171-hook-utf8 pw171-hook-utf8) ",
            "(2 pw171-hook-utf8 pw171-hook-utf8) ",
            "(2 pw171-hook-utf8 pw171-hook-utf8) ",
            "(0 pw171-untouched undecided) ",
            "(0 utf-8-unix utf-8-unix) ",
            "(0 pw171-untouched pw171-hook-utf8 0) ",
            "(0 pw171-untouched pw171-hook-utf8 0) ",
            "(2 pw171-hook-utf8 pw171-hook-utf8 1) ",
            "(0 pw171-untouched undecided 0) ",
            "(0 utf-8 utf-8 1))",
        )
    );
}

/// A process may hold a buffer that is not live, and every neighbour of the
/// read has to say so the way GNU does.
///
/// GNU never refuses the state and guards it at each use instead, which is why
/// there are three `BUFFER_LIVE_P` tests downstream of one
/// `Fget_buffer_create`: `read_and_insert_process_output` (src/process.c:6464),
/// `internal-default-process-sentinel`, whose own comment is "Avoid error if
/// buffer is deleted (probably that's why the process is dead, too)"
/// (:7969-7971), and `setup_process_coding_systems` (:8395).  The three doors
/// IN are as deliberate.  `Fget_buffer_create` returns a buffer object "as
/// given, even if it is dead" (src/buffer.c:581-582), and all four process
/// constructors go through it (:1849-1851, :3091-3094, :3223-3226, :4017).
/// `Fset_process_buffer`'s only check is `CHECK_BUFFER`, which is
/// `CHECK_TYPE (BUFFERP (x), Qbufferp, x)` and nothing else (:1302-1303,
/// src/buffer.h:762-766).  And `Fget_buffer_process` looks its argument up
/// with `Fget_buffer` (:8422), which hands a buffer object straight back -- so
/// a dead buffer still finds its process -- and answers a nil argument with
/// `Qnil` outright (:8421) rather than reaching for the selected window.
///
/// `get_process` (:1045-1048) is the one place that DOES error, with "Attempt
/// to get process for a dead buffer", and it is a PROCESS designator rather
/// than a buffer one; the last row pins that difference so the next reader
/// does not unify them.
///
/// Measured under GNU Emacs 31.0.90 running this test's own program.
#[test]
fn a_process_may_hold_a_buffer_that_is_not_live_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(progn
             (defun pw171n-dead (tag)
               (let ((b (generate-new-buffer (format " *pw171n%s*" tag))))
                 (kill-buffer b) b))
             (list
              ;; make-process accepts it, and the process runs to completion.
              (let* ((b (pw171n-dead "a"))
                     (p (make-process :name "pw171n-a" :buffer b :noquery t
                                      :sentinel #'ignore
                                      :command (list "{sh}" "-c" "printf ab"))))
                (while (process-live-p p) (accept-process-output p 0.05))
                (list (processp p) (eq (process-buffer p) b)
                      (buffer-live-p (process-buffer p))
                      (marker-buffer (process-mark p))
                      (process-status p)))
              ;; make-pipe-process too, which is the other constructor a
              ;; :stderr pipe goes through.
              (let* ((p (make-pipe-process :name "pw171n-b" :buffer (pw171n-dead "b")
                                           :noquery t)))
                (prog1 (list (processp p) (buffer-live-p (process-buffer p))
                             (process-status p))
                  (delete-process p)))
              ;; the DEFAULT sentinel, which must not signal when it cannot
              ;; insert its status message.
              (let* ((p (make-process :name "pw171n-c" :buffer (pw171n-dead "c") :noquery t
                                      :command (list "{sh}" "-c" "printf ab"))))
                (while (process-live-p p) (accept-process-output p 0.05))
                (dotimes (_ 5) (accept-process-output nil 0.02))
                (list (process-status p)))
              ;; `internal-default-process-filter' called by hand answers nil
              ;; and inserts nowhere.
              (let* ((p (make-process :name "pw171n-d" :buffer (pw171n-dead "d") :noquery t
                                      :sentinel #'ignore
                                      :command (list "{sh}" "-c" "sleep 5"))))
                (prog1 (list (internal-default-process-filter p "hi"))
                  (delete-process p)))
              ;; set-process-buffer accepts one, and returns it.
              (let* ((b (pw171n-dead "e"))
                     (p (make-process :name "pw171n-e" :buffer nil :noquery t
                                      :sentinel #'ignore
                                      :command (list "{sh}" "-c" "sleep 5"))))
                (prog1 (list (eq (set-process-buffer p b) b)
                             (buffer-live-p (process-buffer p)))
                  (delete-process p)))
              ;; get-buffer-process finds a process by its DEAD buffer, and
              ;; answers nil for a nil argument even when the selected window
              ;; shows a buffer that has one.
              (let* ((b (pw171n-dead "f"))
                     (p (make-process :name "pw171n-f" :buffer b :noquery t
                                      :sentinel #'ignore
                                      :command (list "{sh}" "-c" "sleep 5")))
                     (q (make-process :name "pw171n-g" :buffer (current-buffer) :noquery t
                                      :sentinel #'ignore
                                      :command (list "{sh}" "-c" "sleep 5"))))
                (prog1 (list (eq (get-buffer-process b) p)
                             (get-buffer-process nil)
                             (eq (get-buffer-process (current-buffer)) q))
                  (delete-process p) (delete-process q)))
              ;; and the one that DOES error, because it is `get_process':
              ;; a dead buffer as a PROCESS designator.
              (let ((b (pw171n-dead "h")))
                (list (condition-case e (process-status b) (error (cadr e)))
                      (condition-case e (delete-process b) (error (cadr e)))))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK (",
            "(t t nil nil exit) ",
            "(t nil open) ",
            "(exit) ",
            "(nil) ",
            "(t nil) ",
            "(t nil t) ",
            "(\"Attempt to get process for a dead buffer\" ",
            "\"Attempt to get process for a dead buffer\"))",
        )
    );
}

/// GNU records a changed child status ASYNCHRONOUSLY, and since ledger 193
/// so does this port -- the divergence, closed.
///
/// `handle_child_signal` (src/process.c:7691) is GNU's SIGCHLD handler.  It
/// walks `FOR_EACH_PROCESS` itself (:7734) and, for every process that is
/// `p->alive` and whose `child_status_changed (p->pid, &status,
/// WUNTRACED | WCONTINUED)` reports a transition (:7741-7742), stamps
/// `p->tick`, `p->raw_status` and `p->raw_status_new` (:7745-7747).  Its own
/// header states the contract: *"All we do is change the status; we do not
/// run sentinels or print notifications"* (:7669-7671).  `Fprocess_status`
/// (:1188-1189) and `Fprocess_exit_status` (:1212-1213) then decode that
/// record with `update_status`, and `process-live-p` is
/// `(memq (process-status process) '(run open listen connect stop))`
/// (lisp/subr.el:3538-3540).
///
/// Measured, `-Q --batch`, GNU Emacs 31.0.90 against this port, with the
/// wait taken out of the probe entirely -- the spin is `float-time` and
/// `process-attributes`, never `accept-process-output`:
///
/// ```text
///                           GNU 31.0.90   this port
///   (process-status p)         exit       run
///   (process-exit-status p)    7          0
///   (process-live-p p)         nil        (run open listen connect stop)
///   (get-process "...")        t          t
///   (memq p (process-list))    t          t
/// ```
///
/// The last two agree and must keep agreeing: `handle_child_signal` never
/// rewrites `Vprocess_alist`.  Only `remove_process` (:957-966) does, and
/// `status_notify` (:7926-7927) is what calls it -- which has not run here.
///
/// **This is a PINNED DIVERGENCE, and it was briefly not one.**  Ledger 193
/// closed it by draining the record at `Context::maybe_quit`, and ledger 198
/// reverses that: GNU's `process_pending_signals` is
/// `pending_signals = false; handle_async_input (); do_pending_atimers ();`
/// (src/keyboard.c:8367-8372) and notifies nothing -- `grep -c status_notify`
/// over it is 0 -- while GNU's SIGCHLD never sets `pending_signals` in the
/// first place (`grep -n 'pending_signals = ' src/*.c` has no hit in
/// `process.c`).  All five `status_notify` calls are subrs that wrote the
/// status themselves (:1129, :1149, :7181) or the wait (:5554, :5854), so
/// **every status GNU discovers asynchronously is notified from the wait.**
///
/// Recording at `maybe_quit` therefore published a status with its sentinel
/// unrun, and programs that bind a variable around their await loop broke:
/// `magit-run-post-commit-hook` is keyed on `last-command`, and
/// `treemacs_magit_package_batch`'s
/// `extending_a_real_commit_schedules_the_same_project_refresh` failed with
/// `(error "Treemacs-Magit idle update was not scheduled")` -- exactly what
/// ledger 180 measured when it declined the synchronous sweep.
/// `a_sentinel_runs_inside_the_wait_while_the_callers_let_is_live_like_gnu`
/// is the probe: GNU 294/294, this port 261/300 with the drain at
/// `maybe_quit`.
///
/// So the record went where GNU keeps it -- inside
/// `wait_reading_process_output` -- and this row went back to being a
/// divergence.  **The port cannot have both**, because the only safe point
/// that closes it is GNU's own (the handler), and a Rust port cannot walk a
/// `HashMap` owned by the Lisp thread there.  Ledger 187 §8.1(b) asked for "a
/// safe point that is not a Lisp observation"; GNU has none either.
///
/// The second half of the pin is what keeps it from hiding a total breakage:
/// after the spin, ONE `accept-process-output` must produce GNU's answer.
/// A pin that only asserted the stale column would pass just as happily if
/// child statuses stopped being recorded at all.
///
/// **The control is the child's own last act**: it touches a marker file
/// immediately before exiting, and `child-ran` is the evidence.  A run in
/// which nothing had started would fail there rather than agreeing with
/// itself.
#[test]
fn a_child_that_exited_with_nobody_waiting_is_still_run_here_and_exit_in_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    // Under the repo's own `target/`, never /tmp (this project's
    // standing rule), and named per pin so two tests cannot collide.
    let marker_dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("target/pw193");
    std::fs::create_dir_all(&marker_dir).expect("marker dir");
    let marker = marker_dir
        .join("pw193dead.marker")
        .to_string_lossy()
        .into_owned();
    let _ = std::fs::remove_file(&marker);
    let result = eval_one(&format!(
        r#"(let* ((marker "{marker}")
                  (p (make-process
                      :name "pw180dead" :noquery t :connection-type 'pipe
                      :buffer (generate-new-buffer " *pw180dead*")
                      :command '("{sh}" "-c" ": > {marker}; exit 7")))
                  (deadline (+ (float-time) 20)))
             ;; A pure-Lisp spin: `wait_reading_process_output' is never
             ;; entered, so only an asynchronous recording could see it.
             (while (and (not (file-exists-p marker)) (< (float-time) deadline)) nil)
             (let ((ran (file-exists-p marker))
                   (spin (+ (float-time) 0.2)))
               ;; Spin past every safe point an ordinary program reaches, so
               ;; "nothing recorded it" is a measurement and not a race.
               (while (< (float-time) spin) nil)
               (let ((before
                      (list (cons 'child-ran ran)
                            (cons 'status (process-status p))
                            (cons 'exit-status (process-exit-status p))
                            (cons 'live-p (and (process-live-p p) t))
                            (cons 'get-process (and (get-process "pw180dead") t))
                            (cons 'in-process-list (and (memq p (process-list)) t)))))
                 ;; ...and now enter GNU's own safe point for this, ONCE.
                 (accept-process-output nil 0.5)
                 (list (cons 'before before)
                       (cons 'after-one-wait
                             (list (cons 'status (process-status p))
                                   (cons 'exit-status
                                         (process-exit-status p))))))))"#
    ));
    let _ = std::fs::remove_file(&marker);

    assert_eq!(
        result,
        "OK ((before (child-ran . t) (status . run) (exit-status . 0) (live-p . t) \
         (get-process . t) (in-process-list . t)) \
         (after-one-wait (status . exit) (exit-status . 7)))"
    );
}

/// The REAPING half of the same divergence, and it is PINNED with it: GNU's
/// handler calls `waitpid`, so GNU's exited child is gone from the OS and
/// this port's is a zombie until something waits (ledger 184's rows 2 and 3).
///
/// `handle_child_signal` reaches the OS through `child_status_changed
/// (p->pid, &status, WUNTRACED | WCONTINUED)` (src/process.c:7741-7742), and
/// `child_status_changed` is `waitpid` -- so the record and the REAP happen
/// in the same call, microseconds after the child dies and with nobody
/// having waited.  Two Lisp answers see that and neither is an
/// `update_status` consumer, which is why ledger 180's wired sweep left both
/// of them divergent (`divergent=2` of 27 rows):
///
/// ```text
///                                                   GNU 31.0.90   this port
///   (cdr (assq 'state (process-attributes pid)))       nil          "Z"
///   (signal-process p 0)                               -1           0
/// ```
///
/// GNU answers `nil` because `/proc` has forgotten the pid, and `-1` because
/// `kill (pid, 0)` fails `ESRCH`.  This port answers `"Z"` and `0` because
/// the child is a zombie: reaped by nothing, so still a process to the
/// kernel.  3 runs each, `-Q --batch`, GNU Emacs 31.0.90 against
/// `target/release/neomacs`, byte-identical on both sides.
///
/// **Why it is still open, after 180, 184, 187 and 193 each took a turn.**
/// 180 §9.1 priced it as an owner problem, and 187 paid that price: `waitpid`
/// has exactly one owner and the compiler enforces it.  193 then closed the
/// rows by reaping at `Context::maybe_quit` -- and 198 reversed that, because
/// the same drain published a status with its sentinel unrun and broke
/// `treemacs-magit`; see the sibling pin for the citations and the 294/294
/// against 261/300.
///
/// The reap and the record are the same call here (`child_status_changed` is
/// `waitpid`), so moving the record to GNU's site moved the reap with it.
/// **Separating them is possible and is NOT attempted**: reaping at
/// `maybe_quit` while withholding the status from Lisp until a wait notifies
/// it would keep these two rows and invent a state GNU does not have -- a
/// record `update_status` refuses to decode (GNU's :717-721 always decodes).
/// It is written down in ledger 198's "Found and NOT fixed" with that cost
/// rather than built here.
///
/// The second half of the pin is the same guard the sibling carries: after
/// the spin, ONE `accept-process-output` must reap the child, so the pin
/// cannot pass by reaping never happening at all.
///
/// **The control is the child's own last act**: `child-ran` is `t` because
/// the child touched a marker file immediately before exiting.
#[test]
fn an_exited_child_is_a_zombie_here_and_reaped_in_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    // Under the repo's own `target/`, never /tmp (this project's
    // standing rule), and named per pin so two tests cannot collide.
    let marker_dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("target/pw193");
    std::fs::create_dir_all(&marker_dir).expect("marker dir");
    let marker = marker_dir
        .join("pw193reap.marker")
        .to_string_lossy()
        .into_owned();
    let _ = std::fs::remove_file(&marker);
    let result = eval_one(&format!(
        r#"(let* ((marker "{marker}")
                  (p (make-process
                      :name "pw184reap" :noquery t :connection-type 'pipe
                      :buffer (generate-new-buffer " *pw184reap*")
                      :command '("{sh}" "-c" ": > {marker}; exit 7")))
                  (pid (process-id p))
                  (deadline (+ (float-time) 20)))
             ;; A pure-Lisp spin: nothing here waits, so nothing reaps.
             (while (and (not (file-exists-p marker)) (< (float-time) deadline)) nil)
             (let ((ran (file-exists-p marker))
                   (spin (+ (float-time) 0.2)))
               (while (< (float-time) spin) nil)
               (let ((before
                      (list (cons 'child-ran ran)
                            (cons 'attrs-state
                                  (cdr (assq 'state (process-attributes pid))))
                            (cons 'signal-0 (signal-process p 0)))))
                 ;; ...and now enter GNU's own safe point for this, ONCE.
                 (accept-process-output nil 0.5)
                 (list (cons 'before before)
                       (cons 'after-one-wait
                             (list (cons 'attrs-state
                                         (cdr (assq 'state
                                                    (process-attributes pid))))))))))"#
    ));
    let _ = std::fs::remove_file(&marker);

    assert_eq!(
        result,
        "OK ((before (child-ran . t) (attrs-state . \"Z\") (signal-0 . 0)) \
         (after-one-wait (attrs-state)))"
    );
}

/// GNU's sweep, transcribed, exercised directly -- and the pidless process
/// it cannot reach.
///
/// [`ProcessManager::record_child_status_changes`] is
/// `handle_child_signal`'s `FOR_EACH_PROCESS` arm (src/process.c:7734-7763)
/// as a function this port can call.  It is not wired to any Lisp entry
/// point (see `UpdateStatusSite::recording` and the pin above), so this test
/// is what keeps it honest: it drives a `ProcessManager` with no wait loop
/// at all, waits until `/proc` says the child is a ZOMBIE -- exited and not
/// reaped -- and asserts that the sweep, and only the sweep, turns `run`
/// into `(exit . 7)`.
///
/// The second half is ledger 165's inversion as a compile-time fact rather
/// than a comment: `handle_child_signal` passes `p->pid` to
/// `child_status_changed` and `get_child_status` opens with
/// `eassert (child > 0)` (src/sysdep.c:462), so a pipe connection is not in
/// the population.  [`SweepableChild::of`] is that membership test and there
/// is no other constructor, so the sweep cannot touch one.
#[cfg(unix)]
#[test]
fn gnus_sigchld_sweep_records_an_exited_child_and_cannot_reach_a_pidless_process() {
    use crate::emacs_core::process::child_status::SweepableChild;

    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut processes = ProcessManager::new();
    let child = processes.create_process_lisp(
        LispString::from_utf8("pw180-sweep"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![LispString::from_utf8("-c"), LispString::from_utf8("exit 7")],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(child, false).expect("spawn child");
    let os_pid = processes
        .get(child)
        .and_then(|proc| proc.os_pid)
        .expect("a spawned child has an OS pid");

    // A pipe process: no child, no pid, so not a member of the population.
    let pipe = processes.create_process_with_kind_lisp(
        LispString::from_utf8("pw180-sweep-pipe"),
        Value::NIL,
        LispString::from_utf8(""),
        Vec::new(),
        crate::emacs_core::process::ProcessKindWithoutDevice::Pipe,
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );

    // Wait for the kernel, not for the clock: a zombie is exited-and-unreaped,
    // which is the exact state GNU's handler would have found.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let is_zombie = |pid: u32| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                // /proc/PID/stat field 3 is the state letter; the comm field
                // may contain spaces and parens, so scan past its last ')'.
                let after_comm = stat.rfind(')').map(|i| i + 1)?;
                stat[after_comm..]
                    .split_whitespace()
                    .next()
                    .map(str::to_string)
            })
            .is_some_and(|state| state == "Z")
    };
    while std::time::Instant::now() < deadline && !is_zombie(os_pid) {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        is_zombie(os_pid),
        "the child must be exited and unreaped before the sweep runs"
    );

    let before = process_effective_status(processes.get(child).expect("child is live"));
    assert_eq!(
        ProcessStatusSymbol::from_status_value(before),
        Some(ProcessStatusSymbol::Run),
        "nothing but the sweep may have observed the exit"
    );
    assert_eq!(
        SweepableChild::of(pipe, processes.get(pipe).expect("pipe is live")),
        None,
        "a pidless process is not in handle_child_signal's population"
    );

    let _recorded = processes.record_child_status_changes(
        crate::emacs_core::wait::WaitStatusNotifySite::for_a_unit_test_of_the_walk_itself(),
    );

    let after = process_effective_status(processes.get(child).expect("child is still listed"));
    assert_eq!(
        ProcessStatusSymbol::from_status_value(after),
        Some(ProcessStatusSymbol::Exit)
    );
    assert_eq!(process_status_code_value(after), 7);
    // GNU's handler does not remove anything from `Vprocess_alist`
    // (:7734-7763 never touches it), so the process is still listed.
    assert!(processes.list_processes().contains(&child));
    // And the pipe is untouched.
    assert_eq!(
        ProcessStatusSymbol::from_status_value(process_effective_status(
            processes.get(pipe).expect("pipe is still live")
        )),
        Some(ProcessStatusSymbol::Run)
    );
}

/// GNU reaps a child exactly once, and never signals a pid it has reaped.
///
/// The rule is stated in `src/process.c:1080-1088`, in the comment that
/// enumerates the three places the main thread records child processes:
///
/// ```text
///    The main Emacs thread invokes waitpid only on child processes that
///    it creates and that have not been reaped.  This avoid races on
///    platforms such as GTK, where other threads create their own
///    subprocesses which the main thread should not reap.  For example,
///    if the main thread attempted to reap an already-reaped child, it
///    might inadvertently reap a GTK-created process that happened to
///    have the same process ID.
/// ```
///
/// and GNU enforces the "creates" half structurally: `waitpid` appears in
/// exactly ONE place in the whole POSIX build -- `get_child_status`,
/// `static` in src/sysdep.c:471 -- reached only through
/// `child_status_changed` and `wait_for_termination`, and it opens with
/// `eassert (child > 0)` (:462).
///
/// This port breaks the "not been reaped" half.  `sys::poll_child_status`
/// is a `waitpid (WNOHANG)` that reaps, and `Process::os_pid` keeps the pid
/// afterwards -- so `deliver_process_signal` (process.rs:10349) will still
/// hand that pid to `kill(2)`, and `LiveProcessIo::terminate_and_reap_children`
/// (:1069) will still hand it to `kill(-pid, SIGKILL)` and to `Child::wait`.
/// A recycled pid is then somebody else's process group, which is the exact
/// race GNU's comment describes.
///
/// The assertion is on the ONE fact that makes all of those unreachable:
/// after a reap there must be no pid left to spell.
#[cfg(unix)]
#[test]
fn a_reaped_child_leaves_no_pid_for_the_kill_path_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let mut processes = ProcessManager::new();
    let child = processes.create_process_lisp(
        LispString::from_utf8("pw187-reaped"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![LispString::from_utf8("-c"), LispString::from_utf8("exit 7")],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(child, false).expect("spawn child");

    let before = pw187_kill_path_pid(&processes, child);
    assert!(
        before.is_some(),
        "a live child must have a pid the kill path can use"
    );
    let os_pid = before.expect("checked");

    // Wait for the kernel, not the clock: a zombie is exited-and-unreaped,
    // so the reap below is this port's FIRST and the state is unambiguous.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !pw187_is_zombie(os_pid) {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        pw187_is_zombie(os_pid),
        "the child must be exited and unreaped before anything reaps it"
    );

    // GNU's `handle_child_signal` body: `child_status_changed` is `waitpid`,
    // so this REAPS.  After it, the pid belongs to the OS again.
    let _recorded = processes.record_child_status_changes(
        crate::emacs_core::wait::WaitStatusNotifySite::for_a_unit_test_of_the_walk_itself(),
    );
    assert!(
        !pw187_is_zombie(os_pid),
        "record_child_status_changes must have reaped the child"
    );

    assert_eq!(
        pw187_kill_path_pid(&processes, child),
        None,
        "this port still holds the pid of a child it has reaped, so \
         deliver_process_signal and terminate_and_reap_children will pass it \
         to kill(2) -- GNU: \"The main Emacs thread invokes waitpid only on \
         child processes that it creates and that have not been reaped\" \
         (src/process.c:1080-1083)"
    );
}

/// GNU does not SIGKILL a process group whose leader it has already reaped,
/// and this port did -- on the ordinary path, with no user action.
///
/// `Fdelete_process` reaches the kill through one gate:
///
/// ```c
///       if (p->alive)
///         record_kill_process (p, Qnil);       /* src/process.c:1135 */
/// ```
///
/// and `record_kill_process` is
///
/// ```c
///   if (p->alive)
///     {
///       record_deleted_pid (p->pid, tempfile);
///       p->alive = 0;
///       kill (- p->pid, SIGKILL);              /* src/callproc.c:202-207 */
///     }
/// ```
///
/// `p->alive` is cleared by `handle_child_signal` at the instant it reaps
/// (src/process.c:7752), so once the child is gone GNU sends nothing.  This
/// port's guard was a fresh `child.try_wait()`, whose answer for a child
/// `sys::poll_child_status` had already reaped is `Err(ECHILD)` -- read as
/// "still running", and followed by `kill(-pid, SIGKILL)`.
///
/// The pid of a reaped child belongs to the OS again, so that signal goes to
/// whatever process group now holds it; and until it is recycled it goes to
/// the child's own group, which is where the child's surviving GRANDCHILDREN
/// are.  That is the Lisp-visible half, and it reproduces:
/// `-Q --batch`, a child that leaves a background grandchild and exits at
/// once, `accept-process-output`, `delete-process`, then a pure-Lisp spin
/// past the grandchild's sleep, 3 runs each:
///
/// ```text
///   :connection-type          GNU 31.0.90     this port, before
///   pipe   grandchild-survived      t                nil
///   pty    grandchild-survived     nil               nil
/// ```
///
/// The pty row agrees for a reason that is not this bug: closing the pty
/// master hangs up the foreground process group, so GNU kills it too.
#[cfg(unix)]
#[test]
fn deleting_a_reaped_child_does_not_kill_its_process_group_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let marker = tmp_file("pw187-orphan");
    let _ = std::fs::remove_file(&marker);

    let mut processes = ProcessManager::new();
    let child = processes.create_process_lisp(
        LispString::from_utf8("pw187-orphan"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![
            LispString::from_utf8("-c"),
            // The grandchild outlives its parent, in its parent's process
            // group.  The parent exits at once.
            LispString::from_utf8(&format!("sh -c 'sleep 2; : > {marker}' & exit 0")),
        ],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    processes.spawn_child(child, false).expect("spawn child");
    let os_pid = processes
        .get(child)
        .and_then(|proc| proc.os_pid)
        .expect("a spawned child has an OS pid");

    // Reap it the way the wait loop does.  This is where `p->alive` goes 0.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        if processes.check_child_status_change(child) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // The control: asked of the KERNEL, so it reads the same in a tree with
    // this fix and in one without it.  A run whose child had not been reaped
    // would agree with itself.
    assert!(
        !pw187_is_zombie(os_pid),
        "the child must be REAPED before the delete path runs, or this pin \
         proves nothing"
    );

    // GNU's `Fdelete_process`: `if (p->alive) record_kill_process (p, Qnil);`
    processes.delete_process(child);

    // Past the grandchild's sleep.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !std::path::Path::new(&marker).exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let survived = std::path::Path::new(&marker).exists();
    let _ = std::fs::remove_file(&marker);
    assert!(
        survived,
        "the grandchild was killed, so delete-process signalled the process \
         group of a pid this port had already reaped -- GNU's \
         record_kill_process sends nothing once p->alive is 0 \
         (src/callproc.c:202-207)"
    );
}

/// The pid this port would hand to `kill(2)` for `id`, or `None` when it has
/// none.  One function so the invariant above survives the spelling of the
/// field it reads.
#[cfg(unix)]
fn pw187_kill_path_pid(
    processes: &ProcessManager,
    id: crate::emacs_core::process::ProcessId,
) -> Option<u32> {
    processes
        .get(id)
        .and_then(|proc| proc.live_io.child.pid_if_unreaped())
}

#[cfg(unix)]
fn pw187_is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            let after_comm = stat.rfind(')').map(|i| i + 1)?;
            stat[after_comm..]
                .split_whitespace()
                .next()
                .map(str::to_string)
        })
        .is_some_and(|state| state == "Z")
}

/// GNU's `update_status` call sites, enumerated, and how each one gets its
/// record.
///
/// The population is mechanical -- `grep -n 'update_status ('
/// src/process.c` gives the definition at :717 and exactly eight call sites
/// -- so it can be a closed type instead of a habit, and this pin is what
/// stops the type from drifting away from the grep.  It asserts ABSOLUTE
/// counts, so a table that lost its rows fails rather than passing over an
/// empty set (ledger 173's law, and 177's table).
///
/// The 4/4 split is the entry's whole finding in one number: four of GNU's
/// eight sites are inside the wait/notification machinery, which this port
/// has too, and four are Lisp entry points that answer from a record GNU's
/// SIGCHLD handler made asynchronously -- which since ledger 193 this port
/// makes asynchronously too, from `HandledSignal::Sigchld`'s drain arm.
/// `Recording::AsynchronouslyRecorded` names both sides, and NEITHER group
/// sweeps at the site: a sweep at the observation is what ledger 180
/// measured and withdrew.
#[test]
fn gnus_eight_update_status_sites_are_enumerated_and_four_are_the_asynchronous_ones() {
    use crate::emacs_core::process::child_status::Recording;
    use crate::emacs_core::process::{UnrecordedStatusRead, UpdateStatusSite};

    assert_eq!(UpdateStatusSite::COUNT, 8);
    assert_eq!(UpdateStatusSite::ALL.len(), 8);

    let mut seen = std::collections::BTreeSet::new();
    let mut asynchronous = Vec::new();
    let mut already = Vec::new();
    for site in UpdateStatusSite::ALL {
        assert!(seen.insert(site.gnu()), "duplicate GNU citation: {site:?}");
        assert!(
            site.gnu().starts_with("src/process.c:"),
            "{site:?} must cite the C line it ports"
        );
        match site.recording() {
            Recording::AsynchronouslyRecorded { by, here } => {
                assert!(!by.is_empty() && !here.is_empty());
                asynchronous.push(site);
            }
            Recording::AlreadyRecorded { by } => {
                assert!(
                    !by.is_empty(),
                    "{site:?} must say where the record was made"
                );
                already.push(site);
            }
        }
    }
    assert_eq!(
        asynchronous.len(),
        4,
        "asynchronous sites: {asynchronous:?}"
    );
    assert_eq!(already.len(), 4, "already-recorded sites: {already:?}");
    assert_eq!(
        asynchronous,
        vec![
            UpdateStatusSite::ProcessStatus,
            UpdateStatusSite::ProcessExitStatus,
            UpdateStatusSite::SendProcess,
            UpdateStatusSite::ProcessSendEof,
        ]
    );

    // The GNU lines, spelled out, so a renumbered citation is a failing test
    // and not a stale comment.
    assert_eq!(UpdateStatusSite::ProcessStatus.gnu(), "src/process.c:1189");
    assert_eq!(
        UpdateStatusSite::ProcessExitStatus.gnu(),
        "src/process.c:1213"
    );
    assert_eq!(UpdateStatusSite::SendProcess.gnu(), "src/process.c:6726");
    assert_eq!(UpdateStatusSite::ProcessSendEof.gnu(), "src/process.c:7453");
    assert_eq!(UpdateStatusSite::DeleteProcess.gnu(), "src/process.c:1143");
    assert_eq!(UpdateStatusSite::StatusNotify.gnu(), "src/process.c:7915");

    // And the enumerated holes: Lisp-visible status reads this port cannot
    // route through `observe` at all.  One, with its reason.
    assert_eq!(UnrecordedStatusRead::COUNT, 1);
    assert_eq!(UnrecordedStatusRead::ALL.len(), 1);
    for hole in UnrecordedStatusRead::ALL {
        assert!(hole.gnu().starts_with("src/"), "{hole:?} needs a citation");
        assert!(!hole.why().is_empty(), "{hole:?} needs a reason");
    }
    assert_eq!(
        UnrecordedStatusRead::ModeLinePercentS.gnu(),
        "src/xdisp.c:29723"
    );
}

/// GNU's EIGHT `p->tick = ++process_tick;` sites, as a table.
///
/// `grep -n 'tick = ++process_tick' src/process.c` returns exactly eight
/// lines on GNU master (verified at master snapshot 98165ff73e8, 2026-08-30:
/// lines 1169, 1189, 6075, 6092, 6101, 6158, 7193, 7752), and **seven of
/// them are not `handle_child_signal`'s** -- which is the whole point of
/// [`StatusChangeSite`].  The table used to carry a NINTH row for an EPIPE
/// tick in `send_process` at ":6927", and that line is still real on the
/// release lineage: emacs-31.1 (a360712c9d, 2026-08-24) has nine sites, the
/// ninth being the `(exit . 256)` arm GNU commit 4d7e6e51dd4 introduced in
/// 2012.  Master commit e381cf1fc97 (2025-08-15) removed it, and that is the
/// arm this port follows: it closes the write fd and touches neither status
/// nor tick.  The eight citations below use emacs-31.1's line numbers for
/// the sites both lineages share, spelled out here so a renumbered line is a
/// failing test rather than a stale comment, and the count is asserted so a
/// ninth site cannot appear without one.
#[test]
fn the_status_change_sites_are_gnus_eight_tick_bumps() {
    use crate::emacs_core::process::StatusChangeSite;
    use crate::emacs_core::process::child_status::{StatusChangeNotifier, StatusChangeRecorder};

    assert_eq!(StatusChangeSite::COUNT, 8);
    assert_eq!(StatusChangeSite::ALL.len(), 8);

    let cited: Vec<&'static str> = StatusChangeSite::ALL.iter().map(|s| s.gnu()).collect();
    assert_eq!(
        cited,
        vec![
            "src/process.c:1128",
            "src/process.c:1148",
            "src/process.c:6058",
            "src/process.c:6075",
            "src/process.c:6084",
            "src/process.c:6141",
            "src/process.c:7178",
            "src/process.c:7746",
        ]
    );

    for site in StatusChangeSite::ALL {
        assert!(
            site.gnu().starts_with("src/process.c:"),
            "{site:?} needs a citation"
        );
        assert!(!site.what().is_empty(), "{site:?} needs a status");
    }

    // GNU consumes four of the eight on the spot -- `status_notify` is called
    // within a few lines of the bump -- and leaves the rest for the wait's own
    // `status_notify` at :5554 / :5854.
    let synchronous: Vec<StatusChangeSite> = StatusChangeSite::ALL
        .into_iter()
        .filter(|site| {
            matches!(
                site.notifier(),
                StatusChangeNotifier::SynchronouslyAtTheSite { .. }
            )
        })
        .collect();
    assert_eq!(
        synchronous,
        vec![
            StatusChangeSite::DeleteProcessConnection,
            StatusChangeSite::DeleteProcessChild,
            StatusChangeSite::NonBlockingConnectFailed,
            StatusChangeSite::ProcessSendSignalSigcont,
        ]
    );

    // And exactly one of the eight has no analogue in this port, with a reason.
    let without: Vec<StatusChangeSite> = StatusChangeSite::ALL
        .into_iter()
        .filter(|site| matches!(site.recorder(), StatusChangeRecorder::NoAnalogue { .. }))
        .collect();
    assert_eq!(without, vec![StatusChangeSite::PtyEioBeforeFork]);
    for site in StatusChangeSite::ALL {
        match site.recorder() {
            StatusChangeRecorder::Here(where_) => {
                assert!(!where_.is_empty(), "{site:?} needs a recording site")
            }
            StatusChangeRecorder::NoAnalogue { why } => {
                assert!(!why.is_empty(), "{site:?} needs a reason")
            }
        }
    }
}

/// `process-send-eof` on a process that has finished but is still listed.
///
/// GNU's `Fprocess_send_eof` gate is unconditional: `update_status`, then
/// `if (! EQ (p->status, Qrun)) error ("Process %s not running: %s", ...)`
/// (src/process.c:7451-7455), reached for every kind except a datagram
/// connection, which returns at :7444-7445 before it.  This port had that
/// gate only on the PTY-stdin path, so `process-send-eof` answered `ok` for
/// a child whose status was already `exit`.
///
/// `delete-exited-processes` nil is what makes the row reachable without
/// depending on any recording: the sentinel has run, the status is settled,
/// and GNU keeps the process in `Vprocess_alist` (:7926-7929).  Measured,
/// `-Q --batch`, three runs each:
///
/// ```text
///                        GNU 31.0.90                          this port, before
///   process-send-eof     error "... not running: finished"    ok
///   process-send-string  error "... not running: finished"    same as GNU
/// ```
#[test]
fn process_send_eof_rejects_a_finished_process_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let ((delete-exited-processes nil)
                 (try (lambda (thunk)
                        (condition-case e (funcall thunk) (error (cadr e))))))
             (let* ((done nil)
                    (p (make-process
                        :name "pw180eof" :noquery t :connection-type 'pipe
                        :buffer (generate-new-buffer " *pw180eof*")
                        :sentinel (lambda (_p _m) (setq done t))
                        :command '("{sh}" "-c" "exit 0"))))
               (while (not done) (accept-process-output p 0.05))
               (list (cons 'status (process-status p))
                     (cons 'listed (and (memq p (process-list)) t))
                     (cons 'send-eof
                           (funcall try (lambda () (process-send-eof p) 'ok)))
                     (cons 'send-string
                           (funcall try (lambda () (process-send-string p "x") 'ok))))))"#
    ));

    assert_eq!(
        result,
        concat!(
            "OK ((status . exit) (listed . t) ",
            "(send-eof . \"Process pw180eof not running: finished\n\") ",
            "(send-string . \"Process pw180eof not running: finished\n\"))",
        )
    );
}

/// Ledger 198: the trigger's SAFE POINT is the wait, and `maybe_quit` is not
/// it -- asserted from both sides in one test.
///
/// GNU's recorder is `handle_child_signal`, run from a SIGCHLD handler
/// (src/process.c:7691, installed by `catch_child_signal` at :8645), and its
/// own header says what it may and may not do:
///
/// ```text
///    All we do is change the status; we do not run sentinels or print
///    notifications.  That is saved for the next time keyboard input is
///    done, in order to avoid timing errors.                  /* :7668-7671 */
/// ```
///
/// **"The next time keyboard input is done" is `wait_reading_process_output`,
/// and the call graph says the same thing three ways.**
///
/// * `process_pending_signals` -- all `maybe_quit` reaches
///   (src/lisp.h:3896-3900 -> src/eval.c:1868-1876) -- is
///   `pending_signals = false; handle_async_input (); do_pending_atimers ();`
///   (src/keyboard.c:8367-8372).  `grep -c status_notify` over it is **0**.
/// * `handle_child_signal` never assigns `pending_signals`: `grep -n
///   'pending_signals = ' src/*.c` returns eleven lines, none in `process.c`.
///   Its wake is `child_signal_notify` (:7766-7767), one `emacs_write` to the
///   self-pipe the `select` inside the wait is watching.
/// * All five `status_notify` calls are `Fdelete_process` (:1129, :1149),
///   `process_send_signal`'s SIGCONT arm (:7181) -- three subrs notifying a
///   status they wrote themselves -- and the wait (:5554, :5854).
///
/// Ledger 193 drained at `maybe_quit` and this test asserted that it did.  It
/// now asserts the opposite, and the reason is Lisp-visible: a record made at
/// `maybe_quit` publishes a status with its sentinel unrun, which is ledger
/// 180's measured regression and which broke `treemacs-magit`.  See
/// `a_sentinel_runs_inside_the_wait_while_the_callers_let_is_live_like_gnu`.
///
/// The test spins over `maybe_quit` and requires that NOTHING moved, then
/// enters one wait and requires GNU's answer.  Both halves are needed: the
/// first is the type-level guarantee observed at run time (`maybe_quit` cannot
/// even spell the call, because
/// [`WaitStatusNotifySite`](crate::emacs_core::wait::WaitStatusNotifySite)'s
/// constructors are private to `wait.rs`), and the second is the ENGAGEMENT
/// check ledger P5.2 demands -- a trigger that never fires would pass the
/// first half perfectly.
///
/// **The control is asked of the KERNEL and not of this port** (ledger 187's
/// rule): the child must be a `/proc` ZOMBIE -- exited and unreaped -- before
/// the spin begins, so a run in which the child had not actually exited fails
/// on the control rather than agreeing with itself.
#[cfg(unix)]
#[test]
fn the_child_status_record_is_the_waits_work_and_maybe_quit_never_does_it() {
    crate::test_utils::init_test_tracing();
    // GNU's `catch_child_signal` runs from `init_process_emacs`, before any
    // child is forked (src/process.c:8640-8643, :8721).
    crate::emacs_core::os_signal::install();

    let sh = find_bin("sh");
    let mut eval = Context::new();
    let child = eval.processes.create_process_lisp(
        LispString::from_utf8("pw193-trigger"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![LispString::from_utf8("-c"), LispString::from_utf8("exit 7")],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    eval.processes
        .spawn_child(child, false)
        .expect("spawn child");
    let os_pid = eval
        .processes
        .get(child)
        .and_then(|proc| proc.os_pid)
        .expect("a spawned child has an OS pid");

    let is_zombie = |pid: u32| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                let after_comm = stat.rfind(')').map(|i| i + 1)?;
                stat[after_comm..]
                    .split_whitespace()
                    .next()
                    .map(str::to_string)
            })
            .is_some_and(|state| state == "Z")
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !is_zombie(os_pid) {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        is_zombie(os_pid),
        "control: the child must be exited and unreaped before the spin"
    );

    // HALF ONE: the spin.  Nothing here waits, and `maybe_quit` is reached
    // over and over -- which is exactly where ledger 193 put the drain.
    let spin_deadline = std::time::Instant::now() + Duration::from_millis(300);
    let mut safe_points: u64 = 0;
    while std::time::Instant::now() < spin_deadline {
        eval.maybe_quit().expect("maybe_quit must not unwind here");
        safe_points += 1;
        std::thread::yield_now();
    }
    assert!(
        safe_points > 1000,
        "the spin must actually reach the safe point many times, or it \
         asserts nothing; reached it {safe_points} times"
    );

    let after_spin =
        process_effective_status(eval.processes.get(child).expect("child is still listed"));
    assert_eq!(
        ProcessStatusSymbol::from_status_value(after_spin),
        Some(ProcessStatusSymbol::Run),
        "`process_pending_signals' notifies nothing (src/keyboard.c:8367-8372, \
         grep -c status_notify = 0), so no safe point outside the wait may \
         record a child status here -- ledger 198"
    );
    assert!(
        is_zombie(os_pid),
        "the reap is the record (child_status_changed IS waitpid), so it does \
         not happen at `maybe_quit' either; this row is the PINNED divergence \
         of ledger 184's rows 2 and 3"
    );

    // HALF TWO: one wait, which is GNU's own site for this
    // (src/process.c:5554, :5854).  Without this half the test above would
    // pass just as well with no trigger at all.
    let waited = format_eval_result(&eval.eval_str("(accept-process-output nil 0.5)"));
    assert!(
        waited.starts_with("OK "),
        "the wait must not signal: {waited}"
    );

    // Read through `get_any`, because the notification RETIRED the process:
    // GNU's `status_notify` takes the `delete-exited-processes` decision
    // (src/process.c:7926-7929) and the default is `t`, so `remove_process`
    // has run.  That the read has to move is itself the evidence that the
    // sentinel ran and not merely that the status was recorded.
    assert!(
        eval.processes.get(child).is_none(),
        "with `delete-exited-processes' at its default, `status_notify' \
         removes the process (src/process.c:7926-7927)"
    );
    let after_wait =
        process_effective_status(eval.processes.get_any(child).expect("child is still known"));
    assert_eq!(
        ProcessStatusSymbol::from_status_value(after_wait),
        Some(ProcessStatusSymbol::Exit),
        "the wait is where GNU notifies an asynchronously discovered status, \
         so one `accept-process-output' must produce GNU's answer"
    );
    assert_eq!(process_status_code_value(after_wait), 7);
    assert!(
        !is_zombie(os_pid),
        "and the record reaped the child, because `child_status_changed' is \
         `waitpid' (src/process.c:7741-7742)"
    );
}

/// GNU's `Fdelete_process` THROWS THE RECORD AWAY before it reads anything --
/// `p->raw_status_new = 0;` (src/process.c:1123) is the first statement after
/// the DNS-cancel block, and it runs unconditionally.
///
/// So a status recorded by the SIGCHLD handler BEFORE `delete-process` was
/// called is not what `delete-process` reports.  What GNU reports instead is
/// its own SIGKILL: `if (p->raw_status_new) update_status (p);` at :1141 can
/// only be true for a record that arrived AFTER :1123, and then
/// `if (! (EQ (symbol, Qsignal) || EQ (symbol, Qexit))) pset_status (p, list2
/// (Qsignal, make_fixnum (SIGKILL)));` (:1146-1147) settles it.
///
/// Measured, `-Q --batch`, a child that exited **7** with nobody waiting and
/// then `delete-process`:
///
/// ```text
///                          GNU 31.0.90   before 193   193 without this fix
///   (process-status p)        signal        signal          exit
///   (process-exit-status p)   9             9               7
/// ```
///
/// **This is a defect the TRIGGER exposed rather than one it caused**, and
/// `UpdateStatusSite::DeleteProcess`'s docstring predicted it in those words:
/// the two editors agreed *"because GNU discards the 7 and this port never
/// recorded it"*.  Once this port records it, the arm that adopted a pending
/// status at the delete site is reachable, and it is not GNU's.
#[test]
fn delete_process_discards_a_status_recorded_before_it_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let marker_dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("target/pw193");
    std::fs::create_dir_all(&marker_dir).expect("marker dir");
    let marker = marker_dir
        .join("pw193delete.marker")
        .to_string_lossy()
        .into_owned();
    let _ = std::fs::remove_file(&marker);
    let result = eval_one(&format!(
        r#"(let* ((marker "{marker}")
                  (p (make-process
                      :name "pw193del" :noquery t :connection-type 'pipe
                      :buffer (generate-new-buffer " *pw193del*")
                      :command '("{sh}" "-c" ": > {marker}; exit 7")))
                  (deadline (+ (float-time) 20)))
             ;; A pure-Lisp spin, so the SIGCHLD trigger is the only thing that
             ;; can have recorded the exit -- and it must then be DISCARDED.
             (while (and (not (file-exists-p marker)) (< (float-time) deadline)) nil)
             (let ((ran (file-exists-p marker)))
               (while (and (process-live-p p) (< (float-time) deadline)) nil)
               (delete-process p)
               (list (cons 'child-ran ran)
                     (cons 'status (process-status p))
                     (cons 'exit-status (process-exit-status p)))))"#
    ));
    let _ = std::fs::remove_file(&marker);

    assert_eq!(
        result,
        "OK ((child-ran . t) (status . signal) (exit-status . 9))"
    );
}

/// GNU runs a process sentinel INSIDE the wait, while the caller's `let`
/// bindings are still live -- and GNU says so in its own words, twice.
///
/// ```text
///   /* Execute the sentinel here.  If we had relied on status_notify
///      to do it later, it will read input from the process before
///      calling the sentinel.  */                    src/process.c:3413, :6160
/// ```
///
/// The five `status_notify` call sites are the whole of GNU's notification
/// surface (`grep -n 'status_notify' src/process.c`): `Fdelete_process`
/// (:1129, :1149), `process_send_signal`'s SIGCONT arm (:7181), and -- for
/// every status GNU discovered ASYNCHRONOUSLY -- `wait_reading_process_output`
/// at **:5554** and **:5854**, both immediately after a `thread_select`
/// returns.  `process_pending_signals`, which is what `maybe_quit` reaches
/// (src/lisp.h:3896-3900 -> src/eval.c:1868-1876), is not among them; its
/// entire body is
///
/// ```c
///   pending_signals = false;
///   handle_async_input ();
///   do_pending_atimers ();                          src/keyboard.c:8367-8372
/// ```
///
/// and `grep -c status_notify` over it is **0**.  GNU's own header for
/// `handle_child_signal` says where the notification goes instead: *"That is
/// saved for the next time keyboard input is done"* (:7669-7671) -- and the
/// function that does keyboard input is `wait_reading_process_output`.
///
/// # What this measures, and why it has no timing race in it
///
/// The shape is magit's: `magit-run-post-commit-hook` is keyed on
/// `last-command`, and its caller binds that AROUND the await loop
/// (`(while (process-live-p p) (accept-process-output nil 0.02))`), so the
/// sentinel has to run inside the wait or the binding is gone.
///
/// The assertion is filtered to the runs in which the loop actually ENTERED a
/// wait, because the unfiltered question does have a race and GNU loses it
/// too: if the child dies before the first `process-live-p`, GNU's handler has
/// already recorded the exit, the loop exits without waiting, and GNU's own
/// sentinel runs late.  Measured, `-Q --batch`, 100 runs per shape:
///
/// ```text
///                         entered a wait   sentinel inside the let
///   GNU 31.0.90  printf; exit       97              97
///   GNU 31.0.90  sleep; printf     100             100
///   GNU 31.0.90  exit               97              97
/// ```
///
/// **294 of 294.** Once a wait is entered, GNU never returns from it leaving a
/// child status recorded and its sentinel unrun.  That is the contract, and it
/// is what ledger 193's drain broke: with the record made at
/// `Context::maybe_quit` this port answered **261 of 300** on the same probe,
/// and `treemacs_magit_package_batch`'s
/// `extending_a_real_commit_schedules_the_same_project_refresh` failed with
/// `(error "Treemacs-Magit idle update was not scheduled")`.
#[test]
fn a_sentinel_runs_inside_the_wait_while_the_callers_let_is_live_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    // Three child shapes, because the defect is sensitive to WHEN the child
    // dies relative to the wait: output-then-exit is the magit shape (git
    // writes, then exits), sleep-then-output puts the death after a real
    // block, and a bare exit is the control that agreed in both editors even
    // while the other two did not.
    let result = eval_one(&format!(
        r#"(let ((shapes '(("printf pw198-hello; exit 0" . out-then-exit)
                           ("sleep 0.05; printf pw198-hello" . sleep-out-exit)
                           ("exit 0" . immediate-exit)))
                 (report nil))
             (dolist (shape shapes (nreverse report))
               (let ((entered 0) (inside 0))
                 (dotimes (_ 40)
                   (let ((observed 'sentinel-never-ran)
                         (waits 0))
                     (let ((proc (make-process
                                  :name "pw198" :noquery t :buffer nil
                                  :connection-type 'pipe
                                  :command (list "{sh}" "-c" (car shape))
                                  :sentinel
                                  (lambda (_p _m)
                                    (when (eq observed 'sentinel-never-ran)
                                      (setq observed last-command))))))
                       ;; magit's shape: the binding the hook is keyed on is
                       ;; live only for the duration of the await.
                       (let ((last-command 'pw198-inside-the-let)
                             (deadline (+ (float-time) 20)))
                         (while (and (process-live-p proc)
                                     (< (float-time) deadline))
                           (setq waits (1+ waits))
                           (accept-process-output nil 0.02)))
                       ;; The `let' has unwound.  Keep pumping, so a sentinel
                       ;; that ran LATE is distinguishable from one that never
                       ;; ran at all -- a probe that could not tell those apart
                       ;; would pass vacuously if sentinels stopped running.
                       (let ((deadline (+ (float-time) 20)))
                         (while (and (eq observed 'sentinel-never-ran)
                                     (< (float-time) deadline))
                           (accept-process-output nil 0.02)))
                       (when (eq observed 'sentinel-never-ran)
                         (error "pw198: the sentinel never ran at all"))
                       (unless (zerop waits)
                         (setq entered (1+ entered))
                         (when (eq observed 'pw198-inside-the-let)
                           (setq inside (1+ inside)))))))
                 (push (list (cdr shape) entered inside) report))))"#
    ));

    let Some(rows) = result.strip_prefix("OK ") else {
        panic!("pw198 probe did not return a value: {result}");
    };
    // Every shape must report `entered == inside`: of the runs that entered a
    // wait, the sentinel ran inside the caller's `let` in ALL of them, which
    // is GNU's 294/294 above.  The counts themselves are printed on failure
    // because "how often" is the whole diagnosis.
    let mut offenders = Vec::new();
    for row in rows
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(") (")
    {
        let fields: Vec<&str> = row.trim_matches(['(', ')']).split_whitespace().collect();
        assert_eq!(
            fields.len(),
            3,
            "pw198 row is not (SHAPE ENTERED INSIDE): {row}"
        );
        let entered: u32 = fields[1].parse().expect("entered count");
        let inside: u32 = fields[2].parse().expect("inside count");
        assert!(
            entered > 0,
            "pw198 shape {} never entered a wait, so it asserts nothing",
            fields[0]
        );
        if entered != inside {
            offenders.push(format!("{}: {inside}/{entered} inside", fields[0]));
        }
    }
    assert!(
        offenders.is_empty(),
        "a wait returned with a child status recorded and its sentinel unrun, \
         which GNU never does (294/294 measured): {}\nfull report: {result}",
        offenders.join(", ")
    );
}

/// The ENGAGEMENT of the unconditional walk and of GNU's tick pair, on the
/// workload they exist for.
///
/// Ledger P5.2's skip was 100% green and fired ZERO times, so a mechanism that
/// can silently never run has to be able to say how often it ran.  Ledger 208
/// deleted the SIGCHLD trigger and made GNU's `status_notify` walk
/// unconditional, so the two numbers that matter changed:
///
/// * `walks` -- the walk is now a rate, not an arming count, and a zero would
///   mean `wait_reading_process_output` stopped calling it;
/// * `visited` -- how many processes GNU's `p->tick != p->update_tick`
///   (src/process.c:7892) put in a visit set.  A zero here means the tick pair
///   decides nothing and the entry is inert.
///
/// `stamped` is reported rather than asserted, and the difference `visited -
/// stamped` is the honest number: it is the work the OLD visit set -- the
/// walk's own return value -- could not have done.
#[cfg(unix)]
#[test]
fn the_status_notify_walk_and_the_tick_pair_are_engaged_on_the_workload() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::os_signal::install();
    let sh = find_bin("sh");

    let before = crate::emacs_core::process::child_status::status_notify_totals();
    let result = eval_one(&format!(
        r#"(dotimes (_ 40 t)
             (let ((p (make-process :name "pw198engage" :buffer nil :noquery t
                                    :connection-type 'pipe
                                    :command '("{sh}" "-c" "printf pw198; exit 0")
                                    :sentinel #'ignore))
                   (deadline (+ (float-time) 20)))
               (while (and (process-live-p p) (< (float-time) deadline))
                 (accept-process-output nil 0.02))))"#
    ));
    assert_eq!(result, "OK t", "the workload itself must succeed");
    let after = crate::emacs_core::process::child_status::status_notify_totals();

    let walks = after.0 - before.0;
    let stamped = after.1 - before.1;
    let visited = after.2 - before.2;
    assert!(
        walks > 0,
        "GNU's status_notify walk never ran across 40 children: \
         walks={walks} stamped={stamped} visited={visited}.  \
         `wait_reading_process_output' no longer calls it."
    );
    assert!(
        visited > 0,
        "GNU's per-process tick pair put NOTHING in a visit set across 40 \
         children: walks={walks} stamped={stamped} visited={visited}.  The \
         mechanism ledger 208 landed decides nothing, which is ledger P5.2's \
         failure mode exactly."
    );
    // Reported rather than asserted; see the docstring.
    eprintln!("PW208 engagement: walks={walks} stamped={stamped} visited={visited}");
}

/// GNU's `child_signal_notify` exists for exactly one reason, and GNU writes
/// it out above `deleted_pid_list` (src/process.c:7539-7547):
///
/// ```text
///    To avoid a deadlock when receiving SIGCHLD while
///    'wait_reading_process_output' is in 'pselect', the SIGCHLD handler
///    will notify the `pselect' using a self-pipe.  The deadlock could
///    occur if SIGCHLD is delivered outside of the 'pselect' call, in
///    which case 'pselect' will not be interrupted by the signal, and
///    will therefore wait on the process's output descriptor for the
///    output that will never come.
/// ```
///
/// So the byte is a WAKE, and GNU puts the read end in the select's own mask
/// to collect it: `add_read_fd (fds[0], child_signal_read, NULL)` in
/// `child_signal_init` (:7590-7595), and `FD_SET (child_fd, &Available)`
/// immediately before the block, under the comment *"We have to be informed
/// when we receive a SIGCHLD signal for an asynchronous process.  Otherwise
/// this might deadlock if we receive a SIGCHLD during `pselect'"*
/// (:5629-5635).
///
/// **This port does not have that wake, and it does not need it -- both
/// halves measured by ledger 200.**
///
/// GNU's own comment exempts a platform whose wait primitive watches the child
/// itself (:7548-7552): *"WINDOWSNT doesn't need this facility because its
/// 'pselect' emulation ... waits on a subprocess handle, which becomes
/// signaled when the process exits"*.  `sys::ChildStatusSource` registers a
/// `pidfd` per child with the wait poller, so this port is that case, and GNU
/// files the general form as a FIXME two lines on (:7554-7557).
///
/// **What is ASSERTED here is the half the port relies on**: a real child's
/// `pidfd` returns a 3s block the moment the child dies.
///
/// **What is MEASURED and deliberately NOT asserted** is the other half: with
/// a confirmed SIGCHLD delivered 200ms into the same 3s block and nothing else
/// happening, the block ran the full **3.000038747s**.  The byte reaches
/// nobody -- `InstallReport::self_pipe_read_fd` is registered with no poller
/// (ledger 184's declared residual, ledger 198 §9.2) and
/// `polling::Poller::wait` catches `ErrorKind::Interrupted` and re-enters the
/// wait itself (polling-3.11.0/src/lib.rs:751-764), so the `EINTR` that
/// `epoll_wait` really does produce never surfaces.
///
/// That is left as a measurement rather than an assertion **on purpose**:
/// asserting it would pin the ABSENCE of a facility, and registering the read
/// end is exactly what ledger 184 and ledger 198 §9.2 leave open.  Ledger 200
/// §9.1 prices what that would then be worth.
#[cfg(unix)]
#[test]
fn a_childs_own_descriptor_returns_the_block_which_is_what_gnu_uses_sigchld_for() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::os_signal::install();

    let sh = find_bin("sh");
    let block = Duration::from_secs(3);
    let prompt = Duration::from_millis(1500);

    // PHASE 1, the assertion: a real child, whose `pidfd` IS registered with
    // this poller, dies 200ms into a 3s block.
    let mut eval = Context::new();
    let child = eval.processes.create_process_lisp(
        LispString::from_utf8("pw200-control"),
        Value::NIL,
        LispString::from_utf8(&sh),
        vec![
            LispString::from_utf8("-c"),
            LispString::from_utf8("sleep 0.2; exit 7"),
        ],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    eval.processes
        .spawn_child(child, false)
        .expect("spawn child");
    let started = Instant::now();
    let events = eval
        .processes
        .wait_for_backend_events(block, ProcessWaitBackendInterest::ProcessesOnly);
    let control_elapsed = started.elapsed();
    assert!(
        control_elapsed < prompt,
        "a child's own `pidfd' must return the block when it dies -- waited \
         {control_elapsed:?} of {block:?}, events {events:?}.  That descriptor \
         is what stands in for GNU's `child_signal_notify' self-pipe here \
         (src/process.c:7548-7552); without it this port would need GNU's \
         wake after all, and it does not have one (see the docstring)."
    );

    // PHASE 2 is the measurement the docstring records and does NOT assert:
    // a SIGCHLD delivered into the same block, with no child dying.  It is run
    // rather than described so the number in the docstring can be re-taken by
    // anyone who doubts it.
    //
    // Since ledger 208 there is no handler and therefore no counter to read,
    // so the control is the disposition itself: SIGCHLD is `SIG_DFL`, the
    // `kill` really happens, and the block runs its full length anyway --
    // which is the whole point.  `polling::Poller::wait` catches
    // `ErrorKind::Interrupted` and re-enters (polling-3.11.0/src/lib.rs:
    // 751-764), so even a delivery that DID interrupt the syscall would not
    // shorten the block.
    let eval = Context::new();
    assert_no_sigchld_handler_is_installed();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        // SAFETY: `kill` with this process's own pid and a valid signal.
        unsafe { libc::kill(libc::getpid(), libc::SIGCHLD) };
    });
    let started = Instant::now();
    let _events = eval
        .processes
        .wait_for_backend_events(block, ProcessWaitBackendInterest::ProcessesOnly);
    let signal_elapsed = started.elapsed();
    waker.join().expect("waker thread");
    eprintln!(
        "PW208 wake: pidfd returned a {block:?} block in {control_elapsed:?}; \
         a SIGCHLD delivered into the same block left it running \
         {signal_elapsed:?}"
    );
}

/// Assert that this build installs NO SIGCHLD disposition, which is what the
/// tests below claim to be measuring.
///
/// Before ledger 208 they disarmed the trigger by hand; 208 deleted it, so the
/// disarm is now the shipped state and the honest control is to read the
/// disposition back rather than to write it.  It also catches the case that
/// would otherwise make these tests pass over nothing -- something else in the
/// test process (a harness, a linked library) having claimed SIGCHLD.
///
/// `SIG_DFL` and not `SIG_IGN` matters: SIGCHLD's default action is "ignore the
/// signal", but `SIG_IGN` is a different thing entirely -- POSIX makes a child
/// of a process whose SIGCHLD is `SIG_IGN` not become a zombie at all, which
/// would reap the children out from under the very rows under test.
#[cfg(unix)]
fn assert_no_sigchld_handler_is_installed() {
    // SAFETY: `old` is fully initialised by `sigaction` before it is read, and
    // a null `act` means "query only".
    let old = unsafe {
        let mut old: libc::sigaction = std::mem::zeroed();
        let rc = libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut old);
        assert_eq!(rc, 0, "could not read SIGCHLD's disposition");
        old
    };
    assert_eq!(
        old.sa_sigaction,
        libc::SIG_DFL,
        "SIGCHLD is not SIG_DFL in this process, so the tests below are not \
         measuring a build without the trigger.  Ledger 208 deleted this \
         port's install; something else has claimed the signal."
    );
}

/// **Ledger 198 s9.1, reproduced rather than reasoned about.**  198 recorded,
/// measured and not fixed:
///
/// > GNU's `child_signal_notify` exists to wake a `select` that has no other
/// > way to learn a child died.  This port registers a `pidfd` per child with
/// > the wait poller (`process/sys/linux.rs`, `sys::ChildStatusSource`), so the
/// > wait already returns the moment a child terminates, and
/// > `check_child_status_change` in the service pass already harvests it.
///
/// This is that sentence as a test, with the trigger taken out of the picture
/// entirely: SIGCHLD is put back to `SIG_DFL` before the child is spawned, so
/// no handler runs, no counter moves and no self-pipe byte is written.
///
/// The two halves are deliberately separate observables, because they are
/// different claims:
///
/// * **the wait RETURNED** -- measured as elapsed wall time against a 10s
///   `accept-process-output` timeout, for a child that exits after 200ms and
///   writes nothing at all, so no pipe readability can explain the return;
/// * **the status was RECORDED** -- `process-status` is `exit`, the exit
///   status is 7, and the sentinel ran.
///
/// `deliveries` is asserted at zero afterwards, so a run in which the disarm
/// silently failed fails HERE rather than agreeing with the claim.
#[cfg(unix)]
#[test]
fn the_pidfd_alone_returns_the_wait_and_records_the_exit_without_any_sigchld_handler() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::os_signal::install();
    assert_no_sigchld_handler_is_installed();

    let sh = find_bin("sh");
    // The elapsed measurement is taken INSIDE Lisp, around the one wait: a
    // Rust-side `Instant` around `eval_one` measures the runtime bootstrap
    // too, which in a debug build is seconds and swamps the answer.
    let result = eval_one(&format!(
        r#"(let* ((seen nil)
                  (p (make-process
                      :name "pw200-nohandler" :noquery t :buffer nil
                      :connection-type 'pipe
                      :command '("{sh}" "-c" "sleep 0.2; exit 7")
                      :sentinel (lambda (_p m) (setq seen (string-trim m)))))
                  (deadline (+ (float-time) 30))
                  (t0 (float-time))
                  (waited nil))
             ;; One long wait, so "it returned" is a fact about the wait and
             ;; not about a caller polling in a loop.
             (accept-process-output p 10)
             (setq waited (- (float-time) t0))
             (while (and (not seen) (< (float-time) deadline))
               (accept-process-output nil 0.02))
             (list (cons 'status (process-status p))
                   (cons 'exit-status (process-exit-status p))
                   (cons 'sentinel seen)
                   (cons 'wait-returned-early (and (< waited 5) t))))"#
    ));
    assert_eq!(
        result,
        r#"OK ((status . exit) (exit-status . 7) (sentinel . "exited abnormally with code 7") (wait-returned-early . t))"#,
        "with no SIGCHLD handler at all, the 10s `accept-process-output' must \
         still RETURN when the child dies (the child writes nothing, so only \
         the registered `pidfd' can return the block) and the wait must still \
         RECORD the exit and run the sentinel"
    );
}

/// Ledger 180 s7.1's contract -- *"once a wait is entered, GNU never returns
/// from it leaving a child status recorded and its sentinel unrun"* -- asked of
/// a build with **no SIGCHLD handler at all**.
///
/// `a_sentinel_runs_inside_the_wait_while_the_callers_let_is_live_like_gnu` is
/// the same probe with the trigger armed, and ledger 198 measured GNU at
/// 294/294 and this port at 261/300 when the record was drained at
/// `maybe_quit`.  This one exists to answer the question 198 raised against its
/// own work and did not answer: **is any of that the trigger's doing?**
///
/// The shapes are 198's, for the reason 198 gave: `out-then-exit` is magit's
/// shape and the one that failed 72/100 at the wrong safe point.
#[cfg(unix)]
#[test]
fn the_sentinel_contract_holds_with_no_sigchld_handler_installed() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::os_signal::install();
    assert_no_sigchld_handler_is_installed();

    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let ((shapes '(("printf pw200-hello; exit 0" . out-then-exit)
                           ("sleep 0.05; printf pw200-hello" . sleep-out-exit)
                           ("exit 0" . immediate-exit)))
                 (report nil))
             (dolist (shape shapes (nreverse report))
               (let ((entered 0) (inside 0))
                 (dotimes (_ 40)
                   (let ((observed 'sentinel-never-ran)
                         (waits 0))
                     (let ((proc (make-process
                                  :name "pw200" :noquery t :buffer nil
                                  :connection-type 'pipe
                                  :command (list "{sh}" "-c" (car shape))
                                  :sentinel
                                  (lambda (_p _m)
                                    (when (eq observed 'sentinel-never-ran)
                                      (setq observed last-command))))))
                       (let ((last-command 'pw200-inside-the-let)
                             (deadline (+ (float-time) 20)))
                         (while (and (process-live-p proc)
                                     (< (float-time) deadline))
                           (setq waits (1+ waits))
                           (accept-process-output nil 0.02)))
                       (let ((deadline (+ (float-time) 20)))
                         (while (and (eq observed 'sentinel-never-ran)
                                     (< (float-time) deadline))
                           (accept-process-output nil 0.02)))
                       (when (eq observed 'sentinel-never-ran)
                         (error "pw200: the sentinel never ran at all"))
                       (unless (zerop waits)
                         (setq entered (1+ entered))
                         (when (eq observed 'pw200-inside-the-let)
                           (setq inside (1+ inside)))))))
                 (push (list (cdr shape) entered inside) report))))"#
    ));
    let Some(rows) = result.strip_prefix("OK ") else {
        panic!("pw200 probe did not return a value: {result}");
    };
    let mut offenders = Vec::new();
    for row in rows
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(") (")
    {
        let fields: Vec<&str> = row.trim_matches(['(', ')']).split_whitespace().collect();
        assert_eq!(
            fields.len(),
            3,
            "pw200 row is not (SHAPE ENTERED INSIDE): {row}"
        );
        let entered: u32 = fields[1].parse().expect("entered count");
        let inside: u32 = fields[2].parse().expect("inside count");
        assert!(
            entered > 0,
            "pw200 shape {} never entered a wait, so it asserts nothing",
            fields[0]
        );
        if entered != inside {
            offenders.push(format!("{}: {inside}/{entered} inside", fields[0]));
        }
    }
    assert!(
        offenders.is_empty(),
        "with the SIGCHLD trigger out of the picture a wait returned with a \
         child status recorded and its sentinel unrun: {}\nfull report: {result}",
        offenders.join(", ")
    );
}

/// **The three pinned rows, on a build with no SIGCHLD handler at all.**
///
/// Ledger 193 landed a SIGCHLD trigger claiming these rows; 198 re-pinned them
/// as divergences with the trigger installed; 200 ran this probe as a
/// within-process A/B -- armed, then with SIGCHLD put back to `SIG_DFL` -- and
/// measured the two reports BYTE-IDENTICAL, which is what said the trigger was
/// not buying them.  Ledger 208 deleted the trigger, so there is no armed arm
/// left to compare against and the A/B collapses to its surviving half: the
/// rows, asserted on the shipped disposition.
///
/// The probe still runs TWICE and still requires the two reports to be equal,
/// because that half of the control is about the rows being stable rather than
/// about the trigger.
///
/// The rows are ledger 180 s9.1/s9.2 and ledger 184's rows 2 and 3, the ones
/// ledger 198 s4 listed as going back to pinned divergences:
///
/// ```text
///                                                 GNU 31.0.90   here
///   (process-status p)          after a pure spin     exit       run
///   (process-exit-status p)                           7          0
///   (process-live-p p)                                nil        t
///   (cdr (assq 'state (process-attributes pid)))      nil        "Z"
///   (signal-process p 0)                              -1         0
/// ```
///
/// Each run also takes the after-one-wait column, so the probe cannot pass by
/// child statuses having stopped working altogether: a run in which nothing is
/// ever recorded fails on `after-one-wait` rather than agreeing with itself.
/// The child's own last act -- touching a marker file -- is the control that it
/// really ran, and `assert_no_sigchld_handler_is_installed` is the control that
/// this really is a build without the trigger.
#[cfg(unix)]
#[test]
fn the_pinned_rows_hold_on_a_build_with_no_sigchld_handler_at_all() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::os_signal::install();

    let sh = find_bin("sh");
    let marker_dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("target/pw200");
    std::fs::create_dir_all(&marker_dir).expect("marker dir");

    let probe = |tag: &str| {
        let marker = marker_dir
            .join(format!("pw200-{tag}.marker"))
            .to_string_lossy()
            .into_owned();
        let _ = std::fs::remove_file(&marker);
        let report = eval_one(&format!(
            r#"(let* ((marker "{marker}")
                      (p (make-process
                          :name "pw200ab" :noquery t :connection-type 'pipe
                          :buffer (generate-new-buffer " *pw200ab*")
                          :command '("{sh}" "-c" ": > {marker}; exit 7")))
                      (pid (process-id p))
                      (deadline (+ (float-time) 20)))
                 ;; A pure-Lisp spin: `wait_reading_process_output' is never
                 ;; entered, so only an asynchronous recording could see it.
                 (while (and (not (file-exists-p marker)) (< (float-time) deadline)) nil)
                 (let ((ran (file-exists-p marker))
                       (spin (+ (float-time) 0.2)))
                   (while (< (float-time) spin) nil)
                   (let ((before
                          (list (cons 'child-ran ran)
                                (cons 'status (process-status p))
                                (cons 'exit-status (process-exit-status p))
                                (cons 'live-p (and (process-live-p p) t))
                                (cons 'attrs-state
                                      (cdr (assq 'state (process-attributes pid))))
                                (cons 'signal-0 (signal-process p 0)))))
                     ;; ...and now enter GNU's own safe point for this, ONCE.
                     (accept-process-output nil 0.5)
                     (list (cons 'before before)
                           (cons 'after-one-wait
                                 (list (cons 'status (process-status p))
                                       (cons 'exit-status (process-exit-status p))
                                       (cons 'attrs-state
                                             (cdr (assq 'state
                                                        (process-attributes pid))))))))))"#
        ));
        let _ = std::fs::remove_file(&marker);
        report
    };

    assert_no_sigchld_handler_is_installed();
    let first = probe("first");
    let second = probe("second");

    assert_eq!(
        first, second,
        "the pinned rows are not stable across two runs in one process"
    );
    assert_eq!(
        first,
        "OK ((before (child-ran . t) (status . run) (exit-status . 0) (live-p . t) \
         (attrs-state . \"Z\") (signal-0 . 0)) \
         (after-one-wait (status . exit) (exit-status . 7) (attrs-state)))",
        "the rows themselves moved; these are ledger 180 s9.1/s9.2 and ledger \
         184's rows 2 and 3, PINNED as divergences by ledger 198 s4"
    );
}

/// **The one thing the child-status walk buys that this port's own service
/// passes do not, measured against GNU rather than argued.**
///
/// GNU's wait notifies with `status_notify (NULL, wait_proc)` (src/process.c:
/// 5554, :5854), and the NULL first argument means its `FOR_EACH_PROCESS`
/// visits the WHOLE alist (:7886-7890) -- even when the caller asked to wait on
/// one process and nothing else.  This port's passes are not like that:
/// `ProcessOutputServiceRequest::TargetOnly` restricts both the live set and
/// the ready set to the target (`live_processes`, `ready_processes`), and that
/// is what `(accept-process-output PROC SECONDS MILLISEC JUST-THIS-ONE)`
/// builds.
///
/// So a second child that dies during a just-this-one wait is invisible to
/// every restricted pass, and only the whole-alist walk can see it.  Measured,
/// `-Q --batch`, GNU Emacs 31.0.90, 5 runs, byte-identical:
///
/// ```text
///   (q-sentinel . "exited abnormally with code 3")   (q-status . exit)
/// ```
///
/// **This is the row that decided ledger 200.**  With the walk run the port
/// answers GNU's line; without it the port answers `(q-sentinel)` and
/// `(q-status . run)`.  What ARMS the walk is the question ledger 200 asked:
/// ledger 193 armed it with a SIGCHLD counter, and 200 measured that the
/// counter is the only thing the signal ever decided -- so the signal went and
/// the walk stayed, unconditional, where GNU runs it.
#[cfg(unix)]
#[test]
fn a_just_this_one_wait_notifies_another_childs_sentinel_like_gnu() {
    crate::test_utils::init_test_tracing();
    let sh = find_bin("sh");
    let result = eval_one(&format!(
        r#"(let* ((qseen nil)
                  (q (make-process :name "pw200q" :noquery t :buffer nil
                                   :connection-type 'pipe
                                   :command '("{sh}" "-c" "sleep 0.2; exit 3")
                                   :sentinel (lambda (_p m) (setq qseen (string-trim m)))))
                  (p (make-process :name "pw200pp" :noquery t :buffer nil
                                   :connection-type 'pipe
                                   :command '("{sh}" "-c" "sleep 5")
                                   :sentinel #'ignore)))
             ;; JUST-THIS-ONE, so every restricted pass in this port sees only
             ;; `p'.  `q' dies 200ms into the 1s block.
             (accept-process-output p 1 nil t)
             (prog1 (list (cons 'q-sentinel qseen)
                          (cons 'q-status (process-status q)))
               (delete-process p)))"#
    ));
    assert_eq!(
        result, r#"OK ((q-sentinel . "exited abnormally with code 3") (q-status . exit))"#,
        "GNU's `status_notify (NULL, wait_proc)' walks the whole alist \
         (src/process.c:5554, :5854 -> :7886-7890), so a just-this-one wait \
         still runs another child's sentinel; measured 5/5 in GNU 31.0.90"
    );
}

/// A write that hits `EPIPE` must not decide the child's fate: GNU's EPIPE
/// arm in `send_process` (src/process.c:6940-6946 on master, as of
/// e381cf1fc97, 2025-08-15) closes the write fd and signals `"Process %s no
/// longer connected to pipe; closed it"` -- and touches NOTHING else.  The
/// child's real status arrives through the SIGCHLD record and
/// `status_notify` runs the sentinel with it.  (The emacs-31.1 release still
/// has the pre-e381cf1fc97 arm at :6923-6931, which synthesizes
/// `(exit . 256)` and bumps the tick; this port follows master.)
///
/// This port used to write `(exit . 256)` into the settled status right in
/// the EPIPE branch.  A terminal status makes the child ineligible for the
/// reap sweep (`SweepableChild::of` admits only `run`/`stop`), and the branch
/// set no `status_notify_pending`, so the real exit was never recorded and
/// the SENTINEL NEVER RAN.  Lisp-visible consequence, measured with lsp-mode
/// against a crashing nixd: `lsp--handle-process-exit` never runs, the dead
/// workspace stays in the session, and every later `process-send-string`
/// reports "Process nixd-lsp not running: exited abnormally with code 256"
/// forever.
///
/// The scenario needs the send to beat the reap sweep to the dead child,
/// which no Lisp program can arrange (any wait sweeps first) -- so the pause
/// between spawn and send is a host-level sleep with no Lisp wait in it.
/// GNU itself cannot reach the EPIPE arm this way (its SIGCHLD is
/// asynchronous, so the send finds the status already settled); the pinned
/// divergence that statuses here settle only at wait sites (ledger 180/184)
/// is exactly what makes the arm reachable, and therefore what its contract
/// protects.
#[test]
fn epipe_on_send_leaves_the_real_child_status_for_the_next_wait_to_reap() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let sh = find_bin("sh");

    let result = eval_one_in_context(
        &mut eval,
        &format!(
            r#"(progn
                 (setq neovm-test-epipe-events nil)
                 (setq neovm-test-epipe-proc
                       (make-process :name "epipe-child"
                                     :command '("{sh}" "-c" "exit 7")
                                     :connection-type 'pipe
                                     :noquery t
                                     :sentinel (lambda (p e)
                                                 (push (list e
                                                             (process-status p)
                                                             (process-exit-status p))
                                                       neovm-test-epipe-events))))
                 t)"#
        ),
    );
    assert_eq!(result, "OK t");

    // The child exits immediately; nothing here runs a Lisp wait, so no
    // sweep records its death before the send below.
    std::thread::sleep(Duration::from_millis(400));

    let send = eval_one_in_context(
        &mut eval,
        r#"(condition-case err
               (progn (process-send-string neovm-test-epipe-proc "x\n") 'sent)
             (error (error-message-string err)))"#,
    );
    assert_eq!(
        send, r#"OK "Process epipe-child no longer connected to pipe; closed it""#,
        "GNU's EPIPE arm closes the pipe and says so"
    );

    // GNU: the EPIPE arm did not settle a status, so the child is still
    // sweepable; the next wait reaps the REAL exit and runs the sentinel.
    let after = eval_one_in_context(
        &mut eval,
        r#"(progn
             (accept-process-output nil 0.3)
             (list neovm-test-epipe-events
                   (process-live-p neovm-test-epipe-proc)
                   (if (memq neovm-test-epipe-proc (process-list)) 'listed 'deleted)))"#,
    );
    assert_eq!(
        after,
        r#"OK ((("exited abnormally with code 7
" exit 7)) nil deleted)"#,
        "the sentinel runs once with the child's real status, and \
         `delete-exited-processes' removes the process afterwards"
    );
}
