# Buffer-Manager View Boundary & Ratchet Design (#844 / rt A3 2c (d))

## 1. Invariant Being Protected (Stop-Exclusion Anchor)

`BufferManager` owns the `ColdWorldLease` and per-context buffer selection for
its `World`; `EditRuntime` embeds `BufferManager` inline at field offset 0
(`#[repr(C)]`, layout-pinned by `context_layout_test` for JIT offsets).
Handing out a raw `&mut BufferManager` or `&mut EditRuntime` lets safe code
execute `std::mem::swap` across contexts (or `std::mem::replace` with a fresh
`BufferManager::new()`), cross-linking `World`-owned storage leases and
marker/overlay ownership. Likewise, safe `&mut Buffer` escapes permit
`std::mem::replace` on a buffer element.

`BufferManagerMut<'a>` and `EditRuntimeMut<'a>`
(`crates/neovm-edit/src/buffer/manager_view.rs`) enforce the non-swappable
capability surface:

- Private `manager: &'a mut BufferManager` / `runtime: &'a mut EditRuntime`
  fields.
- Read-only `impl Deref` (`&self -> &Self::Target`), deliberately omitting
  `DerefMut`, `AsMut`, `BorrowMut`, `IndexMut`, and any safe raw unwrap.
- `!Send + !Sync + !Copy + !Clone` (`static_assertions::assert_not_impl_any!`).
- Single `unsafe` closure-scoped escape hatch
  (`BufferManagerMut::with_buffer_mut_no_swap`), capped at **0** production
  call sites (`MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS = 0`).

## 2. Measured Workspace Call-Site Baseline (`c2ab78b0e9`, 2026-11-26)

Multi-line whole-file scan (`\.edit\s*\.\s*buffers`, `\.edit`, and raw
accessors) measured on `da4a1d4e58` and verified to have **0 delta lines**
across `da4a1d4e58..c2ab78b0e9` (landed 77i tip):

| Surface | Production sites | Files | Breakdown |
| --- | ---: | ---: | --- |
| Direct `.edit.buffers` (all) | **1,281** | 109 | `neovm-eval` 698, `neovm-core` 241, `neovm-system` 170, `neovm-display` 169, `neomacs-layout-engine` 2, `neomacs` 1 (`>75%` are harmless shared reads) |
| Direct `&mut *.edit` / `&mut *.edit.buffers` | **137** | — | `neovm-eval` 110; **only 27 outside `neovm-eval`** (`neovm-core` 21, `neovm-display` 4, `neovm-system` 2) |
| Test / bench `.edit.buffers` | **3,118** | 184 | `neovm-core` 2,054, `neovm-display` 724, `neovm-eval` 334, `neomacs-layout-engine` 6 |
| 7 `Context` split-borrow accessors (retired in Slice 2b) | **229** | — | `neovm-display` 184, `neovm-core` 36, `neovm-eval` 9 |
| `EditHost::edit_and_vars` (remaining 2 entries) | **318** | 12 | `neovm-edit` 311 (incl. 1 default impl in `host/mod.rs:405`), `neovm-text` 7 (`json` 4, `xml` 2, `zlib` 1); sole `impl EditHost` is `Context` (`neovm-eval`) |
| `with_buffer_property_change` (remaining 1 entry) | **6** | 1 | `neovm-edit` 6 (private `fn` inside `textprop/mod.rs`; 0 callers outside that file) |

## 3. Design Horizon — Candidate Boundaries & Why Full Field Privacy Is Deferred

Three compiler-enforced boundary candidates were evaluated against these
measurements:

