# Graphical terminal identity — issue 364

## Contract and agreed test seams

A live graphical terminal must not retain the bootstrap name
`initial_terminal`. Its name and the frame's native display identity must come
from one opened-backend observation. Terminal labels need not be unique.

The user requested reproduction, GNU-source-first diagnosis, a failing test
before the fix, a long-term typed design, and real ace-window coverage. The
agreed seams are Lisp-visible terminal/frame identity and real MELPA ace-window
TTY selection/switching. Native GUI tests belong in `neomacs-gui-tests`; PTY
workflows belong in `neomacs-melpa-tests`' TUI suite. Use `cargo nextest`, not
`cargo test`. Only Linux Wayland can be verified natively here.

## Reproduction and root cause

Issue 364 was reported on macOS. On Linux the same startup defect reproduces
with an inherited Wayland connection: open the compositor socket, pass its FD
through `WAYLAND_SOCKET`, and remove `DISPLAY` and `WAYLAND_DISPLAY` from the
child environment. The real release GUI produced:

```elisp
(:terminal "initial_terminal" :windows 2)
```

The first GUI regression run failed with `:graphical t :initial nil :eligible
nil :windows 2`. It exercised actual GUI startup, without replacing
`terminal-name`. The pre-fix test log is `/tmp/neomacs-364-gui-red.log`.

Previously `host_gui_display_identity` guessed the backend and name from two
environment variables. Missing variables produced no identity. Graphical
terminal initialization updated the output method but only replaced the name
when supplied, leaving the bootstrap name. Frame startup independently repeated
the environment lookup. ace-window's `aw-window-list` rejects terminals named
`initial_terminal`, so the otherwise live GUI windows disappear from its list.

## GNU Emacs source findings

Studied local checkout `/home/exec/Projects/github.com/emacs-mirror/emacs`
at `a360712c9d272d950d8d8255ef74570f7e90b7d9`:

- `src/terminal.c`: `create_terminal` assigns the output kind;
  `init_initial_terminal` assigns `initial_terminal` only to the bootstrap;
  `Fframe_initial_p` checks output kind; `Fterminal_name` returns the stored name
  and explicitly documents that it need not be unique.
- `src/term.c`: TTY initialization uses the device name (`/dev/tty` by default).
- `lisp/term/ns-win.el` and `src/nsterm.m`: NS opens a connection named from
  `system-name` and stores that name in its terminal.
- `lisp/term/w32-win.el` and `src/w32term.c`: Windows uses `w32`.
- `src/androidterm.c`: Android uses `android`.
- `src/pgtkterm.c` and `src/xterm.c`: graphical initialization copies the opened
  display's name to the new terminal.

Neomacs keeps its existing terminal-record lifecycle; copying GNU's allocation
strategy is not necessary to fix the identity contract.

## Design

- `neomacs-display-protocol::display_identity`: closed `GraphicalBackend` enum
  with strum-derived backend labels, validated `GraphicalDisplayIdentity` with
  private fields and no `Default`. Empty, NUL-containing and bootstrap names
  are rejected. An anonymous connection has a backend label, not an invented
  address; it must not export a fallback label as an X11 connection address.
- `neomacs-display-runtime::display_identity`: observes the actual winit display
  handle; environment variables supply backend-specific names, never choose the
  backend. `DisplayIdentityResolver` captures connection provenance before
  winit can consume `WAYLAND_SOCKET`; an inherited FD never adopts an unrelated
  `WAYLAND_DISPLAY` name. Cocoa uses the host name, Windows/Android/Web stable
  backend labels.
- `neovm-core::...::terminal::config`: enum alternatives `Bootstrap`,
  `Tty(TtyTerminalConfig)`, `Graphical(GraphicalDisplayIdentity)`. GUI startup
  cannot omit an identity or use a TTY-only builder. Installation replaces name,
  output kind and runtime together.
- Frontend bootstrap carries the same identity into terminal and frame
  initialization. Non-graphical frames explicitly have no graphical identity.

## Verification commands

