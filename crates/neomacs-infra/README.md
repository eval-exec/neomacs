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

Both preparation and test reads use the existing workspace
`tmp/pinned-fixtures` cache, outside `CARGO_TARGET_DIR`. Under nextest,
`NEXTEST_WORKSPACE_ROOT` selects the live workspace; otherwise the existing
compile-time workspace root applies. Prepare each workspace that will run
this test, including a remapped archive workspace.

The renderer test reads and re-verifies the cache only. Missing or corrupt
bytes fail with `FIXTURE-CACHE:`, the pin, cache error, and the preparation
command. It never downloads or skips. Re-run `cargo xtask image-fixtures` to
repair an absent or corrupt file; preparation retains the existing verified
fetch/atomic-rename behavior. Other callers of `pinned_file` retain their
existing fetch-on-miss behavior.

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
fixture cache root. The Python provisioner reads the ordinary literal pin
constants from `crates/neomacs-infra/src/image_fixtures.rs`; it has no copy
of the URL or digest. It verifies hits, repairs missing/corrupt entries by
hash-checking before an atomic replacement, and bounds download size and
I/O time. Tests remain cache-only.
