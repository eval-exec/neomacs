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

The qualified S1 baseline has one positive client and ten negative cases. The macOS overlays
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
env -u TRYBUILD cargo nextest run -p neovm-core --features contract-fixtures --no-fail-fast \
  --test p0s3_contracts --test p1rv_contracts \
  --test thread_safety_contracts --test chartable_write_contracts
```


## S2 completion qualification

The frozen pre-mailbox completion source passed the Linux positive clients
and all seventeen negatives on rustup 1.96.1 without rust-src. The seven new
FIRST diagnostics match the boundaries below; all ten existing goldens remained
byte-identical. TRYBUILD-unset verification passed the unchanged full CI command
(15 outer runners on this revision). The same frozen pre-mailbox completion
source then passed macOS aarch64 source-free rustup 1.96.1 qualification: seven
new overlays, ten old overlays byte-identical, and all 15 outer CI runners passed
with TRYBUILD unset. Only ARM `.stderr` files were returned. No fixture, family
or runner selection was removed.

All seven single-bound controls passed: each relaxed negative and the unchanged
positive prepare/retire/finish client compiled (14 metadata compilations total).
Each control modified only a frozen copy; exact mutation patches, compiler
arguments and source hashes were retained. Production sources remained untouched.
These results cover completion seams before mailbox integration, not future
stored-event validation or transfer contracts.

`actual_completion.rs` includes the complete production
`runtime/threads/completion.rs`, including the real token fields, Drop,
preparation checks and consuming `retire_thread_completion` method. Its manager
adapter retains the real ThreadIds/ThreadPayload/ThreadPhase definitions and
maps of actual heap words/SharedRoot leases. World lookup and terminal
publication are explicitly unreachable compile-only neighbours. The adapter
never fabricates a token or declares a replacement consume method. These
fixtures establish type boundaries, not registry/GC behavior; runtime sole-edge,
retained-count, completion-order and mismatched-World abort tests remain required.

`actual_completion_exit.rs` includes the unchanged production inherent
`Control::finish` method from `green/scheduler/completion_exit.rs`. Native
scheduler/World neighbours are explicitly compile-only; Baton has a real
HeapIdentity and no public constructor. Neither its held-authority check nor
scheduler actions can succeed in this adapter. The witness parameter, body and
consumer are the production definitions; no local toy finish signature is used.
The positive client type-checks prepare -> real retire -> actual finish. The
real runtime still owns all scheduling, Baton and TLS validation.

The existing `p15_threading_compile_contracts` runner's pass/negative globs select
these additions and ARM include wrappers. The CI command and runner selection
are unchanged. There are seven new negative clients and one new positive client.

| Negative client | Observed Linux FIRST boundary | Observed single-bound counterfactual (positive + relaxed negative both compile) |
|---|---|---|
| completion_clone | E0599: actual CompletingThread has no Clone | Derive Clone for the actual token and remove only Clone from its negative static assertion; this relaxes the one no-clone bound. |
| completion_double_consume | E0382: second call to the actual consuming `retire_thread_completion` uses a moved token | Make the actual token Copy/Clone, remove those two negative static-assert entries, and omit its debug-only Drop (required for Copy). This deliberately removes the one affine-consumption boundary; it is a compile-only mutant and must never execute. |
| completion_constructor_private | E0451: actual CompletingThread fields are private outside preparation | Expose only the token's fields and private consumption vocabulary to the fixture crate; no consume/prepare signature changes. This removes its constructor seal. |
| retired_completion_constructor_private | E0451: actual RetiredCompletion fields are private outside retirement | Expose only the retired witness's fields to the fixture crate; no token or method changes. |
| completion_send | Conditional ThreadTransfer method unavailable because actual CompletingThread is !Send | Add a compile-only unsafe Send impl for the actual token and remove only Send from its negative static assertion. |
| retired_completion_send | Conditional ThreadTransfer method unavailable because actual RetiredCompletion is !Send | Add a compile-only unsafe Send impl for the actual retired witness and remove only Send from its negative static assertion. |
| exit_without_retired_completion | E0308 at actual Control::finish: ThreadId is not RetiredCompletion | Widen the actual method's witness parameter to `impl Into<ThreadId>` and add only the conversion from RetiredCompletion. The existing positive still supplies its retired witness, while the relaxed negative supplies raw ThreadId. All heap/authority checks and scheduler statements remain unchanged; the adapter exposes only the matching real ThreadId name. |

For every counterfactual the positive client compiled alongside that relaxed
negative, and the exact changes were recorded. The mechanically necessary
static-assert/Drop adjustments above are parts of the named single bound, not
additional permissions to change semantics. If any later first error is an
adapter dependency/private-neighbour error, repair the adapter and requalify;
never bless it as a confinement failure. Existing S1 mutations/results above
remain historical S1 evidence; the completion controls here are separate S2
evidence. These compile-only mutants were never executed.

## S2 stored-event constructor seal

The mailbox integration adds one positive client and one negative client (with
an ARM include wrapper). `stored_event.rs` calls the included production
`StoredThreadEvent::try_from_roots(&TaggedHeap, &[SharedRoot; 3])` and reads its
validated `values()` result. The constructor is intentionally crate-visible;
the raw sender/error/data fields are private. The support forwards the actual
TaggedHeap and transport types, without replacing materialization or validation.

The frozen constructor client passed a source-free rustup 1.96.1 metadata compile.
The unmodified negative failed first and only with E0451 for `sender`,
`error_symbol` and `data`. Its single-bound mutation made exactly those three
fields crate-visible in a copied production module: both the unchanged positive
and the relaxed negative compiled. The copied source was restored and its hashes
matched afterward; the active product source was never changed. No private
support error was accepted as the intended failure. These are compile-only
observations, not runtime validation or collector evidence.

On the frozen final mailbox/completion candidate, macOS aarch64 rustup 1.96.1
without rust-src qualified all three positive clients and the existing seventeen
negative goldens before accepting only the new E0451 stored-event golden. The
seventeen old ARM goldens remained byte-identical; full CI then passed all 15
outer runners with TRYBUILD unset. Only ARM stderr files were returned. Final
integrated Linux verification remains the local gate's separate responsibility.
The seven completion mutations above retain their stated pre-mailbox scope.
No existing assertion, fixture or runner selection is removed by this addition.


## S2 condition-wait association seal

The condition-wait clients include the complete production
`green/condition_wait.rs`; they do not copy its validated association or
transition definitions. The narrow neighbour adapter keeps actual heap words
and recursion-depth types, while World, scheduler and error-producing methods
are explicitly unreachable compile-only neighbours. The positive client
type-checks the actual wait entry and completed continuation. The negative
tries to pair arbitrary handles and depth inside the real `ValidatedWait`.

A source-free rustup 1.96.1 metadata control compiled the unchanged positive;
the negative failed first and only with E0451 for the actual private
`condition`, `mutex` and `depth` fields. Publishing exactly those three fields
to their parent in a frozen production-source copy made both clients compile.
No constructor or transition changed; the copied source was restored and its
hash verified. This proves the field seal, not wait behavior or root liveness.

On frozen integrated source 0f37bd77ef with these reviewed fixture additions,
macOS aarch64 source-free rustup 1.96.1 qualified all four positives and
nineteen negatives. The eighteen previous ARM goldens remained byte-identical;
only the new E0451 association golden was accepted. Full CI passed all fifteen
outer runners, zero skipped, with TRYBUILD unset. Only ARM stderr files were
returned, and only the new association golden was copied into the source tree.
Final Linux qualification is a separate gate. No existing fixture, assertion or
runner selection was removed.

Final Linux qualification includes four positive clients and 19 negative
clients. Only the StoredThreadEvent and ValidatedWait constructor goldens
were added; all 17 existing Linux goldens stayed byte-identical. Source-free
rustup 1.96.1 verified the unchanged full CI selection: 15/15 passed with
TRYBUILD unset and no skipped tests. The first diagnostics were reviewed from
the original run; a tooling validator initially confused the existing
native_id_kind_mismatch filename with a golden mismatch. Fixing that parser
did not change or repeat a diagnostic, assertion or negative client.

Final S2 qualification: 344 thread/Green cases pass in each GEN1/GEN0 ×
default/inline-off configuration; architecture passes 67/67. Nine new negative
contracts (19 total) and four positive clients pass on Linux and arm64 macOS;
all 15 CI contract runners pass on each platform. The nine single-bound
relaxations compile both paired clients. The native-barrier insertion and the
stored-event DATA-visit omission turn their exact assertions red in both GENs.
The seven selected GNU thread oracle cases pass in all four configurations.
Nextest reports 32,501 unrelated cases deselected by that family selector, not
ignored cases inside the seven-case family. The official nextest archive was
built once on treeroot and replayed on the main host with workspace remapping
and the unchanged final editor/runtime. No production change was made during
contract golden qualification or archive transfer.
