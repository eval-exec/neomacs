//! Tiered execution subsystem for the Emacs-Lisp VM — the foundation of the
//! modern JIT path. See `bytecode/ELISP_VM_MODERNIZATION.md` for the full
//! design + phased roadmap.
//!
//! Gated behind the `jit` cargo feature, which is **default-ON** (`default =
//! ["jit"]`): both the baseline (Tier-1) Cranelift JIT and the optimizing
//! typed-MIR Tier-2 above it are qualified and shipping. The bytecode
//! interpreter (`bytecode::Vm`) is always the **Tier 0** engine — the
//! correctness oracle that mirrors GNU Emacs 31.0.90 and the deoptimization
//! landing pad. It is never removed.
//!
//! Design rule (carried over from the GC work): every dispatch over an
//! execution tier is an **exhaustive `match`** with no catch-all arm, so adding
//! a tier fails to compile until every site handles it. That is the same
//! compiler-enforced completeness that caught the GC `trace_veclike`
//! use-after-free (an incomplete duplicate with a `_ => {}` arm).
//!
//! # Environment-knob inventory (the authoritative list)
//!
//! Every runtime toggle this subsystem reads, with default and status. Grep
//! anchor: `env::var("NEOVM_`. Keep this table in sync when adding a knob, and
//! give every OPT-IN knob a graduation plan — soak → default-on, or delete —
//! so the surface doesn't accumulate permanently-dead branches.
//!
//! ## Runtime switches (shipping, default-on)
//! | Knob | Default | Meaning |
//! |---|---|---|
//! | `NEOVM_JIT` | on | Kill switch: `0`/`off`/`false`/`no` forces the pure interpreter (the A/B baseline). |
//! | `NEOVM_JIT_THRESHOLD` | 1000 | Tier-up heat threshold ([`Runtime::HOT_THRESHOLD`]); `=1` compiles every compilable function — the differential soak and the strictest oracle configuration. |
//! | `NEOVM_JIT_LOOP_HEAT` | 8 | Heat credited per 256-iteration back-edge wrap (32 iterations ≈ one call; a hot loop tiers up near 32k iterations); `=0` disables loop heat — the pre-loop-heat baseline. |
//! | `NEOVM_JIT_LEVER1` | on | Residual-rooting non-heap skip; `=off` reverts to an unconditional gc_push per residual (single-build A/B). |
//! | `NEOVM_JIT_OSR` | on | Mid-loop interpreter→native transfer (on-stack replacement); `=off` disables. |
//! | `NEOVM_JIT_PROFIT` | on | Profitability gate (calls ≤ arith); `=off` also compiles call-heavy bodies. |
//! | `NEOVM_JIT_INLINE` | on | Bytecode fuser: splice a constant-bytecode callee into its caller before lowering (`jit/inline.rs`); `=off` disables. |
//! | `NEOVM_JIT_REOPT` | on | Deopt-driven reoptimization (`jit/reopt.rs`): a conclusive or repeated deopt widens the site's feedback, retires the stale leaf and recompiles after a re-profile window, with a bounded backoff ladder. `=off`/`0`/`false`/`no`: deopts are still counted (the census) but never widen feedback or invalidate — the pre-reopt behaviour and the single-build A/B arm. |
//! | `NEOVM_JIT_REOPT_HEAT` | `NEOVM_JIT_THRESHOLD` | Re-profile window: interpreted calls between an invalidation and the recompile. |
//! | `NEOVM_JIT_REOPT_MAX` | 4 | Invalidations of one source before each further one climbs a `ReoptLevel` (`Speculative` → `NoInline` → `BaselineOnly` → `Generic` → `Interpreter`). |
//! | `NEOVM_JIT_REOPT_SITE_LIMIT` | 4 | Non-conclusive deopts (overflow, call-site guard, rerun, OSR entry) counted at one pc of one leaf before the forced response. |
//! | `NEOVM_JIT_ISA_CACHE` | on | Build the Cranelift ISA once per register allocator per process (`lowering::jit_isa`) instead of once per compile (host-CPU probe + by-name setting resolution); `=off` rebuilds per compile (single-build A/B; the code is identical). |
//! | `NEOVM_JIT_LAZY_SHIMS` | on | A leaf imports a runtime shim into its CLIF function on first use (`compile::shim_refs`), so it carries only the signatures it calls; `=off` imports the whole declared set up front in the old order (CLIF-identical to before; the machine code is the same either way). Single-build A/B. |
//! | `NEOVM_JIT_PERSISTENT_MODULE` | on | Leaves compile into one long-lived `JITModule` per register allocator per thread (`compile::shared`): shims registered and declared once, Cranelift contexts reused, entries anonymous (or uniquely named under per-function naming), the module replaced every 1024 leaves to bound its bookkeeping; on x86-64 Linux the code goes into a per-thread arena (`compile::code_arena`: pre-reserved pages, one mprotect per leaf, never unmapped). `=off` builds a module per leaf on Cranelift's default memory, as before (single-build A/B; identical machine code). Counters: `[neovm-jit-final-code-memory]`. |
//! | `NEOVM_JIT_MIR_OPAQUE` | on | The MIR tier lowers shim-using ops (variable ops, builtins, `eq`, list ops) through the baseline's emitters; `=0`/`off` makes every such op bail the body to the baseline — an A/B of the adapter alone: the tier gate (`gate:loop-opaque`/`generic-call`/`inline-opaque`) applies either way, so it is not the pre-adapter gate. |
//! | `NEOVM_VAR_CACHE` | on | P1.4 Stage A cached variable tiers (`eval/var_fast.rs`): the read, `setq`, `let` and unbind of a buffer-local or forwarded variable answered from its BLV cache or forwarder, used by the JIT var shims, the interpreter's `varset`/`varbind`/`unbind` and the tree walker's `let`; `=0`/`off`/`none` disables all four, a comma list of `read`,`set`,`bind`,`unbind` enables those (single-build A/B). |
//! | `NEOVM_BUILTIN_FRONTEND` | on | U2.8 builtin front-end diets (`eval/builtin_vars.rs`): the search, syntax and text-property builtins read their control variables through the typed reader, buffer regexp searches / `looking-at` / `string-match` parse and compile once and skip the syntax-property resolver when no Lisp can run, match data is published from the engine registers in place, `skip-syntax-*` parse their class string in place, literal searches borrow the pattern, `parse-partial-sexp` conses its state from the stack; `=0`/`off`/`none` restores every general path (single-build A/B). |
//! | `NEOVM_PPS_PROPERTIZE` | on | U0.7 parity fix (`syntax/pps_propertize.rs`): `parse-partial-sexp` runs `syntax-propertize` where GNU's `scan_sexps_forward` does (entering text `syntax-propertize--done` does not cover, with `parse-sexp-lookup-properties` non-nil); `=0`/`off` never propertizes, for attribution only. |
//!
//! | `NEOVM_SYNTAX_PARSE_CACHE` | on | P3.4 L1 syntax parse cache (`syntax/parse_cache.rs`): `parse-partial-sexp` resumes from recorded loop states of earlier scans with the same FROM, state, options and environment, or answers a repeated TO outright; `=0`/`off` disables; `=verify` recomputes every cached answer with a plain scan, counts and logs mismatches (and fails debug builds). `NEOVM_SYNTAX_PARSE_CACHE_CHUNK` (2048) spaces the recorded states, `NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN` (128) is the shortest scan that starts a run, `NEOVM_SYNTAX_PARSE_CACHE_STATS=PATH` rewrites the counters to PATH every 256 queries. |
//!
//! ## Opt-in features (default-OFF, pending a graduation decision)
//! | Knob | Enable | Meaning / graduation blocker |
//! |---|---|---|
//! | `NEOVM_JIT_OPT` | `legacy` (opt backend off); `off`, `opt` | Backend selector independent of `NEOVM_JIT_TIER2=off\|on`: legacy keeps the existing MIR/baseline front; off selects baseline only; with PROFIT=off, opt selects baseline at T1 and global SSA at feedback T2 upgrades and OSR. Feedback entry upgrades require TIER2=on; PROFIT-selected existing Retier upgrades and EARLY-selected T1 loops can select SSA with TIER2=off (early preserves successful legacy MIR). A final opt feedback plan honors an explicit full-allocator upgrade request when one verified join carries at least two phi values; T1, single-value joins, refused-plan fallback and forced allocator choices keep their existing policy. OSR selects opt directly. Graduation requires F-M2, later pass gates, compile census and GNU soak. With opt selected and PROFIT=off, stable unbanned baseline helpers of 1–64 ops are also worth a T2 rebuild; allocator, dynamic-target and legacy admissions keep main policy. Stable opt helpers retain T1 after temporary CPU budget denial and retry after another work window; actual reservations remain at main’s compile seam. Opt resource waits close Use recording and HOF credit while retaining T1; affordable retries reopen a full fresh stable sampling window. |
//! | `NEOVM_JIT_OPT_PROFILE` | `off` (default); `lists20`, `lists32`, `lists48`, `lists64`, `lists48-osr` | Optional single-variable compile configuration preset: supplies OPT=opt, ADMIT=all, PASSES=fold,bool,reps, PROFIT=lists, EARLY=hot, FAST=on and MAX_OPS=20/32/48/64 only when each individual variable is absent. `lists48-osr` instead supplies PROFIT=primitive-lists with MAX_OPS=48 and REQUIRE_OSR=on; the older presets keep PROFIT=lists and REQUIRE_OSR off. Explicit individual values replace preset defaults verbatim, including empty/invalid values and non-Unicode entries using their original parser fallback; explicit OPT=legacy remains the unchanged MIR/baseline escape. Off/invalid preserves today’s defaults and parser contracts. The surrounding Tier2/Inline2/Feedback/SpecSources/BG knobs are unaffected. Does not enable native counters, naming or reports. Presets add one process-environment variable instead of seven; subprocess environment work is part of the measured workload, so preset qualification still uses actual unset defaults. |
//! | `NEOVM_JIT_OPT_REQUIRE_OSR` | off; `on` | Optional compile-time prerequisite for PROFIT-selected normal entries: require a ready, installed Opt OSR leaf at a qualifying original backedge of the same source in this mutator's cache before SSA selection. Lists/primitive-lists require a list operation within that backedge's lexical span. Negative/pending/baseline entries, other sources/headers, stale heap/obarray, reopt level/prefix or missing actual Opt ownership decline to the original MIR/static-fuser frontend before SSA; useful MIR is retained. OSR selection is unchanged and supplies the evidence. Reads existing cache/code ownership only, with no new runtime state, feedback, native counters or naming. No new retier is scheduled: a Full entry compiled before OSR may remain baseline. A ready leaf proves construction at a hot backedge, not that snapshot transfer executed. Off, PROFIT=off and OPT=legacy/off preserve the prior frontend and CLIF. |
//! | `NEOVM_JIT_OPT_EARLY` | off; `hot`, `on` | With OPT=opt and PROFIT=loops/lists/primitive-lists/kernels, qualifying T1 loops try today's legacy MIR first, retaining it whenever it succeeds. SSA runs only after MIR declines. Lists/primitive-lists retain their list-loop filters here; kernels uses the original loops filter at T1. Hot requires existing source is_hot evidence; on additionally admits a cold FirstSight loop. Neither admits a cold Dispatch, straight-line helper or recursion-only body. MAX_OPS optionally bounds all selected candidates; the opt constructor still refuses more than 1000 ops. Existing source heat and origin are logged at compile time; no recording or native-entry check is added. Early compiles use the old MIR/static-fuser frontend, ignoring INLINE2; fused-v2 SSA remains unsupported. Off preserves the profit policy's original MIR T1 path; legacy/off ignores this knob. |
//! | `NEOVM_JIT_OPT_MAX_OPS` | 0 (disabled); positive integer, e.g. 64 | Optional compile-time original-source and final-emission op bound for PROFIT-selected early/upgrade/OSR bodies. Larger original sources retain legacy entry or baseline OSR emission without an SSA attempt. Selected entry slices enlarged beyond the bound by fusion clear opt request/parameters and use the existing baseline emitter once, without repeating MIR/fusion. Successful early MIR still returns directly. The opt builder's 1000-op constructor ceiling remains. With PROFIT=off or OPT=legacy/off it has no effect. |
//! | `NEOVM_JIT_OPT_PROFIT` | off; `loops`, `lists`, `primitive-lists`, `kernels` | Compile-time profitable SSA selection with OPT=opt. With EARLY=off, all modes preserve legacy MIR and late fusion at T1, for rejected bodies and on SSA refusal; those compiles ignore an explicit INLINE2 selection and use the original static fuser. Loops admits non-call-heavy loops with numeric/list/predicate work; lists additionally requires Car/Cdr/CarSafe/CdrSafe/Setcar/Setcdr inside a backward edge's lexical span, rejecting numeric-only, predicate-only, Cons-only and straight-line bodies without CFG construction. Primitive-lists retains the same list/backedge, size, unsupported-control and j17 constraints, additionally rejecting Aset and every explicit bytecode call form (Call, Apply, CallBuiltin, CallBuiltinSym), including dead occurrences or calls the fuser could inline. Aset is excluded while its advised-function fallback can return an arbitrary value and invalidate the shared emitter's retained-operand alias assumption; this policy does not alter the effect table or the GNU opcode alignment tracked by lead-gnuop. OSR checks the original source as well as its final emission slice so fusion cannot erase call evidence; dedicated primitive-op helper lowerings remain allowed. This classifies bytecode and does not promise a machine leaf. Kernels retains loops and additionally admits hot upgraded helpers of at most 64 ops with at least two useful operations. Sources containing PushConditionCase/Raw, PushCatch, PopHandler or Throw are conservatively rejected before SSA, including dead occurrences, without CFG construction; UnwindProtectPop keeps its existing builder policy. Uses the existing j17 call-heavy classifier without changing its arithmetic/call ratio; no new recording or runtime instrumentation. Selected existing Retier upgrades and OSR work with the surrounding opt-in stack off. Existing reopt bans still prevent SSA emission. Compact-helper blanket T2 admission is retained only with PROFIT=off. Legacy/off ignores this policy. Fused-v2 frame chains remain unsupported; selected entry refusal returns to the old frontend. OSR retains its existing baseline fallback. |
//! | `NEOVM_JIT_OPT_ADMIT` | empty; `args,env,vars,binds,switch,handlers`, `all` | Compile-time admissions for the opt front; each defaults off. Handlers/Throw remain a counted builder refusal until exceptional SSA is implemented. |
//! | `NEOVM_JIT_OPT_PASSES` | none; comma list `fold,bool,reps,gvn,range,licm,sink`; `all,-NAME` | Independent compile-time opt pass selection. All passes default off. Fold enables conservative type propagation, guard folding and jump threading; bool adds normalized predicate flags, closed Boolean phis, compare-to-branch fusion and cold T/NIL views; reps exposes checked fixnum arithmetic, selects TaggedFix/RawInt webs and places cold recovery blocks last. gvn adds dominance-scoped pure reuse, proven cons-load elimination and retained-setter forwarding, with mutable facts killed at calls, polls, reentry and control-flow joins. range proves bounded fixnum intervals and current guarded vector/record bounds; licm moves total invariant scalar work, immutable payload reads and anticipated loop-entry guards. sink carries unboxed numeric SSA tuples and virtual cons fields, materializes at escapes and cold recovery, and guards selected positive finite sqrt calls. Legacy/off emission ignores the selection. |
//! | `NEOVM_JIT_OPT_FAST` | `off` (default), `on` | Elide verified empty Bool/LICM/Sink work and discover Sink before cloning; selected native transformations and independent verification remain unchanged. |
//! | `NEOVM_JIT_OPT_REPORT_FILE` | off (absent); file path | Independent compile-only process evidence for actual opt construction attempts/refusals/successes at the universal normal/selected/OSR lowering seam. A process mutex holds aggregate counters and the first 128 scalar attempt rows; returned leaves remain counted even when later retired/cancelled. The always-called exit seam writes a sealed record directly to this file, never stdout. It does not enable STATS/PROFILE entry counters, naming, feedback, heat or extra IR construction. Known request origins and final op lengths are recorded without source naming/identity lookups. Completeness requires no active attempts/local pending jobs, no poisoned mutex/late events and synchronous backends; threaded backends are conservatively incomplete. Any compile after sealing appends an invalidation marker. Failure to persist that late invalidation aborts this explicitly enabled diagnostic process; qualification requires clean owned-process exit. Initial report failure is missing/partial evidence. Same-binary measurements must enable it in both arms and crosscheck a no-report pilot: ON costs two cold mutex operations per attempt plus bounded exit file output; OFF/legacy generated CLIF is unchanged. |
//! | `NEOVM_JIT_EXIT_FILE` | off (absent); file path | Exit-only admission and compiler-time evidence independent of STATS, PROFILE and OPT_REPORT_FILE. Scalar atomics count actual normal/OSR opt construction, including refused and deferred attempts; no rows, naming, cache walk, entry counters or additional phase clocks. Exit reads existing P0.1 origin totals (including OSR) from the reporting mutator; timing is not a process-wide sum for concurrent mutators. Process-wide construction counts use a conservative seal and append invalidation for any later work. Completeness requires a synchronous backend, no active/local pending work and conserved counters; file IO failure aborts this explicitly enabled diagnostic process. Use the same knob in both measurement arms, with clean owned-process exit and no late record. |
//! | `NEOVM_JIT_OPT_BUILD_CENSUS` | off; `PATH` | Append per-build verdict, ops, blocks, insts, phis, frames, full-stack baseline params, split edges, refinements, frame uses and dead leaders. Captures all attempted bodies without selecting opt code. |
//! | `NEOVM_JIT_DUMP_OPT` | off; `PATH` | Append verified IR text for opt builds (or the build census), with explicit SSA, effects, frames and source positions. |
//! | `NEOVM_JIT_INLINE2` | off; `=named`, `=closure`, `=hof`, `=all` | P2.3's front runs before the backend choice. `named`/`all` admit direct named cells on the existing re-tier path (or self-recursive Full compile), with virtual frames, per-cell identity, attention and depth guards. Named bodies remain observation-free under F-I1 rule 2. `closure`/`all` add in-unit make-closure calls; `hof`/`all` add list-only mapc/mapcar intrinsics, including OSR chains. Legacy MIR refuses fused-v2 bodies. Read at compile time; off preserves the original CLIF. |
//! | `NEOVM_JIT_INLINE2_AT_FIRST_COMPILE` | off; `=1` | Oracle diagnostic: admit named selection at the first compile to exercise THRESHOLD=1 pins. Production named admission otherwise requires the existing re-tier path or self-recursive Full compile. Read once; no effect with INLINE2=off. |
//! | `NEOVM_SYNTAX_PARSE_CACHE_L2` | `=1`/`on`, `=verify` | P3.4 S5–S6 canonical syntax run (`syntax/parse_canon.rs`), with L1 enabled: option-free queries reuse BEGV states by FROM agreement or later full-state synchronization; warm backward-comment scans reuse validated canonical states. `verify` recomputes complete answers and point. Large-file, SMIE and full-suite verify gates pass; S7 graduation awaits the design's quiet-machine run and week of verify-mode oracle CI. |
//! | `NEOVM_JIT_INLINE_ARITH` | `=on` | Level-B native bit-ops (logand/logior/logxor/lognot) with fixnum-guard deopt. Blocker: skips the compiler-macro bounce; a mixed-type loop falls back to the interpreter ungracefully. |
//! | `NEOVM_JIT_INLINE_TYPE_OF` | on | Answer a record's `type-of`/`cl-type-of` inline at an armed JIT site; `=off` calls `neovm_jit_pred_spec` everywhere (single-build A/B). |
//! | `NEOVM_JIT_INLINE_AREF` | on | Inline slot reads at JIT `aref` sites on plain vectors and records; `=off` calls `neovm_jit_aref` everywhere (single-build A/B). |
//! | `NEOVM_JIT_AREF_SLOT0` | off | MEASUREMENT ONLY (P3.2 L0.9 / U2.10): re-emit the retired slot-0 tagged-vector test in inline `aref`/`aset` (no tagged vectors exist since P3.2 L0.8; a vector whose slot 0 is a tag symbol goes to the shim, which answers the same slot). Same-binary A/B of what the test cost. |
//! | `NEOVM_JIT_INLINE_ALLOC` | on | Allocate conses (`Op::Cons`, escaping `MirOp::Cons`) and box floats (float sites under `NEOVM_JIT_FLONUM=off`, flonum escape boxes) inline by bumping the heap's cons/float allocation region (`compile::heap_inline`); `=off` calls `neovm_jit_cons`/`neovm_jit_make_float` everywhere (single-build A/B). |
//! | `NEOVM_JIT_INLINE_HEAP_WRITE` | on | Inline stores at JIT `setcar`/`setcdr` sites when the cons lies outside the write barrier's owner window, and at JIT `aset` sites on owned plain vectors and records whose owner the barrier need not see (`compile::heap_inline`); `=off` calls `neovm_jit_setcar`/`_setcdr`/`_aset` everywhere (single-build A/B). |
//! | `NEOVM_JIT_GEN0_COLLECTION_JOURNAL` | on; `=off`, `=eager` | Default GEN0 stores journal collection-observed owners. The existing window excludes a proven empty local interval after a cold refusal; permanent atomic bitmaps make exact cons queries lock-free, and remembered BLV proofs test their exact bit. GC narrows local bounds and invalidates recent-read shortcuts before reuse. Vectors/records retain exact checks; strings fold the observed mirror into the owned-capacity guard. `eager` journals every native store; `off` preserves prior CLIF. GEN1 retains its existing barriers and journaling. Configuration is immutable; certificates remain mutator-local. |
//! | `NEOVM_JIT_FLONUM` | `resident` | Unboxed float results at `Float`-feedback arithmetic sites (`compile::FlonumMode`): `off` boxes every result at its site (the prior lowering, CLIF-identical); `local` keeps a result unboxed while float arithmetic/compares, stack shuffles and variable reads consume it; `resident` also keeps it across audited ops (calls, `aref`, `aset`, ...) that box only their own operands. Single-build A/B of all three. |
//! | `NEOVM_JIT_TAIL_UNROOTED` | on | A call in tail position (`Return` next, no handler frame active) roots none of the operand-stack slots below its callee, which are dead (`lowering::tail_call_dead_residuals`); `=off` roots them as before, CLIF-identical (single-build A/B). |
//! | `NEOVM_JIT_ARGS_FIRST` | on | A MIR leaf's entry reads its arguments before the hoisted root-window check, so the argument pointer is not live across the check's cold grow call; `=off` reads them after it, as before, CLIF-identical (single-build A/B). |
//! | `NEOVM_JIT_REG_ABI` | off | JIT leaf bodies a direct call can enter (not AOT, not OSR; frameless, not `make-closure`-patched, at most 6 required parameters and no `&optional`/`&rest`) take their arguments in registers and return `(value, status)` in `rax:rdx` (`compile::reg_abi`); native-to-native callers pick the entry by the leaf's `EntryShape` or the spec slot's key flags, both decided once, so a memory-ABI body pays no ABI test. With `DIRECT_CALL=off`, `=off` keeps every entry on the memory ABI `fn(vmctx, args, out, sidecar) -> status`, CLIF-identical to before (single-build A/B). Implied globally by `NEOVM_JIT_DIRECT_CALL=on` unless `NEOVM_JIT_DIRECT_MEMORY=on` or `NEOVM_JIT_DIRECT_SITES=self`; explicit `=on` always retains register bodies and shape marshaling. |
//! | `NEOVM_JIT_DIRECT_CALL` | on | A speculated call of a compiled, exact-arity, frameless byte-code leaf calls the leaf's selected raw entry from the site (attention, epoch, depth and specpdl checks, the lean backtrace push and pop inline; `lowering::emit_direct_bytecode_call`), with `neovm_jit_call_spec` as the slow path; spec slots arm a `direct_entry` (S2.1b), which the site keys on (S2.1c). `=off` arms and emits nothing, CLIF-identical to before (single-build A/B). Defaults to profitable exact self sites (`SITES=self`, `SELF_KERNEL=on`, `SELF_HEAT=off`). Other site policies imply the register ABI globally unless `NEOVM_JIT_DIRECT_MEMORY=on` or `NEOVM_JIT_DIRECT_SITES=self`. |
//! | `NEOVM_JIT_DIRECT_MEMORY` | off | With direct calls on and the register ABI off, exact pass-through frameless JIT callees retain their existing memory ABI and direct sites call `fn(vmctx, args, out, aux) -> status` through the caller's existing argument/result slots. Named and source/constant calls share the existing guards, frame/depth push/pop and cold finish; source calls use the executing object's constant base. Slots publish the raw memory entry last with Release and generated readers use atomic Acquire-or-stronger loads. Optional short calls and rest-list marshaling stay on the shim; explicit `NEOVM_JIT_REG_ABI=on` restores the existing register path and shape reach. No native signature or slot-layout change. Read at compile time and slot arming; `=off` preserves existing direct-call CLIF verbatim (single-build A/B). |
//! | `NEOVM_JIT_INLINE_VARS` | `read` (unset); `=all`, or a comma list of `read`, `set`, `bind` | Variable ops inline in JIT code (`compile::inline_vars`, design `p1-4-inline-binding-blv` Stage B), each the cache-hit prefix of a Rust tier with the unchanged shim as its slow path. `read`: `varref` of a plain cell from its baked address, of a buffer-local variable whose cache is loaded for the current buffer, and of a forwarder's own slot. `set`: `varset` of those three (not for a projected symbol; a buffer-local one only into the buffer's own binding or a non-`local_if_set` default). `bind`: `varbind` and `unbind` (up to 4 bindings) of those three, writing the specpdl entries from the probed templates. Every guard precedes every store, and a store takes the shim during a concurrent mark or owner tracking. Read at compile time; `read` is the default, `set` and `bind` remain opt-in; `=off` emits the former code exactly (single-build A/B). |
//! | `NEOVM_JIT_DIRECT_SITES` | `self` | With direct calls on, which callers emit direct sites (`compile::DirectSitesMode`): `unbounded` only a body that loops or calls itself, or a re-tier of one that proved hot -- a direct site costs its caller's compile about a hundred IR instructions and fourteen blocks more than a shim call, which a straight-line body entered a few thousand times never pays back (elb-bytecomp); `all` every body. `self` (default) emits only exact required-only named calls of the caller's own materialized source, with frameless unpatched bodies admitted by `SELF_KERNEL` and `SELF_HEAT`. It suppresses the implicit global register conversion: only a body with an actual surviving named self-call in its selected baseline/MIR site map gets register ABI, unless `REG_ABI=on` explicitly retains global reach. Self sites use the original register protocol independently of `DIRECT_MEMORY`; source/constant and optional/rest/framed self direct calls decline. Compiler source tokens are immutable scoped facts, never runtime Lisp caches; Release publication pairs with atomic Acquire-or-stronger self-site entry loads. Native signatures and slot layout stay unchanged. Read at compile time and slot arming. |
//! | `NEOVM_JIT_DIRECT_SELF_KERNEL` | on | Admits self sites and their implicit register ABI only when the already computed profitability classifier finds calls do not outnumber arithmetic; applies only with direct calls on and `SITES=self`, and composes with `SELF_HEAT`. Uses the existing classifier, including `GATE_RELAX` and intrinsic credits, with no extra scan or runtime recording. Call-heavy fallback bodies keep their shim; explicit `REG_ABI=on` retains global register entries. Off preserves the original self policy; direct calls off preserves the original CLIF. |
//! | `NEOVM_JIT_DIRECT_SHAPES` | off; `=all`/`=on`, or a comma list of `optional`, `rest`, `constant`, `framed` | With `NEOVM_JIT_DIRECT_CALL=on`, the call shapes beyond a named exact-arity call that direct sites take (P1.1 Stage 2, P1.0 S2.5; `compile::direct_call`). `optional` (2a): a named call of an `&optional` callee (the call fills or lacks its optional slots) passes nil in the registers of the missing ones, and `&optional` bodies take the register ABI. `rest` (2d): a named call of a `&rest` callee (at most 8 arguments) conses the rest list inline after the site's checks and passes it in the last register, and `&rest` bodies take the register ABI; the hit path also checks the slot's key says the leaf takes the call through a list. Both record the call's own arguments in the frame, as GNU's `Bcall` does, and their slow path `neovm_jit_direct_slow` runs `neovm_jit_call_spec` then arms the entry for the site's shape (`spec_slot::arm_shaped_direct_entry`). `constant` (2b): an `Op::Call` whose callee is a constant byte-code object the fuser left a call (a `cl-flet` local, a `lambda` literal) called with exactly its required parameters becomes a source site of the object's own source (`SpecCalleeKind::Constant`): a direct call through the source slot machinery (leaf-slot epoch, frame recording the object), `neovm_jit_call_source_spec` its slow path, the generic call its decline; a callee the lowering cannot prove constant is guarded by its bits. `framed` (2c): exact required-only JIT callees with bindings or handlers keep the memory ABI (named calls up to 8 arguments, source/object calls up to 6). The site pushes the original frame and increments depth inline, then enters `neovm_jit_direct_framed` through the gated lazy shim table; that contained trampoline owns every frame/depth cleanup and preserves boxed precise deoptimization. Slots publish a framed tag after leaf/key initialization with Release; generated framed readers use an atomic Acquire-or-stronger load. AOT sidecars and framed optional/rest marshaling stay on the reference path. Read at compile time and when a body's ABI is decided; unset/`off` is CLIF-identical (single-build A/B). |
//! | `NEOVM_JIT_CALL_CENSUS` | off | `=on`: a measurement mode. Every JIT `Op::Call`/`Op::Apply` site first calls `neovm_jit_call_census`, which counts the call by site kind (`named` spec site, `const` callee, other `value`, `apply` of a function) and callee shape (`exact_direct`, `exact`, `optional_direct`, `optional`, `rest_direct`, `rest`, `framed`, `unarmed`, `declined`, `other`) read from the slot or the callee's armed leaf (`compile::call_census`); the counts print as `call-census:` in `[neovm-jit-final-builtin-leaves]` under `NEOVM_JIT_COMPILE_STATS=1`. Named spec calls select `neovm_jit_call_spec_census`, whose exact accepted-fast counters print as `spec-shim-fast:armed=N,framed=N`; framed includes AOT sidecars. These counters decide Stage 2c, with direct calls off so the denominator covers all named fast entries. The earlier shape cells describe candidates before guards and count bindings/handlers as `framed`. Do not time runs with it on. Read at compile time; off is CLIF-identical and the ordinary spec shim has no runtime census branch. |
//! | `NEOVM_JIT_SWITCH_LOOP_POLICY` | off | Compile-time ops-only loop policy: `on` treats operandless Switch as a possible loop for existing regalloc, direct-site and heap/root-window hoisting policies. `off` preserves the original direct-edge policy and emitted CLIF. Exact baseline GC/quit polls always use resolved CFG edges, independently of this knob. |
//! | `NEOVM_JIT_INLINE_SWITCH` | on | Answer a `switch` jump table whose keys are immediates (nil, t, fixnums, symbols) or cons trees of them -- `pcase` symbol, fixnum and backquote patterns -- inline, behind the table's mutation epoch, with `neovm_jit_switch` as the slow path (`compile/switch_dispatch.rs`); `=off` calls the shim at every switch, CLIF-identical to the lowering before inline dispatch (single-build A/B). |
//! | `NEOVM_JIT_MIR_REACH` | `=dead` (or `all`) | MIR reach admissions (design `p2-5-mir-reach`; the only legacy-MIR bit is `dead`, the others belong to the opt tier). `dead`: the MIR builder skips a block leader no path reaches (the `Return` `seal_ops` appends after a named-let's or `cl-loop`'s final `goto`) instead of bailing the body as `mir-unreachable-block`. Tier-up compiles only (AOT and inline callees never use it). Read at compile time; unset/`off` bails exactly as before (single-build A/B). Census: `reach_dead=`/`dead_leaders=` in the `mir[...]` summary. Blocker: F-R1 (P2.5 §7): admitted bodies must be at parity with the baseline. |
//! | `NEOVM_JIT_EQ_PREFILTER` | on | Answer native `eq`/`symbolp` inline unless an operand is a veclike (a symbol-with-pos is one); `=off` calls `neovm_jit_eq_slow`/`neovm_jit_symbolp_slow` for every mismatch/non-symbol (single-build A/B). |
//! | `NEOVM_JIT_COLD_EXITS` | on (unset); `=off`, `=share` | Mark every exit block cold in both tiers (`compile::cold_exits`, P2.2 O0.2): precise-deopt and rerun blocks, the shared signal exit and per-site handler dispatches, the back-edge poll's slow path and the root-window grow calls, so Cranelift emits them after the hot code instead of between hot blocks. `share` also sends every precise-deopt block of a function through one cold tail that writes the pc/depth/handler cells and returns (each site keeps only its framestate spill). Read at compile time; unset/`off` marks nothing new, CLIF-identical (single-build A/B of all three). Census: `cold_exits[deopt= rerun= signal= dispatch= poll= grow= deopt_tail=]` in the `NEOVM_JIT_COMPILE_STATS=1` summary. Graduation: same-binary neutral or better on the six rows and org (P2.0 T0.3). |
//! | `NEOVM_JIT_BG` | `=sync`, `=on`, `=auto` | Where a JIT compile's backend (Cranelift codegen, register allocation, finalize) runs (`jit/bg.rs`, P2.4). `legacy`: the persistent module defines each leaf in place, as before. `sync`: the front/backend split run in line -- the front packages its finished function with its imports named by shim (`compile::shared::split`) and the eval thread's backend compiles it at once; deterministic, identical machine code. `on` (x86-64 Linux; elsewhere `sync`): entry tier-ups (dispatch, deferral expiry) hand the package to a worker thread `neovm-jit-0` with a backend and code arena of its own; the function stays interpreted (probing with a 32..1024-call backoff) until its leaf is installed at a probe on the eval thread, and every other compile runs as under `sync`. The compile stall (`total_us`) is then the front only. Counters: `[neovm-jit-final-bg]` (enqueued/installed/discarded by reason, backend and queue µs, install latency, probes, in-flight at exit, worker jobs/panics/code bytes). `auto`: `on` when the process's affinity has at least two CPUs, else `sync`. Read once per process. Companions (read once): `NEOVM_JIT_BG_THREADS` (1; clamped to 1..=min(4, CPUs-1)); `NEOVM_JIT_BG_QUEUE` (256 jobs) and `NEOVM_JIT_BG_QUEUE_INSTS` (1M CLIF instructions): a full queue drops its lowest-class, newest job (its function asks again once its heat doubled) or refuses a new one before its front, as does an eval thread holding 512 pending compiles; `NEOVM_JIT_BG_CLASSES=osr,first_sight,entry,upgrade` (default all) limits which compiles defer (`entry` alone is P2.4 B7); `NEOVM_JIT_BG_NICE=<n>` and `NEOVM_JIT_BG_AFFINITY=<cpu>[,<cpu>]` place the workers (measurement); `NEOVM_JIT_BG_STRESS=1` is the soak harness (each worker job starts 0-2 ms late and each finished job reads as running for 1-4 more probes, seeded by its sequence number). |
//! | `NEOVM_JIT_GATE_RELAX` | `=on` | Relax the calls ≤ arith profit gate. Default-on was tried and REVERTED (regressed byte-compile 21%) — measure byte-compile before ever re-flipping. |
//! | `NEOVM_JIT_LEAF` | `opcode,bcall,string` (unset); `=all`/`=on` adds the default-off parts; `=off`, or a comma list of `opcode`, `bcall`, `string`, `vars`, `batch` | Leaf builtins (design `p1-2-builtin-intrinsics`, `compile/leaf_abi.rs`). `opcode`: `Op::Get/Length/Nth/Nthcdr/Elt/Member/Equal/StringEqual/StringLessp` sites call their leaf's bare trampoline (register args, the result's bits or a tag-`001` sentinel) instead of the `neovm_jit_builtin1/2` table shim. `bcall`: an `Op::Call` site speculated on `gethash`, `plist-get` or `get-char-property` (any symbol bound to them) calls the leaf's Bcall trampoline: a guard (no pending quit/signal/profiler tick/throw-on-input, no compiler overrides, not `NEOVM_JIT_FORCE_SLOW_SPEC`, no `debug-on-next-call`, depth below the limit, the function cell unchanged) then the leaf, with no frame on success; a signal pushes GNU's `Bcall` frame lazily (`neovm_jit_leaf_signal_frame`) before dispatch; any guard miss or declined shape runs today's `neovm_jit_call_subr_spec` protocol. `string`: `aref` of a unibyte or all-ASCII multibyte string reads the byte inline (I1), and `aset` stores a same-width byte into owned string storage inline with GNU primitive-opcode semantics (I2); every other shape calls `neovm_jit_aref`/`neovm_jit_aset` as before. `vars` (default off): `Op::SymbolValue` sites call the `symbol-value` leaf's bare trampoline and `Op::Call` sites on `buffer-local-value` its armed one; both are their references' bodies; `buffer-local-value` answers a buffer-local variable loaded for the current buffer through P1.4 Stage A's `Context::read_var_cached`. `batch` (default off): `Op::Call` sites on the first leaf batch -- `assoc` (a TESTFN bounces), `rassq`, `delq`, `copy-sequence`, `symbol-name`, `boundp`, `keywordp` -- call their armed trampolines. Read at compile time only. `opcode,bcall,string` default ON since the F-B gate passed (dhrystone -18% instructions, elb-eieio -4.7%, pack-unpack -4%, board org-editing -1%, no row worse); `=off` emits the former code exactly (single-build A/B). Exit census: `[neovm-jit-final-builtin-leaves]` under `NEOVM_JIT_COMPILE_STATS=1`. |
//! | `NEOVM_JIT_FEEDBACK` | `=record`, `=use` | P2.1 C3/C4 call-target feedback (`jit/feedback.rs`, `compile/call_feedback.rs`): `record` makes the interpreter's `Op::Call` record the targets of the body's NON-constant call sites (a callee that is a variable or a closure, the function an `apply` spreads into, the callback of `mapc`/`mapcar`/`mapcan`/`mapconcat`) into the source's call-site table (a symbol, up to 4 closure sources, or megamorphic; closure instances of one source are one target), compiled code record and count them through the JIT-only `neovm_jit_call_prof`/`_apply_prof`/`_record_call_target` shims for a site's first 15,000 counted executions (its profiling window; after it the shim is a count test and a tail call), and the exit report adds `[neovm-jit-final-calls]` (the census: sites by shape and state, executions, the stability window's late transitions) under `NEOVM_JIT_COMPILE_STATS`. `census`: `record` with no window (every compiled call records, so the census sees transitions after the window: a measurement mode). `use`: the interpreter records and compiles read the targets (`NEOVM_JIT_SPEC_SOURCES` implies it); compiled sites do not record (no tier reads their feedback yet). Unset/`off`: nothing recorded, the interpreter's `Op::Call` pays one field compare as before and the lowering is CLIF-identical (single-build A/B). `NEOVM_JIT_CALL_FEEDBACK=on` is an alias of `record`. Blocker: F-1 R1 (recording tax <= 0.3% instructions on every row). |
//! | `NEOVM_JIT_SPEC_SOURCES` | `=on` | P2.1 C5 closure source slots (`compile/source_slots.rs`): a baseline T1 or OSR compile turns an `Op::Call` whose callee is not a constant and whose recorded target (`NEOVM_JIT_FEEDBACK`, which `on` raises to `use`) is ONE closure source into a guarded call: the callee's source identity (its `runtime` word, `jit_layout::BYTECODE_RUNTIME_WORD_OFFSET`) against the recorded source's, then `neovm_jit_call_source_spec` -- the spec shim's fast path on the source's armed leaf with the instance's own constant base and a frame recording the called object -- which answers `STATUS_NEED_GENERIC` for anything it declines; a miss or a decline runs the site's generic call, so the site never deopts. `NEOVM_JIT_FORCE_SLOW_SPEC=1` declines every call. With `NEOVM_JIT_DIRECT_CALL=on` and `DIRECT_SITES=all` or `unbounded`, the shim arms the slot with the source's register entry (exact arity, frameless) and the site then enters the leaf itself (the named direct call's checks, with the leaf-slot epoch in place of the function epoch, a frame recording the called object, the instance's own constant base as `aux`), the shim being its slow path. Read at compile time; unset/`off` builds no source site, CLIF-identical (single-build A/B). Blocker: F-1 R2 (closure kernel >= -25%, and eieio <= -1.5% or org <= -0.5% or pack-unpack <= -1%). |
//! | `NEOVM_JIT_TIER2` | off | `on` enables the tier spine: entry countdown, cold-poll and window-only HOF credit, bounded feedback stability, compile CPU budget and worthwhile upgrades. Fast leaves may re-tier to Full; stable hot loop/recursive or feedback-call leaves may rebuild through the existing backend with feedback. The old native leaf serves pending background jobs; feedback upgrades retain T1 for counted deopt reversion. Off preserves the heat-driven re-tier and generated CLIF. Default remains off until F-W and compile-cost gates pass. |
//! | `NEOVM_JIT_T2_WINDOW` | 15000 | First T1 work window under `TIER2=on`, counted by entries plus cold-poll and HOF credit. Read at compile time. |
//! | `NEOVM_JIT_T2_LOOP_CREDIT` | 64 | Under `NEOVM_JIT_TIER2` (W1): what one back-edge poll tick (255 taken back edges) takes from the countdown, so a loop leaf requests after about 60k iterations; the mapping-builtin credit is `len / 64` per call. `0` = entry-only: no poll credit and no mapping-builtin credit. Read at compile time. |
//! | `NEOVM_JIT_T2_STABLE` | 4000 | Work units with unchanged feedback after the first request or a T2 revert. |
//! | `NEOVM_JIT_T2_ATTEMPTS` | 4 | Changed snapshots rearm at most this many windows, then only the existing full-allocator re-tier remains admissible. |
//! | `NEOVM_JIT_T2_BUDGET` | 2 (%) | Per-mutator compile CPU allowance as a share of thread CPU; `0` is unlimited. In-flight upgrades reserve their estimate. |
//! | `NEOVM_JIT_T2_BUDGET_FLOOR_MS` | 5 | Initial compile CPU allowance in milliseconds, added to the proportional budget. |
//! | `NEOVM_JIT_T2_MAX_REOPT` | 3 | Counted feedback-upgrade invalidations per source before further feedback upgrades are banned. |
//! | `NEOVM_JIT_T2_STRESS` | off | `1`/`on`: first and stable windows become one, with an unlimited compile budget. Requires `TIER2=on`. |
//! | `NEOVM_JIT_INTRINSICS` | off; `=on`/`=all`, or a comma list of `length`, `nth`, `memq`, `symbol-value` | CLIF intrinsics for leaf builtins (design `p1-2-builtin-intrinsics` §2.7, `compile/intrinsics.rs`), each an inline PREFIX of its opcode site whose miss falls through to the site's usual call (leaf trampoline, value shim or table shim), never to a deopt. `length` (I3): nil, a proper list of at most 64 conses (a bounded walk), a string, a plain vector or record. `nth` (I4): `nth`/`nthcdr`/`elt` of a list at a constant index 0..4, unrolled. `memq` (I5): `memq`/`assq`, and `member` with a fixnum or bare-symbol key, over the first 16 conses by bit identity, only while `symbols-with-pos-enabled` is off. `symbol-value` (I6): a bare symbol whose value cell is plain and bound (the `Op::VarRef` inline read; a nil value is refused unless the symbol is a known constant that is not a dedicated buffer-local). JIT only; read at compile time; unset/`off` emits the former code exactly (single-build A/B). Census: `intrinsic-<name>:inline_sites=` in `[neovm-jit-final-builtin-leaves]`. |
//! | `NEOVM_JIT_LEAF_EFFECTS` | off; `=on` | MIR leaf effects (design `p1-2-builtin-intrinsics` §2.8, commit 11): an opcode site that calls a leaf builtin's body -- its trampoline, the `memq`/`assq` value shims or a pure table entry -- and whose lowering is GC-free (`calls::opcode_site_effects`: the leaf's declared `Effects`, which exclude MAY_GC, MAY_REENTER and MAY_DEOPT; the inline-lowered `aref`/`setcar`/`setcdr` are excluded, measured slower in MIR) no longer keeps a looping body out of the MIR tier (`gate:loop-opaque:<Op>`), and inside a MIR leaf it is no safepoint: the values live across it stay raw and unrooted (`mir_force_tagged` is skipped). JIT only; read at compile time; unset/`off` admits and lowers exactly as before (single-build A/B). The census is the MIR bail keys of `NEOVM_JIT_COMPILE_STATS=1`. |
//! | `NEOVM_REGEX_ANCHOR_ALT` | on (`=off` disables) | P3.3 Stage 0 (`text/regex/emacs.rs` `start_anchor`): a regexp whose every alternative begins with `^` (or `` \` ``) searches line starts (or position 0) only; read when a pattern compiles. Default ON since the board A/B (org-editing -1.4% instructions). |
//! | `NEOVM_REGEX_DFA` | on (`=off` disables, `=verify`) | P3.3 existence DFA (`text/regex/dfa.rs`): `re_search` skips candidates the DFA rejects, `syntax-table` property runs and the lazy-propertize frontier included; `verify` also runs the matcher on every candidate and reports contradicted verdicts. `NEOVM_REGEX_DFA_STATS=1` prints the filter's counters (`[neovm-regex-dfa]`) at exit. Default ON since the F1′ replay and the board A/B (C8, U3.1). |
//! | `NEOVM_FN_STAMPS` | off; `=on`/`=1` | P1.3 A1 per-symbol function-binding stamps (`symbol/fn_stamps.rs`): function-cell writes stamp their symbol, and subr rewrites, compiler-overrides changes, and dump restore raise a floor. `Obarray::fn_unchanged_since(sym, E)` proves that binding unchanged after unrelated clock moves. Stamps are always written; off makes the predicate false. Read once per process; no CLIF change. S2.7 F1 confirmed leaf-resolution savings but failed the load-cycle gate; A3/A4 consumers remain parked, so this knob currently enables only the validity predicate. |
//!
//! ## Measurement / bisection
//! | Knob | Meaning |
//! |---|---|
//! | `NEOVM_JIT_DIRECT_SELF_HEAT` | off; `seen` admits self sites only after nonzero source heat, `hot` only at the normal `NEOVM_JIT_THRESHOLD`; applies only with direct calls on and `SITES=self`. The compiler reads the existing atomic heat once to admit both self sites and their implicit register ABI. Explicit `REG_ABI=on` retains global register entries. Cold first-sight bodies keep their shim and may remain on it indefinitely; naturally dispatch-hot recursion retains its initial direct sites. Off preserves the original self policy; direct calls off preserves the original CLIF. |
//! | `NEOVM_JIT_DIRECT_PROFILE` | off; `=on` counts attempts and guarded native-entry hits for each emitted direct site using atomic RMWs; the exit builtin-leaf census includes owner/source/site ordinal, zero-use sites, misses, and cold slot-arming attempts. Use with `NEOVM_JIT_COMPILE_STATS=1` or `NEOVM_JIT_STATS_FILE` for the report. Diagnostic runs only; off emits identical CLIF. |
//! | `NEOVM_JIT_DIRECT_PROFILE_FILE` | unset; with direct profiling on, append TSV site identities and retained atomic counter addresses before code publication. An ancestor profiler can read counters at edit-loop barriers for exact scoped counts; no generated instructions beyond the profile knob. |
//! | `NEOVM_JIT_MAX_ID` | Compile only functions with id ≤ N (ids assigned in first-hot order) — clean prefix bisection of a misbehaving workload. |
//! | `NEOVM_JIT_DEBUG_ID` | Dump the bytecode body of the one compiled function with this id. |
//! | `NEOVM_JIT_PROFILE` | Append per-function workload-characterization records to this file path: one CSV row per compile attempt (its last three columns are `compiled_id,tier,name`), each followed by a `#mir,compiled_id,verdict` row (why the MIR tier did or did not take the body: `taken`, the pre-build gates such as `gate_opt+gate_prefix`, or the first bail key the census counted; `-` when the compile never reached the MIR tier; see `stats::verdict`), and at exit one `#leaf,compiled_id,name,tier,osr_pc,entries,deopt_at,deopt_rerun,signals,top_deopt_pc` row per compiled leaf, joinable on `compiled_id`. Also turns on per-leaf entry counting and per-function names (see `NEOVM_JIT_COMPILE_STATS`). |
//! | `NEOVM_JIT_COMPILE_STATS` | `=1`: print a running compile-stall summary line every 64 compiles (and every 50,000 dispatch consultations), plus a final `[neovm-jit-final*]` report when the command loop returns (`kill-emacs`, end of `--batch`, batch-error exit 255; not on death by signal). The final report separates the session from startup (`since_command_loop`), counts `function_epoch` bumps by reason, and prints per-leaf deopt/signal/entry counts, compile µs and MIR verdict (`mir=`, as in `NEOVM_JIT_PROFILE`'s `#mir` rows) (`[neovm-jit-final-leaf]`: the most-deopting leaves, then the most-entered, then the costliest remaining compiles). Every compile is also split by why it ran and where its time went (`[neovm-jit-phases]` every 64 compiles, `[neovm-jit-final-phases]` at exit: `origin[dispatch=count/ok/µs,...]` over dispatch, deferral_expired, retier, first_sight, osr, aot_drain; `phase_us[gate,mir_build,fuse,lower,setup,codegen,finalize,other]`, exclusive, summing to the metered stall; see `stats::phases`). The phase split reads the clock only under this knob; the origin rows are always kept. The persistent JIT backend's counters print as `[neovm-jit-final-code-memory]` (leaves per backend, modules created/retired, re-entrant fallbacks, arena regions/pages/code bytes/seals). Under this knob (or `NEOVM_JIT_STATS_FILE`/`NEOVM_JIT_PROFILE`) every JIT leaf compiled from then on also carries a native-entry counter in its prologue (a load, an add and a store: expect call-heavy code to run slightly slower, so do not time runs with a report knob on) and a per-function entry name. With every report knob off the generated code has no counter. |
//! | `NEOVM_JIT_STATS_FILE` | `=<path>`: append every `[neovm-jit-*]` report line to this file instead of stderr (an unopenable path falls back to stderr). Setting it implies `NEOVM_JIT_COMPILE_STATS=1`. |
//! | `NEOVM_JIT_SIZE_UNIT` | Override [`RuntimeState::SIZE_UNIT`] (512): the ops-per-unit divisor scaling the tier-up threshold by body size. |
//! | `NEOVM_JIT_MAX_OPS` | Override [`RuntimeState::MAX_TIER_OPS`] (4096): largest body that tiers at all; `0` = uncapped (the mid-end campaign's acceptance configuration). |
//! | `NEOVM_JIT_REGALLOC` | Force one Cranelift register allocator for every JIT compile: `backtracking` (regalloc2 ion) or `single_pass` (fastalloc). Unset = the policy in `lowering::choose_regalloc` (fast for straight-line bodies, full for loops/OSR, re-tier when hot). |
//! | `NEOVM_JIT_REGALLOC_SMALL_MAX` | Bytecode-op cap for using the full (backtracking) allocator regardless of body shape; default `0` preserves today's allocator policy and CLIF. The forced `NEOVM_JIT_REGALLOC` choice takes precedence; larger bodies keep the original rule. Read once per process, before lowering and ISA selection, including the MIR tier. |
//! | `NEOVM_JIT_PROFIT_DEFER` | Override [`RuntimeState::PROFIT_DEFER_FACTOR`] (4): a body the profitability gate refuses tiers up anyway at `factor × hot_threshold()` calls (`0` = never, the former veto). |
//! | `NEOVM_JIT_RETIER_FACTOR` | Override [`RuntimeState::RETIER_FACTOR`] (16): a fast-allocator leaf is rebuilt with the full allocator at `factor × hot_threshold()` heat; `0` = never. |
//! | `NEOVM_JIT_REGALLOC_CHECKER=1` | Run regalloc2's checker after every allocation (verification harness for the allocator choice). |
//! | `NEOVM_JIT_MEMQ_STEPS` | `=<1..64>`: the bound of `NEOVM_JIT_INTRINSICS=memq`'s inline walk (default 16), for measurement. Read at compile time. |
//! | `NEOVM_JIT_LEAF_ONLY` | `=<name>,<name>` (the builtins' Lisp names, e.g. `nth,gethash`): restrict `NEOVM_JIT_LEAF`'s sites to these leaves -- per-builtin measurement and bisection. Unset = every leaf. Read at compile time. |
//! | `NEOVM_JIT_DUMP_CLIF` | `=<path>`: append every lowered function's CLIF to this file, each under a `;; baseline\|mir ... entry=<declared name>` header (the IR-composition census). Turns per-function entry names on, so `entry=` names the Lisp function. |
//! | `NEOVM_JIT_DUMP_ASM` | `=<path>`: append the final machine code of every JIT leaf (baseline, MIR, OSR) to this file: a `;; ==== <label> name= id= tier= addr= size= regalloc= clif_insts=` header, Cranelift's post-register-allocation disassembly (physical registers, spill/reload moves; `Context::set_disasm`, requested only under this knob), and the finalized code bytes in hex (`xxd -r -p \| objdump -D -b binary -mi386:x86-64 --adjust-vma=<addr>` recovers exact offsets for a perf `sym+offset`). Turns per-function entry names on. Compile-time only; unset = no disassembly work at all. |
//! | `PERF_BUILDID_DIR` | Not ours: `perf record -- cmd` sets it, and cranelift-jit then appends every defined function to `/tmp/perf-<pid>.map`. It also turns on per-function entry names: each JIT leaf is declared as `lisp:<fn>#<id>:baseline\|mir\|osr@<pc>` instead of the shared `__neovm_jit_leaf`/`__neovm_mir_leaf`, so perf attributes JIT samples per Lisp function (`<fn>` is the called symbol, else `anon[<first symbol constants>]`). For `perf record -p` on a running editor, start neomacs with `PERF_BUILDID_DIR=/tmp`. Names are also on under `NEOVM_JIT_DUMP_CLIF` and any report knob. Only the declared name changes; the code is identical. |
//!
//! ## Verification harnesses (force the cold path everywhere; run the suite with each ON)
//! | Knob | Forces |
//! |---|---|
//! | `NEOVM_JIT_INLINE_CENSUS` | off; `=1` records P2.3 candidate sites at compile attempts and bytecode callbacks entered through `apply1_bytecode`, with source/target rows at exit and callback counts since command-loop entry. Read once per process; no generated code changes. Measurement-only: remove after the inlining policy is qualified; keep off during performance gates. |
//! | `NEOVM_JIT_FORCE_DEOPT=1` | Every speculation guard fails → every deopt path executes. |
//! | `NEOVM_JIT_FORCE_SLOW_SPEC=1` | Every spec-call shim takes its stale-epoch re-validate branch on every call. |
//! | `NEOVM_JIT_FORCE_CBSYM_GENERIC=1` | Every CallBuiltinSym intrinsic bounces to its generic fallback. |
//! | `NEOVM_JIT_REOPT_STRESS=1` | Deopt reoptimization at its most eager: site limit 1, re-profile window 1, `MAX_REOPTS` 1, so every source climbs the whole `ReoptLevel` ladder. `NEOVM_JIT_FORCE_DEOPT=1` alone makes reoptimization inert (else it would widen every site to generic and stop exercising the deopt paths); with this knob too it drives every source up the ladder. |
//!
//! ## AOT (`jit/aot.rs`)
//! | Knob | Meaning |
//! |---|---|
//! | `NEOVM_AOT` | `1`/`on`/`force` enables the AOT preload; `force` additionally warns when no usable preload loaded. |
//! | `NEOVM_AOT_RETIER` | Default off. `1`/`on` replaces AOT leaves with JIT code at `NEOVM_JIT_RETIER_FACTOR × NEOVM_JIT_THRESHOLD` heat, including cached native calls. T2 requests use the tier spine; otherwise the existing heat trigger applies. Pending upgrades keep serving native AOT; refused upgrades keep it without repeated requests. `RETIER_FACTOR=0` or a forced allocator disables this upgrade. |
//! | `NEOVM_AOT_PGO` | `1`/`on`/`force` enables PGO collection for the AOT function set. |
//! | `NEOVM_AOT_PREWARM` | `profitable` (default): of the preload's members only the manifest's `m` class (bodies the JIT's profit gate would compile) runs native from call 1; `c` call glue is served when the JIT would compile it, replacing that compile (P4.2 A4). `all`: every member from call 1, as before (single-build A/B). |

