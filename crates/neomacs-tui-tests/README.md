# `neomacs-tui-tests`

End-to-end TUI parity harness comparing Neomacs (`-nw`) against the pinned GNU Emacs oracle over real pseudo-terminals.

## Harness Invariants

Every TUI test must make explicit **which Emacs** and **which terminal database** it ran against:

1. **Attested GNU Emacs oracle only.** Resolve GNU Emacs via `neomacs_tui_tests::oracle::gnu_oracle_program()` (and record `gnu_oracle_stamp()` in diagnostic output). Never hardcode `PathBuf::from("emacs")` or silently fall back to an unattested `PATH` binary or custom env var (`NEOMACS_GNU_EMACS_BINARY`); if the pinned oracle (`parity-reference.toml` / `NEOVM_FORCE_ORACLE_PATH`) is unavailable or non-canonical, the test must fail loudly. Every test file in `crates/neomacs-tui-tests` follows this invariant (#845), enforced by the fail-closed regression guard in `src/oracle_guard_test.rs`.
2. **Explicit terminal database (`TERMINFO`).** Older host terminfo databases (such as macOS's ncurses 5.7 `/usr/share/terminfo`) omit newer entries like `alacritty` and `contour`. When a test requires a `TERM` value absent from the host database, compile the real upstream terminfo source via `tic` into an isolated `TERMINFO` directory (never fabricate a mutant alias from a different terminal type) and report the `terminfo` source in test diagnostics.
3. **Non-blocking PTY master writes.** Both `PtyOutputPump` (`src/pty_output/owner.rs`) and auxiliary non-blocking PTY masters (`write_all_nonblocking_pty` in `src/pty_output.rs`, used by `AuxiliaryTty::send` in `tests/tty_relay.rs`) loop across POSIX short writes, `EINTR`, and `WouldBlock`/`EAGAIN` (waiting on `POLLOUT` within a bounded deadline) while keeping `EIO`/`BrokenPipe`/`WriteZero` fatal and preserving exact committed-prefix accounting.
