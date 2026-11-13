//! Attestation that a Neomacs binary and the Lisp tree it loads came from the
//! same `cargo xtask fresh-build` run.
//!
//! The oracle suites compare a built editor against GNU Emacs.  An editor that
//! is *not* running the tree it was dumped with is not the product under test:
//! a `DEBUG` build without its pdump source-bootstraps the tree, and a tree
//! whose bytecode was deleted or replaced after the build loads sources
//! instead of the compiled files the dump expects.  Both change observable
//! behaviour (macroexpansion order and gensym counters, lazy docstring
//! references, preloaded file lists), so both turn a parity comparison into a
//! comparison against a bootstrap.  A remote lane hit exactly this: an rsync
//! `--delete` removed the freshly built `.elc` files after the build, every
//! Lisp file then loaded as source, and six oracle tests "diverged" from GNU
//! until a fresh build was run.
//!
//! The attestation is a **stamp** written by `cargo xtask fresh-build` next to
//! the binary it built:
//!
//! ```text
//! NEOMACS-FRESH-BUILD-STAMP v1
//! binary_sha256 <hex>
//! lisp_manifest_sha256 <hex>
//! lisp_files <count>
//! ```
//!
//! * `binary_sha256` is the SHA-256 of the binary the recipe points
//!   `NEOVM_BINARY_PATH` at.
//! * `lisp_manifest_sha256` is the SHA-256 of the **generated-Lisp manifest**:
//!   every non-dotfile regular file under `<runtime-root>/lisp`, sorted by
//!   relative path, rendered as `path\0<file-sha256>\n` and hashed.  It covers
//!   sources and compiled `.elc` alike, so deleting, adding or changing any
//!   file after the build changes the manifest.  It is content, not mtime:
//!   `rsync -a` preserves timestamps, and timestamps are therefore not
//!   evidence.
//!
//! Verification is content-based for the same reason.  It also checks the
//! **loadup preloaded set** directly: every `.el` that GNU's `compile-main`
//! rule would byte-compile and that `lisp/loadup.el` preloads must have its
//! `.elc` beside it.  The list comes from `loadup.el` with GNU's own
//! `src/Makefile.in` rule (shared with `xtask`), never from a glob of the
//! tree, so the check names the file the build should have produced.
//!
//! # Cost: one full check per test run, not per test
//!
//! Nextest runs every test in its own process, and the oracle package has tens
//! of thousands of them.  Re-hashing a ~140 MiB Lisp tree and an ~80 MiB
//! binary in every test process would add hours of CPU to a run that takes
//! one.  The first process of a nextest run therefore performs the full
//! check, and the result is cached in
//! `<binary>.fresh-build-stamp.verified`, keyed by the stamp's own SHA-256 and
//! by `NEXTEST_RUN_ID` (nextest sets a fresh id per run).  Later processes of
//! the same run read the cache; a new run (new id) or a new stamp re-verifies.
//! Outside nextest there is no run id and every process verifies fully.
//!
//! # The one override
//!
//! [`OVERRIDE_ENV`] (`NEOVM_SKIP_FRESH_BUILD_STAMP`) skips the check when set
//! to a non-empty value.  It exists for deliberate experiments with a runtime
//! the build did not produce (a hand-made dump, a partially built tree).  Its
//! value is echoed to stderr by [`verify`] so a log always records that the
//! run was unattested.  Nothing in CI or the oracle recipes sets it.
//!
//! The guarantee is deliberately not a security boundary: it keeps honest
//! build/sync accidents from being measured, not a hostile actor from lying.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[path = "../../neovm-core/build_support/compile_main_rule.rs"]
mod compile_main_rule;

/// Suffix appended to the binary path to name its stamp.
pub const STAMP_SUFFIX: &str = ".fresh-build-stamp";

/// First line of a stamp file.
pub const STAMP_MAGIC: &str = "NEOMACS-FRESH-BUILD-STAMP v1";

/// The one documented override.  See the module docs.
pub const OVERRIDE_ENV: &str = "NEOVM_SKIP_FRESH_BUILD_STAMP";

/// Environment variable nextest sets to a fresh id for each run.
pub const RUN_ID_ENV: &str = "NEXTEST_RUN_ID";

