//! Tests for the parity-reference pin.
//!
//! The fixtures are synthetic on purpose.  A guard whose tests need the real
//! pinned mirror can only be exercised on one machine, and --- worse --- could
//! not plant a MISMATCH without rebuilding the very mirror this crate exists to
//! stop anyone rebuilding.  Synthetic dumps carry a real `struct dump_header`
//! prefix, so they travel the same code path the real one does.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// A synthetic editor: a file plus the dump GNU would load beside it.
struct Fixture {
    _dir: tempfile::TempDir,
    executable: PathBuf,
    pdmp: PathBuf,
}

impl Fixture {
    /// `body` is the executable's content; `fingerprint` the 32 bytes the dump
    /// header carries at offset 16.
    fn new(body: &[u8], fingerprint: [u8; FINGERPRINT_LEN]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let executable = dir.path().join("emacs");
        let pdmp = dir.path().join("emacs.pdmp");
        fs::write(&executable, body).expect("write executable");
        let mut dump = Vec::new();
        dump.extend_from_slice(DUMP_MAGIC);
        dump.extend_from_slice(&fingerprint);
        dump.extend_from_slice(&[0x5a; 128]);
        fs::write(&pdmp, &dump).expect("write dump");
        Self {
            _dir: dir,
            executable,
            pdmp,
        }
    }

    fn manifest(&self) -> ReferenceManifest {
        let observed = observe(&self.executable).expect("observe the fixture");
        ReferenceManifest {
            schema: "1".to_string(),
            emacs_version: "31.0.90".to_string(),
            mirror_commit: "0".repeat(40),
            build_time: "2026-06-10T02:39:56-04:00".to_string(),
            fingerprint: observed.fingerprint,
            executable_sha256: observed.executable_sha256,
            executable_size: observed.executable_size,
            pdmp_sha256: observed.pdmp_sha256,
            pdmp_size: observed.pdmp_size,
        }
    }
}

fn pinned_fingerprint() -> [u8; FINGERPRINT_LEN] {
    let mut bytes = [0u8; FINGERPRINT_LEN];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = index as u8;
    }
    bytes
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn a_matching_editor_attests_at_both_depths() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = fixture.manifest();
    for depth in [AttestationDepth::Fingerprint, AttestationDepth::Exhaustive] {
        let attested = attest_against(&fixture.executable, depth, &manifest)
            .unwrap_or_else(|error| panic!("{depth} attestation should pass: {error}"));
        assert_eq!(attested.depth(), depth);
        assert_eq!(
            attested.executable(),
            fs::canonicalize(&fixture.executable)
                .expect("canonicalize")
                .as_path()
        );
        assert_eq!(
            attested.pdmp(),
            fs::canonicalize(&fixture.pdmp)
                .expect("canonicalize")
                .as_path()
        );
    }
}

#[test]
fn the_stamp_carries_the_reference_and_the_depth() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = fixture.manifest();
    let attested = attest_against(
        &fixture.executable,
        AttestationDepth::Fingerprint,
        &manifest,
    )
    .expect("attestation should pass");
    let stamp = ReferenceUse::Attested(attested).stamp();
    assert_eq!(
        stamp,
        "gnu=31.0.90 fingerprint=000102030405 mirror=00000000000 attest=fingerprint"
    );

    let unpinned = ReferenceUse::Unpinned {
        executable: fixture.executable.clone(),
    };
    assert_eq!(
        unpinned.stamp(),
        "gnu=UNATTESTED (NEOMACS_PARITY_REFERENCE=none) attest=none"
    );
    assert!(!unpinned.is_attested());
}

// ---------------------------------------------------------------------------
// Sensitivity: plant a mismatch, confirm the guard fires, and check the message
// ---------------------------------------------------------------------------

