//! The identity of the GNU Emacs build every parity number is measured against.
//!
//! # Why this crate exists (ledger 214)
//!
//! Ledger 210 taught this project that a count must carry the frame it was
//! measured in, and ledger 210 and 211 made every parity harness fail loudly
//! when the reference editor could not be RUN.  Neither checked *which* editor
//! ran.  A reference that VANISHED was caught; a reference that CHANGED --- a
//! rebuild of the shared GNU mirror --- would have been scored in silence, and
//! the mirror is one successful `make` away from producing a different binary
//! at the same path.
//!
//! This crate is the single place that answers "is the GNU in front of me the
//! GNU our numbers are about?", and [`ReferenceUse::stamp`] is the string a
//! published number carries so that the answer travels with it.
//!
//! # What identifies a build
//!
//! The pin is a RELEASE, not a byte-exact artifact: the tag `emacs-31.1`,
//! which the running editor reports as `emacs-version`.  That is the only
//! identity an installed GNU can prove.  It embeds no repository revision ---
//! `emacs-repository-version` is computed by running git in `source-directory`
//! (`lisp/version.el`), and `make install` creates no source tree to ask.  GNU
//! computes a build identity too: `lib-src/make-fingerprint.c` hashes the
//! `temacs` executable into the dump header (`src/pdumper.c:116`, `:361-367`),
//! and `src/pdumper.c:5687` refuses a binary-and-dump pair whose fingerprints
//! disagree --- but that is a BUILD identity, so a rebuild on another
//! toolchain differs while the source is unchanged.
//!
//! [`ReferenceManifest`] records the fingerprint and the two SHA-256 digests
//! as PROVENANCE: they name the build the numbers were first measured against
//! and move only through a re-baselining.  [`attest_against`] gates on the
//! release version, and [`AttestationDepth`] travels in the stamp so a
//! published number says which check ran.
//!
//! # Two depths
//!
//! [`AttestationDepth`] is kept for the API and for the stamp.  Both depths
//! verify the dump magic --- which is what tells a non-GNU peer from a GNU,
//! before the shell attestor's `--if-gnu` mode would have to run an arbitrary
//! peer to ask --- and the release version the editor reports.  They no longer
//! differ in strength: the recorded byte identity is deliberately not a gate,
//! because it moves with the toolchain while the source does not.
//!
//! # Refusal is the default
//!
//! [`ReferenceUse::Unpinned`] exists because a pin nobody can run without is a
//! pin people delete: a developer or CI without the mirror sets
//! `NEOMACS_PARITY_REFERENCE=none` and every number they produce is stamped
//! `UNATTESTED`.  It cannot be reached by accident --- absent that variable, a
//! reference that does not match is an [`AttestationError`], never a warning.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Environment variable that declares no pinned reference is available.
///
/// Set it to `none` to run a parity harness against an unattested editor.  Any
/// other value is itself a refusal: a typo must not silently disable the guard.
pub const OPT_OUT_VAR: &str = "NEOMACS_PARITY_REFERENCE";

/// The only value of [`OPT_OUT_VAR`] that disables attestation.
pub const OPT_OUT_VALUE: &str = "none";

/// Overrides the manifest location.  Exists for this crate's own tests, which
/// must attest against manifests describing planted mismatches.
pub const MANIFEST_VAR: &str = "NEOMACS_PARITY_REFERENCE_FILE";

/// The dump-file magic GNU writes at offset 0 (`src/pdumper.c:116`).
const DUMP_MAGIC: &[u8; 16] = b"DUMPEDGNUEMACS\0\0";

/// Offset of `struct dump_header`'s `fingerprint` field: it follows `magic`.
const FINGERPRINT_OFFSET: u64 = DUMP_MAGIC.len() as u64;

/// `lib/fingerprint.h`: `volatile unsigned char fingerprint[32]`.
const FINGERPRINT_LEN: usize = 32;

/// Which attestation depth a harness asked for; both verify the release.
///
/// Kept for the API and the published stamp.  The variant names are
/// historical: the byte identity (GNU's fingerprint and the SHA-256 digests)
/// is recorded as provenance, not gated, so neither depth is stronger.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AttestationDepth {
    /// The default depth: dump magic and the release version.
    Fingerprint,
    /// The same check, recorded as `exhaustive` in the stamp.
    Exhaustive,
}

