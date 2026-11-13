# D1 cache raw-to-owner chain audit

**Result: 25 source formation expressions; zero cache-local witnesses satisfying all three repair conditions.** The proposal adds precise owner-chain comments at all 25 expressions. No executable repair or `from_mut` churn is justified by the specified criterion in this file.

Base: `06bef6ee903a84307b59bd636ac14ba6fc626d41`, extracted with `git show`. The committed cache.rs blob and artifact original both hash to **5f69d75541c862930acfcbf794d9a3671f9fa840**. Every line below is at that base, not at a proposal's shifted lines.

This audit is limited to **owner-chain provenance** in `crates/neovm-eval/src/emacs_core/runtime/jit/cache.rs`. It does not certify raw pointer validity, rooting, cache/slot lifetime, OSR shape, public API unsafety, or an ancestor owner formed in a caller outside this file. Those obligations remain with the relevant dispatch-chain owner. No declaration, kind, fixture, slot writer, or runtime behavior change is proposed here.

## Inventory and criterion

The three conditions are:

- A: this code forms a reference owner from a raw pointer.
- B: a write to the overlapping borrowed allocation occurs through an **older** raw pointer (or a derivative of that older pointer), while that reference must remain usable.
- C: this reference owner is used again after that candidate raw operation.

A raw write to a different allocation is not an alias violation of this owner. A pointer freshly derived from the current callback owner is not an older ancestor. A copied scalar is not an owner. A new reference after native return is not a use of an earlier reference just because both bindings have the same spelling. These distinctions are determined from actual types, lexical blocks, signatures and later uses, not variable names.

The exhaustive source-expression inventory is **19 explicit raw borrow/dereference expressions, three raw `ctx.as_ref()` expressions, and three implicit method-receiver reference expressions**. `audit.tsv` / `audit.json` contain source function, line, actual expression/owner, A/B/C, the failed condition, concrete use/lifetime evidence, and the exact proposed comment for every row.

| Base line | Source function | Owner formed | Why A+B+C does not hold |
| --- | --- | --- | --- |
| 962 | try_run_osr_probe | mutable Context in bind_frame seed block | C fails: last use extends jit_bind_stack970; block ends972 before native entry977–982. |
| 985 | try_run_osr_probe | implicit shared Vec receiver for jit_bind_stack.len | C fails: only copied usize escapes this debug-assert statement; later raw deopt hooks cannot reuse the temporary. |
| 1063 | osr_lookup | shared live Context, including cfg-test reuse1119 | B fails: the older raw ctx reaches naming/environment metadata readers, never a Context writer/native body. |
| 1596 | callee_name_hint | shared Context captured by names_this_leaf closure | B fails: raw input is shadowed; closure/read helpers use only the current typed owner. |
| 2522 | request_upgrade | ObarrayView receiver and shared Obarray manager | B fails: older ctx reaches shared naming/environment reads; compiler receives typed obarray, not a raw Context writer. |
| 2622 | probe_tier2_upgrade | optional shared Context in upgrading arm | B/C fail: only typed probe/install occurs; raw request_upgrade is in the mutually exclusive other arm. |
| 2845 | try_run_compiled | ObarrayView receiver/shared manager in cache closure | B fails: while borrowed, raw consumers only probe/compile/read metadata; closure ends3013 before native dispatch3029. |
| 2878 | try_run_compiled | optional shared Context in pending arm | B/C fail: pending arm returns typed probe/install result immediately. |
| 2887 | try_run_compiled | optional shared Context call temporary | C fails: drain_ready_in consumes it and returns no reference; temporary is not reused. |
| 3076 | armed_leaf_for_stack_call | implicit shared CompiledLeaf receiver of accepts | B/C fail: accepts reads required/arity/rest only; owner ends with bool result. Raw scalar tuple fields create no extra owners. |
| 3117 | armed_leaf_for_native_call | implicit shared CompiledLeaf receiver of accepts | B/C fail: same read-only temporary; later heat changes target RuntimeState. |
| 3140 | run_armed_leaf | shared CompiledLeaf from leaf pointer | B fails: old raw leaf is shadowed; typed leaf only is forwarded. Native Context writes target another allocation; leaf counters/spec cells use interior mutability. |
| 3211 | resolve_spec_call_leaf | optional shared Context | C fails: last use is install_pending3213, before subsequent raw upgrade probe; insertion closure does not capture live. |
| 3224 | resolve_spec_call_leaf | ObarrayView receiver/shared manager in insertion closure | B fails: raw ctx goes only to shared metadata readers; typed compiler receives obarray. No native body runs. |
| 3316 | run_resolved_leaf | mutable charge Context | C fails: charged block ends3318 before raw callback3322. |
| 3326 | run_resolved_leaf | newly formed release Context | B/C fail: native already returned; release writes through this current typed owner and no later raw call follows. |
| 3376 | run_resolved_leaf_native | mutable charge Context | C fails: block ends3378 before raw callback3389. |
| 3393 | run_resolved_leaf_native | newly formed release Context | B/C fail: release follows native return and uses only the current typed owner. |
| 3437 | run_with_bytecode_stack_probe_sampled | mutable stack-probe owner | B fails: callback ctx is genuinely &mut Context, supplied by the typed helper; its cast derives a current-owner descendant. Older outer raw ctx is not captured. |
| 3586 | deopt_resume_outcome | mutable Context | B fails: incoming raw is shadowed; resume_deopt receives current typed owner only, and cache retains no older pointer for a callback. |
| 3668 | dispatch_raw_signal | mutable Context | B fails: raw is shadowed; typed dispatch_signal_flow_cold only, then result conversion. |
| 3799 | finish_native_run | mutable Context in DeoptAt arm | B fails: typed resume only in this arm; older raw note_deopt call belongs to another match arm. |
| 3870 | prepare_entry_for_test | optional shared Context, then current typed view/manager reborrow | B fails: subsequent raw environment entry only reads; compiler receives typed obarray. |
| 3896 | prepare_entry_for_test | optional shared Context call temporary | B/C fail: typed shared install/validation only; no reference result/later owner use. |
| 3949 | prepare_upgrade_for_test | optional shared Context call temporary | B/C fail: prior raw probe/wait already finished; typed shared drain tail call returns no reference. |