#![cfg_attr(not(feature = "jit"), allow(dead_code))]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::emacs_core::intern::SymId;

/// Cranelift codegen backend (Phase 3+). Only compiled with the `jit` feature,
/// since it links Cranelift. Today it exposes a self-contained smoke path that
/// proves the codegen toolchain works inside neovm-core's own build before any
/// bytecode is lowered onto it — the same "prove the tool, then build on it"
/// discipline used to validate TSan before trusting the concurrent GC.
#[cfg(feature = "jit")]
pub mod backend;

/// Baseline bytecode → native lowering (Phase 3b+). Compiles the leaf,
/// straight-line opcode subset to machine code and bails to the interpreter on
/// anything else. Only built with the `jit` feature. See `jit/compile.rs`.
#[cfg(feature = "jit")]
pub mod compile;
#[cfg(feature = "jit")]
pub(crate) mod vframe;

/// Per-thread compiled-code cache + the tier-up entry point the dispatch seam
/// calls ([`cache::try_run_compiled`]). Only built with the `jit` feature.
#[cfg(feature = "jit")]
pub mod cache;

/// Bytecode-level inlining (see the module docs).
#[cfg(feature = "jit")]
pub mod inline;

/// MIR: typed SSA IR for the optimizing Tier-2 (above the baseline `compile`).
/// Live: `compile::compile_bytecode_function_inner` builds the MIR for pure
/// required-only bodies, runs the pure inliner + type/unboxing/guard-elision and
/// cons-escape passes, and lowers it via `compile::lower_mir_pure`, falling back
/// to the baseline tier otherwise. Only built with the `jit` feature. See
/// `jit/mir.rs`.
#[cfg(feature = "jit")]
pub mod mir;

