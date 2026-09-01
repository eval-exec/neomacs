# Neomacs package ecosystem tests

This crate verifies the user-visible package lifecycle through the editor's
own package APIs:

1. Create an isolated home under `<workspace>/tmp/melpa`.
2. Refresh a package archive and install a scenario's requested packages.
3. Exit the editor.
4. Start a fresh editor with the same isolated home.
5. Initialize packages and run the scenario probe.

The Rust harness owns orchestration, isolation, timeouts, and diagnostics.
Package behavior lives in one `.el` probe per scenario under `scenarios/`.
Each report includes phase timings plus the sorted installed package/version
graph. CI shows successful reports as well as retaining stdout and stderr in
phase-specific failures.

## Test layers

- `frozen_packages.rs` exercises GNU Emacs's small checked-in package archive.
  It is a fast contract for dependency resolution, tar extraction, generated
  autoloads, byte compilation, and restart persistence.
- `live_melpa.rs` hard-codes package names and versions, downloads one complete
  dependency transaction below `./tmp`, then gives that same local transaction
  to GNU Emacs and Neomacs. No third-party package payload is tracked by Git.
- Multi-probe batches (`CachedPackageOracle::run_batch` /
  `parity_tests::batch_support`) run many named Elisp probes in one GNU Emacs
  process and one Neomacs process (setup once per editor; cases keep separate
  expect-test snapshots). Each logical probe is a named
  `fn …() -> ParityBatchCase` constructor; the package-level Rust `#[test]`
  builds a `Vec<ParityBatchCase>` and runs one dual-editor batch. A case that
  cannot restore global editor state must opt into `.fresh_process()` while
  other cases in the suite remain batched.
- `src/parity_tests/dash/` and `src/parity_tests/s/` use the same package-level
  batch model for their comprehensive API corpora. Named logical cases cover
  normal, empty, boundary, mutation, evaluation-count, Unicode, and signal
  behavior without paying a process pair for every snapshot.
- Inline `expect-test` snapshots pin the complete normalized `OK` value or
  `ERR` signal. The harness reports differential, outcome-kind, GNU Emacs
  snapshot, and Neomacs snapshot failures together instead of short-circuiting
  after the first class of failure. Set
  `NEOMACS_MELPA_AUDIT_BATCH_ISOLATION=1` to compare every batch-capable case
  with fresh-process results, including fresh-process quarantines, and fail if
  sharing changes either editor's outcome. Setup-outcome cases are excluded
  because their package setup deliberately signals outside batch catchers.
  Normal runs never retry a divergent shared case in a fresh process; explicit
  isolation and the opt-in audit keep that diagnostic cost out of the main
  parity run.
- `upstream_package_ert.rs` runs grouped contracts from GNU Emacs's
  `test/lisp/emacs-lisp/package-tests.el` through a structured ERT adapter.
  The EOL and asynchronous-refresh groups remain explicit ignored tests until
  their Neomacs divergences are fixed.
- `package_lifecycle.rs` covers dependency autoremove, deletion persistence
  across a fresh process, rejection of packages requiring a future Emacs
  version, and package quickstart activation in a fresh process. The upstream
  signature contract is required when `gpg` is available; CI installs GnuPG
  so it cannot silently skip there.
- `package_vc.rs` installs from a workspace-local Git repository, restarts,
  upgrades to a new commit, restarts again, deletes the package, and verifies
  that deletion survives one more restart. It never contacts a remote host.
All Rust tests are library unit-test modules loaded from the
`src/parity_tests/` and `src/tui_parity_tests/` trees; the latter contains
packages whose public workflow requires a real interactive terminal. This
crate has no Cargo integration-test targets.
The GNU package-resource contracts are required CI checks. The current MELPA
oracle runs on scheduled and explicitly dispatched CI workflows.

## Package selection order

New parity corpora are added in descending MELPA download-count order, not
alphabetically. `melpa-top500-roadmap.tsv` ranks the top 500 packages by
downloads and marks each `covered` (a corpus exists under `src/parity_tests/`
or `src/tui_parity_tests/`) or `todo`; work through the highest-ranked `todo`
rows first. Regenerate the roadmap after adding a corpus or to refresh the
download counts:

```sh
scripts/melpa-top500-roadmap.py
```

## Practical workflow quality (no weak tests)

Parity cases must exercise package behavior, not merely prove symbols exist.

**Do not** add cases whose main assertion is:

- `(commandp …)` / `(fboundp …)` / `(featurep …)` catalogs
- “defaults registered” lists with no driven workflow
- reimplemented copies of package logic (call the package instead)
- inverted or accidental-pass fixtures

Every case should call a public or documented entry point and assert
non-trivial state (buffer text, point, overlays, trees, command lines, errors,
keymap bindings of real commands, file layout, and so on).

## Package lock

`../neomacs-melpa-test-support/melpa-package-lock.tsv` is the single source of
truth for reproducible MELPA inputs shared by batch and interactive adapters.
Each sorted package row owns its version, immutable source revisions, build
rule, and a sorted comma-separated list of direct dependency names (`-` means
none). Every dependency must name another row; because each name has exactly
one pinned version, dependency versions are resolved from that row instead of
being duplicated on each edge.

After preparing package caches, compare their `Package-Requires` headers with
the lock or update dependency cells without changing source pins:

```sh
scripts/melpa-derive-dependencies.py
scripts/melpa-derive-dependencies.py --write
```

## Local commands

Build the release runtime first:

```sh
mkdir -p ./tmp
TMPDIR="$PWD/tmp" cargo xtask fresh-build --release
```

Run the default suite. The pinned Dash and `s` parity corpora prepare their
package caches below `./tmp` on the first run:

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
NEOMACS_MELPA_ORACLE_EMACS="/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs" \
cargo nextest run -p neomacs-melpa-tests --no-fail-fast
```

Run the live lifecycle canary explicitly:

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
cargo nextest run -p neomacs-melpa-tests \
  --run-ignored only \
  -E 'test(=parity_tests::live_melpa::live_melpa_ecosystem_installs_and_survives_restart)' \
  --no-fail-fast
```

Run every comprehensive Dash parity test:

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
cargo nextest run -p neomacs-melpa-tests \
  -E 'test(~parity_tests::dash::)' \
  --no-fail-fast
```

Run every comprehensive `s` parity test:

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
cargo nextest run -p neomacs-melpa-tests \
  -E 'test(~parity_tests::s::)' \
  --no-fail-fast
```

After intentionally updating a package pin or accepted GNU Emacs behavior,
refresh inline snapshots through the same differential oracle:

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
UPDATE_EXPECT=1 \
cargo nextest run -p neomacs-melpa-tests \
  -E 'test(~parity_tests::dash::)' \
  --no-fail-fast
```

Review every snapshot diff before committing it. Divergent GNU Emacs and
Neomacs outcomes fail before `expect-test` can update the snapshot.

When MELPA publishes a new version, update only the hard-coded version next to
the package name in `DASH_MELPA_PIN`, `S_MELPA_PIN`, or the relevant package
matrix entry. Catalogs, dependency metadata,
tarballs, extracted files, and generated local archives stay under `./tmp`.

GNU Emacs selection checks `NEOMACS_MELPA_ORACLE_EMACS`, then
`NEOVM_ORACLE_EMACS`, then `ORACLE_EMACS`, then the adjacent local GNU Emacs
source checkout, and finally `emacs` on `PATH`.