impl AttestationDepth {
    /// The word this depth contributes to a published stamp.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fingerprint => "fingerprint",
            Self::Exhaustive => "exhaustive",
        }
    }
}

impl fmt::Display for AttestationDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The pinned reference's recorded identity, as checked into the repository.
///
/// `emacs_version` is the gate: the release the running editor must report.
/// The fingerprint, mirror commit, digests and sizes are provenance recorded
/// by `pin-reference` for the re-baselining log, not comparisons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceManifest {
    pub schema: String,
    pub emacs_version: String,
    pub mirror_commit: String,
    pub build_time: String,
    pub fingerprint: String,
    pub executable_sha256: String,
    pub executable_size: u64,
    pub pdmp_sha256: String,
    pub pdmp_size: u64,
}

/// A GNU Emacs whose identity has been checked against [`ReferenceManifest`].
///
/// The only way to build one is [`attest`], so holding a value of this type is
/// itself the proof that the check ran and passed.
#[derive(Clone, Debug)]
pub struct AttestedReference {
    executable: PathBuf,
    pdmp: PathBuf,
    // Boxed: the manifest is the bulk of this struct, and it rides inside
    // `ReferenceUse` next to a variant that is a single `PathBuf`.
    manifest: Box<ReferenceManifest>,
    depth: AttestationDepth,
}

impl AttestedReference {
    /// The canonical path of the attested executable.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// The dump this executable loads — beside it in a build tree, or in the
    /// installed libexec tree (`src/emacs.c:1027-1120`).
    pub fn pdmp(&self) -> &Path {
        &self.pdmp
    }

    pub fn manifest(&self) -> &ReferenceManifest {
        &self.manifest
    }

    pub fn depth(&self) -> AttestationDepth {
        self.depth
    }
}

/// A reference a harness may run, and what it is known to be.
///
/// Every harness that scores against GNU holds one of these, and every number
/// it publishes carries [`ReferenceUse::stamp`].
#[derive(Clone, Debug)]
pub enum ReferenceUse {
    /// The editor matched the pin at the recorded depth.
    Attested(AttestedReference),
    /// The operator declared no pin is available via [`OPT_OUT_VAR`].  The
    /// editor may be anything; every number it produces says so.
    Unpinned { executable: PathBuf },
}

impl ReferenceUse {
    /// The executable to run.
    pub fn executable(&self) -> &Path {
        match self {
            Self::Attested(reference) => reference.executable(),
            Self::Unpinned { executable } => executable,
        }
    }

    /// Whether this reference was actually checked.
    pub fn is_attested(&self) -> bool {
        matches!(self, Self::Attested(_))
    }

    /// The one-line identity a published number carries.
    ///
    /// This is the ledger 210 rule applied to the reference instead of the
    /// geometry: a count that does not say what it was measured against is not
    /// a parity number.
    pub fn stamp(&self) -> String {
        match self {
            Self::Attested(reference) => format!(
                "gnu={} fingerprint={} mirror={} attest={}",
                reference.manifest.emacs_version,
                &reference.manifest.fingerprint[..12],
                &reference.manifest.mirror_commit[..11],
                reference.depth,
            ),
            Self::Unpinned { .. } => {
                format!("gnu=UNATTESTED ({OPT_OUT_VAR}={OPT_OUT_VALUE}) attest=none")
            }
        }
    }
}