/// Owned global-SSA mid-end, selected only by the opt-in backend knob.
#[cfg(feature = "jit")]
pub(crate) mod opt;

/// AOT (ahead-of-time) object emission (Phase R1c): emit the same CLIF the JIT
/// does, but through Cranelift's `ObjectModule`, producing a relocatable `.o`
/// that is linked to a `.so`, `dlopen`'d, and inserted as a pre-warmed
/// `CompiledLeaf`. Only built with the `jit` feature. See `jit/aot.rs`.
#[cfg(feature = "jit")]
pub mod aot;

/// Deopt-driven reoptimization: classification and census of every deopt,
/// feedback widening, invalidation and backoff. Only built with the `jit`
/// feature. See `jit/reopt.rs`.
#[cfg(feature = "jit")]
pub mod reopt;

/// Per-source, per-pc retreat words deopts set (`SiteRetreat`). Always
/// built: `RuntimeState` holds the table.
pub(crate) mod retreat;

/// Background compilation: the front/backend split of a JIT compile and
/// where its backend runs (`NEOVM_JIT_BG`). Only built with the `jit`
/// feature. See `jit/bg.rs`.
#[cfg(feature = "jit")]
pub(crate) mod bg;
/// Per-source feedback (`SourceFeedback`). Always built: `RuntimeState`
/// holds it.
pub(crate) mod feedback;