1. **Candidate A — Sealed non-`pub` fields (`#[repr(C)]` leaf-private
   `EditRuntime.buffers` + `pub(crate) Context.edit`):**
   - *Important Rust constraint:* wrapping a `pub` field in a
     `#[repr(transparent)]` newtype does **not** block `std::mem::swap(&mut
     ctx1.edit.buffers, &mut ctx2.edit.buffers)` — `std::mem::swap` works on
     any `pub` field regardless of wrapper type. True compiler enforcement
     requires making the fields non-`pub`.
   - *Cost:* Because Rust has no `pub(read)` field visibility, making
     `Context.edit` / `EditRuntime.buffers` non-`pub` breaks every harmless
     shared read and disjoint field borrow (`&ctx.edit.buffers` alongside `&mut
     ctx.frames`). Upper-crate-only sealing (`pub(crate) Context.edit` + 7
     split-borrow accessors) costs **~812 prod + 2,784 test sites (~3,600
     sites)** while leaving `neovm-eval` unsealed; full sealing adds
     `neovm-eval` + `edit_and_vars` + `with_buffer_property_change` for
     **~1,834 prod + 3,118 test sites (~4,950 sites across 293 files)**, `>75%`
     of which is pure read-only churn.
   - *Ruling:* **Deferred (not permanently rejected).** Today's cost/benefit
     bar does not justify ~3,600–4,950 mostly read-only rewrites to close ~137
     `&mut` expressions (only 27 outside `neovm-eval`). **Re-open trigger:**
     if a future `ContextParts` borrow-splitting refactor already touches
     `.edit.buffers` read sites across crates, sealing `Context.edit` and
     `EditRuntime.buffers` becomes cheap and should be revisited against this
     table.
2. **Candidate B — Lifetime-branded `BufferManagerMut<'view>` (invariant
   lifetime / rank-2 closure):**
   - *Rejected:* `BufferManagerMut` already omits `DerefMut`, so swapping the
     underlying `BufferManager` already fails with `E0596`. Branding only blocks
     swapping two local view handles, breaks non-closure / tuple split-borrow
     callers (~300+ sites) and `?`/early-return control flow, and does nothing
     against `&mut ctx.edit.buffers` without Candidate A.
3. **Candidate C — `#[must_use]` / sealed capability token on `BufferManager`
   methods:**
   - *Rejected:* `BufferManagerMut` already carries `#[must_use]`. Requiring a
     token on `BufferManager` methods cannot prevent `std::mem::swap` or
     `std::mem::replace`, because neither calls any method on `BufferManager`.

### Staged Migration Plan

- **Slice 2a (landed in 77i as `6e0a88ae82`):** Gate `Context::buffer_manager_mut` behind
  `#[cfg(any(test, feature = "test-support"))]` and enforce production absence
  via fail-closed `toml::Table` + dual-pass `cargo metadata` (`default` +
  `--all-features`) reachability. Raw accessors `11 -> 10`.
- **Slice 2b (this slice):** Retire all 7 raw `Context` split-borrow accessors
  (`frame_manager_and_buffers_mut` and 6 siblings) by migrating all ~229 prod
  call sites (plus tests) to the existing `BufferManagerMut<'_>` split-borrow
  methods and updating `window_markers` / `display_query_hooks` /
  `window_cmds` / `minibuffer` helpers to take `&mut BufferManagerMut<'_>`.
  Zero read-only `.edit.buffers` churn. Raw accessors **`10 -> 3`** (leaving
  only `neovm-edit`-owned `edit_and_vars` [2 entries] and
  `with_buffer_property_change` [1 entry]).
- **Slice 2c (next):** Eliminate the 27 direct `&mut *.edit` / `&mut
  *.edit.buffers` expressions outside `neovm-eval` and add a fail-closed
  expression ratchet capping `&mut *.edit` at `0` outside `neovm-eval` and an
  exact shrink-only constant inside `neovm-eval`.
- **Slice 2d (optional, un-scheduled):** Retire the surviving 3 allowlist
  entries (`EditHost::edit_and_vars` [318 call sites] +
  `with_buffer_property_change` [6 call sites], **`3 -> 0`**) under the
  triggers in §3.1.

### 3.1 Surviving 3 Allowlist Entries: Waypoint (`3 -> 0`), Not a Permanent Floor

**`ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS = 3` is a waypoint, not a permanent
borrow-checker floor.** None of the three survivors needs a borrow that
`EditRuntimeMut<'_>` / `BufferManagerMut<'_>` cannot express; all three are
retirable to `0` without new borrow-splitting machinery:

