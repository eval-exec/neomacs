use std::ffi::OsStr;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

#[cfg(unix)]
use neomacs_melpa_test_support::MelpaSandbox;

use super::{
    DisplayCheckpoint, PackageDisplayContract, PairTimeout, SymmetricDisplayEnvironment,
    TerminalProfile, canonical_executable_identity, validate_distinct_editor_identities,
};

#[test]
fn terminal_profile_owns_the_symmetric_display_environment() {
    let display = SymmetricDisplayEnvironment::from(TerminalProfile::TrueColor);
    assert_eq!(
        display
            .set_entries()
            .map(|(key, value)| (key.to_str(), value.to_str()))
            .collect::<Vec<_>>(),
        vec![(Some("COLORTERM"), Some("truecolor"))]
    );
    assert_eq!(display.removed_entries().count(), 0);

    let removed = SymmetricDisplayEnvironment::from(TerminalProfile::Indexed256);
    assert_eq!(removed.set_entries().count(), 0);
    assert_eq!(
        removed
            .removed_entries()
            .map(OsStr::to_str)
            .collect::<Vec<_>>(),
        vec![Some("COLORTERM")]
    );
    assert_eq!(TerminalProfile::default(), TerminalProfile::TrueColor);
}

#[test]
fn package_display_contract_defaults_to_resolved_rgb_exact_display() {
    assert_eq!(
        PackageDisplayContract::default(),
        PackageDisplayContract::ExactDisplay
    );
    assert_eq!(
        DisplayCheckpoint::new("visible screen").contract,
        PackageDisplayContract::ExactDisplay
    );
    assert_eq!(
        DisplayCheckpoint::raw_terminal("wire state").contract,
        PackageDisplayContract::RawTerminal
    );
}

#[test]
fn pair_timeout_requires_asymmetry_to_be_explicit() {
    let same = std::time::Duration::from_secs(8);
    assert_eq!(PairTimeout::same(same).split(), (same, same));

    let gnu = std::time::Duration::from_secs(6);
    let neomacs = std::time::Duration::from_secs(12);
    assert_eq!(
        PairTimeout::per_editor(gnu, neomacs).split(),
        (gnu, neomacs)
    );
}

#[test]
fn editor_identity_rejects_accidental_same_binary_but_allows_calibration() {
    let gnu = Path::new("/canonical/gnu-emacs");
    let neo = Path::new("/canonical/neomacs");
    assert!(validate_distinct_editor_identities(gnu, neo, false).is_ok());
    assert!(validate_distinct_editor_identities(gnu, gnu, false).is_err());
    assert!(validate_distinct_editor_identities(gnu, gnu, true).is_ok());
}

#[cfg(unix)]
#[test]
fn editor_identity_canonicalizes_symlink_aliases_before_comparison() {
    let sandbox = MelpaSandbox::new("tui-editor-identity-contract")
        .expect("create owned executable-identity sandbox below ./tmp");
    let executable = sandbox.root().join("real-editor");
    let alias = sandbox.root().join("editor-alias");
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").expect("write owned executable fixture");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("make owned fixture executable");
    symlink(&executable, &alias).expect("create owned executable symlink alias");

    let executable = canonical_executable_identity(&executable)
        .expect("canonicalize the real executable fixture");
    let alias =
        canonical_executable_identity(&alias).expect("canonicalize the executable symlink alias");
    assert_eq!(alias, executable);
    assert!(validate_distinct_editor_identities(&executable, &alias, false).is_err());
}