/// The tier spine's trigger: the T1 countdown, its request and the upgrade
/// decision (`NEOVM_JIT_TIER2`). Only built with the `jit` feature. See
/// `jit/tier2.rs`.
#[cfg(feature = "jit")]
pub(crate) mod tier2;

/// Always-on metering of the synchronous compile stalls the cache-miss path
/// pays on the eval thread — the evidence base for background compilation.
/// Only built with the `jit` feature. See `jit/stats.rs`.
#[cfg(feature = "jit")]
pub mod stats;

#[cfg(feature = "jit")]
pub use cache::{note_seam_interp_fallback, try_run_compiled};

/// Which execution tier currently backs a compiled function.
///
/// This enum models only the interpreter tier; the compiled tiers — the
/// baseline Cranelift JIT and the optimizing typed-MIR Tier-2 — live in
/// `compile.rs` and are selected there, reached via [`Plan::Compiled`]. Do NOT
/// add a catch-all when matching on this — let the compiler enforce that each
/// new tier is handled everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tier {
    /// Tier 0 — interpret the function's bytecode `ops` via `bytecode::Vm`.
    #[default]
    Bytecode,
}

/// The action the dispatcher takes for one invocation of a compiled function.
/// Exhaustive by design (mirrors [`Tier`]).
#[derive(Debug)]
pub enum Plan {
    /// Run the Tier-0 bytecode interpreter.
    Interpret,
    /// The function is hot — consult the JIT (the baseline tier or the
    /// optimizing typed-MIR Tier-2, selected in `compile.rs`): compile-on-first-
    /// use and run native, or fall back to the interpreter on a deopt /
    /// non-compilable body. See [`cache::try_run_compiled`].
    Compiled,
}