#[test]
fn a_rebuilt_reference_is_refused_by_its_fingerprint() {
    // The incident this crate exists for: same path, same sizes, different
    // build.  A rebuild produces a new `temacs` and therefore a new
    // fingerprint, so the cheap depth alone must catch it.
    let manifest = Fixture::new(b"pinned emacs", pinned_fingerprint()).manifest();

    let mut rebuilt_fingerprint = pinned_fingerprint();
    rebuilt_fingerprint[0] ^= 0xff;
    let rebuilt = Fixture::new(b"pinned emacs", rebuilt_fingerprint);

    let error = attest_against(
        &rebuilt.executable,
        AttestationDepth::Fingerprint,
        &manifest,
    )
    .expect_err("a different build must be refused");
    match &error {
        AttestationError::Mismatch {
            field,
            expected,
            actual,
            ..
        } => {
            assert_eq!(*field, "build fingerprint");
            assert!(expected.starts_with("000102"), "expected: {expected}");
            assert!(actual.starts_with("ff0102"), "actual: {actual}");
        }
        other => panic!("expected a fingerprint mismatch, got {other:?}"),
    }
    let rendered = error.to_string();
    assert!(
        rendered.contains("parity reference MISMATCH on build fingerprint"),
        "message must name the refusal: {rendered}"
    );
    assert!(
        rendered.contains("pin-reference"),
        "message must name the deliberate re-baselining path: {rendered}"
    );
    assert!(
        rendered.contains("NEOMACS_PARITY_REFERENCE=none"),
        "message must name the opt-out: {rendered}"
    );
}

