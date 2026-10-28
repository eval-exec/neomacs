use std::path::PathBuf;

use sha2::{Digest, Sha256};

use super::{
    FIXTURE_DIRECTORY, GNU_ELPA_LABEL, GnuElpaPackage, PreparedTarball, hex_digest, install_form,
};
use crate::packages::elpa_lock::locked_elpa_sources;

#[test]
fn the_lock_manifest_and_the_typed_pin_set_agree() {
    let rows = locked_elpa_sources().expect("parse the elpa lock manifest");
    assert_eq!(
        rows.len(),
        GnuElpaPackage::ALL.len(),
        "every pinned GNU ELPA package needs exactly one lock row"
    );
    for row in rows {
        let package = GnuElpaPackage::from_pin(row.name, row.version).unwrap_or_else(|| {
            panic!(
                "lock row {} {} has no typed GnuElpaPackage pin",
                row.name, row.version
            )
        });
        assert_eq!(package.name(), row.name);
        assert_eq!(package.version(), row.version);
    }
    for package in GnuElpaPackage::ALL {
        assert!(
            rows.iter()
                .any(|row| row.name == package.name() && row.version == package.version()),
            "typed pin {} {} has no lock row",
            package.name(),
            package.version()
        );
    }
}

#[test]
fn every_typed_pin_has_a_checksummed_tarball_fixture() {
    for package in GnuElpaPackage::ALL {
        let source = package.source().unwrap_or_else(|error| panic!("{error}"));
        let path = package.fixture_path();
        let bytes = std::fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "vendored {GNU_ELPA_LABEL} fixture {} for {} {} is missing (expected sha256 {}): \
                 {error}",
                path.display(),
                package.name(),
                package.version(),
                source.sha256
            )
        });
        assert_eq!(
            hex_digest(&Sha256::digest(&bytes)),
            source.sha256,
            "fixture {} does not match its lock row",
            path.display()
        );
        let descriptor = format!(
            "{}-{}/{}-pkg.el",
            package.name(),
            package.version(),
            package.name()
        );
        let pkg_el = tar_entry(&bytes, &descriptor)
            .unwrap_or_else(|| panic!("fixture {} does not contain {descriptor}", path.display()));
        let text = std::str::from_utf8(pkg_el).expect("descriptor is UTF-8");
        let declaration = format!(
            "(define-package \"{}\" \"{}\"",
            package.name(),
            package.version()
        );
        assert!(
            text.contains(&declaration),
            "fixture {} does not describe {} {} (expected {declaration:?})",
            path.display(),
            package.name(),
            package.version()
        );
        let recorded_commit = text
            .split(":commit \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next());
        assert_eq!(
            recorded_commit,
            Some(source.commit),
            "fixture {} records commit {:?}, lock row says {}",
            path.display(),
            recorded_commit,
            source.commit
        );
    }
}

#[test]
fn gnu_elpa_dependencies_stay_inside_the_vendored_set() {
    for package in GnuElpaPackage::ALL {
        for dependency in package.dependencies() {
            assert!(
                GnuElpaPackage::ALL.contains(dependency),
                "{:?} depends on {:?}, which is not a pinned GnuElpaPackage",
                package,
                dependency
            );
            assert_ne!(*dependency, package, "{package:?} depends on itself");
        }
    }
    assert_eq!(
        GnuElpaPackage::USE_PACKAGE.install_order(),
        vec![GnuElpaPackage::BIND_KEY, GnuElpaPackage::USE_PACKAGE],
        "use-package requires bind-key, and package-install-file resolves no dependencies"
    );
    assert_eq!(
        GnuElpaPackage::COMPAT.install_order(),
        vec![GnuElpaPackage::COMPAT]
    );
}

#[test]
fn missing_and_corrupt_fixtures_fail_with_the_file_and_checksum() {
    let package = GnuElpaPackage::MINIBUFFER_LINE;
    let source = package.source().expect("lock row");

    let missing = std::env::temp_dir().join(format!(
        "neomacs-absent-gnu-elpa-fixture-{}.tar",
        std::process::id()
    ));
    let error = package
        .verified_tarball_at(&missing, &source)
        .expect_err("a missing fixture must fail");
    assert!(error.contains(missing.to_str().unwrap()), "{error}");
    assert!(error.contains(&source.sha256), "{error}");
    assert!(error.contains(FIXTURE_DIRECTORY), "{error}");

    let directory = tempfile::tempdir().expect("temp dir");
    let corrupt = directory.path().join("minibuffer-line-0.1.tar");
    let bogus = b"not a package tarball";
    std::fs::write(&corrupt, bogus).expect("write corrupt fixture");
    let error = package
        .verified_tarball_at(&corrupt, &source)
        .expect_err("a corrupt fixture must fail");
    assert!(error.contains(corrupt.to_str().unwrap()), "{error}");
    assert!(error.contains("checksum"), "{error}");
    assert!(
        error.contains(&hex_digest(&Sha256::digest(bogus))),
        "{error}"
    );
}

#[test]
fn the_install_form_uses_vendored_tarballs_and_never_the_archive() {
    let installs = GnuElpaPackage::USE_PACKAGE
        .install_order()
        .into_iter()
        .map(|package| {
            let source = package.source().expect("lock row");
            let path = PathBuf::from(format!(
                "/vendored/{}-{}.tar",
                package.name(),
                package.version()
            ));
            PreparedTarball { source, path }
        })
        .collect::<Vec<_>>();
    let form = install_form(&installs);
    assert!(form.contains("(package-install-file tarball)"), "{form}");
    // The verification loop interns the name, so the pinned name must be a
    // string: `(intern 'use-package)` signals wrong-type-argument.
    assert!(form.contains("(cons \"use-package\" \"2.4.6\")"), "{form}");
    for forbidden in [
        "package-refresh-contents",
        "package-archives",
        "https://",
        "elpa.gnu.org",
    ] {
        assert!(
            !form.contains(forbidden),
            "the install form must not touch the network ({forbidden}):\n{form}"
        );
    }
    let bind_key = form.find("bind-key-2.4.1.tar").expect("bind-key tarball");
    let use_package = form
        .find("use-package-2.4.6.tar")
        .expect("use-package tarball");
    assert!(
        bind_key < use_package,
        "dependencies must install before the package that requires them:\n{form}"
    );
    assert!(form.contains("NEOMACS-PACKAGE-CACHE:ready"));
    assert!(form.contains("Installed package version mismatch"));
}

/// Minimal ustar reader for the test fixtures: name and data of the first
/// regular-file entry that matches.  GNU ELPA's release tarballs use plain
/// short names, so pax/GNU long-name headers are not needed here.
fn tar_entry<'a>(archive: &'a [u8], wanted: &str) -> Option<&'a [u8]> {
    let mut offset = 0usize;
    while offset + 512 <= archive.len() {
        let header = &archive[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name_end = header[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let name = std::str::from_utf8(&header[..name_end]).ok()?;
        let size_field = std::str::from_utf8(&header[124..136]).ok()?;
        let size_field = size_field.trim_matches(|character| character == '\0' || character == ' ');
        let size = usize::from_str_radix(size_field, 8).ok()?;
        let data_start = offset + 512;
        let data_end = data_start.checked_add(size)?;
        let data = archive.get(data_start..data_end)?;
        let typeflag = header[156];
        if (typeflag == b'0' || typeflag == 0) && name == wanted {
            return Some(data);
        }
        offset = data_start + size.div_ceil(512) * 512;
    }
    None
}
