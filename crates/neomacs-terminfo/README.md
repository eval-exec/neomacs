# neomacs-terminfo

Private safe interface to ncurses capability snapshots and numeric expansion.
Editor policy stays in `neomacs`. Only `src/native.rs` permits unsafe code.

`Database::load` copies requested values under the same lock used for expansion.
Snapshots own their bytes; ncurses owns its terminal cache. `expand_numeric`
validates a conservative numeric grammar with regex, then calls native tparm
with nine C long arguments. It rejects string-consuming directives. Native
variables follow the current ncurses terminal context, not individual snapshots.
This crate must be the sole owner of ncurses access in the process.

Builds use pkg-config on Linux/macOS. Direct consumers can read
`DEP_NEOMACS_TERMINFO_RUNTIME_LIBDIRS` in their build script to preserve runtime
search paths. The fallback without pkg-config assumes an unsplit native library.
Other platforms return `UnsupportedPlatform`.

Validation (requires ncurses development libraries, tic, and cargo-nextest):

```sh
cargo nextest run -p neomacs-terminfo --locked
python3 crates/neomacs-terminfo/tests/linkage.py
```

See [design](../../docs/design/neomacs-terminfo-decision.md) and
[native contracts and research](../../docs/design/terminfo-native-research.md).

Numeric division and remainder are preflighted using the actual parameters and
native variable values under the same mutex as expansion. The executed branch,
32-bit arithmetic, zero division, and ncurses' dropped stack pushes determine
whether a signed division can trap. ncurses still produces all output and owns
variable updates. See [follow-up](../../docs/design/terminfo-numeric-followup.md).

For implicit termcap formats, preflight checks all possible initial argument
counts (0 through 9), conservatively rejecting a call if any count could trap.
Explicit parameter formats use their actual stack. Width/precision limits and
string-parameter restrictions remain; this is not unrestricted native tparm.


Padded output is available through `Padding`, independently of the owned lookup
snapshot. It keeps a duplicated output descriptor, creates a temporary native
terminal under the existing mutex, and calls `tputs` only for control sequences.
Text must be written directly. See
[the output design](../../docs/design/terminal-capability-completion.md) for native
state restoration, arithmetic bounds, callback contracts, and platform tests.
