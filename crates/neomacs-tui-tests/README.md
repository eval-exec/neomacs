# `neomacs-tui-tests`

End-to-end TUI parity harness comparing Neomacs (`-nw`) against the pinned GNU Emacs oracle over real pseudo-terminals.

## Harness Invariants

Every TUI test must make explicit **which Emacs** and **which terminal database** it ran against:

1. **Attested GNU Emacs oracle only.** Resolve GNU Emacs via `neomacs_tui_tests::oracle::gnu_oracle_program()` (and record `gnu_oracle_stamp()` in diagnostic output). Never hardcode `PathBuf::from("emacs")` or silently fall back to an unattested `PATH` binary; if the pinned oracle (`parity-reference.toml` / `NEOVM_FORCE_ORACLE_PATH`) is unavailable or non-canonical, the test must fail loudly.
2. **Explicit terminal database (`TERMINFO`).** Older host terminfo databases (such as macOS's ncurses 5.7 `/usr/share/terminfo`) omit newer entries like `alacritty` and `contour`. When a test requires a `TERM` value absent from the host database, either supply an explicit isolated `TERMINFO` directory compiled with `tic` and document which capabilities the test depends on, or report the `terminfo` source in failure diagnostics so a missing entry is never mistaken for an editor regression.
3. **Non-blocking PTY master writes.** `PtyOutputPump` puts the PTY master descriptor in `O_NONBLOCK` mode and drives both reads and writes through a single owner (`src/pty_output/owner.rs`). A short `write(2)` is normal POSIX progress when input exceeds the kernel PTY buffer (for example 1022 bytes on macOS/XNU vs 4096 bytes on Linux); `Owner::input` loops across short writes, drains master output between chunks, and waits on `POLLOUT`/`POLLIN` for `WouldBlock` within the request deadline while preserving exact committed-prefix accounting on real errors (`ZeroWrite`, `BrokenPipe`/`EIO`, `OutputClosed`, `Cancelled`, `Deadline`).