/// The values a fresh build attests for one binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stamp {
    /// SHA-256 of the binary the stamp was written beside.
    pub binary_sha256: String,
    /// SHA-256 of the generated-Lisp manifest (see the module docs).
    pub lisp_manifest_sha256: String,
    /// Number of Lisp files the manifest covered; a diagnostic.
    pub lisp_files: usize,
}

/// The outcome of a successful attestation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StampVerdict {
    /// The stamp matches the binary and its Lisp tree.
    Verified {
        /// SHA-256 recomputed from the binary.
        binary_sha256: String,
        /// Number of Lisp files the manifest covered.
        lisp_files: usize,
    },
    /// The operator set [`OVERRIDE_ENV`]; nothing was checked.
    Overridden {
        /// The override variable's name.
        variable: &'static str,
        /// The value it was set to, echoed for the log.
        value: String,
    },
}

impl StampVerdict {
    /// The line a harness should emit for an override, or `None`.
    pub fn log_line(&self) -> Option<String> {
        match self {
            Self::Verified { .. } => None,
            Self::Overridden { variable, value } => Some(format!(
                "{variable}={value}: fresh-build stamp verification SKIPPED at the operator's request; \
                 the editor's Lisp tree is NOT attested"
            )),
        }
    }
}

/// Path of the stamp written beside `binary`.
pub fn stamp_path(binary: &Path) -> PathBuf {
    let mut name = binary.as_os_str().to_os_string();
    name.push(STAMP_SUFFIX);
    PathBuf::from(name)
}

fn verification_cache_path(binary: &Path) -> PathBuf {
    let mut name = stamp_path(binary).into_os_string();
    name.push(".verified");
    PathBuf::from(name)
}

fn verification_lock_path(binary: &Path) -> PathBuf {
    let mut name = stamp_path(binary).into_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

/// SHA-256 of a file's bytes, lowercase hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The generated-Lisp manifest: `(sha256, file count)` for every non-dotfile
/// regular file under `lisp_root`, in sorted relative-path order.
///
/// Symlinked directories are not followed and symlinked files are not
/// included: the build produces a real tree, and following links would let a
/// verification depend on state outside the tree it is attesting.
pub fn lisp_manifest(lisp_root: &Path) -> io::Result<(String, usize)> {
    let mut files = Vec::new();
    collect_lisp_files(lisp_root, Path::new(""), &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (relative, full) in &files {
        let digest = sha256_file(full)?;
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        hasher.update(digest.as_bytes());
        hasher.update(b"\n");
    }
    Ok((hex(&hasher.finalize()), files.len()))
}

fn collect_lisp_files(
    root: &Path,
    relative: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;
        let child_relative = relative.join(&name);
        if file_type.is_dir() {
            collect_lisp_files(&entry.path(), &child_relative, out)?;
        } else if file_type.is_file() {
            let rendered = child_relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rendered, entry.path()));
        }
    }
    Ok(())
}

fn render_stamp(stamp: &Stamp) -> String {
    format!(
        "{STAMP_MAGIC}\nbinary_sha256 {}\nlisp_manifest_sha256 {}\nlisp_files {}\n",
        stamp.binary_sha256, stamp.lisp_manifest_sha256, stamp.lisp_files
    )
}

/// Parse a stamp file.  Unknown keys are rejected: a stamp this code does not
/// understand cannot attest anything.
pub fn parse_stamp(contents: &str) -> Result<Stamp, String> {
    let mut lines = contents.lines();
    match lines.next() {
        Some(line) if line.trim() == STAMP_MAGIC => {}
        Some(other) => {
            return Err(format!(
                "fresh-build stamp has an unknown header line {other:?}; expected {STAMP_MAGIC:?}. \
                 Re-run `cargo xtask fresh-build --release`."
            ));
        }
        None => return Err("fresh-build stamp is empty".to_string()),
    }
    let mut binary = None;
    let mut manifest = None;
    let mut files = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(' ') else {
            return Err(format!("fresh-build stamp has a malformed line {line:?}"));
        };
        match key {
            "binary_sha256" => binary = Some(value.trim().to_string()),
            "lisp_manifest_sha256" => manifest = Some(value.trim().to_string()),
            "lisp_files" => {
                files = Some(value.trim().parse::<usize>().map_err(|_| {
                    format!("fresh-build stamp has a non-numeric lisp_files value {value:?}")
                })?)
            }
            other => {
                return Err(format!(
                    "fresh-build stamp has an unknown key {other:?}; this code does not know how to \
                     attest it. Re-run `cargo xtask fresh-build --release`."
                ));
            }
        }
    }
    let binary_sha256 = binary.ok_or("fresh-build stamp is missing binary_sha256")?;
    let lisp_manifest_sha256 =
        manifest.ok_or("fresh-build stamp is missing lisp_manifest_sha256")?;
    let lisp_files = files.ok_or("fresh-build stamp is missing lisp_files")?;
    Ok(Stamp {
        binary_sha256,
        lisp_manifest_sha256,
        lisp_files,
    })
}