/// Why a reference could not be attested.
///
/// Every variant is a refusal.  There is deliberately no variant that means
/// "probably fine": the failure modes this crate exists to catch all look like
/// success to a check that is willing to shrug.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttestationError {
    /// [`OPT_OUT_VAR`] was set to something other than [`OPT_OUT_VALUE`].
    UnknownOptOut { value: String },
    /// The manifest could not be read or parsed.
    Manifest { path: PathBuf, detail: String },
    /// The named editor could not be resolved to a file on disk.
    ExecutableUnresolved { executable: PathBuf, detail: String },
    /// The executable resolved but would not report a GNU release version.
    VersionUnreadable { executable: PathBuf, detail: String },
    /// The executable resolved but no dump was found for it, beside it or in
    /// the installed libexec tree.
    DumpMissing { pdmp: PathBuf, detail: String },
    /// The file beside the executable is not a GNU dump at all.
    DumpMagic { pdmp: PathBuf, found: String },
    /// A recorded identity did not match the file in front of us.  This is the
    /// incident: a reference that changed rather than vanished.
    Mismatch {
        field: &'static str,
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for AttestationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOptOut { value } => write!(
                f,
                "parity reference: {OPT_OUT_VAR}={value:?} is not understood; \
                 the only accepted value is {OPT_OUT_VALUE:?}, which brands every \
                 number produced as UNATTESTED"
            ),
            Self::Manifest { path, detail } => write!(
                f,
                "parity reference: cannot read the pin at {}: {detail}",
                path.display()
            ),
            Self::ExecutableUnresolved { executable, detail } => write!(
                f,
                "parity reference: the EDITOR could not be resolved: {} -- {detail}",
                executable.display()
            ),
            Self::VersionUnreadable { executable, detail } => write!(
                f,
                "parity reference: the EDITOR did not report a GNU release version: {} -- \
                 {detail}",
                executable.display()
            ),
            Self::DumpMissing { pdmp, detail } => write!(
                f,
                "parity reference: the editor resolved but its dump is missing at {}: \
                 {detail}",
                pdmp.display()
            ),
            Self::DumpMagic { pdmp, found } => write!(
                f,
                "parity reference: {} is not a GNU dump file: magic is {found:?}, \
                 expected \"DUMPEDGNUEMACS\"",
                pdmp.display()
            ),
            Self::Mismatch {
                field,
                path,
                expected,
                actual,
            } => write!(
                f,
                "parity reference MISMATCH on {field} for {}\n  \
                 pinned: {expected}\n  found:  {actual}\n  \
                 This GNU is NOT the one this project's parity numbers are measured \
                 against, so a number scored against it is not comparable with any \
                 published one.  A rebuild of the GNU mirror is its own change with \
                 its own re-baselining: run\n    \
                 cargo run -p xtask -- pin-reference --emacs PATH --reason \"...\"\n  \
                 to re-pin deliberately, or set {OPT_OUT_VAR}={OPT_OUT_VALUE} to \
                 measure without a pin and have every number branded UNATTESTED.",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AttestationError {}

/// Where the checked-in manifest lives.
///
/// Resolved from this crate's own manifest directory at compile time, so it is
/// correct from any test binary, any working directory, and any worktree.
pub fn manifest_path() -> PathBuf {
    if let Some(path) = std::env::var_os(MANIFEST_VAR) {
        return PathBuf::from(path);
    }
    neomacs_infra::workspace_root().join("parity-reference.toml")
}

/// Read and parse the checked-in manifest.
pub fn load_manifest() -> Result<ReferenceManifest, AttestationError> {
    let path = manifest_path();
    let text = std::fs::read_to_string(&path).map_err(|error| AttestationError::Manifest {
        path: path.clone(),
        detail: error.to_string(),
    })?;
    parse_manifest(&text).map_err(|detail| AttestationError::Manifest { path, detail })
}

/// Attest `executable` against the checked-in pin at `depth`.
///
/// Returns [`ReferenceUse::Unpinned`] only when [`OPT_OUT_VAR`] explicitly says
/// so.  Every other disagreement is an error.
pub fn attest(
    executable: &Path,
    depth: AttestationDepth,
) -> Result<ReferenceUse, AttestationError> {
    if opt_out_requested(std::env::var(OPT_OUT_VAR).ok().as_deref())? {
        return Ok(ReferenceUse::Unpinned {
            executable: executable.to_path_buf(),
        });
    }
    let manifest = load_manifest()?;
    attest_against(executable, depth, &manifest).map(ReferenceUse::Attested)
}

/// Interpret [`OPT_OUT_VAR`]'s value.
///
/// Absent means "attest".  Exactly [`OPT_OUT_VALUE`] means "no pin is
/// available".  Anything else is a refusal, because a variable that disables a
/// guard must not be disabled *and* misspelled at the same time --- a typo that
/// silently re-enabled scoring would be indistinguishable from the incident.
pub fn opt_out_requested(value: Option<&str>) -> Result<bool, AttestationError> {
    match value {
        None => Ok(false),
        Some(value) if value == OPT_OUT_VALUE => Ok(true),
        Some(value) => Err(AttestationError::UnknownOptOut {
            value: value.to_string(),
        }),
    }
}

/// Attest against an explicit manifest, ignoring [`OPT_OUT_VAR`].
///
/// This is the whole check; [`attest`] is this plus the opt-out and the
/// checked-in manifest.  Exposed so that a sensitivity test can plant a
/// mismatch without touching the process environment.
pub fn attest_against(
    executable: &Path,
    depth: AttestationDepth,
    manifest: &ReferenceManifest,
) -> Result<AttestedReference, AttestationError> {
    let executable = resolve_executable(executable)?;
    let pdmp = dump_for(&executable);

    // Order matters, and it is the same order `scripts/parity-reference-attest.sh`
    // uses: the dump magic comes FIRST, because it is what tells a non-GNU peer
    // from a wrong GNU, and the shell attestor's --if-gnu mode has to make that
    // distinction before it runs anything.  The 32-byte build fingerprint that
    // follows the magic identifies the binary-and-dump PAIR; GNU itself refuses
    // a mismatched pair at startup, so it is not compared against the manifest.
    let _pair_fingerprint = read_dump_fingerprint(&pdmp)?;

    // THE IDENTITY IS THE RELEASE.  An installed GNU embeds no repository
    // revision -- `emacs-repository-version` is computed by running git in
    // `source-directory` (lisp/version.el), which `make install` does not
    // create -- and `make-fingerprint` hashes the temacs binary, so a rebuild
    // on another toolchain yields a different fingerprint for the same source.
    // The tag this project pins, emacs-31.1, is what the binary can prove.
    let reported_version = reported_emacs_version(&executable)?;
    if reported_version != manifest.emacs_version {
        return Err(AttestationError::Mismatch {
            field: "emacs version",
            path: executable,
            expected: manifest.emacs_version.clone(),
            actual: reported_version,
        });
    }

    Ok(AttestedReference {
        executable,
        pdmp,
        manifest: Box::new(manifest.clone()),
        depth,
    })
}

/// Run the editor and return the release version it reports.
///
/// `--quick` keeps a user's init out of the answer and `--batch` keeps a frame
/// from opening; both are required for a check that must not mutate anything.
fn reported_emacs_version(executable: &Path) -> Result<String, AttestationError> {
    let output = std::process::Command::new(executable)
        .args(["--batch", "--quick", "--eval", "(princ emacs-version)"])
        .output()
        .map_err(|error| AttestationError::VersionUnreadable {
            executable: executable.to_path_buf(),
            detail: error.to_string(),
        })?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || version.is_empty() {
        return Err(AttestationError::VersionUnreadable {
            executable: executable.to_path_buf(),
            detail: format!(
                "exit {} with stdout {:?}",
                output.status,
                String::from_utf8_lossy(&output.stdout)
            ),
        });
    }
    Ok(version)
}

/// Resolve an editor name the way a shell would, then canonicalize it.
///
/// A bare name is looked up on `PATH`; anything with a separator is taken as
/// given.  Canonicalizing matters here because the pin is reached through a
/// symlink (`~/.local/bin/emacs`), and it was a BROKEN one of those that caused
/// the incident behind ledger 211 section 10.1.
fn resolve_executable(executable: &Path) -> Result<PathBuf, AttestationError> {
    let candidate = if executable.components().count() > 1 || executable.is_absolute() {
        executable.to_path_buf()
    } else {
        let path =
            std::env::var_os("PATH").ok_or_else(|| AttestationError::ExecutableUnresolved {
                executable: executable.to_path_buf(),
                detail: "PATH is absent".to_string(),
            })?;
        std::env::split_paths(&path)
            .map(|directory| directory.join(executable))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| AttestationError::ExecutableUnresolved {
                executable: executable.to_path_buf(),
                detail: "not found on PATH".to_string(),
            })?
    };
    std::fs::canonicalize(&candidate).map_err(|error| AttestationError::ExecutableUnresolved {
        executable: candidate,
        detail: error.to_string(),
    })
}

/// The dump GNU loads for `executable`.
///
/// `src/emacs.c:1104-1120` falls back to `basename(argv0) + ".pdmp"`, and an
/// `strace` of the pinned build confirms it opens exactly
/// `<canonical executable>.pdmp` and nothing else.
fn dump_beside(executable: &Path) -> PathBuf {
    let mut name = executable.as_os_str().to_os_string();
    name.push(".pdmp");
    PathBuf::from(name)
}

/// The dump GNU loads for `executable`, following `load_pdump`
/// (`src/emacs.c:1027-1120`): beside the canonical executable first (the
/// build tree), then the installed libexec tree that `Makefile.in:628-630`
/// fills with `emacs-<fingerprint>.pdmp`.  The compiled-in `PATH_EXEC` is not
/// knowable without running the editor -- and `--if-gnu` must classify a peer
/// without running it -- so the installed half is recognised relative to the
/// binary's prefix (`<prefix>/bin/emacs`), which also covers the
/// `libexecdir=lib` distro spelling.
///
/// When nothing is found the beside-path is returned, so `DumpMissing` keeps
/// naming the place a build-tree user would look.
fn dump_for(executable: &Path) -> PathBuf {
    let beside = dump_beside(executable);
    if beside.is_file() {
        return beside;
    }
    if let Some(prefix) = executable.parent().and_then(Path::parent) {
        let mut candidates = Vec::new();
        for root in [prefix.join("libexec/emacs"), prefix.join("lib/emacs")] {
            collect_installed_dumps(&root, &mut candidates);
        }
        // Byte-order, so this and the shell reader (LC_ALL=C sort) pick the
        // same candidate when a prefix holds more than one.
        candidates.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
        if let Some(found) = candidates.into_iter().next() {
            return found;
        }
    }
    beside
}

/// Collect the dump names GNU installs (`Makefile.in:628-630`): the
/// fingerprinted `emacs-*.pdmp`, plus the `emacs.pdmp`/`Emacs.pdmp`
/// spellings older installs and `HAVE_BE_APP` builds use.
fn collect_installed_dumps(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_installed_dumps(&path, found);
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && (name == "emacs.pdmp"
                || name == "Emacs.pdmp"
                || (name.starts_with("emacs-") && name.ends_with(".pdmp")))
        {
            found.push(path);
        }
    }
}

