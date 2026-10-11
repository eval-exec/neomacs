//! Private native-notification counterparty for account-free Clatter tests.

use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{EmacsRuntime, PreparedPackageSet};

const SERVICE: &str = include_str!("../fixtures/clatter-notification-service.el");
const CLIENT: &str = include_str!("../fixtures/clatter-process-filter-notification.el");

struct ScopedProcess(Child);

impl Drop for ScopedProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_file(path: &Path, process: &mut ScopedProcess) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0) {
            return Ok(());
        }
        if let Some(status) = process.0.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "counterparty exited {status} before {}; inspect sibling logs",
                path.display()
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(format!(
        "timed out waiting for {}; inspect sibling logs",
        path.display()
    ))
}

/// Owns a private bus and a real GNU notification service, never a desktop bus.
/// Both subprocesses are killed and reaped when the fixture leaves scope.
pub struct PrivateClatterService {
    // Drop the service before the bus it uses.
    _service: ScopedProcess,
    _bus: ScopedProcess,
    control: PathBuf,
    address: String,
}

impl PrivateClatterService {
    pub fn start(control: &Path) -> Result<Self, String> {
        fs::create_dir_all(control.join("home")).map_err(|error| error.to_string())?;
        fs::create_dir_all(control.join("tmp")).map_err(|error| error.to_string())?;
        let address_file = control.join("bus-address");
        let socket_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut bus = ScopedProcess(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .arg(format!(
                    "--address=unix:abstract=neomacs-clatter-{}-{socket_id}",
                    std::process::id()
                ))
                .env("TMPDIR", control.join("tmp"))
                .stdout(File::create(&address_file).map_err(|error| error.to_string())?)
                .stderr(File::create(control.join("bus.log")).map_err(|error| error.to_string())?)
                .spawn()
                .map_err(|error| format!("start private Clatter bus: {error}"))?,
        );
        wait_for_file(&address_file, &mut bus)?;
        let address = fs::read_to_string(&address_file).map_err(|error| error.to_string())?;
        let address = address.trim().to_owned();
        if !address.starts_with("unix:abstract=") {
            return Err(format!("unexpected private bus address {address:?}"));
        }
        let script = control.join("service.el");
        fs::write(&script, SERVICE).map_err(|error| error.to_string())?;
        let mut service = ScopedProcess(
            EmacsRuntime::gnu_emacs()
                .with_env("DBUS_SESSION_BUS_ADDRESS", &address)
                .with_env("NEOMACS_CLATTER_CONTROL", control.as_os_str())
                .with_env("HOME", control.join("home").into_os_string())
                .with_env("TMPDIR", control.join("tmp").into_os_string())
                .command()
                .args(["--batch", "-Q", "-l"])
                .arg(&script)
                .stdout(
                    File::create(control.join("service.stdout"))
                        .map_err(|error| error.to_string())?,
                )
                .stderr(
                    File::create(control.join("service.stderr"))
                        .map_err(|error| error.to_string())?,
                )
                .spawn()
                .map_err(|error| format!("start GNU Clatter notification service: {error}"))?,
        );
        wait_for_file(&control.join("service-ready"), &mut service)?;
        Ok(Self {
            _service: service,
            _bus: bus,
            control: control.to_owned(),
            address,
        })
    }

    pub fn control(&self) -> &Path {
        &self.control
    }

    /// Apply only to the owned client command; the parent environment is untouched.
    pub fn environment(&self) -> Vec<(OsString, OsString)> {
        vec![
            (
                "DBUS_SESSION_BUS_ADDRESS".into(),
                self.address.clone().into(),
            ),
            (
                "NEOMACS_CLATTER_CONTROL".into(),
                self.control.clone().into_os_string(),
            ),
            ("HOME".into(), self.control.join("home").into_os_string()),
            ("TMPDIR".into(), self.control.join("tmp").into_os_string()),
            ("RUST_LOG".into(), "warn".into()),
        ]
    }

    pub fn configure_runtime(&self, mut runtime: EmacsRuntime) -> EmacsRuntime {
        for (key, value) in self.environment() {
            runtime = runtime.with_env(key, value);
        }
        runtime
    }

    pub fn write_client_script(&self, packages: &PreparedPackageSet) -> Result<PathBuf, String> {
        let script = self.control.join("client.el");
        fs::write(&script, format!("{}\n{CLIENT}", packages.startup_elisp()))
            .map_err(|error| error.to_string())?;
        Ok(script)
    }

    pub fn client_result(&self) -> Result<serde_json::Value, String> {
        read_json(&self.control.join("client.json"))
    }

    pub fn notification(&self) -> Result<serde_json::Value, String> {
        read_json(&self.control.join("notify.json"))
    }

    /// Independent literal established by the pinned package running on GNU.
    pub fn expected_notification() -> serde_json::Value {
        serde_json::json!({
            "count": 1, "app-name": "CLatter", "summary": "DM from alice",
            "body": "hello from local fixture", "actions": [], "urgency": 1,
            "category": "im.received", "timeout": 5000
        })
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}
