//! The one GNU Emacs oracle resolver for every Rust test.
//!
//! Resolution has exactly two inputs:
//!
//! 1. `NEOVM_FORCE_ORACLE_PATH`, when set, names the executable.
//! 2. Otherwise the editor `emacs` on `PATH` is used.
//!
//! Either way the executable is attested against the checked-in
//! `parity-reference.toml` by [`neomacs_parity_reference::attest`] — the same
//! identity check the oracle harness runs — so holding a [`GnuOracle`] proves
//! the test is about to compare against the pinned release, not merely against
//! *an* Emacs.
//!
//! Resolution failure PANICS with the env var, the pinned identity, and the
//! one-recipe build script in the message.  There is deliberately no way to
//! get a value that means "probably fine" and no way to skip: a test that
//! needs GNU either runs against the pinned reference or fails loudly.  A
//! green run that silently measured nothing is the failure this module exists
//! to prevent.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use neomacs_parity_reference::{
    AttestationDepth, AttestationError, ReferenceUse, load_manifest, manifest_path,
};

/// Environment variable that names the GNU executable directly.
pub const ENV_OVERRIDE: &str = "NEOVM_FORCE_ORACLE_PATH";

/// The one recipe that builds an oracle the pin accepts.
pub const BUILD_SCRIPT: &str = "scripts/ci/build-gnu-oracle.sh";

/// An executable that attested as the pinned GNU Emacs release.
///
/// The only constructor is [`gnu_oracle`] / its fallible form, so holding this
/// value is itself the proof that the identity check ran and passed.
#[derive(Debug)]
pub struct GnuOracle {
    reference: ReferenceUse,
}

impl GnuOracle {
    /// The attested executable, canonicalized when the pin was checked.
    pub fn executable(&self) -> &Path {
        self.reference.executable()
    }

    /// The attested executable as an owned path for callers that move it into
    /// a `Command`.
    pub fn executable_path(&self) -> PathBuf {
        self.reference.executable().to_path_buf()
    }

    /// The full reference, including the recorded manifest.
    pub fn reference(&self) -> &ReferenceUse {
        &self.reference
    }

    /// Whether the identity check actually ran (false only for the explicit
    /// `NEOMACS_PARITY_REFERENCE=none` operator opt-out).
    pub fn is_attested(&self) -> bool {
        self.reference.is_attested()
    }

    /// The one-line identity a test's numbers can be reported with.
    pub fn stamp(&self) -> String {
        self.reference.stamp()
    }

    fn try_resolve() -> Result<Self, String> {
        let requested = std::env::var_os(ENV_OVERRIDE).map(PathBuf::from);
        let executable = requested.clone().unwrap_or_else(|| PathBuf::from("emacs"));
        match neomacs_parity_reference::attest(&executable, AttestationDepth::Fingerprint) {
            Ok(reference) => Ok(Self { reference }),
            Err(error) => Err(failure_message(requested.as_deref(), &executable, &error)),
        }
    }
}

/// Resolve the pinned GNU Emacs once per process, failing loudly.
///
/// Attestation runs the candidate once (`--batch --quick`) and reads the dump
/// header, so the result is cached.  Nextest gives each test its own process;
/// a libtest process reuses one attestation for its whole test set.
pub fn gnu_oracle() -> &'static GnuOracle {
    static ORACLE: OnceLock<Result<GnuOracle, String>> = OnceLock::new();
    match ORACLE.get_or_init(GnuOracle::try_resolve) {
        Ok(oracle) => oracle,
        // Panic on every call, not just the first: a test runner that catches
        // one test's panic must still fail the next test that needs GNU.
        Err(message) => panic!("{message}"),
    }
}

fn failure_message(
    requested: Option<&Path>,
    executable: &Path,
    error: &AttestationError,
) -> String {
    let manifest = match load_manifest() {
        Ok(manifest) => format!(
            "{} (GNU {}, mirror commit {})",
            manifest_path().display(),
            manifest.emacs_version,
            manifest.mirror_commit
        ),
        Err(error) => format!("{} (unreadable: {error})", manifest_path().display()),
    };
    let request = match requested {
        Some(path) => format!("{ENV_OVERRIDE}={}", path.display()),
        None => format!(
            "{ENV_OVERRIDE} is not set, so the editor `{}` on PATH was attested",
            executable.display()
        ),
    };
    format!(
        "no GNU Emacs oracle could be resolved, and a test that needs GNU must not \
         skip or compare against an unpinned editor.\n\
         \x20 request: {request}\n\
         \x20 pinned reference: {manifest}\n\
         \x20 attestation error: {error}\n\
         Build the canonical oracle with the one recipe:\n\
         \x20   {BUILD_SCRIPT} <mirror_commit> <prefix>\n\
         then set {ENV_OVERRIDE}=<prefix>/bin/emacs (or put that bin directory \
         first on PATH)."
    )
}
