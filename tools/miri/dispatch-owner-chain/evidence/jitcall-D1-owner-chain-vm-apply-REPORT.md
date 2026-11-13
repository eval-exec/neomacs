# D1 owner-chain cross-check: vm.rs / apply.rs

**Confirmed: two raw Context/subowner scopes in vm.rs, no additional raw Context formation in either file, and no all-three repair witness.** The proposal adds comments only at vm.rs8623 and8729. apply.rs remains byte-for-byte unchanged. No symmetry repair, pointer-spelling churn, declaration change or behavior change is proposed.

Pinned base: **06bef6ee903a84307b59bd636ac14ba6fc626d41**. Original source copies were extracted with git show and independently matched these committed blobs:

| Source | Blob |
| --- | --- |
| crates/neovm-eval/src/emacs_core/runtime/bytecode/vm.rs | 6ed2028ef0a9fa0626a4b9cf6384df89e78badc7 |
| crates/neovm-eval/src/emacs_core/runtime/eval/apply.rs | 1dddc7b12d35efbcd60cfc17feea3cbafafa0ec8 |

## The two Context/subowner scopes

A means a reference owner is formed from raw; B means an overlapping write through an **older ancestor raw pointer** while that owner remains usable; C means subsequent use of that owner. Writes through a pointer freshly derived from the current owner are descendants, not B. Writes to another allocation do not invalidate this owner. A copied scalar is not an owner. Separate lifetime/GC/code-origin contracts are not replaced by this chain test.

| Source site | Actual type and owner | B/C evidence | Result |
| --- | --- | --- | --- |
| vm.rs8623, nested marshal_and_run | `heap: &mut WorldView<TaggedHeap>` from `&mut (*ctx_ptr).tagged_heap`; alloc_cons reborrows its heap through that current view | Last heap use is alloc_cons8631 in the rest loop. Only copied rest bits are stored at8633. The older ctx_ptr reaches native entry8635 and fallback8643 after that owner last use; heap is never used again. | **C fails.** No owner survives/reappears across that callback. |
| vm.rs8729, interp_fallback_native | `&mut Context` is passed to Vm::from_context and stored in Vm.ctx | Vm.ctx is explicitly `&'a mut Context` at2447, and from_context takes that same typed reference at2804. After8729 this function never uses or forwards ctx_ptr or captures it in a closure. Only vm.execute_with_func_value8731 uses the current typed owner. | **B fails.** No older ancestor ctx_ptr write occurs after owner formation. |

`Context.tagged_heap` is explicitly **WorldView<TaggedHeap>** in eval/mod.rs3371. The first scope is a field/view owner, not a bare TaggedHeap Box or an invented whole-Context reference. The Vm.ctx field declaration is at2447 in this pinned source.

`owner-chain-comments.patch` states the failed condition immediately at each formation. It contains only those two comment additions. `vm.proposed.rs` is the whole proposal; `apply.proposed.rs` equals its original.

## Typed Context facts and false raw-formation matches

apply.rs1493 is `let ctx = &mut *self` in `Context::record_sequence_call_roots(&mut self,...)`. **A fails:** self is already a typed mutable Context reference. Neither the spelling `*self` nor the new binding creates a raw Context owner. No comment/edit is proposed there.

Every `&mut *self.ctx` / `&*self.ctx` in vm.rs is likewise a typed reborrow of the declared `&'a mut Context` field. This includes the HookRuntime methods, interpreter builtin arms, binding functions, keyboard-macro helpers, backtrace observations and owner-to-raw derivation at4593. They are not a family of raw Context formations merely because they use `*`.

The ctx parameters at vm.rs8153 and8649 are explicitly `&mut Context` in their enclosing function signatures. `core::ptr::from_mut(&mut *ctx)` derives a raw pointer from that current typed owner. It does not construct another owner from raw and requires no symmetry rewrite here.

apply.rs2667,2767,3056,3225 and3313 cast typed `self: &mut Context` to raw for cache dispatch. They have no intermediate raw-to-Context reference formation in this file. Pure typed Context parameters were not counted as raw formation sites. This does not replace the separate caller/callee owner-chain audit at a downstream formation.

vm.rs1545 and1587 dereference **ManuallyDrop<InterpreterFrameAux>**, declared at1428. They are typed auxiliary-state reborrows, not raw pointers or Context aliases.

## All non-Context formations

The exhaustive source table is `audit.tsv` / `audit.json`: **25 VM entries** comprising 24 direct raw-reference constructors and one additional conservative helper-mediated unchecked code loan. Two are the Context/subowner sites above; **23 are non-Context**. apply.rs has zero raw-reference constructors in this source-level inventory. Every entry records function, expression, target owner, live extent, A/B/C and concrete failed condition.

