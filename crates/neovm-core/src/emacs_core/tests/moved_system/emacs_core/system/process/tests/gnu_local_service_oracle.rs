//! GNU `src/process.c:4115-4116` (Emacs 31.1): a local `:service` whose
//! byte length is `>= sizeof(sockaddr_un.sun_path)` signals
//! `(error "Service name too long")` for the server, the client connect,
//! and a datagram server. The limit is 104 on Darwin and 108 on Linux.

const LOCAL_SERVICE_TOO_LONG_FORM: &str = r#"(let ((limit (if (eq system-type 'darwin) 104 108)))
  (list
   (condition-case err
       (make-network-process :name "too-long-server" :family 'local :server t
                             :service (make-string limit ?a))
     (error err))
   (condition-case err
       (make-network-process :name "too-long-client" :family 'local
                             :service (make-string limit ?b))
     (error err))
   (condition-case err
       (make-network-process :name "too-long-dgram" :family 'local :server t
                             :type 'datagram :service (make-string limit ?c))
     (error err))))"#;

const LOCAL_SERVICE_TOO_LONG_EXPECT: &str = include_str!("gnu_local_service_too_long.expected");

#[test]
fn local_service_too_long_matches_gnu_oracle() {
    crate::test_utils::init_test_tracing();
    let mut command = crate::test_utils::gnu_probe_command();
    let gnu_form = format!(
        r#"(progn
          (unless (string= emacs-version "31.1")
            (error "local service oracle requires GNU Emacs 31.1, got %s" emacs-version))
          (let ((print-escape-newlines t)) (prin1 {LOCAL_SERVICE_TOO_LONG_FORM}))
          (terpri))"#
    );
    let output = command
        .args(["-Q", "--batch", "--eval", &gnu_form])
        .output()
        .expect("run the pinned GNU Emacs oracle");
    assert!(
        output.status.success(),
        "GNU local-service oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_gnu = String::from_utf8(output.stdout).expect("GNU oracle UTF-8 stdout");
    assert_eq!(
        actual_gnu.trim_end(),
        LOCAL_SERVICE_TOO_LONG_EXPECT.trim_end(),
        "GNU oracle fixture"
    );
    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    let actual_neovm = eval
        .eval_str(LOCAL_SERVICE_TOO_LONG_FORM)
        .expect("over-long local service");
    let mut options = neovm_eval::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    assert_eq!(
        neovm_eval::emacs_core::print::print_value_with_options(&actual_neovm, options),
        LOCAL_SERVICE_TOO_LONG_EXPECT.trim_end(),
        "make-network-process must signal GNU's service-name error"
    );
}