/// Write the fresh-build stamp beside `binary`.
///
/// Called by `cargo xtask fresh-build` after the final dump and the
/// compile-main pass, when both the binary and the Lisp tree are final.  Any
/// earlier verification cache is removed: a new stamp invalidates cached
/// verdicts for the old one.
pub fn write_stamp(binary: &Path, lisp_root: &Path) -> io::Result<PathBuf> {
    let binary_sha256 = sha256_file(binary)?;
    let (lisp_manifest_sha256, lisp_files) = lisp_manifest(lisp_root)?;
    let stamp = Stamp {
        binary_sha256,
        lisp_manifest_sha256,
        lisp_files,
    };
    let path = stamp_path(binary);
    let temporary = path.with_extension("fresh-build-stamp.tmp");
    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(render_stamp(&stamp).as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temporary, &path)?;
    let _ = fs::remove_file(verification_cache_path(binary));
    Ok(path)
}

/// GNU's `src/Makefile.in` rule: the `.el` sources `loadup.el` preloads and
/// that must be byte-compiled.
///
/// The parse is GNU's own sed pipeline:
/// `sed -n 's/^[ \t]*(load "\([^"]*\)".*/\1/p' loadup.el` followed by
/// `s/$/.elc/; s/\.el\.elc/.el/`, with GNU's three non-preloaded names removed
/// and `loaddefs.elc` always added.  Files that declare `no-byte-compile: t`
/// are loaded as sources and are not required to have bytecode.
pub fn parse_preloaded_sources(loadup: &Path, lisp_root: &Path) -> io::Result<Vec<PathBuf>> {
    let contents = fs::read_to_string(loadup)?;
    Ok(parse_preloaded_sources_from_str(&contents, lisp_root))
}

/// [`parse_preloaded_sources`] against already-read `loadup.el` contents.
pub fn parse_preloaded_sources_from_str(contents: &str, lisp_root: &Path) -> Vec<PathBuf> {
    let mut targets = std::collections::BTreeSet::new();
    for line in contents.lines() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let Some(rest) = trimmed.strip_prefix("(load \"") else {
            continue;
        };
        let Some((library, _)) = rest.split_once('"') else {
            continue;
        };
        let target = if library.ends_with(".el") {
            library.to_string()
        } else {
            format!("{library}.elc")
        };
        targets.insert(target);
    }
    targets.remove("leim/leim-list.el");
    targets.remove("site-load.elc");
    targets.remove("site-init.elc");
    targets.insert("loaddefs.elc".to_string());

    let mut sources = Vec::new();
    push_preloaded_source("loaddefs.elc", lisp_root, &mut sources);
    for target in targets {
        if target != "loaddefs.elc" {
            push_preloaded_source(&target, lisp_root, &mut sources);
        }
    }
    sources
}

fn push_preloaded_source(target: &str, lisp_root: &Path, out: &mut Vec<PathBuf>) {
    let Some(stem) = target.strip_suffix(".elc") else {
        return;
    };
    let source = lisp_root.join(format!("{stem}.el"));
    if source.is_file()
        && !compile_main_rule::source_declares_no_byte_compile(&source).unwrap_or(false)
    {
        out.push(source);
    }
}

/// Preloaded sources whose `.elc` is missing.
///
/// The loadup list is the contract: every preloaded `.el` that GNU's
/// `compile-main` rule would compile must have bytecode beside it, or the
/// editor loads that file as source and is not the dumped product.
pub fn missing_preloaded_bytecode(lisp_root: &Path) -> io::Result<Vec<PathBuf>> {
    let loadup = lisp_root.join("loadup.el");
    let sources = parse_preloaded_sources(&loadup, lisp_root)?;
    let mut missing = Vec::new();
    for source in sources {
        let compiled = source.with_extension("elc");
        if !compiled.is_file() {
            missing.push(compiled);
        }
    }
    Ok(missing)
}