// ---------------------------------------------------------------------------
// Phase 1 — feedback. The runtime-observed information later tiers speculate on.
// ---------------------------------------------------------------------------

/// Type/target feedback observed at one CALL site (the JIT's most important
/// speculation input — it enables direct-call inlining).
///
/// Holds a [`SymId`], NOT a function `Value`: a `SymId` is a stable runtime
/// index, never a heap pointer, so feedback is **GC-safe** — the collector never
/// has to trace it, and it never dangles. The optimizing tier turns
/// `Monomorphic(sym)` into a direct/inlined call guarded by a dependency on that
/// symbol's function cell (Phase 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallFeedback {
    /// This site has not executed yet.
    Uninit,
    /// Every observed call so far went to the same named function.
    Monomorphic(SymId),
    /// Conflicting / non-symbol callees seen — no useful speculation.
    Megamorphic,
}

impl CallFeedback {
    /// Pack into one `u64` for lock-free atomic storage. Low 2 bits tag the
    /// variant; a `SymId`'s `u32` rides in the upper bits.
    #[inline]
    const fn pack(self) -> u64 {
        match self {
            CallFeedback::Uninit => 0b00,
            CallFeedback::Monomorphic(SymId(n)) => ((n as u64) << 2) | 0b01,
            CallFeedback::Megamorphic => 0b10,
        }
    }

    #[inline]
    fn unpack(bits: u64) -> Self {
        match bits & 0b11 {
            0b00 => CallFeedback::Uninit,
            0b01 => CallFeedback::Monomorphic(SymId((bits >> 2) as u32)),
            0b10 => CallFeedback::Megamorphic,
            // The mask yields only 0..=3 and 0b11 is a reserved (unused) tag;
            // treat it as the safe over-approximation rather than panicking.
            _ => CallFeedback::Megamorphic,
        }
    }
}

/// Operand types observed at one ARITHMETIC site — the input the lowering
/// needs to stop emitting a fixnum guard for code that is never fixnum.
///
/// `lowering::stack_as_raw` guards fixnum and branches to a deopt block
/// otherwise, so a float or bignum operand bails the whole compiled body.
/// `nbody` gets **3.6%** from the JIT and `pidigits` gets **-6.5%** — the
/// compiler needs to know which sites are worth lowering that way.
///
/// The default (`FixnumOnly`) is the zero slot, and it means exactly what the
/// lowering already assumes, so a site that never runs or only ever sees
/// fixnums needs no recording at all: the arithmetic opcodes record on their
/// SLOW arm only, which they already branch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFeedback {
    /// Never seen a non-fixnum operand pair (or never executed).
    FixnumOnly,
    /// Every non-fixnum pair so far was floats (fixnums alongside are fine —
    /// they promote). Lowerable as `f64`.
    Float,
    /// A bignum, marker or non-number operand: nothing an unboxed lowering
    /// can take. (A fixnum overflow or zero divisor records nothing.)
    Other,
}

impl NumericFeedback {
    /// Classify the operands an arithmetic site took, for the lowering's
    /// benefit. The interpreter's slow arithmetic arm and the deopt
    /// classifier (`jit::reopt`) share this one classification.
    ///
    /// `Float` means every operand is a float or a fixnum and at least one is
    /// a float: exactly what an `f64` lowering can take, promoting the
    /// fixnums. A bignum, marker or non-number is `Other`. All fixnums — an
    /// overflow, a zero divisor, `1+` at the boundary — is `FixnumOnly`, the
    /// operand types the fixnum lowering assumes, and the interpreter records
    /// nothing: one overflow during warm-up must not turn a fixnum-hot site
    /// into a generic-fallback site for good (`Other` is sticky, and it takes
    /// the body out of the MIR tier).
    #[inline]
    pub fn of_operands(args: &[crate::emacs_core::value::Value]) -> Self {
        let mut saw_float = false;
        for arg in args {
            if arg.is_float() {
                saw_float = true;
            } else if !arg.is_fixnum() {
                return NumericFeedback::Other;
            }
        }
        if saw_float {
            NumericFeedback::Float
        } else {
            NumericFeedback::FixnumOnly
        }
    }

    /// Packed into the tag `CallFeedback` leaves reserved. A bytecode
    /// instruction is either a call or an arithmetic op, never both, so the
    /// two lattices never share a slot — and the encodings are chosen so that
    /// reading one as the other still yields the SAFE answer either way
    /// (`Megamorphic` / `Other`), rather than relying on that disjointness.
    #[inline]
    const fn pack(self) -> u64 {
        match self {
            NumericFeedback::FixnumOnly => 0b00,
            NumericFeedback::Float => 0b0111,
            NumericFeedback::Other => 0b1011,
        }
    }

    #[inline]
    fn unpack(bits: u64) -> Self {
        match bits {
            0b00 => NumericFeedback::FixnumOnly,
            0b0111 => NumericFeedback::Float,
            // Anything else in the slot is a call lattice value or an unknown
            // encoding: the safe over-approximation, never a float guess.
            _ => NumericFeedback::Other,
        }
    }
}

/// How far deopt-driven reoptimization (`jit::reopt`) has pulled one
/// source's speculation back: the ceiling every later compile of that source
/// respects. Monotone per source; a redefinition is a new source and starts
/// at [`ReoptLevel::Speculative`]. Runtime state only, never dumped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ReoptLevel {
    /// Compile from feedback (the behaviour without reoptimization).
    Speculative = 0,
    /// No call site is spliced (fuser), MIR-inlined or bit-op intrinsified.
    NoInline = 1,
    /// `NoInline`, and the MIR tier is skipped: every guard left is a precise
    /// baseline deopt, attributable to a pc.
    BaselineOnly = 2,
    /// `BaselineOnly`, and every arithmetic site takes the generic fallback:
    /// GNU's `Bplus` shape (a fixnum fast path, else the builtin), which never
    /// deopts.
    Generic = 3,
    /// Interpreter only: entry and OSR compiles refuse this source.
    Interpreter = 4,
}

impl ReoptLevel {
    /// The level stored as `v` (saturating to [`ReoptLevel::Interpreter`]).
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => ReoptLevel::Speculative,
            1 => ReoptLevel::NoInline,
            2 => ReoptLevel::BaselineOnly,
            3 => ReoptLevel::Generic,
            _ => ReoptLevel::Interpreter,
        }
    }

    /// One step further back (saturating).
    pub const fn next(self) -> Self {
        Self::from_u8(self as u8 + 1)
    }

    /// Lower-case name for reports and traces.
    pub const fn name(self) -> &'static str {
        match self {
            ReoptLevel::Speculative => "speculative",
            ReoptLevel::NoInline => "no_inline",
            ReoptLevel::BaselineOnly => "baseline_only",
            ReoptLevel::Generic => "generic",
            ReoptLevel::Interpreter => "interpreter",
        }
    }
}

/// A per-function feedback vector — one slot per bytecode instruction, lazily
/// allocated on first use (when the instruction count is known). Slots for
/// non-call instructions stay [`CallFeedback::Uninit`]. Lock-free
/// (`AtomicU64`), `Send + Sync` — sound to hold inline on a GC-managed function
/// alongside the concurrent collector (the mutator is the only writer).
#[derive(Debug, Default)]
pub struct FeedbackVec {
    slots: OnceLock<Box<[AtomicU64]>>,
}

impl FeedbackVec {
    #[inline]
    pub const fn new() -> Self {
        Self {
            slots: OnceLock::new(),
        }
    }

    /// Allocate (once) `len` zeroed slots. Idempotent; a benign race just keeps
    /// whichever allocation wins.
    #[inline]
    fn slots(&self, len: usize) -> &[AtomicU64] {
        self.slots
            .get_or_init(|| (0..len).map(|_| AtomicU64::new(0)).collect())
    }

    /// Record an observed callee `sym` at call-site `pc` (instruction index);
    /// `ops_len` is the function's instruction count, for lazy sizing. Drives
    /// the `Uninit -> Monomorphic -> Megamorphic` lattice.
    #[inline]
    pub fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        let slots = self.slots(ops_len);
        let Some(slot) = slots.get(pc) else { return };
        let next = match CallFeedback::unpack(slot.load(Ordering::Relaxed)) {
            CallFeedback::Uninit => CallFeedback::Monomorphic(sym),
            // Unchanged target — no store needed (stays monomorphic).
            CallFeedback::Monomorphic(seen) if seen == sym => return,
            CallFeedback::Monomorphic(_) => CallFeedback::Megamorphic,
            CallFeedback::Megamorphic => return,
        };
        slot.store(next.pack(), Ordering::Relaxed);
    }

    /// Record the operand types observed at arithmetic site `pc`. Called only
    /// from the opcodes' non-fixnum arm, so the fixnum fast path pays nothing.
    /// Drives `FixnumOnly -> Float -> Other`, with `Other` sticky. Returns
    /// whether the slot moved (the deopt reoptimizer's "learned something").
    #[inline]
    pub fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) -> bool {
        let slots = self.slots(ops_len);
        let Some(slot) = slots.get(pc) else {
            return false;
        };
        let next = match (NumericFeedback::unpack(slot.load(Ordering::Relaxed)), seen) {
            (NumericFeedback::Other, _) => return false,
            (NumericFeedback::Float, NumericFeedback::Float) => return false,
            (_, NumericFeedback::FixnumOnly) => return false,
            (_, seen) => seen,
        };
        slot.store(next.pack(), Ordering::Relaxed);
        true
    }

    /// Operand-type feedback at arithmetic site `pc`.
    #[inline]
    pub fn numeric_at(&self, pc: usize) -> NumericFeedback {
        match self.slots.get() {
            None => NumericFeedback::FixnumOnly,
            Some(slots) => slots.get(pc).map_or(NumericFeedback::FixnumOnly, |s| {
                NumericFeedback::unpack(s.load(Ordering::Relaxed))
            }),
        }
    }

    /// Feedback at call-site `pc` (or `Uninit` if unallocated / out of range).
    #[inline]
    pub fn call_at(&self, pc: usize) -> CallFeedback {
        match self.slots.get() {
            None => CallFeedback::Uninit,
            Some(slots) => slots.get(pc).map_or(CallFeedback::Uninit, |s| {
                CallFeedback::unpack(s.load(Ordering::Relaxed))
            }),
        }
    }
}

