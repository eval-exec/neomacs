# Vendored GNU ELPA release tarballs

Each file is the exact release tarball GNU ELPA published at
`https://elpa.gnu.org/packages/<name>-<version>.tar`, committed so the test
suites install pinned GNU ELPA packages without contacting the network.

The pinned set is `crates/neomacs-infra/src/packages/elpa-package-lock.tsv`;
the installer (and the typed `GnuElpaPackage` pins) live in
`crates/neomacs-infra/src/packages/elpa_archive.rs`.  Every lock row must have
a tarball here whose sha256 matches the row, and every tarball must contain a
top-level `<name>-<version>/<name>-pkg.el` whose `define-package` name and
version match the pin; `cargo test -p neomacs-infra` enforces both.

## Adding or updating a pin

1. Download the release tarball from GNU ELPA over HTTPS:

       curl -fsS -O https://elpa.gnu.org/packages/<name>-<version>.tar
       sha256sum <name>-<version>.tar

2. Add the row to `elpa-package-lock.tsv` (package, version, the `:commit` from
   the tarball's `<name>-pkg.el`, sha256).  Keep rows sorted by package name.

3. Add the typed pin (`GnuElpaPackage::<NAME>`) and its `ALL` entry in
   `elpa_archive.rs`; if the package declares a runtime dependency that is
   itself a GNU ELPA package, add it to `GnuElpaPackage::dependencies`.
   `package-install-file` resolves no dependencies, so the installer
   materializes both from fixtures.

4. Run `cargo test -p neomacs-infra`.

These tarballs are third-party GNU ELPA releases; each carries its own
license and copyright headers.