/// Verify the stamp for `binary` against `lisp_root`.
///
/// Reads [`OVERRIDE_ENV`] and [`RUN_ID_ENV`] from the process environment; the
/// cache is described in the module docs.  On any mismatch returns an
/// actionable, multi-line message.
pub fn verify(binary: &Path, lisp_root: &Path) -> Result<StampVerdict, String> {
    let override_value = std::env::var(OVERRIDE_ENV).ok();
    let run_id = std::env::var(RUN_ID_ENV).ok();
    let verdict = verify_with(
        binary,
        lisp_root,
        override_value.as_deref(),
        run_id.as_deref(),
    )?;
    if let Some(line) = verdict.log_line() {
        eprintln!("{line}");
    }
    Ok(verdict)
}

/// [`verify`] with the environment supplied by the caller (for tests and for
/// callers that already resolved the override deliberately).
pub fn verify_with(
    binary: &Path,
    lisp_root: &Path,
    override_value: Option<&str>,
    run_id: Option<&str>,
) -> Result<StampVerdict, String> {
    if let Some(value) = override_value {
        if !value.is_empty() {
            return Ok(StampVerdict::Overridden {
                variable: OVERRIDE_ENV,
                value: value.to_string(),
            });
        }
    }

    let path = stamp_path(binary);
    let stamp_text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "no readable fresh-build stamp at {}: {error}.\n\
             This binary has no attested Lisp tree. Run `cargo xtask fresh-build --release` for \
             this binary (the stamp is written beside it), or set {OVERRIDE_ENV}=1 to override \
             deliberately.",
            path.display()
        )
    })?;
    let stamp = parse_stamp(&stamp_text)?;
    let stamp_sha = sha256_bytes(stamp_text.as_bytes());

    if let Some(run_id) = run_id.filter(|id| !id.is_empty()) {
        if let Some(verdict) = read_cache(binary, &stamp_sha, run_id) {
            return verdict;
        }
        return with_verification_lock(binary, || {
            if let Some(verdict) = read_cache(binary, &stamp_sha, run_id) {
                return verdict;
            }
            let verdict = full_check(binary, lisp_root, &stamp);
            write_cache(binary, &stamp_sha, run_id, &verdict);
            verdict
        });
    }

    full_check(binary, lisp_root, &stamp)
}

fn full_check(binary: &Path, lisp_root: &Path, stamp: &Stamp) -> Result<StampVerdict, String> {
    let missing = missing_preloaded_bytecode(lisp_root).map_err(|error| {
        format!(
            "could not read the loadup preloaded list under {}: {error}",
            lisp_root.display()
        )
    })?;
    if !missing.is_empty() {
        let mut listed = missing
            .iter()
            .take(8)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ");
        if missing.len() > 8 {
            listed.push_str(&format!("\n  ... and {} more", missing.len() - 8));
        }
        return Err(format!(
            "the Lisp tree at {} is missing bytecode required by loadup.el ({} preloaded .el \
             without a .elc):\n  {listed}\n\
             The editor would load those files as sources instead of the compiled product. \
             Re-run `cargo xtask fresh-build --release` for this binary (a --no-byte-compile \
             build cannot back an oracle run), or set {OVERRIDE_ENV}=1 to override deliberately.",
            lisp_root.display(),
            missing.len()
        ));
    }

    let binary_sha256 = sha256_file(binary).map_err(|error| {
        format!(
            "could not hash the Neomacs binary {}: {error}",
            binary.display()
        )
    })?;
    if binary_sha256 != stamp.binary_sha256 {
        return Err(format!(
            "the fresh-build stamp at {} belongs to a different binary:\n  sha256({}) = {}\n  \
             stamp records {}\n\
             Run `cargo xtask fresh-build --release` for this binary, or set {OVERRIDE_ENV}=1 to \
             override deliberately.",
            stamp_path(binary).display(),
            binary.display(),
            binary_sha256,
            stamp.binary_sha256
        ));
    }

    let (manifest_sha256, lisp_files) = lisp_manifest(lisp_root).map_err(|error| {
        format!(
            "could not hash the Lisp tree under {}: {error}",
            lisp_root.display()
        )
    })?;
    if manifest_sha256 != stamp.lisp_manifest_sha256 {
        return Err(format!(
            "the Lisp tree at {} does not match the manifest recorded by the fresh-build stamp \
             for {}:\n  sha256 of {} files = {}\n  stamp records {}\n\
             A sync, deletion or regeneration changed the tree after the build (this is the \
             deleted-.elc failure: the editor then loads sources). Re-run `cargo xtask \
             fresh-build --release` for this binary, or set {OVERRIDE_ENV}=1 to override \
             deliberately.",
            lisp_root.display(),
            binary.display(),
            lisp_files,
            manifest_sha256,
            stamp.lisp_manifest_sha256
        ));
    }

    Ok(StampVerdict::Verified {
        binary_sha256,
        lisp_files,
    })
}

