# Test infrastructure fixtures

Prepare the pinned NYAN XPM image before running renderer image tests:

```sh
cargo xtask image-fixtures
cargo nextest run -p neomacs-renderer-wgpu --lib -E 'test(nyan_mode_xpm_keeps_its_gray_ladder)'
```

The explicit preparation command downloads the image at nyan-mode commit
`09904af23adb839c6a9c1175349a1fb67f5b4370` and verifies SHA-256
`d80164170278176347b1c01abc70c552ca2fcb655e279bea9f369206eb2d415e`.
The URL, filename, commit, and digest have one home in `image_fixtures`.
No image bytes are vendored.

Both preparation and test reads share one revision-keyed cache across worktrees,
outside `CARGO_TARGET_DIR`. `NEOMACS_PINNED_FIXTURE_CACHE` wins when nonempty,
then `$XDG_CACHE_HOME/neomacs/pinned-fixtures`, then
`$HOME/.cache/neomacs/pinned-fixtures`, with `<workspace>/tmp/pinned-fixtures`
only as a last resort when neither XDG nor HOME is set (under nextest,
`NEXTEST_WORKSPACE_ROOT` selects the live workspace; otherwise the compile-time
workspace root applies). Each entry lives at `<cache_root>/<sha256>/<name>` so
worktrees pinning the same revision hit the same verified file without
re-fetching, while worktrees pinning different revisions of the same filename do
not collide.

The renderer test reads and re-verifies the cache only. Missing or corrupt
bytes fail with `FIXTURE-CACHE:`, the pin, cache error, and the preparation
command. It never downloads or skips. Re-run `cargo xtask image-fixtures` to
repair an absent or corrupt file; preparation verifies downloads before staging
in a temporary directory inside the cache root and publishing atomically via
rename. Other callers of `pinned_file` retain their existing fetch-on-miss
behavior.

`cargo xtask image-fixtures` accepts no arguments. The existing
`cargo xtask infra` / `cargo run -p neomacs-infra` CLI keeps selecting `infra`
through the package's `default-run` setting.

Archive runners can prepare the same Rust-declared pin without building a
Rust executable:

```sh
python3 scripts/ci/prefetch-image-fixtures.py --workspace "$GITHUB_WORKSPACE"
```

Run this before archive nextest and use that same directory for
`--workspace-remap`. The display CI job does this explicitly. Its archive
`--extract-to "$TMPDIR/nextest"` directory contains binaries, and is not the
fixture cache root. The Python provisioner resolves the exact same shared cache
root and `<sha256>/<name>` entry layout as `neomacs_infra::pinned`, reads the
ordinary literal pin constants from `crates/neomacs-infra/src/image_fixtures.rs`
(with no copy of the URL or digest), verifies hits, repairs missing/corrupt
entries by hash-checking before staging in a temporary directory and publishing
atomically, and bounds download size and I/O time. Tests remain cache-only.