#[test]
fn a_resized_executable_is_refused_without_hashing_it() {
    let manifest = Fixture::new(b"pinned emacs", pinned_fingerprint()).manifest();
    let longer = Fixture::new(b"pinned emacs and one more byte", pinned_fingerprint());
    let error = attest_against(&longer.executable, AttestationDepth::Fingerprint, &manifest)
        .expect_err("a different size must be refused");
    assert!(
        matches!(
            &error,
            AttestationError::Mismatch {
                field: "executable size",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn only_the_exhaustive_depth_catches_an_edit_that_preserves_size_and_fingerprint() {
    // This is the whole reason `AttestationDepth` is an enum and not a bool.
    // The fingerprint is computed over `temacs` at BUILD time, so a shipped
    // file edited afterwards keeps it.  The cheap depth is honest about not
    // seeing that; the exhaustive one does.
    let pinned = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = pinned.manifest();
    let edited = Fixture::new(b"patchd emacs", pinned_fingerprint());
    assert_eq!(
        manifest.executable_size,
        observe(&edited.executable)
            .expect("observe")
            .executable_size,
        "the fixture must isolate content from size",
    );

    attest_against(&edited.executable, AttestationDepth::Fingerprint, &manifest)
        .expect("the cheap depth cannot see a post-build edit, and must not pretend to");

    let error = attest_against(&edited.executable, AttestationDepth::Exhaustive, &manifest)
        .expect_err("the exhaustive depth must see it");
    assert!(
        matches!(
            &error,
            AttestationError::Mismatch {
                field: "executable sha256",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn an_edited_dump_is_refused_by_the_exhaustive_depth() {
    let pinned = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = pinned.manifest();
    let mut dump = fs::read(&pinned.pdmp).expect("read dump");
    let last = dump.len() - 1;
    dump[last] ^= 0xff;
    fs::write(&pinned.pdmp, &dump).expect("rewrite dump");

    attest_against(&pinned.executable, AttestationDepth::Fingerprint, &manifest)
        .expect("the header is untouched, so the cheap depth passes");
    let error = attest_against(&pinned.executable, AttestationDepth::Exhaustive, &manifest)
        .expect_err("the dump content changed");
    assert!(
        matches!(
            &error,
            AttestationError::Mismatch {
                field: "dump sha256",
                ..
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn a_missing_dump_is_refused_and_named_as_the_dump() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = fixture.manifest();
    fs::remove_file(&fixture.pdmp).expect("remove dump");
    let error = attest_against(
        &fixture.executable,
        AttestationDepth::Fingerprint,
        &manifest,
    )
    .expect_err("a missing dump must be refused");
    assert!(
        matches!(&error, AttestationError::DumpMissing { .. }),
        "got {error:?}"
    );
}

#[test]
fn a_file_that_is_not_a_dump_is_named_as_such_rather_than_misread() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let mut manifest = fixture.manifest();
    // Long enough to reach the magic check: a shorter file is refused earlier,
    // as a header that could not be READ, which is a different fact.
    fs::write(
        &fixture.pdmp,
        b"this is not a dump file at all, not one single bit of one",
    )
    .expect("overwrite");
    let observed_size = fs::metadata(&fixture.pdmp).expect("metadata").len();
    manifest.pdmp_size = observed_size;
    let error = attest_against(
        &fixture.executable,
        AttestationDepth::Fingerprint,
        &manifest,
    )
    .expect_err("a non-dump must be refused");
    match &error {
        AttestationError::DumpMagic { found, .. } => {
            assert!(found.starts_with("this is not a du"), "found: {found}");
        }
        other => panic!("expected a magic refusal, got {other:?}"),
    }

    // A file too short to hold a header is refused as a header that could not
    // be read, not as a bad magic: the two are different facts about the dump.
    fs::write(&fixture.pdmp, b"short").expect("truncate");
    manifest.pdmp_size = fs::metadata(&fixture.pdmp).expect("metadata").len();
    let error = attest_against(
        &fixture.executable,
        AttestationDepth::Fingerprint,
        &manifest,
    )
    .expect_err("a truncated dump must be refused");
    assert!(
        matches!(&error, AttestationError::DumpMissing { .. }),
        "got {error:?}"
    );
}

#[test]
fn a_missing_editor_is_refused_as_the_editor() {
    let error = attest_against(
        Path::new("/nonexistent/definitely-not-an-editor"),
        AttestationDepth::Fingerprint,
        &Fixture::new(b"x", pinned_fingerprint()).manifest(),
    )
    .expect_err("a missing editor must be refused");
    assert!(
        matches!(&error, AttestationError::ExecutableUnresolved { .. }),
        "got {error:?}"
    );
}

// ---------------------------------------------------------------------------
// The opt-out
// ---------------------------------------------------------------------------

#[test]
fn the_opt_out_is_exact_and_absence_means_attest() {
    assert!(!opt_out_requested(None).expect("absent means attest"));
    assert!(opt_out_requested(Some("none")).expect("none means unpinned"));
    for typo in ["None", "NONE", "1", "true", "yes", "", "none "] {
        let error =
            opt_out_requested(Some(typo)).expect_err("only the exact value may disable the guard");
        assert!(
            matches!(&error, AttestationError::UnknownOptOut { .. }),
            "{typo:?} gave {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The manifest format
// ---------------------------------------------------------------------------

/// The manifest as this repository checks it in.
fn checked_in_manifest_text() -> String {
    fs::read_to_string(manifest_path()).expect("the checked-in manifest must be readable")
}

#[test]
fn the_checked_in_manifest_parses_and_round_trips() {
    let manifest =
        parse_manifest(&checked_in_manifest_text()).expect("the checked-in pin must parse");
    assert_eq!(manifest.schema, "1");
    let rendered = render_manifest_keys(&manifest);
    assert_eq!(
        parse_manifest(&rendered).expect("a rendered manifest must parse"),
        manifest,
        "pin-reference writes what this crate reads"
    );
}

#[test]
fn the_parser_refuses_rather_than_skips() {
    let base = render_manifest_keys(&ReferenceManifest {
        schema: "1".to_string(),
        emacs_version: "31.0.90".to_string(),
        mirror_commit: "a".repeat(40),
        build_time: "t".to_string(),
        fingerprint: "b".repeat(64),
        executable_sha256: "c".repeat(64),
        executable_size: 1,
        pdmp_sha256: "d".repeat(64),
        pdmp_size: 2,
    });
    parse_manifest(&base).expect("the baseline must parse");

    // Each of these is a way a pin could silently stop meaning what it says.
    let corruptions: [(&str, String); 7] = [
        (
            "a line the format does not define",
            format!("{base}[table]\n"),
        ),
        ("a duplicate key", format!("{base}pdmp_size = \"3\"\n")),
        ("an unknown key", format!("{base}pdmp_sha512 = \"e\"\n")),
        (
            "an unquoted value",
            base.replace("pdmp_size = \"2\"", "pdmp_size = 2"),
        ),
        (
            "a non-hex digest",
            base.replace(&"d".repeat(64), &"z".repeat(64)),
        ),
        (
            "a truncated digest",
            base.replace(&"d".repeat(64), &"d".repeat(63)),
        ),
        (
            "an unrecognised schema",
            base.replace("schema = \"1\"", "schema = \"2\""),
        ),
    ];
    for (what, text) in corruptions {
        assert!(
            parse_manifest(&text).is_err(),
            "the parser must refuse {what}"
        );
    }

    // ...and must still accept the things the format DOES define.
    let decorated = format!("# a comment\n\n{base}\n   \n# trailing note\n");
    parse_manifest(&decorated).expect("comments and blank lines are part of the format");
}

#[test]
fn a_missing_key_is_named() {
    let text = "schema = \"1\"\n";
    let error = parse_manifest(text).expect_err("an incomplete pin must be refused");
    assert!(error.contains("missing key"), "{error}");
}

// ---------------------------------------------------------------------------
// The two readers, held together
// ---------------------------------------------------------------------------
//
// There are two implementations of this check because four harnesses need it
// and two of them are shell.  Two implementations that are not tested against
// each other drift, and a guard that drifts is a guard that stops guarding one
// of its callers without telling anyone.  These tests run the SHELL attestor
// over the same planted fixtures the Rust one sees and require them to reach
// the same verdict --- and, when they pass, to print the same stamp.

fn attestor_script() -> PathBuf {
    Path::new(env!("CARGO_WORKSPACE_DIR")).join("scripts/parity-reference-attest.sh")
}

/// Write `manifest` to a file the shell attestor can read, comments and all.
fn write_manifest(dir: &Path, manifest: &ReferenceManifest) -> PathBuf {
    let path = dir.join("parity-reference.toml");
    fs::write(
        &path,
        format!(
            "# a planted pin, written by {}\n\n{}",
            module_path!(),
            render_manifest_keys(manifest)
        ),
    )
    .expect("write manifest");
    path
}

/// What the shell attestor said: exit status and its single stdout line.
fn shell_attest(executable: &Path, depth: AttestationDepth, manifest: &Path) -> (i32, String) {
    let output = std::process::Command::new("bash")
        .arg(attestor_script())
        .arg(executable)
        .arg(depth.as_str())
        .env(MANIFEST_VAR, manifest)
        .env_remove(OPT_OUT_VAR)
        .output()
        .expect("the shell attestor must be runnable");
    (
        output
            .status
            .code()
            .expect("the attestor must not be signalled"),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    )
}

#[test]
fn both_readers_agree_on_a_matching_reference() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = fixture.manifest();
    let manifest_path = write_manifest(fixture._dir.path(), &manifest);

    for depth in [AttestationDepth::Fingerprint, AttestationDepth::Exhaustive] {
        let rust = ReferenceUse::Attested(
            attest_against(&fixture.executable, depth, &manifest).expect("rust attestation"),
        );
        let (code, stamp) = shell_attest(&fixture.executable, depth, &manifest_path);
        assert_eq!(code, 0, "the shell attestor must accept the pin at {depth}");
        assert_eq!(
            stamp,
            rust.stamp(),
            "the two readers must publish the same stamp at {depth}"
        );
    }
}

#[test]
fn both_readers_refuse_the_same_planted_references() {
    let pinned = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest = pinned.manifest();
    let manifest_path = write_manifest(pinned._dir.path(), &manifest);

    let mut rebuilt_fingerprint = pinned_fingerprint();
    rebuilt_fingerprint[7] ^= 0xff;

    // (what was planted, the editor, the depth that must refuse it)
    let planted: Vec<(&str, Fixture, AttestationDepth)> = vec![
        (
            "a rebuild: a new fingerprint at the same sizes",
            Fixture::new(b"pinned emacs", rebuilt_fingerprint),
            AttestationDepth::Fingerprint,
        ),
        (
            "a resized executable",
            Fixture::new(b"pinned emacs, longer", pinned_fingerprint()),
            AttestationDepth::Fingerprint,
        ),
        (
            "a post-build edit that keeps the size and the fingerprint",
            Fixture::new(b"patchd emacs", pinned_fingerprint()),
            AttestationDepth::Exhaustive,
        ),
    ];

    for (what, fixture, depth) in planted {
        let rust = attest_against(&fixture.executable, depth, &manifest);
        assert!(rust.is_err(), "the rust reader must refuse {what}");
        let (code, stamp) = shell_attest(&fixture.executable, depth, &manifest_path);
        assert_eq!(code, 3, "the shell reader must refuse {what}");
        assert!(
            stamp.is_empty(),
            "a refused attestation must publish no stamp for {what}: {stamp:?}"
        );
    }

    // A missing dump is a refusal for both, and is not the same fact as a
    // mismatch: ledger 211 section 10.1 bought that distinction, keep it.
    let orphan = Fixture::new(b"pinned emacs", pinned_fingerprint());
    fs::remove_file(&orphan.pdmp).expect("remove dump");
    assert!(
        attest_against(&orphan.executable, AttestationDepth::Fingerprint, &manifest).is_err(),
        "the rust reader must refuse an editor with no dump"
    );
    let (code, _) = shell_attest(
        &orphan.executable,
        AttestationDepth::Fingerprint,
        &manifest_path,
    );
    assert_eq!(
        code, 3,
        "the shell reader must refuse an editor with no dump"
    );
}

#[test]
fn both_readers_refuse_the_same_malformed_pins() {
    // The parsers are the part most likely to drift apart, and a parser that
    // shrugs is how a pin silently stops being checked.  Every corruption here
    // must stop BOTH readers.
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let base = render_manifest_keys(&fixture.manifest());

    let corruptions: [(&str, String); 7] = [
        (
            "a line the format does not define",
            format!("{base}[table]\n"),
        ),
        ("a duplicate key", format!("{base}pdmp_size = \"3\"\n")),
        ("an unknown key", format!("{base}pdmp_sha512 = \"e\"\n")),
        (
            "an unquoted value",
            base.replace("schema = \"1\"", "schema = 1"),
        ),
        (
            "a non-hex fingerprint",
            base.replace(&fixture.manifest().fingerprint, &"z".repeat(64)),
        ),
        (
            "a truncated fingerprint",
            base.replace(&fixture.manifest().fingerprint, &"a".repeat(63)),
        ),
        (
            "an unrecognised schema",
            base.replace("schema = \"1\"", "schema = \"2\""),
        ),
    ];

    for (what, text) in corruptions {
        let path = fixture._dir.path().join("planted-pin.toml");
        fs::write(&path, &text).expect("write planted pin");
        assert!(
            parse_manifest(&text).is_err(),
            "the rust reader must refuse {what}"
        );
        let (code, stamp) = shell_attest(&fixture.executable, AttestationDepth::Fingerprint, &path);
        assert_eq!(code, 3, "the shell reader must refuse {what}");
        assert!(
            stamp.is_empty(),
            "a refused pin must publish no stamp for {what}: {stamp:?}"
        );
    }
}

#[test]
fn both_readers_treat_the_opt_out_as_exact() {
    let fixture = Fixture::new(b"pinned emacs", pinned_fingerprint());
    let manifest_path = write_manifest(fixture._dir.path(), &fixture.manifest());
    let run = |value: &str| {
        let output = std::process::Command::new("bash")
            .arg(attestor_script())
            .arg(&fixture.executable)
            .env(MANIFEST_VAR, &manifest_path)
            .env(OPT_OUT_VAR, value)
            .output()
            .expect("run the shell attestor");
        (
            output.status.code().expect("not signalled"),
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
    };

    let (code, stamp) = run("none");
    assert_eq!(code, 0);
    assert_eq!(
        stamp,
        ReferenceUse::Unpinned {
            executable: fixture.executable.clone()
        }
        .stamp(),
        "an unpinned run must be branded identically by both readers"
    );

    for typo in ["None", "NONE", "1", "true", "yes"] {
        let (code, stamp) = run(typo);
        assert_eq!(code, 3, "{typo:?} must not disable the shell guard");
        assert!(stamp.is_empty(), "{typo:?} published {stamp:?}");
        assert!(
            opt_out_requested(Some(typo)).is_err(),
            "{typo:?} must not disable the rust guard"
        );
    }
}

// ---------------------------------------------------------------------------
// The other half of the pair: this port's own binary
// ---------------------------------------------------------------------------
//
// GNU is PINNED and its predicate is equality with a recorded identity.  This
// port is NOT pinnable --- it changes every commit --- so its predicate is
// CORRESPONDENCE with the tree being measured, and only the case where the
// binary cannot be placed on that tree's history at all is a refusal.  The
// stubs stand in for a real build because the verdicts are about what a binary
// REPORTS, and building three different revisions to test three verdicts would
// cost an hour to exercise thirty lines of shell.

/// A stand-in for `neomacs --version` reporting `revision`.
fn port_stub(dir: &Path, name: &str, revision: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\nprintf '%s\\n' 'Neomacs 0.0.15\nGit commit: {revision}'\n"),
    )
    .expect("write port stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

fn port_attest(binary: &Path) -> (i32, String) {
    let output = std::process::Command::new("bash")
        .arg(attestor_script())
        .arg("--port")
        .arg(binary)
        .output()
        .expect("the shell attestor must be runnable");
    (
        output.status.code().expect("not signalled"),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    )
}

fn head_revision() -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(Path::new(env!("CARGO_WORKSPACE_DIR")))
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
#[cfg(unix)]
fn a_port_binary_built_from_head_is_placed_at_head() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = port_stub(dir.path(), "neomacs", &head_revision());
    let (code, stamp) = port_attest(&stub);
    assert_eq!(code, 0, "a binary built from HEAD must attest: {stamp}");
    assert!(
        stamp.contains("place=HEAD") && stamp.starts_with("neo="),
        "the stamp must place the binary on the tree: {stamp}"
    );
}

#[test]
#[cfg(unix)]
fn a_port_binary_from_another_history_is_refused() {
    // The one port case that refuses.  A binary built from a commit this tree
    // has never seen cannot be talking about this tree, so a number scored with
    // it is not a number about this tree.
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = port_stub(dir.path(), "neomacs", &"dead0beef".repeat(5)[..40]);
    let (code, stamp) = port_attest(&stub);
    assert_eq!(code, 3, "a divergent binary must be refused");
    assert!(
        stamp.is_empty(),
        "a refused port must publish no stamp: {stamp:?}"
    );
}

#[test]
#[cfg(unix)]
fn a_port_binary_that_reports_no_revision_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = port_stub(dir.path(), "neomacs", "unknown");
    let (code, _) = port_attest(&stub);
    assert_eq!(
        code, 3,
        "a binary that cannot say what it was built from cannot be tied to a tree"
    );
}

#[test]
#[cfg(unix)]
fn a_port_binary_that_cannot_be_run_is_refused_as_the_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, _) = port_attest(&dir.path().join("no-such-binary"));
    assert_eq!(code, 3);
}
