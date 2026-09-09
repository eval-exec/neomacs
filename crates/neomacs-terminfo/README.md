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

Numeric division and remainder require an immediately preceding nonnegative
literal divisor, such as `%{256}%/`. Computed divisors are rejected because
native signed division can trap on INT_MIN / -1. Constants are validated on
directive boundaries so escaped literal text is preserved.

Division programs also have a conservative total-push limit to ensure the
literal divisor fits ncurses' stack (19 explicit or 10 implicit pushes).