```sh
cargo nextest run -p neomacs-gui-tests --test terminal_identity --run-ignored ignored-only
cargo nextest run -p neomacs-display-protocol -p neovm-core --lib -E 'package(neomacs-display-protocol) | test(terminal::) | test(display_identity) | test(getenv_display)'
cargo nextest run -p neomacs -p neomacs-display-runtime --lib
NEOMACS_PARITY_REFERENCE=none cargo nextest run -p neomacs-melpa-tests --test melpa_tui -E 'test(ace_window)'
cargo xtask fresh-build --profile release --no-byte-compile
```

The local GNU dump currently differs from the repository's pinned fingerprint.
The explicit `NEOMACS_PARITY_REFERENCE=none` run is an **unattested behavioral
comparison**, not a comparable pinned parity measurement. The pin is unchanged.
TUI tests load pinned ace-window/avy without replacing `terminal-name`, exercise
three-window labels, cancellation and two-window automatic switching, and check
candidate counts, selected buffers and absence of edits. Report sequence numbers
prevent a stale echo-area message from satisfying a later checkpoint.

Native macOS, Windows, Android and browser behavior is not verified on this host.

## Review follow-up

Spec review caught the inherited-FD-plus-stale-display-name case. A second real
GUI test reproduced it before the follow-up fix: the terminal and frame both
reported `wayland-unused-issue-364`, despite the actual connection being the
inherited socket. Evidence: `/tmp/neomacs-364-gui-provenance-red.log`.
Standards review found no hard violation; the fallible identity constructor is
now the production interface, with the old infallible Wayland convenience
restricted to unit-test fixtures and the unused X11 convenience removed.

The intermediate release passed the original inherited-socket regression and
failed only the stale-display-name regression (`/tmp/neomacs-364-gui-intermediate.log`).
The final follow-up is rebuilt with the same release features but only the
`neomacs` binary; `fresh-build --skip-build --no-byte-compile` then regenerates
the matching role images and dumps. No default features were changed.

Do not run Lisp-consuming test suites concurrently with dump regeneration:
`fresh-build` briefly removes generated Lisp inputs (for example
`lisp/leim/leim-list.el`). The first broad run overlapped this interval and
reported affected process/bootstrap failures. Those cases must be rerun with
the generated inputs stable; that run is not evidence of new identity failures.

The broad run completed 11,759 tests: 11,735 passed, 24 failed, 59 skipped.
The stable-input rerun exercised all failures plus protocol/runtime/terminal
coverage: 1,775 passed and six failed. All additional process and pdump failures
cleared, including the UTF-16 and stderr-pipe cases. Remaining failures:

- Three existing frontend video expectations when running without `video`.
- Existing frontend batch-startup error reporting and GUI `next-line` tests.
- The bytecode inventory check: `loaddefs.elc` and `neomacs-effects.elc` are
  absent. They were not generated because byte compilation was explicitly
  disabled.

Logs: `/tmp/neomacs-364-suite.log` and `/tmp/neomacs-364-rerun.log`.

## Final results

- `cargo check -p neomacs` passed.
- Clean focused nextest run: **1,757 passed**, no failures. Log:
  `/tmp/neomacs-364-focused-green.log`.
- Final release build and matching pdump generation succeeded, without byte
  compilation. Logs: `/tmp/neomacs-364-release-final.log` and
  `/tmp/neomacs-364-pdump-final.log`.
- Both real Wayland GUI regressions passed against that binary: anonymous
  inherited socket and inherited socket with an unused display name. Both
  expose `wayland` as the terminal and frame display label, not the bootstrap
  name. Log: `/tmp/neomacs-364-gui-green.log`.
- Both real ace-window PTY workflows passed against the final binary and the
  local GNU reference (explicitly unattested). Log:
  `/tmp/neomacs-364-tui-final.log`.
- Ordinary named-Wayland GUI startup also exited cleanly; result:
  `/tmp/neomacs-364-named-wayland-result.el`.
- Existing user changes to `lisp/ldefs-boot.el` were preserved byte-for-byte:
  SHA-256 `ff6e7afd590a92f90d7dce6d5d09394c4f69a148182e9b6a1f8684bbd278223c`.

Final standards review: no hard violations. Two optional interface cleanups
remain outside this fix: TTY builder placement and the separate owner-name
argument retained for existing bootstrap/TTY callers. Final spec review: no
remaining implementation findings. Native verification remains Linux-only.