impl Clone for FeedbackVec {
    /// A clone starts with no feedback (per-instance, like the heat counter).
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// Per-SOURCE runtime tiering + profiling state, shared by every
/// `ByteCodeFunction` instance that `make-closure` derives from one prototype
/// (see [`Runtime`], the handle). NOT part of the dumped representation
/// (`DumpByteCodeFunction`) — pure runtime state, started cold each session.
/// Relaxed atomics: the mutator is the only writer today, and being `Sync`
/// keeps the heap object sound alongside the concurrent collector.
#[derive(Debug)]
pub struct RuntimeState {
    /// Coarse invocation hotness (saturating at `u32::MAX`). The feedback that
    /// later phases use to decide when to tier a function up.
    heat: AtomicU32,
    /// The `cache::rejection_epoch()` at which the JIT rejected this body as
    /// `NotCompilable` (0 = never). While it equals the live epoch the
    /// dispatcher answers `Interpret` outright: the cache would answer
    /// `NotCompilable` and fall back to the interpreter anyway, so the seam
    /// trip it saves (argument copy, root save/restore, probe) is pure waste.
    /// `cache::clear` bumps the epoch (the cache would retry, so does this);
    /// a grown `make-closure` prefix resets it with its eviction. Without the
    /// `jit` feature there is no cache to reject anything, so it is only ever
    /// written.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    native_rejected_epoch: AtomicU64,
    /// Heat at which a DEFERRED body is compiled (0 = not deferred): one the
    /// profitability gate refused (`NotProfitable`), or one whose leaf a
    /// deopt invalidated (`jit::reopt`: the end of its re-profile window).
    /// Either way the body already proved hot, so once the deferral expires
    /// `dispatch_sized` answers `Compiled` at once. For the profit gate: a
    /// call-heavy body RUNS faster native but its compile is dear (org editing probe, 2026-09-05: ~38M
    /// instructions per admitted body vs ~830 saved per native entry), so it
    /// pays off only past ~18k calls: the gate was +6.4% on a 5-pass session
    /// and −8.8% on a 50-pass one. Deferring to `profit_defer_factor() ×
    /// hot_threshold()` lets long sessions win without taxing short ones.
    /// The dispatcher answers `Interpret` without a cache probe until then.
    profit_deferred_heat: AtomicU32,
    /// Everything the interpreter observed about this source
    /// ([`feedback::SourceFeedback`]); compiles read it through their
    /// snapshot.
    feedback: feedback::SourceFeedback,
    /// Process-unique identity assigned on first JIT compilation attempt (0 =
    /// unassigned). Keys this function's entry in the per-thread compiled-code
    /// cache ([`cache`]). Monotonic and never reused, so a freed function's
    /// stale cache entry can never be mis-looked-up after the (non-moving) GC
    /// reuses its address — a new function gets a new id. Reset to 0 on clone.
    compiled_id: AtomicU64,
    /// Set by the AOT preload prepopulate (R2-C3) when this function's leaf was
    /// inserted into the compiled cache at startup: `dispatch` then serves the
    /// prewarmed native leaf FROM CALL 1 instead of interpreting until the heat
    /// threshold — the piece that lets the AOT preload cover ONE-SHOT startup
    /// elisp, which never gets hot. One relaxed load on the dispatch path,
    /// never set in the default (AOT-off) configuration.
    aot_prewarmed: std::sync::atomic::AtomicBool,
    /// Set once this body has been compiled and its numeric feedback read.
    ///
    /// Feedback is an input to COMPILATION; once it has been consumed every
    /// further record is dead weight. That is not a micro-optimization on a
    /// body whose arithmetic is all non-fixnum: there the opcodes' "slow" arm
    /// is the ONLY arm, so `pidigits` was paying the recording on every
    /// arithmetic operation it performs — 1.4% of the row. Gating on `heat`
    /// instead does NOT work: a body entered through the armed or speculated
    /// paths barely bumps it (`pidigits` sat at heat 1332 after 333 calls).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    numeric_feedback_consumed: std::sync::atomic::AtomicBool,
    /// Slice C1 of the JIT call seam: the compiled leaf the interpreter's
    /// `Bcall` arm enters directly, as a raw `*const CompiledLeaf`. Zero = empty.
    /// Dereferencing requires the arming mutator's TLS cache to remain live,
    /// and `leaf_slot_epoch` to equal `cache::leaf_slot_epoch()`.
    ///
    /// P7.11's shared-source/local-activation split must replace this slot
    /// before several mutators enter the same source concurrently: the epoch
    /// supplies neither cache ownership nor an atomic pointer/epoch pair.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    leaf_slot: AtomicU64,
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    leaf_slot_epoch: AtomicU64,
    /// Widest `make-closure` patch seen for this source: the number of leading
    /// constant slots that hold PER-INSTANCE captured values (the prototype
    /// carries placeholder symbols `V0..Vn` there — `byte-compile-make-closure`).
    /// A shared native leaf must never bake, speculate on, or symbol-tag those
    /// slots; it loads them through the executing callee's constant vector at
    /// run time (`compile.rs` "dynamic prefix"). Monotone; recorded by
    /// `builtin_make_closure`, which also evicts any leaf compiled under a
    /// narrower prefix. GNU keeps no such record because GNU byte-code objects
    /// carry no JIT state at all (native-comp attaches to the subr); this port
    /// hung tiering state on the object `make-closure` copies, so the patch
    /// width must be visible to the code that shares that state.
    patched_prefix: AtomicU32,
    /// Deopt-driven invalidations of this source (saturating): the backoff
    /// input of `jit::reopt`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    reopt_count: std::sync::atomic::AtomicU8,
    /// Counted T2 invalidations, saturating and independent of P0.4's ladder.
    /// Sources may be shared by mutators; Relaxed RMWs preserve this monotone
    /// observational ban without publishing leaf pointers or Lisp state.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    t2_reopts: std::sync::atomic::AtomicU8,
    /// Permanent AOT exclusion after this source requested a JIT re-tier.
    /// Shared by mutators: monotone Relaxed stores/loads publish no code
    /// pointer or Lisp state; each mutator owns its upgrade/cache.
    /// Independent of heat, whose observational updates can race.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    aot_retiered: std::sync::atomic::AtomicBool,
    /// [`ReoptLevel`] as `u8`: the ceiling every later compile of this source
    /// respects. Monotone.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    reopt_level: std::sync::atomic::AtomicU8,
    /// What deopts taught this source about each pc, allocated on the first
    /// mark: among others, the `Op::Call` pcs that must not be spliced,
    /// MIR-inlined or intrinsified ([`retreat::RetreatBit::NoInline`]).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    site_retreat: retreat::SiteRetreatTable,
    /// Test-only: pin this function to the Tier-0 interpreter regardless of
    /// hotness (the benchmark harness measures native vs interpreter in ONE
    /// process — a hot copy and a forced-cold copy — to cancel the
    /// cross-process CPU-frequency variance that wrecks a two-process A/B).
    /// Absent from the production library (only the test binary carries it).
    #[cfg(test)]
    force_interpret: std::sync::atomic::AtomicBool,
}

/// Source of process-unique [`Runtime::compiled_id`] values. Ids are
/// `fetch_add + 1` so 0 stays reserved for "unassigned".
static NEXT_COMPILED_ID: AtomicU64 = AtomicU64::new(0);

/// Invocations before a function tiers up to the JIT. Defaults to
/// [`Runtime::HOT_THRESHOLD`]; the `NEOVM_JIT_THRESHOLD` environment variable
/// overrides it — e.g. `=1` runs every compilable function through the JIT,
/// the every-function differential soak used to qualify default-on (Phase 9).
/// Body-size unit for the tier-up budget (`RuntimeState::dispatch_sized`): a
/// body of `n` ops must be called `hot_threshold() * max(1, n / unit)` times
/// before it tiers, so the (size-proportional) compile cost is amortized over
/// proportionally more interpreted calls before it is paid. Defaults to
/// [`RuntimeState::SIZE_UNIT`]; `NEOVM_JIT_SIZE_UNIT` overrides it (`0`
/// disables the scaling — every body tiers at the flat threshold).
pub fn size_unit() -> u32 {
    static UNIT: OnceLock<u32> = OnceLock::new();
    *UNIT.get_or_init(|| {
        std::env::var("NEOVM_JIT_SIZE_UNIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::SIZE_UNIT)
    })
}

/// `NEOVM_JIT_PROFIT_DEFER`: the factor a profitability-refused body's tier-up
/// is deferred by (× [`hot_threshold`]); `0` = refuse forever, as before.
/// Defaults to [`RuntimeState::PROFIT_DEFER_FACTOR`].
pub fn profit_defer_factor() -> u32 {
    #[cfg(test)]
    if let Some(forced) = PROFIT_DEFER_TEST_OVERRIDE.with(|c| c.get()) {
        return forced;
    }
    static FACTOR: OnceLock<u32> = OnceLock::new();
    *FACTOR.get_or_init(|| {
        std::env::var("NEOVM_JIT_PROFIT_DEFER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::PROFIT_DEFER_FACTOR)
    })
}

#[cfg(test)]
std::thread_local! {
    static PROFIT_DEFER_TEST_OVERRIDE: std::cell::Cell<Option<u32>> =
        const { std::cell::Cell::new(None) };
}

/// Test-only: pin [`profit_defer_factor`] for this thread.
#[cfg(test)]
pub(crate) fn force_profit_defer_for_test(factor: Option<u32>) {
    PROFIT_DEFER_TEST_OVERRIDE.with(|c| c.set(factor));
}

/// `retier_heat` cache: `0` = not read yet, `u64::MAX` = no re-tier crossing,
/// else `1 + heat`. The env var it derives from cannot change under us, so a
/// racing double read resolves to the same value.
static RETIER_HEAT_CACHE: AtomicU64 = AtomicU64::new(0);

/// Heat at which a fast-allocator leaf is rebuilt with the full allocator
/// ([`RuntimeState::RETIER_FACTOR`] × [`hot_threshold`]); `None` = never
/// (`NEOVM_JIT_RETIER_FACTOR=0`). Under `NEOVM_JIT_TIER2=on` there is no
/// heat crossing either: the T1 leaf's countdown requests the re-tier
/// instead (`tier2`).
#[inline]
pub fn retier_heat() -> Option<u32> {
    #[cfg(all(test, feature = "jit"))]
    if let Some(at) = retier_heat_test_override() {
        return at;
    }
    // A plain relaxed load rather than a `OnceLock<Option<u32>>` read: the
    // armed-leaf entry consults this on EVERY compiled call, and there a
    // `OnceLock` costs its initialized-flag branch plus an acquire fence.
    match RETIER_HEAT_CACHE.load(Ordering::Relaxed) {
        0 => retier_heat_init(),
        u64::MAX => None,
        at => Some((at - 1) as u32),
    }
}

/// `NEOVM_JIT_RETIER_FACTOR` ([`RuntimeState::RETIER_FACTOR`] unset); `0`
/// turns the re-tier off, under either trigger.
pub fn retier_factor() -> u32 {
    static FACTOR: OnceLock<u32> = OnceLock::new();
    *FACTOR.get_or_init(|| {
        std::env::var("NEOVM_JIT_RETIER_FACTOR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::RETIER_FACTOR)
    })
}

/// The heat crossing for a process with `factor` and the tier spine `t2`.
fn retier_heat_for(factor: u32, t2: bool) -> Option<u32> {
    (factor != 0 && !t2).then(|| hot_threshold().saturating_mul(factor))
}

/// Whether the tier spine owns the re-tier (`NEOVM_JIT_TIER2`).
fn tier2_on() -> bool {
    #[cfg(feature = "jit")]
    {
        compile::jit_tier2().on
    }
    #[cfg(not(feature = "jit"))]
    {
        false
    }
}

/// A test thread that forced the tier-spine knobs answers from them, not
/// from the process cache.
#[cfg(all(test, feature = "jit"))]
fn retier_heat_test_override() -> Option<Option<u32>> {
    compile::tier2_forced_for_test().map(|knob| retier_heat_for(retier_factor(), knob.on))
}

#[cold]
#[inline(never)]
fn retier_heat_init() -> Option<u32> {
    let at = retier_heat_for(retier_factor(), tier2_on());
    RETIER_HEAT_CACHE.store(
        at.map_or(u64::MAX, |heat| u64::from(heat) + 1),
        Ordering::Relaxed,
    );
    at
}

/// Largest body (in ops) the JIT tiers up at all; bigger bodies stay on the
/// interpreter. Defaults to [`RuntimeState::MAX_TIER_OPS`]; `NEOVM_JIT_MAX_OPS`
/// overrides it (`0` = no cap).
pub fn max_tier_ops() -> u32 {
    static CAP: OnceLock<u32> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("NEOVM_JIT_MAX_OPS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::MAX_TIER_OPS)
    })
}

pub fn hot_threshold() -> u32 {
    static THRESHOLD: OnceLock<u32> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("NEOVM_JIT_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Runtime::HOT_THRESHOLD)
    })
}

/// Heat credited per backward-branch *wrap* — one wrap is
/// [`LOOP_BACKEDGES_PER_WRAP`] (256) loop iterations, so this weights 256 loop
/// iterations as ≈ one function invocation (`dispatch` credits +1 per call).
/// This is the tier-up signal for a body dominated by a long INNER LOOP but
/// called only a handful of times: `dispatch` alone would never make it hot
/// (heat counts calls), so a hot loop in a rarely-called function stayed in the
/// interpreter forever. `NEOVM_JIT_LOOP_HEAT` overrides it; **`=0` disables
/// loop heat** — the pre-loop-heat behavior and the A/B baseline.
pub fn loop_heat_per_wrap() -> u32 {
    static V: OnceLock<u32> = OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("NEOVM_JIT_LOOP_HEAT")
            .ok()
            .and_then(|s| s.parse().ok())
            // 8 ⇒ 32 loop iterations ≈ one invocation ⇒ a loop tiers up (and
            // OSR fires) near 32k total iterations. The old credit of 1
            // needed 256k iterations — a 60k-iteration hot loop in a
            // once-called function (the realworld buffer bench's insert and
            // scan loops) never left the interpreter.
            .unwrap_or(8)
    })
}

/// Whether the JIT is active at runtime. The `jit` cargo feature compiles the
/// JIT *in*; this switch turns tier-up on/off WITHOUT a recompile, so a single
/// binary can run pure-interpreter or JIT-backed. Default on; `NEOVM_JIT=0`
/// (also `off`/`false`/`no`) forces the interpreter — a kill switch and the
/// A/B-measurement knob (no more `NEOVM_JIT_THRESHOLD=<huge>` hack).
pub fn jit_runtime_enabled() -> bool {
    #[cfg(test)]
    if jit_forced_off_for_test() {
        return false;
    }
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
std::thread_local! {
    static JIT_OFF_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run this thread as `NEOVM_JIT=0` would (tests only): no tier-up, no OSR,
/// and every new `Vm` interprets. For tests that compare interpreter runs
/// op for op, where a callee turning hot between runs would change the
/// trace.
#[cfg(test)]
pub fn force_jit_off_for_test(off: bool) {
    JIT_OFF_FOR_TEST.with(|c| c.set(off));
}

#[cfg(test)]
pub(crate) fn jit_forced_off_for_test() -> bool {
    JIT_OFF_FOR_TEST.with(|c| c.get())
}

#[cfg(test)]
std::thread_local! {
    static CALL_FEEDBACK_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force per-call feedback collection on/off on the current thread (tests
/// only): `NEOVM_JIT_FEEDBACK=record` or `off`.
#[cfg(test)]
pub fn force_call_feedback_for_test(on: bool) {
    CALL_FEEDBACK_TEST_OVERRIDE.with(|c| c.set(Some(on)));
    feedback::force_feedback_mode_for_test(Some(if on {
        feedback::FeedbackMode::Record
    } else {
        feedback::FeedbackMode::Off
    }));
}

/// Whether the VM records per-call-site target feedback (`record_call`) on the
/// `Op::Call` hot path.
///
/// **Default OFF.** The feedback vector ([`CallFeedback`] / [`FeedbackVec`]) is
/// the optimizing tier's most important input — `Monomorphic(sym)` is what a
/// future MIR tier turns into a direct/inlined call. But NO tier consumes it
/// today: ordinary bytecode-to-bytecode calls never reach the compile seam
/// (`dispatch_sized` sees ~0.15% of calls), so recording a callee at every call
/// is pure overhead — measured +7.4% Ir / +14.2% cycles on the 3M-call
/// microbenchmark and 3–5% Ir on org-editing, feeding a decision nothing reads.
///
/// This gate stops the *collection*, not the mechanism: `record_call`,
/// `call_at`, and the whole `CallFeedback` lattice are retained unchanged. When
/// the consuming tier is wired, flip this default (or gate it on that tier being
/// active) and the feedback flows again. `NEOVM_JIT_CALL_FEEDBACK=on` re-enables
/// it now for A/B measurement and for the feedback tests.
/// ISOLATION KNOB (measurement only). `NEOVM_JIT_BCALL_TIER=off`: on the
/// adaptive policy, `Op::Call` no longer consults the tier dispatcher
/// (`dispatch_bytecode_call_from_stack` -> `dispatch_sized`: one function call
/// plus the heat atomics and threshold arithmetic) -- it interprets directly,
/// exactly as the interpreter-only policy does at that site, while the monomorphic
/// call cache still stays UNPOPULATED. Isolates the dispatcher's per-call cost
/// from the cache-miss cost. Functions called only through Bcall can no longer
/// tier up while this is set.
pub fn jit_bcall_tier_skipped() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("NEOVM_JIT_BCALL_TIER").as_deref() == Ok("off"))
}

/// ISOLATION KNOB (measurement only). `NEOVM_JIT_BCALL_CACHE=on`: on the
/// adaptive policy, the call-target resolver populates the one-entry
/// monomorphic cache (`RecentInterpreterCall`) for iteratively-enterable
/// bytecode callees, as the interpreter-only policy does, so the repeated call
/// takes the cached fast path and reaches neither the resolver nor the tier
/// dispatcher. Isolates the cache-miss + re-resolve cost. Cached callees stop
/// accumulating Bcall heat while this is set.
pub fn jit_bcall_cache_forced() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("NEOVM_JIT_BCALL_CACHE").as_deref() == Ok("on"))
}

