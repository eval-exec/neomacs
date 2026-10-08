//! Public native D-Bus marshaling parity on an owned Linux abstract bus.

use std::{
    fs::File,
    process::{Child, Command},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct PrivateBus {
    child: Child,
    address: String,
    _scratch: tempfile::TempDir,
}

impl PrivateBus {
    fn start() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        std::fs::create_dir_all(&root).unwrap();
        let scratch = tempfile::Builder::new()
            .prefix("dbus-oracle-")
            .tempdir_in(root)
            .unwrap();
        let address_file = scratch.path().join("address");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!(
                "--address=unix:abstract=neomacs-oracle-{}-{nonce}",
                std::process::id()
            ))
            .stdout(File::create(&address_file).unwrap())
            .stderr(File::create(scratch.path().join("bus.log")).unwrap())
            .spawn()
            .expect("private dbus-daemon is required");
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            let address = std::fs::read_to_string(&address_file).unwrap();
            if address.ends_with('\n') {
                break address.trim().to_owned();
            }
            if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("private D-Bus daemon did not publish its address");
            }
            thread::sleep(Duration::from_millis(10));
        };
        assert!(address.starts_with("unix:abstract="));
        Self {
            child,
            address,
            _scratch: scratch,
        }
    }

    fn parity(&self, form: &str) {
        crate::common::assert_oracle_parity_with_env(
            form,
            &[
                ("DBUS_SESSION_BUS_ADDRESS", &self.address),
                ("DBUS_FATAL_WARNINGS", "0"),
            ],
        );
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn dbus_native_marshalling_valid_compounds() {
    crate::common::return_if_neovm_enable_oracle_proptest_not_set!();
    PrivateBus::start().parity(
        r#"(progn
      (require 'dbus)
      (mapcar (lambda (payload)
        (dbus-send-signal :session nil "/org/neomacs/Oracle" "org.neomacs.Oracle" "Payload" payload)
        'sent)
       '((:array (:dict-entry :string "key" (:variant :uint32 7)))
         (:array (:struct :string "a" :uint32 1) (:struct :string "b" :uint32 2))
         (:array :signature "{sv}")
         (:variant (:struct :string "value" :uint32 2))
         (:array))))"#,
    );
}

#[test]
fn dbus_native_marshalling_rejects_malformed_containers() {
    crate::common::return_if_neovm_enable_oracle_proptest_not_set!();
    PrivateBus::start().parity(r#"(progn
      (require 'dbus)
      (mapcar (lambda (payload)
        (condition-case data
          (progn (dbus-send-signal :session nil "/org/neomacs/Oracle" "org.neomacs.Oracle" "Payload" payload) 'unexpected-success)
          (t (car data))))
       '((:dict-entry :string "key" :uint32 1)
         (:array (:dict-entry (:array "key") :uint32 1))
         (:array (:dict-entry "key"))
         (:variant :string "one" :string "two")
         (:struct)
         (:array :string "one" :uint32 2)
         (:array (:struct "one") (:struct "two" 3)))))"#);
}

#[test]
fn dbus_native_marshalling_rejects_invalid_path_and_signature() {
    crate::common::return_if_neovm_enable_oracle_proptest_not_set!();
    PrivateBus::start().parity(r#"(progn
      (require 'dbus)
      (mapcar (lambda (arguments)
        (condition-case data
          (progn (apply #'dbus-send-signal :session nil "/org/neomacs/Oracle" "org.neomacs.Oracle" "Payload" arguments) 'unexpected-success)
          (t (car data))))
       '((:object-path "invalid") (:signature "INVALID"))))"#);
}
