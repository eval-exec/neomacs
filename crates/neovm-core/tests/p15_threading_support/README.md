# Threading payload type contracts

These cases include the production `heap_types/threading.rs` definitions.
The support module mirrors only the neighbours needed to type-check that
source. Its string wrapper retains the real LispString's auto traits and
forwards public byte/root APIs. Private storage queries are explicitly
unreachable compile-only glue; these fixtures do not validate GC tracing or
accounting. The positive client must compile before negative diagnostics are
accepted. Runtime sole-edge and stopped-collection tests validate tracing.

The conditional ThreadTransfer witness follows the existing contracts-repair
pattern: diagnostic locations stay in fixture/crate code. Native ThreadId,
MutexId and CondVarId are Send; an owning ThreadPayload is confined.
Production visibility is never widened to accommodate a fixture.

| Negative fixture | Intended boundary | Counterfactual mutation | Observed result |
|---|---|---|---|---|
| heap_object_constructor_private | Actual concrete heap objects are crate-private | Make the three actual object types public | Compiles (exit 0) |
| identity_issuer_clone | An issuer cannot duplicate identities | Derive Clone for the actual issuer and its ordinal counter | Compiles (exit 0) |
| native_id_kind_mismatch | Thread, mutex and condvar identities are distinct | Alias mutex/condvar identities to ThreadId | Compiles (exit 0) |
| native_id_literal | Identity fields cannot be forged | Make the actual native_id fields crate-visible | Compiles (exit 0) |
| payload_send | Owning payload cannot cross a carrier thread | Replace its !Send assertion with an explicit invalid Send impl in the compile-only mutant | Compiles (exit 0) |
| runtime_handle_private | Actual runtime validation handles are crate-private | Make the two actual handle types public | Compiles (exit 0) |
| unchecked_condvar_association | Association requires its validating constructor | Make the actual association field crate-visible | Compiles (exit 0) |
| unchecked_mutex_reference | Mutex word and identity cannot be paired unchecked | Make the actual id/value fields crate-visible | Compiles (exit 0) |
| unchecked_recursion_depth | Positive depth requires its constructor | Make the actual tuple field crate-visible | Compiles (exit 0) |
| unchecked_thread_name | Name requires string-or-nil validation | Make the actual tuple field crate-visible | Compiles (exit 0) |

There is one positive client and ten negative cases. The macOS overlays
include unchanged common fixture bodies; only their expected diagnostics are
per target. Linux cannot bless an arm64_macos golden. Goldens must contain no
rustc-library/rust-src excerpts and must verify with TRYBUILD unset.

All ten counterfactuals compiled successfully, each together with the positive
client. Eight controls changed a byte-identical copy of the included production
payload module; the two external visibility controls changed the actual object
or runtime handle declarations in an isolated worktree. Each run relaxed only
the boundary named above. The mutation harness verified unchanged source hashes
after restoring all files; restored-production TRYBUILD-unset verification passed.
No counterfactual is part of the production source.

On x86_64 Linux and aarch64 macOS (rustc 1.96.1), the positive client and all ten
negative cases passed with TRYBUILD unset. Each host also passed all 16 runners
in the CI command below. Linux ran without installed rust-src; only the ten
macOS diagnostic overlays were returned from the macOS host. No golden contains
compiler-library source excerpts.

```sh
env -u TRYBUILD cargo test -p neovm-core --features contract-fixtures --no-fail-fast \
  --test p0s3_contracts --test p1rv_contracts \
  --test thread_safety_contracts --test chartable_write_contracts
```