///
/// Since P2.1 C3 this is `NEOVM_JIT_FEEDBACK` ([`feedback::feedback_mode`]),
/// with `NEOVM_JIT_CALL_FEEDBACK=on` its alias for `record`; the interpreter
/// records the targets of its NON-constant call sites into the source's
/// call-site table ([`feedback::CallSites`]), not this per-op word.
#[inline]
pub fn call_feedback_collection_enabled() -> bool {
    #[cfg(test)]
    if let Some(o) = CALL_FEEDBACK_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    feedback::feedback_mode().records()
}

/// OSR (on-stack replacement): transfer a hot loop in a rarely-/once-called
/// function into native code MID-execution (the case loop-heat's next-entry
/// tier-up cannot reach). Default ON since the `mod` arith-intrinsic made the
/// transferred loop a measured win on builtin-call-bearing bodies (list
/// workload −25% wall; the shimmed-builtin overhead previously ate the
/// transfer's gain — the reason this started life opt-in). Kill switch:
/// `NEOVM_JIT_OSR=off` (same spelling family as `NEOVM_JIT`); the interpreter
/// marshals its live operand and binding stacks into a native OSR entry.
/// Handler/save operations and nonlexical functions remain ineligible.
/// Off ⇒ the back-edge stays a pure interpreter loop, zero added cost.
pub fn jit_osr_on() -> bool {
    #[cfg(test)]
    if let Some(o) = OSR_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_OSR").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
thread_local! {
    static OSR_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force OSR on/off on the current thread (tests only), overriding the env gate.
#[cfg(test)]
pub fn force_osr_for_test(on: bool) {
    OSR_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

impl RuntimeState {
    /// Invocations before a function is "hot" enough to tier up.
    ///
    /// Tuned via `jit_bench_threshold_economics` (eval_test.rs), an
    /// interleaved debug-build A/B of 1_000 vs the previous placeholder
    /// 10_000 across 1.2k/3k/20k-call workloads: 1_000 halves end-to-end
    /// wall time for the 3k-20k call population (the functions a 10_000
    /// threshold strands in the interpreter forever) and only regresses
    /// ~1.2k-call functions by the one-time compile cost — which a debug
    /// build heavily inflates, so the release regression is smaller still.
    /// Compilation is the only cost lowering adds; going far lower (100)
    /// starts compiling barely-warm functions for no amortized win.
    /// `NEOVM_JIT_THRESHOLD` still overrides per process.
    pub const HOT_THRESHOLD: u32 = 1_000;

    #[inline]
    pub const fn new() -> Self {
        Self {
            heat: AtomicU32::new(0),
            native_rejected_epoch: AtomicU64::new(0),
            profit_deferred_heat: AtomicU32::new(0),
            feedback: feedback::SourceFeedback::new(),
            compiled_id: AtomicU64::new(0),
            aot_prewarmed: std::sync::atomic::AtomicBool::new(false),
            numeric_feedback_consumed: std::sync::atomic::AtomicBool::new(false),
            leaf_slot: AtomicU64::new(0),
            leaf_slot_epoch: AtomicU64::new(0),
            patched_prefix: AtomicU32::new(0),
            reopt_count: std::sync::atomic::AtomicU8::new(0),
            t2_reopts: std::sync::atomic::AtomicU8::new(0),
            aot_retiered: std::sync::atomic::AtomicBool::new(false),
            reopt_level: std::sync::atomic::AtomicU8::new(0),
            site_retreat: retreat::SiteRetreatTable::new(),
            #[cfg(test)]
            force_interpret: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Number of leading constant slots that are per-instance (`make-closure`
    /// patched) for this source; 0 for a plain function.
    #[inline]
    pub fn patched_prefix(&self) -> usize {
        self.patched_prefix.load(Ordering::Relaxed) as usize
    }

    /// Record a `make-closure` patch of width `n`. Returns `Some(compiled_id)`
    /// when the recorded prefix GREW while a compiled id was already assigned:
    /// any leaf compiled for that id assumed the narrower prefix (it may have
    /// baked a slot that is now per-instance) and must be evicted by the
    /// caller before the next dispatch.
    ///
    /// Every instance of a prototype patches the same width, so the common
    /// case is a plain load that finds the prefix already recorded; only a
    /// widening pays the read-modify-write.
    #[inline]
    pub fn note_patched_prefix(&self, n: usize) -> Option<u64> {
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        if n <= self.patched_prefix.load(Ordering::Relaxed) {
            return None;
        }
        let prev = self.patched_prefix.fetch_max(n, Ordering::Relaxed);
        if n > prev {
            // The caller evicts the cached verdict for this id; forget ours
            // too, so the next dispatch re-consults the cache like before.
            self.native_rejected_epoch.store(0, Ordering::Relaxed);
            self.compiled_id()
        } else {
            None
        }
    }

    /// Defer this body's tier-up to `heat` (see `profit_deferred_heat`).
    pub(crate) fn defer_tier_up(&self, heat: u32) {
        self.profit_deferred_heat.store(heat, Ordering::Relaxed);
    }

    /// The heat this body's tier-up is deferred to (0 = not deferred).
    pub(crate) fn deferred_heat(&self) -> u32 {
        self.profit_deferred_heat.load(Ordering::Relaxed)
    }

    /// Whether a profitability deferral is still holding at heat `now`.
    #[inline]
    fn tier_up_deferred(&self, now: u32) -> bool {
        let at = self.profit_deferred_heat.load(Ordering::Relaxed);
        at != 0 && now < at
    }

    /// Whether this body's deferral has run out: it was deferred and its heat
    /// has reached the deferral point, so the next compile bypasses the gate.
    pub(crate) fn profit_deferral_expired(&self) -> bool {
        let at = self.profit_deferred_heat.load(Ordering::Relaxed);
        at != 0 && self.heat() >= at
    }

    /// Record that the JIT rejected this body (`CacheEntry::NotCompilable`)
    /// under NotCompilable generation `epoch` — see `native_rejected_epoch`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn mark_native_rejected(&self, epoch: u64) {
        self.native_rejected_epoch.store(epoch, Ordering::Relaxed);
    }

    /// Whether a remembered `NotCompilable` verdict is still current, i.e. a
    /// cache probe would only re-find it.
    #[inline]
    fn native_rejected(&self) -> bool {
        #[cfg(feature = "jit")]
        {
            let stamped = self.native_rejected_epoch.load(Ordering::Relaxed);
            stamped != 0 && stamped == super::jit::cache::rejection_epoch()
        }
        #[cfg(not(feature = "jit"))]
        {
            false
        }
    }

    /// Record one invocation and decide how to run it. The caller MUST handle
    /// the returned [`Plan`] exhaustively.
    ///
    /// Counts the invocation and returns [`Plan::Compiled`] once the function
    /// crosses [`hot_threshold`] (default [`Runtime::HOT_THRESHOLD`]), else
    /// [`Plan::Interpret`]. The compiled plan only means "the JIT may run this"
    /// — the cache still falls back to the interpreter on deopt, and a body
    /// the cache has rejected as `NotCompilable` is remembered here
    /// (`native_rejected_epoch`) so it answers `Interpret` without the trip.
    /// Default [`size_unit`]: bodies up to this many ops tier at the flat
    /// `hot_threshold()`; larger ones need proportionally more calls. Tuned on
    /// the fontify gate (font-lock closures now tier since `make-closure`
    /// instances share heat): a 352-op keyword matcher cost ~80 ms / ~135M Ir
    /// to compile and ran break-even natively, so a flat threshold paid the
    /// whole compile inside one fontification for nothing. V8's interrupt
    /// budget is the precedent for scaling tier-up by bytecode length.
    ///
    /// 512 since 2026-09-15 (was 64), with [`Self::MAX_TIER_OPS`] raised to
    /// 4096: the byte compiler's workhorses are big and called tens of times
    /// per file — `byte-optimize-lapcode` is 2,528 ops and ~64 calls, which at
    /// 64 ops a unit needed 39,000 calls, and `byte-compile-out-toplevel` (288
    /// ops) was over the old cap — so they never left the interpreter. elb
    /// bytecomp -11.5% instructions (50 compiles: 19.83G -> 17.54G, GNU
    /// 16.0G), org-editing -3.7%, org-editing-heavy -0.7%; the other ELB rows
    /// within noise; a 3-file compile pays +4.7% in compile time.
    pub const SIZE_UNIT: u32 = 512;

    /// Default [`retier_heat`] factor: a leaf compiled with the fast register
    /// allocator (`lowering::RegallocChoice::Fast`) is rebuilt with the full
    /// one once its heat reaches this many [`hot_threshold`]s. 16 = 16,000
    /// calls at the default threshold: the call-heavy benchmark (3M calls)
    /// spends 0.5% of them on the fast code; an editing session's leaves
    /// (hundreds to a few thousand calls) never pay the second compile.
    pub const RETIER_FACTOR: u32 = 16;

    /// Default [`profit_defer_factor`]: `0` keeps the profitability gate a
    /// veto (a refused body never compiles); `K` defers the compile to
    /// `K × hot_threshold()` calls instead.
    ///
    /// Chosen by same-binary sweeps (2026-09-05, instructions, medians of 3,
    /// every run checked for exit status and output; `tmp/rr/wf2/ab-k.sh`),
    /// each arm vs the veto, with call-heavy bodies on the fast allocator:
    ///
    /// | fixture                      | K=2    | K=4    | K=8    |
    /// |------------------------------|--------|--------|--------|
    /// | org editing, 5 passes        | −0.05% | −0.13% | −0.23% |
    /// | org editing, 25 passes       | −1.71% | −1.58% | −1.58% |
    /// | org editing, 50 passes       | −3.36% | −3.18% | −2.50% |
    /// | byte-compile cc-engine.el    | −0.86% | −1.13% | −0.72% |
    /// | 3M-call benchmark            | +0.00% | +0.00% | −0.00% |
    /// | 200-function compile fixture | +0.61% | +0.48% | +0.31% |
    ///
    /// A call-heavy body runs faster native (~830 instructions per entry on
    /// org) but its compile is dear, so admitting it at the flat threshold
    /// (gate off) wins in long sessions and loses in short ones: org 5 passes
    /// +3.1%, 50 passes −7.0%, the compile fixture +10.6%. 4 takes most of
    /// the long-session win and the best byte-compile point while every
    /// short session stays within half a percent of the veto.
    pub const PROFIT_DEFER_FACTOR: u32 = 4;

    /// Default [`max_tier_ops`].
    ///
    /// Originally (2026-08-28) the same 352-op font-lock matcher cost ~80 ms
    /// to compile for break-even native code, so the cap was a compile-stall
    /// guard. Re-measured 2026-08-31: compile cost is now effectively linear
    /// (~3.5 µs/op on loop-shaped bodies, 20 µs/op on branch/deopt-heavy
    /// matchers; that matcher compiles in 7.15 ms, whole type-sim compile
    /// total 12.6 ms).
    ///
    /// CORRECTED 2026-09-01: the old "break-even with the interpreter"
    /// finding was an artifact — the "interpreted" halves of those A/Bs ran
    /// ~99.99% NATIVE, because OSR ignores the cap (this gate covers only
    /// entry dispatch) and, in benches, ignored `force_interpret` (fixed:
    /// `is_hot` now honors it). With an honest Tier-0 baseline the >256-op
    /// fixture (`jit_bench_big_body_matcher_shape`) runs 12.6x FASTER
    /// native. The cap's practical effect is therefore only to delay the
    /// ENTRY tier for big bodies whose loops OSR anyway; lifting it is
    /// pending a real-workload A/B (byte-compile watch per the GATE_RELAX
    /// precedent) — see the mid-end campaign notes.
    ///
    /// That A/B (2026-09-15, see [`Self::SIZE_UNIT`]) raised it to 4096: a
    /// stall guard, not a profit gate, sized to admit the byte compiler's
    /// 2,528-op `byte-optimize-lapcode`.
    pub const MAX_TIER_OPS: u32 = 4096;

    /// [`dispatch`](Self::dispatch) with the tier-up budget scaled by the body
    /// size (`ops_len`): bodies above [`max_tier_ops`] never tier, and the hot
    /// threshold is multiplied by `max(1, ops_len / size_unit())`. The seam
    /// call sites use this; the unsized `dispatch` is the flat rule (tests,
    /// tiny bodies).
    #[inline]
    pub fn dispatch_sized(&self, ops_len: usize) -> Plan {
        if !jit_runtime_enabled() {
            return Plan::Interpret;
        }
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return Plan::Interpret;
        }
        let prev = self.heat.load(Ordering::Relaxed);
        let now = prev.saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        if self.aot_prewarmed.load(Ordering::Relaxed) {
            return Plan::Compiled;
        }
        if self.native_rejected() || self.tier_up_deferred(now) {
            #[cfg(feature = "jit")]
            super::jit::stats::record_dispatch(false);
            return Plan::Interpret;
        }
        // A body the profit gate refused tiers up exactly at its deferral
        // heat. It already proved hot once — usually on first sight from
        // compiled code, where the seam compiles with no threshold at all —
        // so the size-scaled first-sight threshold below must not apply
        // again: for the 200–800-op font-lock bodies it meant 5–12K more
        // calls, i.e. never within a session (org op −7.6% Ir when they run
        // native).
        if self.profit_deferred_heat.load(Ordering::Relaxed) != 0 {
            #[cfg(feature = "jit")]
            super::jit::stats::record_dispatch(true);
            return Plan::Compiled;
        }
        let cap = max_tier_ops();
        if cap != 0 && ops_len > cap as usize {
            return Plan::Interpret;
        }
        let threshold = hot_threshold();
        let unit = size_unit();
        let factor = if unit == 0 {
            1
        } else {
            u32::try_from(ops_len / unit as usize)
                .unwrap_or(u32::MAX)
                .max(1)
        };
        let plan = if now >= threshold.saturating_mul(factor) {
            Plan::Compiled
        } else {
            Plan::Interpret
        };
        #[cfg(feature = "jit")]
        super::jit::stats::record_dispatch(matches!(plan, Plan::Compiled));
        plan
    }

    #[inline]
    pub fn dispatch(&self) -> Plan {
        // Runtime kill switch (NEOVM_JIT=0): never tier up — pure interpreter,
        // no recompile. The early return also skips the heat bump, so a disabled
        // JIT is strictly cheaper than an enabled-but-cold one.
        if !jit_runtime_enabled() {
            return Plan::Interpret;
        }
        // Test-only: a forced-cold function never tiers up (benchmark A/B).
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return Plan::Interpret;
        }
        // Saturating bump — a long-lived hot function must never wrap to cold.
        let prev = self.heat.load(Ordering::Relaxed);
        let now = prev.saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        if self.aot_prewarmed.load(Ordering::Relaxed) {
            Plan::Compiled
        } else if self.native_rejected() || self.tier_up_deferred(now) {
            Plan::Interpret
        } else if now >= hot_threshold() {
            Plan::Compiled
        } else {
            Plan::Interpret
        }
    }

    /// This function's compiled-cache id, assigning a fresh process-unique one
    /// on first call (idempotent under races). Used only by [`cache`].
    #[inline]
    pub fn compiled_id_or_assign(&self) -> u64 {
        let cur = self.compiled_id.load(Ordering::Acquire);
        if cur != 0 {
            return cur;
        }
        // `+ 1` keeps 0 reserved for "unassigned".
        let fresh = NEXT_COMPILED_ID.fetch_add(1, Ordering::Relaxed) + 1;
        match self
            .compiled_id
            .compare_exchange(0, fresh, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => fresh,
            // Another thread won the race; adopt its id, discard ours.
            Err(actual) => actual,
        }
    }

    /// This function's compiled-cache id if one was ALREADY assigned (it has been
    /// compiled/hot), else `None` — WITHOUT assigning a fresh one. The AOT-PGO drain
    /// uses this to intersect the obarray walk with the hot set without minting ids
    /// for the many never-compiled bound functions it walks past.
    #[inline]
    pub fn compiled_id(&self) -> Option<u64> {
        let cur = self.compiled_id.load(Ordering::Acquire);
        (cur != 0).then_some(cur)
    }

    /// True once this function has crossed the tier-up threshold.
    #[inline(always)]
    pub fn is_hot(&self) -> bool {
        // A forced-cold function must never read as hot — the OSR gate
        // consults is_hot() directly, and OSR ignoring force_interpret is
        // how a benchmark's "interpreter" half silently ran ~99.99% native
        // (the Phase-0 big-body baseline was really the baseline-OSR leaf).
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return false;
        }
        self.heat.load(Ordering::Relaxed) >= hot_threshold()
    }

    /// Current invocation count.
    /// Advance the call heat by one (what `dispatch_sized` does before its
    /// tier decision) and return the new value — the direct stack entry keeps
    /// the re-tier trigger honest without the rest of the dispatcher.
    #[cfg(feature = "jit")]
    #[inline(always)]
    pub(crate) fn bump_heat(&self) -> u32 {
        let now = self.heat.load(Ordering::Relaxed).saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        now
    }

    /// The heat WITHOUT advancing it — for a probe that may decline and let
    /// another probe of the same call do the advancing.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn peek_heat(&self) -> u32 {
        self.heat.load(Ordering::Relaxed)
    }

    #[cfg(all(feature = "jit", test))]
    pub(crate) fn force_interpret_for_test(&self) -> bool {
        self.force_interpret.load(Ordering::Relaxed)
    }

    /// The leaf armed for the direct stack entry, if it was armed under
    /// `epoch` (the current `cache::leaf_slot_epoch()`); a stale slot reads
    /// as empty and is re-armed through the cache by the tier-up entry.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn armed_leaf_slot(&self, epoch: u64) -> Option<*const compile::CompiledLeaf> {
        let ptr = self.leaf_slot.load(Ordering::Relaxed);
        (ptr != 0 && self.leaf_slot_epoch.load(Ordering::Relaxed) == epoch)
            .then_some(ptr as *const compile::CompiledLeaf)
    }

    /// Arm the direct stack entry with a leaf resolved from the cache under
    /// `epoch` on the same mutator that later probes it. These Relaxed stores
    /// do not publish a consistent pair to other mutators; the cache owner and
    /// its Cell-based deopt storage remain thread-confined (P7.11).
    #[cfg(feature = "jit")]
    #[inline(always)]
    pub(crate) fn arm_leaf_slot(&self, leaf: *const compile::CompiledLeaf, epoch: u64) {
        self.leaf_slot_epoch.store(epoch, Ordering::Relaxed);
        self.leaf_slot.store(leaf as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn heat(&self) -> u32 {
        self.heat.load(Ordering::Relaxed)
    }

    /// Backward branches per loop-heat wrap — the interpreter's `branch_to!`
    /// quit counter is a `u8`, so it wraps (and calls [`note_loop_work`]) once
    /// per 256 backward branches. Documented here so the loop-heat weighting
    /// (256 iterations ≈ one call) is discoverable from the `Runtime` API.
    ///
    /// [`note_loop_work`]: Self::note_loop_work
    pub const LOOP_BACKEDGES_PER_WRAP: u32 = 256;

    /// Credit loop work toward tier-up: called from the interpreter's
    /// backward-branch quit-counter wrap (`bytecode/vm.rs`), i.e. once per
    /// [`LOOP_BACKEDGES_PER_WRAP`](Self::LOOP_BACKEDGES_PER_WRAP) iterations, so
    /// the per-iteration cost is amortized to ~nothing. A function whose body is
    /// a long INNER LOOP but which is CALLED only a few times never crosses
    /// [`hot_threshold`] on `dispatch`'s per-call bump alone; this accumulates
    /// heat from the loop itself so the NEXT entry tiers it up. The CURRENT
    /// interpreted call still runs to completion in Tier 0 — there is no
    /// on-stack replacement, so a body called exactly once sees no benefit
    /// (that is the OSR follow-up). Saturating: a long-lived loop must never
    /// wrap heat back to cold. Respects the [`jit_runtime_enabled`] kill switch
    /// and the `NEOVM_JIT_LOOP_HEAT=0` off knob.
    #[inline]
    pub fn note_loop_work(&self) {
        let credit = loop_heat_per_wrap();
        if credit == 0 || !jit_runtime_enabled() {
            return;
        }
        let prev = self.heat.load(Ordering::Relaxed);
        self.heat
            .store(prev.saturating_add(credit), Ordering::Relaxed);
    }

    /// Test-only: force this function "hot" so the next [`dispatch`](Self::dispatch)
    /// tiers it up, without driving `HOT_THRESHOLD` real invocations.
    /// Mark this function as served by a prepopulated AOT leaf (see the
    /// field doc): `dispatch` returns `Plan::Compiled` from call 1.
    pub(crate) fn mark_aot_prewarmed(&self) {
        self.aot_prewarmed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether this function is marked to run a preload leaf from call 1.
    pub(crate) fn is_aot_prewarmed(&self) -> bool {
        self.aot_prewarmed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn set_hot_for_test(&self) {
        self.heat.store(Self::HOT_THRESHOLD, Ordering::Relaxed);
    }

    /// Test-only: set the heat outright (re-tier tests).
    #[cfg(test)]
    pub(crate) fn set_heat_for_test(&self, heat: u32) {
        self.heat.store(heat, Ordering::Relaxed);
    }

    /// Test-only: pin this function to the Tier-0 interpreter forever (the
    /// forced-cold half of the benchmark A/B; see `force_interpret`).
    #[cfg(test)]
    pub(crate) fn set_cold_for_test(&self) {
        self.force_interpret
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record an observed callee `sym` at the call site at instruction `pc`
    /// (`ops_len` = the function's instruction count, for lazy sizing).
    #[inline]
    pub fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        self.feedback.record_call(pc, ops_len, sym);
    }

    /// Call-site feedback observed at instruction `pc`.
    #[inline]
    pub fn call_feedback(&self, pc: usize) -> CallFeedback {
        self.feedback.call_at(pc)
    }

    /// Record the operand types seen at the arithmetic site at instruction
    /// `pc` — see [`NumericFeedback`]. Returns whether the site's slot moved.
    #[inline]
    pub fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) -> bool {
        self.feedback.record_numeric(pc, ops_len, seen)
    }

    /// Operand-type feedback observed at arithmetic site `pc`.
    #[inline]
    pub fn numeric_feedback(&self, pc: usize) -> NumericFeedback {
        self.feedback.numeric_at(pc)
    }

    /// Whether this body's arithmetic sites still want their operand types
    /// recorded — see `numeric_feedback_consumed`.
    #[inline]
    pub fn wants_numeric_feedback(&self) -> bool {
        !self
            .numeric_feedback_consumed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark this body's numeric feedback as read by a compile.
    #[inline]
    pub fn note_numeric_feedback_consumed(&self) {
        self.numeric_feedback_consumed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // --- Deopt-driven reoptimization state (`jit::reopt`). Relaxed atomics:
    // the mutator is the only writer, as for every other field. ---

    /// The ceiling every later compile of this source respects.
    #[inline]
    pub fn reopt_level(&self) -> ReoptLevel {
        ReoptLevel::from_u8(self.reopt_level.load(Ordering::Relaxed))
    }

    /// Deopt-driven invalidations of this source so far (saturating).
    #[inline]
    pub fn reopt_count(&self) -> u8 {
        self.reopt_count.load(Ordering::Relaxed)
    }

    /// Record one invalidation whose cause asks for at least `floor`, and
    /// return the level later compiles must respect. Past `max_reopts`
    /// invalidations each further one climbs one level, so a source can be
    /// invalidated at most `max_reopts + 4` times before it is interpreted
    /// for good.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn note_reopt(&self, floor: ReoptLevel, max_reopts: u8) -> ReoptLevel {
        let count = self.reopt_count().saturating_add(1);
        self.reopt_count.store(count, Ordering::Relaxed);
        let cur = self.reopt_level();
        let mut next = cur.max(floor);
        if count > max_reopts {
            next = next.max(cur.next());
        }
        self.reopt_level.store(next as u8, Ordering::Relaxed);
        next
    }

    /// Make the interpreter record numeric feedback again, until the next
    /// compile consumes it.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn reopen_numeric_feedback(&self) {
        self.numeric_feedback_consumed
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Join `seen` into arithmetic site `pc`'s feedback (what a deopt proved
    /// the site meets). Returns whether the site's slot moved.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn widen_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) -> bool {
        self.feedback.record_numeric(pc, ops_len, seen)
    }

    /// Forbid splicing, MIR-inlining or intrinsifying the call site at `pc`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn mark_call_site_no_inline(&self, pc: usize, ops_len: usize) {
        if let Some(site) = self.site_retreat.site(pc, ops_len) {
            site.set(retreat::RetreatBit::NoInline);
        }
    }

    /// Test-only: set the reoptimization ceiling outright.
    #[cfg(test)]
    pub(crate) fn set_reopt_level_for_test(&self, level: ReoptLevel) {
        self.reopt_level.store(level as u8, Ordering::Relaxed);
    }

    /// Whether a deopt forbade inlining the call site at `pc`.
    #[inline]
    pub(crate) fn call_site_no_inline(&self, pc: usize) -> bool {
        self.site_retreat.has(pc, retreat::RetreatBit::NoInline)
    }

    /// Count one precise deopt at `pc` in this source's persistent deopt
    /// history (P2.1 C2); returns the new count (saturating at 255, 0 for a
    /// pc outside the body the table was sized for).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn note_deopt_history(&self, pc: usize, ops_len: usize) -> u8 {
        self.site_retreat
            .site(pc, ops_len)
            .map_or(0, retreat::SiteRetreat::note_deopt)
    }

    /// Precise deopts ever counted at `pc` of this source, across all of its
    /// leaves (0 before the first; a read never allocates).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumer: the T2 retreat (P2.1 C9-C11)")
    )]
    #[inline]
    pub(crate) fn deopt_history(&self, pc: usize) -> u8 {
        self.site_retreat
            .get(pc)
            .map_or(0, retreat::SiteRetreat::count)
    }

    /// Empty the interpreter's direct-entry leaf slot: only this source's
    /// slot can hold this source's leaf, so an invalidation disarms it here
    /// instead of bumping the global `cache::leaf_slot_epoch`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn disarm_leaf_slot(&self) {
        self.leaf_slot.store(0, Ordering::Relaxed);
    }

    /// Stop serving this source as AOT-prewarmed: `dispatch_sized` would
    /// otherwise answer `Compiled` from call 1 and skip a re-profile window.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn clear_aot_prewarmed(&self) {
        self.aot_prewarmed.store(false, Ordering::Relaxed);
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