fn read_cache(
    binary: &Path,
    stamp_sha: &str,
    run_id: &str,
) -> Option<Result<StampVerdict, String>> {
    let text = fs::read_to_string(verification_cache_path(binary)).ok()?;
    let mut lines = text.splitn(2, '\n');
    let key = lines.next()?;
    let expected_key = format!("{stamp_sha} {run_id}");
    if key != expected_key {
        return None;
    }
    let payload = lines.next().unwrap_or("");
    if let Some(ok) = payload.strip_prefix("ok\n") {
        let mut fields = ok.split(' ');
        let binary_sha256 = fields.next()?.to_string();
        // The payload line ends with a newline; trim it or every process
        // reads a parse failure and falls back to a full re-verification
        // (measured: ~5s per test process on the 222MB binary+tree pair).
        let lisp_files = fields.next()?.trim().parse::<usize>().ok()?;
        Some(Ok(StampVerdict::Verified {
            binary_sha256,
            lisp_files,
        }))
    } else if let Some(message) = payload.strip_prefix("fail\n") {
        Some(Err(message.to_string()))
    } else {
        None
    }
}

fn write_cache(
    binary: &Path,
    stamp_sha: &str,
    run_id: &str,
    verdict: &Result<StampVerdict, String>,
) {
    let payload = match verdict {
        Ok(StampVerdict::Verified {
            binary_sha256,
            lisp_files,
        }) => format!("ok\n{binary_sha256} {lisp_files}\n"),
        Ok(StampVerdict::Overridden { .. }) => return,
        Err(message) => format!("fail\n{message}\n"),
    };
    let path = verification_cache_path(binary);
    let temporary = path.with_extension("verified.tmp");
    let contents = format!("{stamp_sha} {run_id}\n{payload}");
    if fs::write(&temporary, contents).is_ok() {
        let _ = fs::rename(&temporary, &path);
    }
}