The Obarray expressions include the implicit shared `ObarrayView` receiver and returned shared manager loan in the same source row, rather than pretending the field is a Box or a raw pointer. `Context.obarray` is **ObarrayView** (eval/mod.rs3373); its Deref uses `NonNull<Obarray>::as_ref` under the view's stable lease in world.rs391–399. At3870 the map closure's ctx parameter is already the current shared Context produced by the inventoried raw as_ref. Its field reborrow is not a separate older raw Context formation. There is no direct NonNull conversion in cache.rs itself.

## Callee/callback proof checked

- **Compile environment:** `compile/inline_vars.rs:141–169` takes `*const Context`, forms a shared reference, reads threading mode/obarray/projection, and stores metadata pointers in thread-local CompileEnvScope. It does not mutate Context or enter Lisp/native code. These observations discharge B for shared metadata borrows spanning that raw call; they do not certify that helper's public safety/lifetime API.
- **Pending installation:** `cache::{install_pending,probe_upgrade,drain_ready_in,drain_ready_pending,probe_osr_pending,adopt_pending_entry}` pass only `Option<&Context>`/`&Context` to `bg::install`. `bg.rs:743–884` reads Context epoch/binding/override metadata; writes job/leaf/cache/runtime state. It has no older raw Context parameter/capture and documents no Lisp allocation/safepoint.
- **Sampled stack probe:** `eval/native_stack.rs:115–191` takes `owner: &mut T` and `f: impl FnOnce(&mut T)`. It ends field-probe borrows, calls f(owner), and restores the limit through owner after a growing callback/unwind. Thus cache.rs3444's ctx parameter is **&mut Context**, not the outer raw ctx. The cast is already current-owner derivation. Replacing it with `std::ptr::from_mut(ctx)` would be spelling-only, not a repair; no such change is proposed.
- **Leaf arity:** `compile/leaf.rs:1200–1204` accepts only reads required/arity/has_rest. No raw writer or returned reference can overlap its implicit shared receiver.
- **Typed resume/signal tails:** cache passes the current typed mutable owner, never a retained copy of its incoming raw pointer. Callee-internal ownership remains a separate audit; the cache-local chain has no B through an older ctx capture.

## Completeness and exclusions

`lexical-candidates.json` records all broad `&*`/`&mut *`, as_ref/as_mut and manual implicit raw-receiver candidates. Every unsafe expression and every raw dereference in this file was reviewed, including closures, call temporaries, cfg(test) helpers and the debug-only assertion. The implicit len/accepts receivers are included even though a search for literal `&*` alone misses them.

`exclusions.tsv` records **15 typed as_ref/as_mut calls**, the typed &*obs dereference, the plain copied depth read, and the static typed-address creation. Types were checked at their declarations:

- DenseCache slots are Vec<Option<CacheEntry>>; its as_mut at248 is Option::as_mut.
- tier1_fallback is RefCell<Option<Rc<CompiledLeaf>>>.
- front_diag::begin returns Option<Box<Attempt>>.
- ByteCodeFunction.constants is **LispValueVec**, not a raw pointer or assumed Arc.
- fused is Option<Rc<FusedBody>>; DeoptResume.inlined is Option<Box<InlinedResume>>; OSR_CACHE entry extraction uses Option<OsrEntry>.
- cache.rs3290's leaf comes from typed CacheEntry::live_leaf -> &Rc<CompiledLeaf>, and its obs is **Box<LeafObs>**. This is not a raw-pointer owner formation.
- cache.rs3418 copies usize depth directly; cache.rs434 makes a raw address from an existing typed static reference. Neither creates a reference owner from a raw pointer.

There are no direct raw as_mut, NonNull reference operations, slice::from_raw_parts[_mut], Box/Rc::from_raw, transmute-based owner formation, or pointer read/write owner constructors in this source. `Rc::as_ptr`/slot words/ptr::eq are raw identity creation/comparison, not raw-to-owner conversion. Pure typed Context parameters were not counted as raw formations.

## Proposal and checks

- `cache.proposed.rs`: whole source proposal.
- `owner-chain-comments.patch`: comments only at the 25 inventoried expressions. Every comment names B or C and the concrete failing reason. Existing incoming-pointer/lifetime SAFETY contracts remain in place.
- `repairs.tsv`: header only, intentionally zero behavioral repairs. No all-three witness was established; inventing a from_mut edit would misstate this result.
- `prepare_audit.py`, `verification.json`: reproducible source inventory/proposal checks.

`rustfmt --emit stdout --edition 2024 --config skip_children=true` parsed the proposal while discarding stdout. Removing comments leaves an identical **22,447-token** stream in original and proposal. No source was formatted in place. No cargo build, nextest test, mutation run, commit, lane mail, or live source edit occurred. This is source investigation, not a gate PASS or a completion claim for D1's other dispatch-chain members.
