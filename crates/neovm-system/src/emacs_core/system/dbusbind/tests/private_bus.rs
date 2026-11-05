//! A private D-Bus daemon for tests that must not share the ambient buses.
//!
//! The machine's session bus is shared with every other process of the user
//! (and with every other test process / lane on this host). When its daemon
//! disconnects a peer under load, libdbus reports the connection closed and a
//! test that merely pumps it looks broken even though its own messages are
//! fine. GNU's D-Bus tests run under `dbus-run-session` for the same reason.
//! Tests that check dispatch start their own daemon, point both ambient bus
//! addresses at it, and tear it down with the test.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) struct PrivateBus {
    child: Child,
    dir: PathBuf,
    pub(crate) address: String,
    killed: bool,
}

impl PrivateBus {
    /// Start `dbus-daemon` on a private socket and return it with its address.
    pub(crate) fn start(tag: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "neovm-dbus-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the private bus directory");
        let mut child = Command::new("dbus-daemon")
            .arg("--session")
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--address=unix:tmpdir={}", dir.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn dbus-daemon for the test's private bus");
        let mut reader = BufReader::new(child.stdout.take().expect("daemon stdout"));
        let mut address = String::new();
        reader
            .read_line(&mut address)
            .expect("dbus-daemon prints its address");
        let address = address.trim().to_owned();
        assert!(
            address.starts_with("unix:"),
            "dbus-daemon printed an unexpected address: {address:?}"
        );
        Self {
            child,
            dir,
            address,
            killed: false,
        }
    }

    /// Point the client library's ambient buses at this daemon until the
    /// returned guard drops. Both `:session` and `:system` are redirected so
    /// `lisp/net/dbus.el`'s `dbus--init` cannot reach a host bus either.
    pub(crate) fn install_env(&self) -> PrivateBusEnv {
        PrivateBusEnv::install(&self.address)
    }

    /// Kill the daemon and wait for it, so the client's socket sees EOF.
    pub(crate) fn kill(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.kill();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Restores the caller's ambient bus addresses on drop.
pub(crate) struct PrivateBusEnv {
    session: Option<std::ffi::OsString>,
    system: Option<std::ffi::OsString>,
}

impl PrivateBusEnv {
    fn install(address: &str) -> Self {
        let session = std::env::var_os("DBUS_SESSION_BUS_ADDRESS");
        let system = std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS");
        // SAFETY: tests are process-per-test under nextest, and this guard
        // restores the exact previous values on drop.
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address);
            std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", address);
        }
        Self { session, system }
    }
}

impl Drop for PrivateBusEnv {
    fn drop(&mut self) {
        // SAFETY: restoring the values saved in `install`, on the same thread.
        unsafe {
            match self.session.take() {
                Some(value) => std::env::set_var("DBUS_SESSION_BUS_ADDRESS", value),
                None => std::env::remove_var("DBUS_SESSION_BUS_ADDRESS"),
            }
            match self.system.take() {
                Some(value) => std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", value),
                None => std::env::remove_var("DBUS_SYSTEM_BUS_ADDRESS"),
            }
        }
    }
}