| VM source line(s) | Owner / construction | Scope and chain result |
| --- | --- | --- |
| 872,880 | shared/mutable operand `[Value]` slice from StackCursor.base | Constructor templates returned under typed cursor loans. No ancestor base writer inside either constructor. StackCursor is non-Copy, and publication consumes it before callbacks/Vec growth. Driver scalar reads end before writes; live Car/Cdr top refs write through their current slice owner and end before error pop/publish. |
| 1085 | &ByteCodeFunction via InterpreterFunction NonNull::as_ref | Frame code loan; no older function pointer writes in this path. Published-code immutability and frame/root lifetime are separate existing contracts. Context writes target another allocation. |
| 1139 | &[Op] via ActiveCodeView NonNull::as_ref | Frame instruction view. ops_ptr is read only; there is no instruction mutation through it. |
| 2051,2074 | &SubrObj from validated veclike pointer | Metadata guards / copied dispatch tuple only. No callback or metadata writer in the reference's extent; no reference escapes to the selected builtin. |
| 3680,5133 | parenthesized StringObj.header raw-field reference | Validation block;5133 is the stk_push! macro definition. Header last use precedes argument/frame work or ensure_stack_push_capacity/callback. No Context cast. |
| 4092 | mutable `[Value]` slice over cursor.base | Passed only to install_exact_callee1662, which stores through the current slice and returns (). Later raw cursor writes/Vec growth occur after that loan's last use. |
| 5330,5360,5393,5394,5395,5476 | &Op via raw instruction fetch/lookahead | Current opcode match / fusion lookahead. No older raw opcode writer; operand and Context writes are to different allocations. These rows rely on the existing immutable sealed-stream contract for origin/bounds/lifetime, not a new provenance claim. |
| 7261,7262 | buffer-slots/defaults `[Value]` slices from raw fat pointers | Passed only to find_symbol_value_in_buffer; the BLV work mutates the separate Obarray manager, not buffer storage. Slice options have no later use after the call. |
| 8170,8220,8321,8416 | shared CompiledLeaf from spec/cache pointer | Shape/arming or typed native runner scope. Old raw leaf is not used for an overlapping mutable leaf write. Context writes target another object; permitted Cell state is not an older mutable leaf borrow. Cache lifetime/slot validity remain separate obligations. |
| 8211 | unchecked ByteCodeObj.data helper loan | `bytecode_data_materialized_by_caller` explicitly returns &'static ByteCodeFunction. value_api.rs188–202 shows its ByteCodeObj raw-field borrow. It does **not** cast to Context. Type/materialization/lifetime remain the unsafe caller contract; no older code-object writer appears here. |
| 8535 | immutable native-argument `[Value]` slice | Local frame-copy arm only. push_backtrace_frame copies the words and returns no argument reference. The slice has no later use when depth/native callbacks begin. |

The buffer-slot pointers are concretely *const [Value], Subr pointers are *const SubrObj, instruction pointers are *const Op / NonNull<[Op]>, and leaf pointers are *const CompiledLeaf. They were not classified by variable names such as ptr or owner.

## Exhaustiveness method and exclusions

This uses exact source plus a lexical mask and manually checked formal/field/return types, **not a claimed Rust AST or MIR analysis**. All unsafe expressions, explicit and parenthesized borrow/dereference forms, raw slices, NonNull references, raw-receiver methods, casts, from_raw/from_mut/from_ref directions and unchecked owner-returning helpers were inspected. Macro formation sites are recorded at their source definitions (e.g. stk_push!), rather than inventing compiler expansion counts. Ordinary standard-library container internals and checked Value projections outside these files are not recounted as new raw Context formations.

`exclusions.tsv` / `exclusions.json` preserve **98 classified candidate rows across96 source locations**;8153/8649 appear in both typed-reborrow and reverse-cast categories. This includes all typed get_unchecked/get_unchecked_mut calls. Their typed Vec/slice/StackCursor receivers do not reinterpret a raw Context: StackCursor's actual raw-slice constructors are separately inventoried at872/880, copied Value/scalar reads end temporary loans, and mutable assignments use the current element owner.

The eight excluded as_ref calls are:

- vm.rs640: Option<LispByteVec> gnu_bytecode_bytes (chunk.rs231).
- vm.rs3140/3160: generic `A: AsRef<[Value]>` arguments, explicit in the function bounds.
- vm.rs9893/9949: selected-resume Options.
- apply.rs1855: Result::as_ref;2488/2527: selected-resume Options.

The actual NonNull::as_ref operations at vm1085/1139 are included. There is no direct raw as_mut in either file. NonNull::new_unchecked + ptr::slice_from_raw_parts_mut at1113–1116 creates only an empty raw slice pointer; NonNull::from at1076/1129 goes from an existing typed code/slice owner to raw. Neither is itself a reference loan.

apply's native-argument reads and detach/promote loops read i64 words into copied Values; std::ptr::copy_nonoverlapping / pointer writes initialize storage; Vec::set_len updates its typed container. None silently constructs a Rust Context reference. SymId::from_raw creates a numeric symbol ID, and Value::from_bits creates a tagged word, not a Context owner. No transmute or helper returning a Context from a raw/tagged input was found. The unchecked code helper at8211 was inspected and separately inventoried rather than omitted.

## Verification and limits

Only source/proposal checks were run. Both proposal files parsed with `rustfmt --emit stdout --edition 2024 --config skip_children=true`, stdout discarded; no in-place formatting. Removing comments leaves identical original/proposal token streams: **57,800 tokens for vm.rs and 25,574 for apply.rs**. No assertion, control path, cfg, kind, API declaration, pointer derivation or product behavior changed.

No live source edit, build, nextest test, Miri run, commit or lane mail occurred. Parent-reported Miri controls were not executed here and are not presented as this audit's evidence. No gate PASS or general certification of raw execution/public API safety is claimed. The source result supports the parent's two precise failed-condition comments and rejects speculative symmetry repairs.
