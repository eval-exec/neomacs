# Neomacs test fonts

This dev-only crate downloads the pinned Spleen 2.2.0 release, a
commit-pinned W3C WOFF2 collection test, Noto Color Emoji 2.051, and the
fonts-ubuntu 0.869+git20240321 / DejaVu 2.37 faces the Linux GUI CI job
installs (`neomacs_test_fonts::desktop_fonts()`) on first use, verifies their
SHA-256 digests (archives and every extracted face), and caches the selected
font files in the shared directory from `font_fixture_cache_root`. A nonempty
`NEOMACS_FONT_FIXTURE_CACHE` wins, then
`$XDG_CACHE_HOME/neomacs/font-fixtures`, then
`$HOME/.cache/neomacs/font-fixtures`. The workspace `tmp/font-fixtures`
directory is only the last resort, when neither home nor XDG is set. No font
binaries are stored in the Git repository. The cache is not per
`CARGO_TARGET_DIR`.

`cargo xtask font-fixtures` downloads and sha-verifies every pin once. Tests
still download on a miss. A download or pin failure panics with the prefix
`FIXTURE-FETCH:`, the URL, the pinned SHA-256, and the I/O error. Tests do
not skip. Delete the cache directory to force a clean download.
