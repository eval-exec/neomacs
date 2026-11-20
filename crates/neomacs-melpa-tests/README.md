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
- `../neomacs-melpa-test-support/src/source_lock.rs` builds every scenario's
  packages from the revision pinned in `melpa-package-lock.tsv` (see "Package
  lock" below), so a run never depends on current upstream state. No
  third-party package payload is tracked by Git.
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
- `src/parity_tests/etaf_*` and `src/gui_parity_tests/etaf_*` cover the pinned
  `emacs-taf` ecosystem: a load/API-surface test for every package, behavioural
  batches for the pure engines and readers, and real-GUI page/panel probes.
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
- The interactive package-TUI corpus moved to the `neomacs-tui-tests` package
  (`tests/package_tui/`) and runs in the tui shards, while the batch and GUI
  corpora remain in this library test target. Keeping the interactive corpus
  in the tui crate's single test binary avoids linking the engine once per
  test file. The opt-in `tui` module in `neomacs-melpa-test-support` supplies
  the reusable scenario pipeline. Every package TUI scenario launches the
  same prepared package set in GNU Emacs and Neomacs, declares a closed
  terminal profile, and must observe the same readiness checkpoint in both
  editors before the sessions can be driven.
  Complete-screen checkpoints default to exact visible display parity: text,
  geometry, cursor, face/style classes, and resolved RGB colors. A deliberately
  named raw-terminal checkpoint remains available for tests whose subject is
  the terminal wire representation itself, such as indexed palette values.
The GNU package-resource contracts are required CI checks.

## Package selection order

New parity corpora are added in descending MELPA download-count order, not
alphabetically. `melpa-top500-roadmap.tsv` ranks the top 500 packages by
downloads and marks each `covered` (a corpus exists under `src/parity_tests/`
or `crates/neomacs-tui-tests/tests/package_tui/`) or `todo`; work through the
highest-ranked `todo`
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

Run only the real-terminal package workflows after building the release
runtime (the corpus lives in the tui crate now):

```sh
TMPDIR="$PWD/tmp" \
NEOMACS_TUI_NEOMACS_BIN="$PWD/target/release/neomacs" \
NEOMACS_BIN="$PWD/target/release/neomacs" \
NEOMACS_MELPA_ORACLE_EMACS="/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs" \
cargo nextest run -p neomacs-tui-tests --release -E 'test(~package_tui::)' \
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

## ETAF ecosystem corpus

The pinned `emacs-taf` packages are the project's largest real-world divergence
surface. `parity_tests/etaf_ecosystem` loads every package (32 modules) and
compares the feature plus the package-prefixed `fboundp`/`boundp` surface;
batch modules cover `ecss` (cascade/specificity/layers/var), `ebox`
(render/flex/mutation), `ebox-playground` (fixed-viewport file rendering,
same-basename companion loading, preview replacement, source errors, close
lifecycle and .ebox file mode), `ekp` (layout plans/justification), `etaf`
(refs/Data/animation/View), `etaf-ui` (color/QR/icons/editor), `etaf-ncm`
(session/library/search/transport/panes/LRC/layout/palette through its
documented scripted-client seam, so no network, Node, mpv or login),
`etaf-rss` (RSS/Atom/JSON/RDF parsing, OPML, HTML blocks/wrapping, store
round-trip, and local file:// subscribe/search/scopes with no network),
`etaf-bili` (popular/search/selection/play through its scripted client, plus
WBI signing and response parsing), `etaf-pi` (the pure wire-event projection:
streaming drafts, agent lifecycle, failures, queue, dialogs, authoritative
messages), `etaf-weread` (shelf/search refresh, selection clamping, book
detail caching, highlight/thought grouping, pane toggles, export and backend
failures through its documented scripted client), `etaf-pomodoro` (creation
validation, start/pause toggling, extension, reset, style cycling and
stopping through the public session API), `etaf-desktop` (window lifecycle,
geometry policy, workspaces, settings and panels, tiling splits, session
round-trips and the application/instance registry, plus mounted canvas text),
`etaf-ascii-video` (source normalization, render-mode/speed/file/URL
validation before a decoder starts, and player-command errors outside a
player buffer; frame decoding is timing-dependent and not pinned),
`etaf-screensaver` (scene registry and lookup, custom registration and
replacement, directory loading with probe-written spec files, start/stop
lifecycle and first-key dismissal; the MELPA build installs no bundled scene
resource directories, and animated frame content is not pinned),
`etaf-gamekit` (the built-in engine descriptions, adapter registration,
duplicate/replace/unregister behavior, game-definition validation, and
pre-process start failures; no engine subprocess is spawned),
`etaf-posframe` (panel-option validation, batch-frame palette derivation,
empty-panel queries and non-panel operation errors; child-frame creation is
covered by the GUI corpus), `etaf-video-studio` (open/state through a
test-written fake CLI, the startup list/status protocol, project loading and
clip selection, CLI error and missing-executable reporting, close and
outside-buffer errors, and local title validation; no real project store,
ffmpeg or network), `etaf-algo-lab` (catalog/lifecycle, player controls,
A*/Dijkstra/BFS runs and unreachable goals, all four sorts and headless
comparison, and laboratory switching through published Hosts; animated frame
text is reduced to counts and matches), and the
`etaf-md`/`etaf-org-preview` headless readers. `gui_parity_tests/etaf_*` adds
real X11 probes for the md/org pages and the posframe child frame.

`etaf-reader`, `etaf-youtube`, `etaf-video-player`, `maillist`, the
`etaf-weread` local highlight index, and other `etaf-db` consumers open a
SQLite library through Emacs' built-in `sqlite` support. The attested GNU 31.1
oracle profile has no `SQLITE` feature, so those store-backed corpora cannot
run against it: a local GNU in this environment reports `sqlite-open` unbound
too. Their load surface and non-SQLite behaviour are covered, and the
store-backed behavior remains **UNATTESTED** until the oracle recipe includes
SQLite. (Probing them against another GNU build would break the oracle pin.)

The GUI probes run both editors on one X11 display with a fixed DejaVu Sans
Mono 10pt face, so pixel values do not move with the host's font, and assert
**strict parity**: each probe's GNU observation is pinned, and Neomacs must
produce the identical observation. There is deliberately no pinned Neomacs
outcome, so a divergence cannot be blessed by re-recording it. The GUI shard
is therefore red until these known bugs are fixed (they are failures, not
expected results):

- a posframe anchored at `(cells 1 1)` lands at top 17 in GNU and top 68 here;
- one default line is 17 px in GNU and 18 px here (a 12-line page is 245 vs
  256 px tall);
- a face set to `:height 100` reports 101 in GNU and 100 here, and scaled
  heading heights differ by 2;
- every ETAF-mounted buffer line here (previews, panels, the desktop canvas)
  starts with U+200B.

`frame-live-p` reports each backend's terminal type (`x` in GNU, `neo` here),
as GNU's `Fframep`-based contract requires; the posframe probe records the
`frame-live-p`/`framep` agreement instead of the backend symbol so backend
identity is not compared as if it were a divergence. The panel palettes agree.

The etaf-algo-lab crash surfaced 2026-11-19 is fixed. Mounting either
laboratory used to abort Neomacs with an allocation failure while GNU renders
it; the core dump showed unbounded recursion in `neovm_value`'s
`to_equal_key_depth_swp`, the stored equal key of a shared structure (the
laboratories abort at 4, 8 and 32 GB limits, so it was unbounded work, not a
memory budget). The stored-key walk is now node-budgeted and membership still
compares live key objects, so GNU's observable behavior is unchanged.
`etaf_algo_lab_real_gui_workspace_matches_gnu` is a strict parity test and is
red for the remaining observations plus 594 vs 581 pixels for the same 34 body
lines.

The etaf-desktop GUI test is also red for a separate reason: GNU's own
`etaf-desktop-mount` comes back with an empty buffer in this environment, and
the probe's stale match data then raises `args-out-of-range 0 0`; the Neomacs
side still renders the canvas. That is a probe/environment issue to fix, not a
Neomacs divergence.

When MELPA publishes a new version, update only the hard-coded version next to
the package name in `DASH_MELPA_PIN`, `S_MELPA_PIN`, or the relevant package
matrix entry. Catalogs, dependency metadata,
tarballs, extracted files, and generated local archives stay under `./tmp`.

GNU Emacs selection checks `NEOMACS_MELPA_ORACLE_EMACS`, then
`NEOVM_ORACLE_EMACS`, then `ORACLE_EMACS`, then the adjacent local GNU Emacs
source checkout, and finally `emacs` on `PATH`.
