# PR 363 Windows terminal parity audit

Audit date: 2026-09-09. Neomacs: `1a16429025` (merged PR 363);
comparison: `git diff 1a16429025^...1a16429025`. GNU Emacs local source:
`a360712c9d272d950d8d8255ef74570f7e90b7d9` under
`/home/exec/Projects/github.com/emacs-mirror/emacs`.

Requirement: “what gnu/emacs can do, neomacs need can do too.” This is a source
audit of terminal capabilities, output setup, and the input seam, not Windows
runtime validation or a complete audit of the Windows editor. No tests were run.

## Findings

### 1. Windows color capabilities come from TERM guesses, not console capabilities

Priority: P1 parity gap; **pre-existing**, not introduced by PR 363.

`crates/neomacs-terminfo/src/lib.rs:79` returns `UnsupportedPlatform` for every
Windows database load. `crates/neomacs/src/terminal_capabilities.rs:86` converts
that failure to `None`. `crates/neomacs/src/tty_init.rs:59` then follows this
platform-independent decision table:

| Windows environment | Neomacs reported color cells |
| --- | --- |
| TERM unset or empty | 0 |
| TERM containing `256color` | 256 |
| Any other nonempty TERM | 8 |

`COLORTERM=truecolor` cannot affect this fallback: it is used only after a
successful database load. These values reach the Lisp terminal state through
`crates/neovm-core/src/emacs_core/display/terminal/pure.rs:491` and the renderer
through `tty_init.rs:314`. `backend/tty/rif.rs:2851` gates color emission on a
positive count, so an unset TERM suppresses colors even in a capable console.

GNU's Windows path does not require a terminfo entry or this TERM heuristic.
GNU `src/term.c:4716` installs Windows attributes and initial direct colors.
`src/w32console.c:633` negotiates output mode and sets the effective color
count to 16,777,216 for VT success or 16 for legacy output (`:654`).

Source reproduction: evaluate the above three branches with Windows' always
failing loader. This establishes the result without assuming how a particular
shell sets TERM. A native Windows test should compare the Lisp color count and
realized face output under these environments and both console modes.

### 2. Raw ANSI output has no matching Windows output-mode setup or legacy fallback

Priority: P1 parity gap; **pre-existing**, not introduced by PR 363.

`crates/neomacs/src/tty_init.rs:318` enables crossterm raw mode, then `:325`
writes `tty_enter_sequence()` directly to stdout. The sequence at `:293`
contains ANSI alternate-screen, input-mode, cursor, and erase commands. Color
emission similarly creates ANSI bytes in
`crates/neomacs-display-runtime/src/backend/tty/rif.rs:2874` and `:2901`.

The locked crossterm 0.29.0 implementation of
`src/terminal/sys/windows.rs:31` changes the **input** console mode only.
Its output VT negotiation lives separately in `src/ansi_support.rs:16`,
invoked by `supports_ansi` at `:33`. Searching the Neomacs application and
display runtime found no call to that API, Windows `SetConsoleMode`, or
crossterm command execution that would establish output VT mode for this path.
Thus enabling input raw mode is not evidence that raw ANSI output will work.

GNU explicitly sets `ENABLE_VIRTUAL_TERMINAL_PROCESSING`, checks success, and
changes the color model (`src/w32console.c:633`). It also installs native
cursor/clear/glyph hooks in `initialize_w32_display` (`:963`) and retains
non-VT output branches (`:424`, `:496`).

Affected condition: a console output handle starts with VT processing disabled,
or cannot support VT output. The source has no negotiation/fallback for that
condition; precise visible failure still requires native Windows execution.

### 3. Fallback claims styled underlines that GNU's Windows path does not install

Priority: P2 capability-reporting discrepancy; **pre-existing**.

`crates/neomacs-display-protocol/src/tty_capabilities.rs:567` constructs the
Windows fallback with all styled-underline sequences (`:575`). The capability
predicate returns true for them at `:641`. GNU's Windows initialization
(`src/term.c:4727`) installs ordinary underline only; the extended style lookup
at `:4694` is in the non-DOS/Windows branch. GNU's Windows face writer emits
ordinary underline for any nonzero underline (`src/w32console.c:883`).
This is a source-level policy difference, not proof that a modern Windows
terminal cannot render those extra styles. Test the intended Lisp predicate
and emission parity before deciding whether additional functionality should
be exposed deliberately.

## Input and PR attribution

There is a real Windows input path: the non-Unix `read_tty_events` at
`crates/neomacs-display-runtime/src/tty_input.rs:234` polls crossterm and maps
parsed key events, resize events, and mouse events. Missing terminfo function
keys alone is therefore **not** evidence that Windows keys fail.
GNU likewise installs `w32_console_read_socket` separately
(`src/w32console.c:986`). Detailed key/modifier equivalence remains unverified.

Before PR 363, `open_platform_terminal_capability_database` explicitly returned
`None` on Windows and `expand_capability_parameter` returned `None` there.
The merge did not change `tty_init.rs`, the renderer fallback, or this input
path. No new Windows functional regression was established by this audit.
Moving ncurses behind a safe crate neither creates nor resolves the existing
Windows capability and output gaps.

## Recommended validation boundary

Add a Windows-specific capability/setup path, using an existing dependency for
console operations where it meets GNU behavior. Validate native Windows
VT-enabled, initially VT-disabled, and legacy/failure cases, varying TERM and
checking `tty-display-color-cells`, face predicates, and rendered output.
Run Rust tests with `cargo nextest`; Windows runtime evidence is still required
even if pure decision-table tests pass on Linux.
