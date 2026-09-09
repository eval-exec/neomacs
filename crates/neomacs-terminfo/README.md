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

Numeric division and remainder use a bounded constant/stack analysis before
native expansion. This accepts literal and character divisors, computed
constants, and variables assigned within the program. Both conditional paths
must prove safe: inherited variables and parameters remain unknown, and
arithmetic overflow discards a constant fact. A divisor that could trigger
native INT_MIN / -1 (or remainder) is rejected. ncurses still produces all
output and owns variable state. See [follow-up](../../docs/design/terminfo-numeric-followup.md).

Division programs must fit ncurses' 20-slot stack on every path. Long programs
that consume their intermediate values are accepted. Implicit termcap arguments
reserve up to nine unknown slots; overflowing programs remain unsupported.
