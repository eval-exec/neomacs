//! Attested oracle resolution for TUI comparisons.
//!
//! A TUI comparison has two executables that can silently be the wrong
//! build:
//!
//! * GNU. The workspace's one resolver ([`neomacs_test_oracle::gnu`]) attests
//!   the executable against the checked-in `parity-reference.toml`
//!   (`NEOVM_FORCE_ORACLE_PATH`, else the `emacs` on `PATH`; a failure panics
//!   with the recipe). That check is by dump fingerprint, and a same-commit
//!   rebuild can pass it while shipping a different preloaded Lisp tree: a
//!   native-compiled build has inline docstrings where the canonical
//!   byte-compiled build has lazy `(#$ . POS)` references, so every preloaded
//!   closure's doc slot differs — exactly the values the mode-line property
//!   oracles compare. [`gnu_oracle_program`] therefore also probes the
//!   preloaded doc-slot shape and refuses anything but the canonical build.
//!
//! * Neomacs. A binary without its matching pdump does not fail: it silently
//!   source-bootstraps the Lisp tree with `purify-flag` nil, which changes
//!   dumped-state observables (lazy doc refs become absolute paths, no
//!   `preloaded-file-list` dump state). [`assert_neomacs_dumped_runtime`]
//!   refuses that case for the oracle tests instead of measuring the
//!   bootstrap.
//!
//! Neither helper falls back to an unattested editor, and neither can be
//! skipped: a comparison that cannot obtain the pinned pair panics with the
//! command that builds it.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The version `parity-reference.toml` pins and the mode-line oracles print.
const PINNED_VERSION: &str = "31.1";

/// Elisp that classifies the preloaded doc slot of `file-remote-p`.
///
/// The canonical byte-compiled GNU build loads this function from `files.elc`
/// with a lazy docstring, so slot 4 is `("files.elc" . POS)`. A
/// native-compiled (or otherwise rebuilt) tree carries the docstring itself,
/// and a non-byte-code definition cannot answer the comparison at all.
const DOC_SLOT_PROBE: &str = "(progn \
     (princ emacs-version) \
     (princ \" \") \
     (let ((f (and (fboundp 'file-remote-p) (symbol-function 'file-remote-p)))) \
       (princ (cond ((not (byte-code-function-p f)) \"non-bytecode\") \
                    ((< (length f) 5) \"short-bytecode\") \
                    ((stringp (aref f 4)) \"inline-docstring\") \
                    ((and (consp (aref f 4)) \
                          (stringp (car (aref f 4))) \
                          (integerp (cdr (aref f 4)))) \
                     (if (equal (file-name-nondirectory (car (aref f 4))) \
                                \"files.elc\") \
                         \"lazy-doc-ref\" \
                       \"lazy-doc-ref-other\")) \
                    (t \"unknown-doc-slot\")))))";

/// The attested, canonical GNU Emacs executable for a TUI comparison.
///
/// # Panics
///
/// When no attested oracle can be resolved, when the executable is not the
/// pinned version, or when its preloaded Lisp tree is not the canonical
/// byte-compiled one. The message names the executable, the observed state,
/// and the one build recipe.
pub fn gnu_oracle_program() -> PathBuf {
    let program = neomacs_test_oracle::gnu::gnu_oracle().executable_path();
    assert_canonical_gnu_doc_tree(&program);
    assert_recorded_fingerprint(&program);
    program
}

/// Recorded build fingerprints for canonical oracles whose location is known.
///
/// Keyed by a canonical-path SUFFIX, not by an absolute home directory: local
/// and treeroot install the same campaign layout under each host's home, so
/// the suffix is the host-stable identity. macmini records its own build when
/// its value is recorded. A canonical path with NO recorded suffix is still
/// gated by the version and lazy-doc probes. A path WITH one must also match
/// its recorded sha256, so a same-commit rebuild at the campaign's oracle
/// location (for example `-O3 -march=native -flto`, still non-native and
/// therefore carrying lazy docstrings) is refused instead of silently
/// becoming the reference.
const RECORDED_CANONICAL_BUILDS: &[(&str, &str)] = &[
    // Local + treeroot canonical oracle, recorded by the lead 2026-11-07.
    (
        ".local/gnu-emacs-oracle/a360712c9d27/bin/emacs-31.1",
        "4499316d14249cf360a347ff567a4691c371220d7dfd70753d631419a2bfddf5",
    ),
    // The 2026-10-28 GSettings + bundled Tree-sitter recipe build whose
    // executable_sha256 parity-reference.toml records.
    (
        ".local/gnu-emacs-oracle/a360712c9d27-gsettings-treesit/bin/emacs-31.1",
        "ebe445d3db56d694527431b0ded2f7b8528cfe931f8f6839a9e53c247a5a1af3",
    ),
];