/// The per-function handle to [`RuntimeState`], living inline on
/// `ByteCodeFunction` (only when the `jit` feature is on). One pointer; derefs
/// to the shared state.
///
/// SHARED ACROSS `make-closure` INSTANCES (cite-and-overturn of the earlier
/// "a cloned function starts cold — profiling is per-instance" rule, which
/// `ByteCodeFunction::clone` enforced by resetting this to `Runtime::new()`):
/// `make-closure` clones the prototype for EVERY closure instantiation, so
/// per-instance heat meant closure-shaped code — font-lock keyword lambdas,
/// jit-lock, hooks, i.e. interactive editing — never accumulated heat and never
/// tiered, while a threshold-1 soak compiled 11.6K distinct instances of ~500
/// sources. The clone IS faithful to GNU (`Fmake_closure` memcpys the whole
/// prototype vector too); the divergence was hanging mutable tiering state on
/// the object being copied. Sharing the state by SOURCE (the same identity
/// `source_id` already preserves through `make-closure`) is the GNU-shaped
/// fix: heat, feedback, compiled id AND the patched-prefix record all ride the
/// same handle, so heat and compiled artifact are shared TOGETHER — sharing
/// heat alone would make every instance tier at once and compile its own copy
/// (the `NEOVM_JIT_GATE_RELAX` 21%-slower byte-compile precedent).
#[derive(Debug)]
pub struct Runtime {
    shared: std::sync::Arc<RuntimeState>,
}

impl Runtime {
    pub const HOT_THRESHOLD: u32 = RuntimeState::HOT_THRESHOLD;

    /// A fresh, cold, unshared state — for a NEW source (reader, decoder,
    /// `make-byte-code`, pdump restore). Clones share instead.
    #[inline]
    pub fn new() -> Self {
        Self {
            shared: std::sync::Arc::new(RuntimeState::new()),
        }
    }
}

impl std::ops::Deref for Runtime {
    type Target = RuntimeState;
    #[inline]
    fn deref(&self) -> &RuntimeState {
        &self.shared
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Runtime {
    /// A clone SHARES the source's state (see the type docs) — a
    /// `make-closure` instance inherits the prototype's heat, feedback and
    /// compiled leaf, and contributes its own calls to them.
    fn clone(&self) -> Self {
        Self {
            shared: std::sync::Arc::clone(&self.shared),
        }
    }
}

#[cfg(test)]
#[path = "tests/jit_test.rs"]
mod tests;