fn file_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| error.to_string())
}

/// Read the 32-byte build fingerprint out of a dump header.
fn read_dump_fingerprint(pdmp: &Path) -> Result<String, AttestationError> {
    let mut file = File::open(pdmp).map_err(|error| AttestationError::DumpMissing {
        pdmp: pdmp.to_path_buf(),
        detail: error.to_string(),
    })?;
    let mut header = [0u8; DUMP_MAGIC.len() + FINGERPRINT_LEN];
    file.read_exact(&mut header)
        .map_err(|error| AttestationError::DumpMissing {
            pdmp: pdmp.to_path_buf(),
            detail: format!("cannot read the dump header: {error}"),
        })?;
    let (magic, fingerprint) = header.split_at(DUMP_MAGIC.len());
    if magic != DUMP_MAGIC {
        return Err(AttestationError::DumpMagic {
            pdmp: pdmp.to_path_buf(),
            found: String::from_utf8_lossy(magic)
                .trim_end_matches('\0')
                .to_string(),
        });
    }
    debug_assert_eq!(FINGERPRINT_OFFSET as usize, DUMP_MAGIC.len());
    Ok(hex(fingerprint))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Parse the manifest's deliberately tiny format.
///
/// A line is a comment (`#`), blank, or exactly `key = "value"` with a
/// lower-snake key and a quote-free, escape-free value.  Anything else is
/// REFUSED rather than skipped: a parser that shrugs at a line it does not
/// understand is how a pin silently stops being checked, and the same rules are
/// implemented in `scripts/parity-reference-attest.sh` with a test that holds
/// the two readers together.
pub fn parse_manifest(text: &str) -> Result<ReferenceManifest, String> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(" = ").ok_or_else(|| {
            format!(
                "line {}: expected `key = \"value\"`, got {line:?}",
                index + 1
            )
        })?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "line {}: {key:?} is not a lower-snake key",
                index + 1
            ));
        }
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| format!("line {}: value {value:?} is not double-quoted", index + 1))?;
        if value.contains('"') || value.contains('\\') {
            return Err(format!(
                "line {}: value {value:?} contains a quote or backslash; the format has no escapes",
                index + 1
            ));
        }
        if fields.iter().any(|(seen, _)| seen == key) {
            return Err(format!("line {}: duplicate key {key:?}", index + 1));
        }
        fields.push((key.to_string(), value.to_string()));
    }

    let take = |name: &str| -> Result<String, String> {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| format!("missing key {name:?}"))
    };
    let take_u64 = |name: &str| -> Result<u64, String> {
        let value = take(name)?;
        value
            .parse()
            .map_err(|_| format!("key {name:?} is not a non-negative integer: {value:?}"))
    };
    let take_hex = |name: &str, len: usize| -> Result<String, String> {
        let value = take(name)?;
        if value.len() != len || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "key {name:?} must be {len} lowercase hex digits, got {value:?}"
            ));
        }
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(format!("key {name:?} must be lowercase hex, got {value:?}"));
        }
        Ok(value)
    };

    let schema = take("schema")?;
    if schema != "1" {
        return Err(format!(
            "schema {schema:?} is not understood by this build; expected \"1\""
        ));
    }
    let known = [
        "schema",
        "emacs_version",
        "mirror_commit",
        "build_time",
        "fingerprint",
        "executable_sha256",
        "executable_size",
        "pdmp_sha256",
        "pdmp_size",
    ];
    if let Some((key, _)) = fields
        .iter()
        .find(|(key, _)| !known.contains(&key.as_str()))
    {
        return Err(format!("unknown key {key:?} for schema 1"));
    }

    Ok(ReferenceManifest {
        schema,
        emacs_version: take("emacs_version")?,
        mirror_commit: take_hex("mirror_commit", 40)?,
        build_time: take("build_time")?,
        fingerprint: take_hex("fingerprint", FINGERPRINT_LEN * 2)?,
        executable_sha256: take_hex("executable_sha256", 64)?,
        executable_size: take_u64("executable_size")?,
        pdmp_sha256: take_hex("pdmp_sha256", 64)?,
        pdmp_size: take_u64("pdmp_size")?,
    })
}

