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
        let root = neomacs_infra::workspace_root().join("tmp");
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

    fn parity(&self, form: &str, expected: expect_test::Expect) {
        crate::common::assert_oracle_parity_with_env_expect(
            form,
            &[
                ("DBUS_SESSION_BUS_ADDRESS", &self.address),
                ("DBUS_FATAL_WARNINGS", "0"),
            ],
            expected,
        );
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn valid_compounds_expect() -> expect_test::Expect {
    expect_test::expect![[r#""OK (sent sent sent sent sent)""#]]
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
        valid_compounds_expect(),
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
         (:array (:struct "one") (:struct "two" 3)))))"#, expect_test::expect![[r#""OK (wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument)""#]]);
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
       '((:object-path "invalid") (:signature "INVALID"))))"#, expect_test::expect![[r#""OK (dbus-error dbus-error)""#]]);
}

/// Exercise the public evaluator in a child so snapshot selection stays local.
#[test]
fn dbus_native_marshalling_snapshot_rejects_wrong_outcome() {
    const CHILD: &str = "NEOMACS_DBUS_ORACLE_SNAPSHOT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        PrivateBus::start().parity("'wrong-outcome", valid_compounds_expect());
        return;
    }
    let selector = format!(
        "{}::dbus_native_marshalling_snapshot_rejects_wrong_outcome",
        module_path!().split_once("::").unwrap().1
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &selector, "--nocapture"])
        .env(CHILD, "1")
        .env("NEOVM_ORACLE_MODE", "snapshot")
        .env_remove("UPDATE_EXPECT")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "snapshot accepted an intentionally incorrect evaluator outcome: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("expect test failed"),
        "child failed without a snapshot mismatch: {}{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn dbus_native_marshalling_private_bus_uses_relocated_workspace() {
    const CHILD: &str = "NEOMACS_DBUS_ORACLE_RELOCATED_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let bus = PrivateBus::start();
        let expected =
            std::path::PathBuf::from(std::env::var_os("NEXTEST_WORKSPACE_ROOT").unwrap())
                .join("tmp");
        assert_eq!(bus._scratch.path().parent().unwrap(), expected);
        return;
    }
    let scratch_root = neomacs_infra::workspace_root().join("tmp");
    std::fs::create_dir_all(&scratch_root).unwrap();
    let relocated = tempfile::Builder::new()
        .prefix("relocated-dbus-oracle-")
        .tempdir_in(scratch_root)
        .unwrap();
    let selector = format!(
        "{}::dbus_native_marshalling_private_bus_uses_relocated_workspace",
        module_path!().split_once("::").unwrap().1
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &selector, "--nocapture"])
        .env(CHILD, "1")
        .env("NEXTEST_WORKSPACE_ROOT", relocated.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "relocated private bus failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