fn recorded_fingerprint_for(program: &Path) -> Option<(PathBuf, &'static str)> {
    let canonical = std::fs::canonicalize(program).ok()?;
    let text = canonical.to_string_lossy();
    RECORDED_CANONICAL_BUILDS
        .iter()
        .find(|(suffix, _)| text.ends_with(suffix))
        .map(|(_, sha)| (canonical, *sha))
}

fn assert_recorded_fingerprint(program: &Path) {
    let Some((path, expected)) = recorded_fingerprint_for(program) else {
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "could not read the recorded canonical oracle {} for its sha256: {error}",
            path.display()
        )
    });
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        panic!(
            "the GNU Emacs oracle at {} is a rebuild: its sha256 is {actual}, \
             but the recorded canonical build is {expected}.\n\
             This refuses a same-commit rebuild at the campaign's canonical \
             oracle path (for example -O3 -march=native -flto, or a \
             native-compiled Lisp tree) even when it reports GNU 31.1 with \
             lazy docstrings. Rebuild with the one recipe \
             (scripts/ci/build-gnu-oracle.sh <mirror_commit> <prefix>) or \
             record the new canonical build (path + sha256) before using it.\n\
             Per-host builds are recorded separately; a host with no recorded \
             entry is gated by the version and lazy-doc probes.",
            path.display()
        );
    }
}

/// The one-line identity of the resolved GNU oracle, for a gate log header.
pub fn gnu_oracle_stamp() -> String {
    neomacs_test_oracle::gnu::gnu_oracle().stamp()
}

/// Refuse a GNU that attests by dump fingerprint but carries a non-canonical
/// Lisp tree. Split out so the classification is unit-testable.
fn assert_canonical_gnu_doc_tree(program: &Path) {
    let output = Command::new(program)
        .args(["--batch", "--quick", "--eval", DOC_SLOT_PROBE])
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "the GNU Emacs oracle {} could not run the canonical-build probe: {error}\n\
                 Build the canonical oracle with scripts/ci/build-gnu-oracle.sh \
                 <mirror_commit> <prefix> and point NEOVM_FORCE_ORACLE_PATH at \
                 <prefix>/bin/emacs.",
                program.display()
            )
        });
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        panic!(
            "the GNU Emacs oracle {} failed the canonical-build probe \
             (status {:?}).\nstdout:\n{stdout}\nstderr:\n{stderr}",
            program.display(),
            output.status.code()
        );
    }
    let mut tokens = stdout.split_whitespace();
    let version = tokens.next().unwrap_or_default();
    let doc_slot = tokens.next().unwrap_or_default();
    if let Err(message) = check_oracle_identity(program, version, doc_slot) {
        panic!("{message}");
    }
}

/// Check the probe's two tokens. The returned `Result` keeps the refusal text
/// testable without spawning GNU.
fn check_oracle_identity(program: &Path, version: &str, doc_slot: &str) -> Result<(), String> {
    if version != PINNED_VERSION {
        return Err(format!(
            "the GNU Emacs oracle {} reports version {version:?}, but every TUI \
             comparison is pinned to GNU {PINNED_VERSION} (the parity-reference \
             manifest).\nBuild the pinned oracle with \
             scripts/ci/build-gnu-oracle.sh <mirror_commit> <prefix> and point \
             NEOVM_FORCE_ORACLE_PATH at <prefix>/bin/emacs.",
            program.display()
        ));
    }
    match doc_slot {
        "lazy-doc-ref" => Ok(()),
        "lazy-doc-ref-other" => Err(format!(
            "the GNU Emacs oracle {} is the pinned version but its preloaded \
             `file-remote-p` docstring comes from a different file than \
             `files.elc`; it is not the canonical build.\nBuild the canonical \
             oracle with scripts/ci/build-gnu-oracle.sh <mirror_commit> \
             <prefix>.",
            program.display()
        )),
        other => Err(format!(
            "the GNU Emacs oracle {} is GNU {version} but not the canonical \
             byte-compiled build: the preloaded function `file-remote-p` doc \
             slot is {other}, where the canonical build has the lazy \
             (\"files.elc\" . POS) reference. A same-commit rebuild can attest \
             by dump fingerprint while shipping native-compiled or inline \
             docstrings; every mode-line property comparison then differs at \
             preloaded closure doc slots (see \
             scripts/ci/build-gnu-oracle.sh, non-native profile).\nBuild the \
             canonical oracle with scripts/ci/build-gnu-oracle.sh \
             <mirror_commit> <prefix> and point NEOVM_FORCE_ORACLE_PATH at \
             <prefix>/bin/emacs.",
            program.display()
        )),
    }
}