fn with_verification_lock<T>(binary: &Path, check: impl FnOnce() -> T) -> T {
    let path = verification_lock_path(binary);
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .ok();
    #[cfg(unix)]
    if let Some(file) = &file {
        use std::os::unix::io::AsRawFd;
        // SAFETY: `file` owns a valid descriptor for the duration of the call.
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_EX);
        }
    }
    let result = check();
    #[cfg(unix)]
    if let Some(file) = &file {
        use std::os::unix::io::AsRawFd;
        // SAFETY: `file` owns a valid descriptor for the duration of the call.
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_UN);
        }
    }
    let _ = file;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        binary: PathBuf,
        lisp_root: PathBuf,
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path();
        let lisp_root = root.join("lisp");
        fs::create_dir_all(&lisp_root).expect("lisp root");
        fs::write(
            lisp_root.join("loadup.el"),
            "(load \"subr\")\n(load \"simple\")\n",
        )
        .expect("loadup");
        for name in ["loaddefs", "subr", "simple"] {
            fs::write(lisp_root.join(format!("{name}.el")), format!(";; {name}\n"))
                .expect("source");
            fs::write(
                lisp_root.join(format!("{name}.elc")),
                format!(";ELC {name}\n"),
            )
            .expect("bytecode");
        }
        fs::write(lisp_root.join("data.txt"), "not lisp\n").expect("data");
        // The one tracked bootstrap file fresh-build regenerates and then
        // restores before stamping; it is part of the manifest like any file.
        fs::write(lisp_root.join("ldefs-boot.el"), ";; tracked autoloads\n").expect("ldefs-boot");
        let binary = root.join("neomacs");
        fs::write(&binary, b"fake binary for the stamp test\n").expect("binary");
        write_stamp(&binary, &lisp_root).expect("stamp");
        Fixture {
            _directory: directory,
            binary,
            lisp_root,
        }
    }

    #[test]
    fn a_fresh_tree_verifies() {
        let fixture = fixture();
        let verdict = verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect("verify");
        match verdict {
            StampVerdict::Verified {
                binary_sha256,
                lisp_files,
            } => {
                assert_eq!(binary_sha256, sha256_file(&fixture.binary).unwrap());
                assert!(lisp_files >= 7, "manifest covered {lisp_files} files");
            }
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn a_deleted_preloaded_elc_refuses() {
        let fixture = fixture();
        fs::remove_file(fixture.lisp_root.join("subr.elc")).expect("delete .elc");
        let message =
            verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect_err("must refuse");
        assert!(
            message.contains("subr.elc"),
            "the refusal must name the missing bytecode: {message}"
        );
        assert!(
            message.contains("loadup.el"),
            "the refusal must cite the loadup contract: {message}"
        );
    }

    #[test]
    fn a_mismatched_stamp_refuses() {
        let fixture = fixture();
        let path = stamp_path(&fixture.binary);
        let text = fs::read_to_string(&path).expect("stamp");
        let tampered = text.replace(
            &sha256_file(&fixture.binary).unwrap(),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert_ne!(tampered, text, "the test must actually change the stamp");
        fs::write(&path, tampered).expect("tamper");
        let message =
            verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect_err("must refuse");
        assert!(
            message.contains("belongs to a different binary"),
            "the refusal must name the binary mismatch: {message}"
        );
    }

    #[test]
    fn a_missing_stamp_refuses() {
        let fixture = fixture();
        fs::remove_file(stamp_path(&fixture.binary)).expect("delete stamp");
        let message =
            verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect_err("must refuse");
        assert!(
            message.contains("fresh-build"),
            "the refusal must name the build recipe: {message}"
        );
    }

    #[test]
    fn the_override_reports_itself() {
        let fixture = fixture();
        fs::remove_file(stamp_path(&fixture.binary)).expect("delete stamp");
        let verdict = verify_with(&fixture.binary, &fixture.lisp_root, Some("1"), None)
            .expect("override must run");
        match &verdict {
            StampVerdict::Overridden { variable, value } => {
                assert_eq!(*variable, OVERRIDE_ENV);
                assert_eq!(value, "1");
            }
            other => panic!("expected Overridden, got {other:?}"),
        }
        let line = verdict.log_line().expect("override must be logged");
        assert!(
            line.contains("NEOVM_SKIP_FRESH_BUILD_STAMP=1"),
            "the log line must echo the override: {line}"
        );
    }

    /// A fresh-build stamp rewrite invalidates the run cache: the cache key
    /// includes the stamp's own sha256, so the same nextest run id re-verifies
    /// against the new stamp instead of trusting the old verdict.
    #[test]
    fn a_new_stamp_invalidates_the_run_cache() {
        let fixture = fixture();
        verify_with(&fixture.binary, &fixture.lisp_root, None, Some("run-a"))
            .expect("first verify");
        // A rebuild: new binary bytes AND a stamp written for them.
        fs::write(&fixture.binary, b"a rebuilt binary\n").expect("replace binary");
        write_stamp(&fixture.binary, &fixture.lisp_root).expect("rewrite stamp");
        // The new stamp's key must miss the old cache entry; prove it by
        // breaking the tree after the stamp was written.
        fs::write(
            fixture.lisp_root.join("data.txt"),
            "changed after the stamp\n",
        )
        .expect("edit the tree");
        let message = verify_with(&fixture.binary, &fixture.lisp_root, None, Some("run-a"))
            .expect_err("the new stamp must re-verify and refuse the changed tree");
        assert!(
            message.contains("does not match the manifest"),
            "the refusal must name the manifest: {message}"
        );
    }

    #[test]
    fn a_run_id_cache_hit_skips_reverification_and_a_new_run_id_does_not() {
        let fixture = fixture();
        verify_with(&fixture.binary, &fixture.lisp_root, None, Some("run-cache"))
            .expect("first verify");
        fs::write(&fixture.binary, b"changed after the first check\n").expect("change binary");
        verify_with(&fixture.binary, &fixture.lisp_root, None, Some("run-cache"))
            .expect("the same run id must hit the cache");
        let message = verify_with(&fixture.binary, &fixture.lisp_root, None, Some("run-other"))
            .expect_err("a different run id must verify fully");
        assert!(
            message.contains("belongs to a different binary"),
            "the new run must see the changed binary: {message}"
        );
    }

    /// Peer O1: without nextest's run id there is no verification cache to
    /// shortcut, so every call re-hashes and sees a change made between calls.
    #[test]
    fn without_a_run_id_every_verification_is_full() {
        let fixture = fixture();
        verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect("first verify");
        assert!(
            !verification_cache_path(&fixture.binary).exists(),
            "a run without NEXTEST_RUN_ID must not write a verification cache"
        );
        fs::write(&fixture.binary, b"a changed binary\n").expect("change binary");
        let message = verify_with(&fixture.binary, &fixture.lisp_root, None, None)
            .expect_err("the second verify must see the changed binary");
        assert!(
            message.contains("belongs to a different binary"),
            "no cache may mask the change: {message}"
        );
    }

    /// Peer pinning requirement: a deliberate post-stamp edit of the tracked
    /// bootstrap file makes the preflight/harness refuse.
    #[test]
    fn a_post_stamp_ldefs_boot_edit_refuses() {
        let fixture = fixture();
        fs::write(
            fixture.lisp_root.join("ldefs-boot.el"),
            ";; edited after the stamp\n",
        )
        .expect("edit ldefs-boot");
        let message =
            verify_with(&fixture.binary, &fixture.lisp_root, None, None).expect_err("must refuse");
        assert!(
            message.contains("does not match the manifest"),
            "the refusal must name the manifest: {message}"
        );
    }

    #[test]
    fn the_manifest_ignores_mtime_only_rewrites_but_not_content() {
        let fixture = fixture();
        let before = lisp_manifest(&fixture.lisp_root).expect("manifest").0;
        let path = fixture.lisp_root.join("simple.el");
        let contents = fs::read(&path).expect("read");
        fs::write(&path, &contents).expect("rewrite same bytes");
        let after = lisp_manifest(&fixture.lisp_root).expect("manifest").0;
        assert_eq!(before, after, "content must decide, not timestamps");
        fs::write(&path, b";; changed\n").expect("change content");
        let changed = lisp_manifest(&fixture.lisp_root).expect("manifest").0;
        assert_ne!(before, changed, "changed content must change the manifest");
    }

    #[test]
    fn gnu_loadup_rule_is_the_shared_one() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().join("lisp");
        for name in [
            "subr.el",
            "emacs-lisp/cl-lib.el",
            "ldefs-boot.el",
            "leim/leim-list.el",
            "site-load.el",
        ] {
            if let Some(parent) = root.join(name).parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(root.join(name), "").expect("source");
        }
        let sources = parse_preloaded_sources_from_str(
            "(load \"subr\")\n  (load \"emacs-lisp/cl-lib\")\n(load \"ldefs-boot.el\")\n\
             (load \"leim/leim-list.el\")\n(load \"site-load\")\n",
            &root,
        );
        let rendered = sources
            .iter()
            .map(|path| {
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert!(rendered.contains(&"subr.el".to_string()));
        assert!(rendered.contains(&"emacs-lisp/cl-lib.el".to_string()));
        // GNU's sed pipeline rewrites `foo.el.elc` back to `foo.el`, so a
        // loadup line that names a `.el` explicitly is a source load and is
        // not in the byte-compiled preloaded set.
        assert!(!rendered.iter().any(|path| path == "ldefs-boot.el"));
        assert!(!rendered.iter().any(|path| path.contains("leim-list")));
        assert!(!rendered.iter().any(|path| path.contains("site-load")));
        // `loaddefs.elc` is always inserted by GNU's lisp.mk rule, but this
        // tree has no loaddefs.el, so it is not a source here.
        assert!(!rendered.iter().any(|path| path.contains("loaddefs")));
    }
}