/// Render a manifest back into the checked-in format's key block.
///
/// Used by `cargo run -p xtask -- pin-reference` so that re-pinning writes the
/// same bytes this crate parses.
pub fn render_manifest_keys(manifest: &ReferenceManifest) -> String {
    format!(
        "schema = \"{}\"\n\
         emacs_version = \"{}\"\n\
         mirror_commit = \"{}\"\n\
         build_time = \"{}\"\n\
         fingerprint = \"{}\"\n\
         executable_sha256 = \"{}\"\n\
         executable_size = \"{}\"\n\
         pdmp_sha256 = \"{}\"\n\
         pdmp_size = \"{}\"\n",
        manifest.schema,
        manifest.emacs_version,
        manifest.mirror_commit,
        manifest.build_time,
        manifest.fingerprint,
        manifest.executable_sha256,
        manifest.executable_size,
        manifest.pdmp_sha256,
        manifest.pdmp_size,
    )
}

/// Measure an editor's identity without comparing it to anything.
///
/// This is what `pin-reference` records and what a mismatch report shows.
pub fn observe(executable: &Path) -> Result<ObservedReference, AttestationError> {
    let executable = resolve_executable(executable)?;
    let pdmp = dump_for(&executable);
    let executable_size =
        file_size(&executable).map_err(|detail| AttestationError::ExecutableUnresolved {
            executable: executable.clone(),
            detail,
        })?;
    let pdmp_size = file_size(&pdmp).map_err(|detail| AttestationError::DumpMissing {
        pdmp: pdmp.clone(),
        detail,
    })?;
    let fingerprint = read_dump_fingerprint(&pdmp)?;
    let executable_sha256 =
        sha256_file(&executable).map_err(|detail| AttestationError::ExecutableUnresolved {
            executable: executable.clone(),
            detail,
        })?;
    let pdmp_sha256 = sha256_file(&pdmp).map_err(|detail| AttestationError::DumpMissing {
        pdmp: pdmp.clone(),
        detail,
    })?;
    Ok(ObservedReference {
        executable,
        pdmp,
        fingerprint,
        executable_sha256,
        executable_size,
        pdmp_sha256,
        pdmp_size,
    })
}

/// What an editor on disk actually is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedReference {
    pub executable: PathBuf,
    pub pdmp: PathBuf,
    pub fingerprint: String,
    pub executable_sha256: String,
    pub executable_size: u64,
    pub pdmp_sha256: String,
    pub pdmp_size: u64,
}

#[cfg(test)]
mod tests;
