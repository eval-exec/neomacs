# Symbol value-cell ordering model

This independent test workspace models the P6.1 symbol cell protocol used by
P7.4. It adds no feature, dependency, code or layout to the production workspace.
Loom is pinned to 0.7.2, num_enum to 0.7.6 and static_assertions to 1.1.0; its own
lockfile fixes the transitive graph. Redirect's coded representation uses
TryFromPrimitive/IntoPrimitive, checked masked decoding and typed encoding.

The production reference at preparation time is
`bc1f81d06f0f7ec6a8431134bf904b0dda903fb5`:

| Model operation | Production reference | Ordering |
| --- | --- | --- |
| Open write window | `symbol/cell.rs::seqlock_enter` | Relaxed odd increment, Release fence |
| Publish payload | `CellWrite::publish` | Release machine-word store |
| Publish changed redirect | `SymbolFlags::store_byte` | Relaxed byte store after payload |
| Close write window | `seqlock_exit` | Release even increment |
| Begin read attempt | `read_symbol_children` | Acquire sequence load; reject odd |
| Read redirect | `SymbolFlags::load_redirect` | Relaxed byte load |
| Read payload | `LispSymbol::load_word_acquire` | Acquire machine-word load |
| Read function and plist | `tagged/header.rs::load_value_atomic` | Separate Acquire machine-word loads |
| Validate read attempt | `read_symbol_children` | Acquire fence, Relaxed sequence recheck |

Function and plist setters publish separate Release stores **outside**
`CellWrite`. The model therefore allows each to be old or new independently.
It does not assert that the whole four-field read forms one transaction.

The writer endpoint is non-Clone, non-Copy and non-Sync. Its write window holds
an exclusive borrow, closes with one atomic operation, and is also non-Clone,
non-Copy and non-Sync. Adjacent static assertions pin these contracts. Private,
sealed protocol types specify the exact production orderings and each control.
All shared words use Loom atomics. There is no unsafe code or payload dereference.
The modeled flags byte preserves fixed unrelated high bits (0x60) while the GNU
redirect occupies the low two bits; no behavior depends on those high bits.

## Cases and controls

The production cases cover Plain→Alias/Localized/Forwarded, Alias→Plain,
Forwarded→Localized, and payload updates within each of the four arms. The
initial and final snapshots are checked explicitly; an accepted concurrent
snapshot must have exactly the initial or final `(redirect, payload)` pair.
Opaque alias/BLV/forwarder words deliberately resemble mock heap values, so only
Plain cells may contribute that word to the children. Function and plist are
checked independently. A typed `ValueCell`/`MockValue` oracle supplies expected
children independently of the raw tag/mask interpreter. Minor collections trace mock heap values; major
collections also trace mock symbol values.

Two negative controls independently remove the writer or reader fence without
changing any field ordering. Each must produce the specific mixed-pair assertion
failure; an unrelated panic cannot satisfy the test.

Two distinct positive experiments prevent a misleading fence claim:

- A same-tag payload update, with both fences removed, retains payload
  Release/Acquire and has no Relaxed tag publication to protect.
- A named stronger-tag experiment uses Release/Acquire **tag** accesses and
  removes both fences. These are stronger field orderings than production and
  are never substituted into its cases or negative controls.

The final debug run passed all 10 tests (six production tests covering nine
transitions, two negative controls, two distinct positive experiments).
Both single-fence omissions produced the specified mixed-pair assertion:
the reader accepted `Plain(5)` when the old pair was `Alias(5)` and the new pair
was `Plain(9)`. Neither control weakened the payload/function/plist orderings.
The same-tag and stronger-tag experiments passed without fences. These outcomes
do not establish that either fence can be removed from the production protocol.

The initial run also passed 10/10. Its child-list oracle was then improved to use
typed values independently of the raw interpreter, and the second run passed
10/10 again. The final run after the num_enum conversion passed 10/10 with
unchanged protocol orderings and typed oracle. Raw logs, toolchain and source
receipts, file hashes and prior sources are retained under
`tmp/codex/p74/loom/`; the final log is `model-nextest-v3.log`. All three runs used
nextest with 16 test threads, Rust 1.96.1 and an unoptimized debug target.

## Scope and limits

One preinitialized symbol, one writer, one reader and one write window are
modeled. The sequence changes from 0 to 1 to 2, so it cannot wrap. One reader-loop
attempt is explored; rejection represents a production retry. This checks
accepted snapshots, not eventual retry convergence or fairness. The model fixes
two threads and a 128-branch safety bound, with no permutation, time, preemption
or checkpoint truncation. A branch-bound panic is a test failure.

This does not validate the raw-pointer atomic casts in production, allocation
alignment, pointer lifetime, chunk leases, payload ownership, SATB completeness,
mark-gate transitions, forwarder interior publication or multiple mutators.
Those obligations remain with their production types and tests. The model is a
reviewed copy of the protocol: changing production orderings requires checking
this correspondence again.

[Loom's documentation](https://docs.rs/loom/0.7.2/loom/) requires instrumenting
shared synchronization with its replacements and documents incomplete coverage
of some Relaxed-memory executions. Passing this bounded model is not a complete
C11 memory-model proof. The intended Relaxed-tag fence bridge follows Rust's
[fence synchronization rule](https://doc.rust-lang.org/std/sync/atomic/fn.fence.html):
a Release fence before a store can synchronize with an Acquire fence after a
load that observes that store. The payload's own Release/Acquire operations must
remain represented even when that makes an experiment's fences redundant.

## Run

Use the repository's Nix shell and the lane's isolated target. The prepared
driver is `tmp/codex/p74/loom/run-model-v3.sh`; it runs nextest with 16 test threads
and writes direct logs under the lane scratch directory. No Lisp runs here.
The dependency-preparation driver only resolves the lockfile and formats this
standalone crate; it compiles and executes no tests.