| Allowlist entry | Kind | Borrow-checker proof of retirability | Why deferred past Slice 2b | Retirement trigger (`3 -> 0`) |
| --- | --- | --- | --- | --- |
| `neovm-edit::host::edit_and_vars` (`EditHost` trait method, `crates/neovm-edit/src/emacs_core/editing/host/mod.rs:400`) + `neovm-eval::edit_host::edit_and_vars` (`impl EditHost for Context`, `crates/neovm-eval/src/emacs_core/runtime/eval/edit_host.rs:430`) | **Waypoint** (2 entries = 1 logical trait seam) | The non-swappable twin `EditHost::edit_view_and_vars(&mut self) -> (EditRuntimeMut<'_>, Self::Vars<'_>)` **already exists** on both the trait (`host/mod.rs:404`) and `impl EditHost for Context` (`edit_host.rs:441`) using the exact same disjoint field borrow (`&mut self.edit` + `VariableView::new(&self.obarray, &self.owner)`). `EditRuntimeMut<'_>` (`manager_view.rs:721-789`) **already** forwards `buffers_mut()` and setters for all 5 `pub(crate)` change-hook/combine-after-change fields on `EditRuntime`. | 318 call sites (311 inside `neovm-edit` — the owner crate of `EditRuntime`/`BufferManager`, many passing `&mut edit.buffers` into crate-internal `&mut BufferManager` helpers — plus 7 in `neovm-text` [`json` 4, `xml` 2, `zlib` 1]; `edit_view_and_vars` has 0 callers today). `Context` is the workspace's sole `impl EditHost`, so exposure outside `neovm-edit` is limited to 7 `neovm-text` call sites. | Retire in optional Slice 2d if either: (a) `EditHost::edit_and_vars` gains callers outside `neovm-edit`/`neovm-text` or a second `impl EditHost` is added; or (b) after Slice 2c lands, we decide to drive `ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS` to `0` by migrating the 7 `neovm-text` + 311 `neovm-edit` sites onto `edit_view_and_vars()` (making `edit_view_and_vars` the required trait method) and deleting `edit_and_vars`. |
| `neovm-edit::textprop::with_buffer_property_change` (private `fn`, `crates/neovm-edit/src/emacs_core/editing/textprop/mod.rs:1630`) | **Waypoint** (subordinate to `edit_and_vars`) | Private helper inside a single file (`0` external callers, `6` call sites in `textprop/mod.rs`). Its `impl FnOnce(&mut BufferManager) -> EvalResult` closure parameter only exists because all 6 callers forward to `textprop`'s `builtin_*_in_buffers(&mut BufferManager, ...)` / `put_text_property_in_buffer_byte_range` helpers, whose sibling string-fallback `else`/`None` arms pass `&mut host.edit_and_vars().0.buffers`. Switching the closure to `&mut BufferManagerMut<'_>` via `host.edit_view_and_vars().0.buffers_mut()` requires no new borrow capability. | Subordinate to `edit_and_vars` + the 6 `builtin_*_in_buffers` signatures inside `textprop/mod.rs` (migrating only `with_buffer_property_change` without its sibling `else`/`None` arms would force duplicating or wrapping those helpers). | Retires automatically alongside `edit_and_vars` in Slice 2d (or in a self-contained `textprop/mod.rs` cleanup that switches those 6 helpers to `&mut BufferManagerMut<'_>`), shrinking the allowlist `3 -> 2` or `3 -> 0`. |

Do **not** add a fourth entry to `ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS`;
any new `Context` or cross-crate split-borrow must return `BufferManagerMut<'_>`
or `EditRuntimeMut<'_>` directly.

## 4. Permanent `syn` Ratchet Blind-Spot List

Because `buffer_manager_view_ratchet_test.rs` is a per-file `syn` AST walker
without name resolution, macro expansion, or type inference, the following
escape classes are permanent syntactic blind spots:

| Escape shape | Why `syn` cannot see it | Second line of defence (observed-failing control vs. human review) |
| --- | --- | --- |
| `impl Deref { type Target = evil::BufferManager }` with interior mutability (`Cell<*mut BufferManager>`) + sibling `DerefMut` handle in `manager_view.rs` | `syn` matches only the last path segment (`BufferManager`); `is_allowed_deref_method` only blocks direct `&mut Self::Target` return from `deref` itself. | Direct `DerefMut` on `BufferManagerMut` is caught by observed-failing `trybuild` `E0596` (`buffer_manager_view_cannot_be_swapped.rs`). **Shadowed `evil::BufferManager` + interior-mutability sibling handle: UNCOVERED — human code review of `manager_view.rs` only.** |
| External/sibling `DerefMut` wrapper (`struct MutHandle<'a>(&'a mut BufferManager)` implementing `DerefMut<Target = BufferManager>`) returned from an inherent method | `FORBIDDEN_VIEW_MUT_TRAITS` only checks impls on `BufferManagerMut`/`EditRuntimeMut`; `deref_mut` returns `&mut Self::Target` (last segment `Target`, not `BufferManager`). | **UNCOVERED today — human code review only.** (Planned Slice 2c's fail-closed `&mut *.edit` expression ratchet will catch constructing `MutHandle(&mut ctx.edit.buffers)` outside `neovm-eval`.) |
| Erased callable (`Box<dyn FnOnce(&mut T)>`) or generic callback forwarding `&mut BufferManager` | `syn` has no type inference or monomorphization and cannot tell whether `T = BufferManager`. | **UNCOVERED — human code review only.** |
| Direct `&mut ctx.edit.buffers` / `&mut ctx.edit` in expression bodies | The signature scanner inspects item/method signatures, not arbitrary expression bodies, and `Context.edit` / `EditRuntime.buffers` remain `pub`. | **UNCOVERED today — human code review only.** (Tracked for Slice 2c.) |
| Macro-generated `impl` / raw accessor outside `manager_view.rs` | `syn` runs pre-macro-expansion and only bans macros unconditionally inside `manager_view.rs` (plus unallowlisted `include!` workspace-wide). | Inside `manager_view.rs`: observed-failing `GuardedImplMacro` / `GuardedFileMacro` (`negative_guarded_*_macro_is_rejected`). **Outside `manager_view.rs`: UNCOVERED — human code review only.** |

## 5. Which Checks Are Allowed to Be Trusted (and How Breakage Surfaces)

Not every check in this repository carries the same trust grade or executes in
the same gate:

1. **Compiler-enforced privacy & `#[cfg]` boundaries (highest trust — runs on
   every `cargo check` / `cargo build`):**
   - `BufferManagerMut` / `EditRuntimeMut` private inner fields, `#[cfg(any(test,
     feature = "test-support"))]` on `Context::buffer_manager_mut` (Slice 2a),
     and deletion of all 7 raw split-borrow methods in `context_api.rs` (Slice
     2b).
   - *What would have to break to miss a regression:* a production dependency
     enabling `neovm-eval/test-support` (guarded by check #3 below) or someone
     re-introducing a raw method/public field.
2. **Compile-fail API contracts (`buffer_manager_view_compile_contracts` via
   `trybuild`, validated by `scripts/check_compile_contracts.py`):**
   - Proves `rustc` rejects `DerefMut`, `into_manager`/`into_runtime`,
     `get_mut`, and un-`unsafe` `with_buffer_mut_no_swap` on `BufferManagerMut`
     / `EditRuntimeMut`.
   - *Where it runs / blind spot:* runs **only** under
     `--features contract-fixtures --test thread_safety_contracts` (one gate
     row, NOT default `cargo nextest run -p neovm-core --lib`). Furthermore,
     `scripts/check_compile_contracts.py --self-test` must itself be run when
     runner counts change — `da4a1d4e58` bumped `EXPECTED` 19 -> 20 without
     running `--self-test` and left `--self-test` red until caught and fixed in
     `01c42e93ca`.
3. **Fail-closed Cargo manifest + resolved-graph audit
   (`no_production_crate_dependencies_enable_neovm_eval_test_support`):**
   - Runs in default `cargo nextest run -p neovm-core --lib`. Combines
     `toml::Table` root/member AST auditing with BOTH `cargo metadata
     --format-version 1 --offline` (default) and `--all-features` reachability
     walks, failing closed on any unresolved enabled non-dev edge, schema
     defect, or `test-support` activation (including `[workspace].exclude` and
     `[patch.crates-io]`).
   - *What would have to break to miss a regression:* a new Cargo feature syntax
     not modeled in our solver, or a non-`test-support` feature name on
     `neovm-eval` gating `buffer_manager_mut`.
4. **Syntactic `syn` ratchet (`buffer_manager_view_surface_and_escape_ratchet`):**
   - Runs in default `cargo nextest run -p neovm-core --lib`.
   - *Trust boundary:* trustworthy for catching **accidental** regressions over
     enumerated AST shapes (new raw `&mut BufferManager`/`&mut EditRuntime`
     accessors, `with_buffer_mut_no_swap` prod calls `> 0`, weakening
     `manager_view.rs`). It is **not** a soundness boundary against the
     permanent blind spots in §4.