/// Refuse a Neomacs binary that would boot by sourcing the Lisp tree.
///
/// `pdumper-stats` is non-nil only when the shipped dump was found and its
/// fingerprint matched; a missing or mismatched dump silently falls back to a
/// source bootstrap, which is a different startup state and not what a TUI
/// oracle comparison may measure.
///
/// # Panics
///
/// When the binary cannot run or does not report a loaded pdump.
pub fn assert_neomacs_dumped_runtime(program: &Path) {
    let output = Command::new(program)
        .args([
            "--batch",
            "--no-build-details",
            "--eval",
            "(princ (if (pdumper-stats) \"NEOMACS-PDUMPED\" \"NEOMACS-SOURCE-BOOTSTRAP\"))",
        ])
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "could not run the Neomacs binary {} to check its pdump: \
                 {error}\nSet NEOMACS_TUI_NEOMACS_BIN to a runtime produced by \
                 cargo xtask fresh-build --release.",
                program.display()
            )
        });
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() && stdout.contains("NEOMACS-PDUMPED") {
        assert_fresh_build_stamp(program);
        return;
    }
    let tail = |text: &str| {
        let mut lines: Vec<&str> = text.lines().rev().take(8).collect();
        lines.reverse();
        lines.join("\n")
    };
    panic!(
        "the Neomacs binary {} did not boot from its matching pdump \
         (status {:?}); it would source-bootstrap the Lisp tree instead.\n\
         A source bootstrap runs with `purify-flag` nil, so dumped-state \
         observables differ from the shipped runtime (lazy doc references \
         become absolute paths, preloaded dump state is absent) and the TUI \
         oracle would compare the bootstrap, not the product.\n\
         Point NEOMACS_TUI_NEOMACS_BIN at a runtime produced by \
         cargo xtask fresh-build --release (binary plus its pdump).\n\
         stdout tail:\n{}\nstderr tail:\n{}",
        program.display(),
        output.status.code(),
        tail(&stdout),
        tail(&stderr)
    );
}

/// Refuse an editor whose binary and Lisp tree did not come from the same
/// `cargo xtask fresh-build` run.
///
/// Same attestation as the Elisp oracle harness
/// (`neomacs_test_env::fresh_build_stamp`): the binary's SHA-256 and the
/// generated-Lisp manifest must match the stamp beside the binary, and every
/// preloaded `.el` from `loadup.el` must have its `.elc`.  A pdumped binary
/// over a tree whose bytecode was deleted still boots, but it loads sources
/// and the comparison measures the bootstrap.  One full check runs per nextest
/// run; later test processes reuse the cached verdict.
///
/// `NEOVM_SKIP_FRESH_BUILD_STAMP=<non-empty>` is the documented override and
/// its value is echoed by the verifier.
fn assert_fresh_build_stamp(program: &Path) {
    static ATTESTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ATTESTED.get_or_init(|| {
        // The Lisp tree under test is the runtime root's, not necessarily the
        // workspace's: a staged install or an out-of-tree runtime is selected
        // with NEOMACS_RUNTIME_ROOT, and the spawned editor reads that tree.
        let runtime_root = std::env::var_os("NEOMACS_RUNTIME_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(crate::workspace_root);
        let lisp_root = runtime_root.join("lisp");
        match neomacs_test_env::fresh_build_stamp::verify(program, &lisp_root) {
            Ok(neomacs_test_env::fresh_build_stamp::StampVerdict::Verified { .. }) => {}
            Ok(neomacs_test_env::fresh_build_stamp::StampVerdict::Overridden { .. }) => {}
            Err(message) => panic!("{message}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_doc_slot_is_accepted() {
        assert_eq!(
            check_oracle_identity(Path::new("/opt/oracle/bin/emacs"), "31.1", "lazy-doc-ref"),
            Ok(())
        );
    }

    #[test]
    fn inline_docstring_oracle_is_refused_with_the_recipe() {
        let error =
            check_oracle_identity(Path::new("/stale/bin/emacs"), "31.1", "inline-docstring")
                .expect_err("an inline-docstring oracle must be refused");
        assert!(error.contains("/stale/bin/emacs"), "{error}");
        assert!(
            error.contains("not the canonical byte-compiled build"),
            "{error}"
        );
        assert!(error.contains("build-gnu-oracle.sh"), "{error}");
    }

    #[test]
    fn wrong_version_is_refused() {
        let error = check_oracle_identity(Path::new("/usr/bin/emacs"), "30.2", "lazy-doc-ref")
            .expect_err("a non-pinned version must be refused");
        assert!(error.contains("GNU 31.1"), "{error}");
    }

    #[test]
    fn foreign_lazy_doc_source_is_refused() {
        let error =
            check_oracle_identity(Path::new("/other/bin/emacs"), "31.1", "lazy-doc-ref-other")
                .expect_err("a non-files.elc lazy reference must be refused");
        assert!(error.contains("files.elc"), "{error}");
    }
}
