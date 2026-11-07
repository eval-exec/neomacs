# Issue #590: printer termination and GNU compatibility

Investigated 2026-10-25 against NeoMacs baseline `e6822c4c33`. GNU Emacs was studied first at local revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout was used for read-only analysis, respecting its `AGENTS.md`. This note records source findings and design inferences. It does not establish which ETAF cached value caused the reported hang, and does not claim the original application reproduction has been fixed.

## GNU behavior that constrains the fix

The public controls have separate meanings: `print-circle` preserves circular/shared identity with labels; `print-length` limits emitted elements; `print-level` limits nesting according to the printer's rules. `print-continuous-numbering` and `print-number-table` can retain labels across calls. A fix must preserve readable shared structure, rather than merely stop recursion. [GNU output-variable documentation](https://www.gnu.org/software/emacs/manual/html_node/elisp/Output-Variables.html), [pinned manual source](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/doc/lispref/streams.texi#L875-L957), [circular read syntax](https://www.gnu.org/software/emacs/manual/html_node/elisp/Circular-Objects.html).

The pinned GNU implementation establishes these details:

- Printing builds shared-object labels in an iterative preprocessing pass, then removes entries seen only once. Candidate objects are recorded before their children are visited.
- Emission uses tagged continuation records for lists, dotted tails, vectors and hash tables. Flat list traversal includes Brent cycle detection.
- The depth-200 error applies when `print-circle` is nil. It is not a universal limit on circle-enabled values.
- The emission loop checks for quit at each object. An unreadable-object callback temporarily runs with its callback variable bound to nil, preventing accidental callback recursion.
- Obarrays print an opaque count and are excluded from GNU's circle-candidate predicate; their symbol buckets are not traversed for this representation.

[Preprocessing and candidates](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L1292-L1514), [opaque obarrays](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L2126-L2137), [continuations, depth and quit](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L2171-L2332), [unreadable callback](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L1699-L1733).

One compatibility detail was checked with the actual GNU executable, not inferred from variable names:

```elisp
(prin1
 (let ((print-level 0))
   (list (prin1-to-string '((1)))
         (prin1-to-string [[1]]))))
;; => ("..." "[[1]]")
```

The current GNU implementation checks this level limit in the cons branch; vector nesting is not truncated by that probe. A new blanket limit on all container types would change the oracle result. [Pinned cons/vector dispatch](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L2528-L2640).

The probe required the existing development-library environment (`source tmp/issue582-dev-env.sh`, followed by `export TMPDIR="$PWD/tmp"`). No GNU files were changed or generated.

## NeoMacs baseline observations

`PrintState` currently combines `PrintOptions`, an optional internal `PrintCircleState`, the evaluator/standalone context, callback table state, depth, and two traversal stacks. There are two numbering implementations: an internal Rust map and an externally visible Lisp `print-number-table`. In particular, `print_circle == true` with `state.circle == None` is intentional when the external table is selected; that combination alone does not prove missing bookkeeping. [State and modes](../../crates/neovm-core/src/emacs_core/lisp/print/mod.rs).

The circle-candidate predicate includes obarrays, but `object_identity_key` has no obarray identity. Actual obarray emission is opaque, as in GNU. Removing inconsistent traversal/classification may improve parity, but adding an obarray identity alone is not evidence that the ETAF hang is explained.

Preprocessing is iterative, while the emission path recursively calls `write_value_stateful` for children. It must preserve mutable callback behavior: the callback can change printing options or the live number table; child reads and root ownership already have dedicated guards. There is no explicit quit-check call in this printer module at the baseline. The main investigation must determine which graph, options, entry point and callback state are active in the failing application, rather than infer a cause from repeated stack frames alone.

## Design inferences for a durable fix

These are proposed implementation properties, not additional GNU requirements:

1. **Make numbering ownership explicit.** An enum can distinguish ordinary printing, internal numbering, and external Lisp-table numbering. Callback-driven table changes still need their existing semantics. Avoid representing external numbering as an apparently absent internal map, or treating an option flag as proof that a valid table is available.
2. **Unify recursive-object classification.** A helper returning a typed recursive-object view with its identity and child-access behavior can prevent the candidate predicate, identity function and traversal from silently disagreeing. Opaque values should remain opaque. Exhaustive matching can make newly added object types require an explicit printer decision. [Rust exhaustive matching](https://doc.rust-lang.org/book/ch06-02-match.html).
3. **Use explicit traversal continuations where recursion is the reproduced cause.** Encode pending element visits, list tails, delimiters and container positions in an enum, following the existing GNU shape. Pending Lisp values must remain rooted, and borrowed container data must be reacquired after callbacks/GC. Keep traversal state private to the printer invocation and its owning mutator.
4. **Separate compatibility limits from liveness.** Preserve GNU depth, list-tail cycle references, label ordering, print overrides, and truncation output. Check evaluator cancellation during long traversals, and propagate a typed error/quit outcome without recursively formatting the offending object. A generic output-size cap would be a policy change; a shared acyclic graph can legitimately require very large output when sharing notation is disabled.

The smallest proven defect should be fixed and covered first. A full continuation conversion is justified only where it addresses the minimized reproduction or establishes the missing termination invariant; it should not become an unrelated rewrite.

## Evidence needed from implementation

Use public printing functions for regression/oracle checks. Cover internal and external numbering, deep acyclic nesting, shared DAGs, cycles, flat cyclic tails, nonempty/empty strings, closures/bytecode slots, records, hash tables, opaque obarrays, and callback-driven changes. Verify cancellation independently from exact-output compatibility. Run the minimized case and the original ETAF scenario separately, with explicit time/memory bounds and isolated state under repository `./tmp/`.

## Reproduction results and scope

The exact reported ETAF cached value has not been obtained or reproduced. The six reported package versions were downloaded from the primary [ETAF archive](https://etaf.top/packages/), into `tmp/issue590-etaf/packages`, with SHA-256 pins recorded locally. GNU completed the fresh/warm model controls and a mounted, warm 1,000-song control under explicit bounded execution. These controls use generated library data, not the reporter's sanitized state.

On NeoMacs, explicit printing of the mounted 10-song runtime with `print-circle=nil` raised `Apparently circular structure being printed`, matching GNU. With `print-circle=t`, printing completed and emitted 1,967,989 bytes (GNU: 1,978,667 bytes). The subsequent explicit `etaf-ncm-close` stalled in `to_equal_key_depth_swp` → `closure_to_equal_key` → `puthash` and hit the owned runner's 2 GiB RSS bound. Its captured stack contained no printer frames. A skip-close repeat instead stalled in display layout / `buffer-text-pixel-size`. Neither result establishes the original printer runaway; the teardown structural-hashing and layout paths are separate observations.

A minimized printer liveness defect **was reproduced** without ETAF. The public `prin1-to-string` call prints a depth-35 acyclic shared vector DAG with `print-circle`, `print-level`, and `print-length` nil. This tiny input legitimately has enormous expanded output, making interruption essential rather than proving an invalid cycle. The owned runner sends SIGTERM after printing begins. GNU exited with status 15 approximately 20 ms after the signal and ran `kill-emacs-hook`. Baseline NeoMacs exceeded the five-second termination bound, never ran the hook, and required an owned SIGKILL. Its debugger stack repeatedly visited `write_value_stateful_rooted`, `write_value_stateful_inner`, and `with_default_cycle_guard`, matching the reported printer-stack shape. Local evidence: `tmp/issue590-etaf/dag-termination-fixture.el`, `gnu-dag-sigterm-baseline.json`, and `neo-dag-sigterm-baseline.json`.

The confirmed cause is a missing evaluator attention/quit poll during printer object traversal. A deterministic synchronous probe makes `print-unreadable-function` set `quit-flag` while printing the first object of a vector. GNU's print call signals to its surrounding `condition-case`; baseline NeoMacs returns the complete string first and delivers quit outside that handler. This independently exposes the missing safe point without process timing or large allocation.

The synchronous public reproduction should return `caught`:

```elisp
(let ((print-circle nil)
      (print-unreadable-function
       (lambda (_object _escape) (setq quit-flag t) "Q")))
  (condition-case nil
      (prin1-to-string (vector (make-marker) '(after)))
    (quit 'caught)))
```

For the asynchronous control, run this in an owned subprocess, record a marker
immediately before printing, then send SIGTERM after the marker. Bound process
time and RSS; the expanded output is intentionally too large to finish:

```elisp
(let ((value 0) (print-circle nil) (print-level nil) (print-length nil))
  (dotimes (_ 35) (setq value (vector value value)))
  (prin1-to-string value))
```

## Test-first evidence and implementation boundary

The public regression was written and executed before the implementation change:

| Seam | Baseline result | Evidence under `tmp/` |
| --- | --- | --- |
| Unit: surrounding handler must receive printer quit | Failed: returned a string instead of `caught` | `issue590-quit-red.log` |
| Live GNU oracle: circle nil/t, inhibit-quit controls, interpreted/compiled callers | Failed: NeoMacs exited 255 with uncaught `Quit` | `issue590-oracle-quit-red.log` |
| TUI interactive evaluation | Failed: uncaught `Quit` instead of the caught result | `issue590-printer-quit-tui-red.log` |

The initial GUI fixture also failed, but that failure was not valid printer
regression evidence: its timer callback inherited `inhibit-quit=t`, correctly
deferring quit even after the fix. The corrected GUI form explicitly binds
`inhibit-quit=nil` to exercise interruptible interactive evaluation. Its final
X11/Wayland runs provide post-fix verification; the corrected fixture was not
rerun against the baseline executable. The unit, live-oracle, TUI, and external
SIGTERM failures above independently establish the original RED evidence.

The focused implementation dispatches exhaustively on the existing `PrinterContext`: evaluator printing calls `Context::maybe_quit`, while standalone formatting has no evaluator and retains its behavior. A nonlocal result is stored as the existing typed `Flow` failure, stopping traversal and preserving inhibit-quit, shutdown-hook, and normal unwind policy. It introduces neither a new output-size cap nor different circle/depth formatting rules.

The initial fix passed all 125 selected printer unit tests (`tmp/issue590-printer-green.log`). Safety review then identified that the new poll can enter Lisp shutdown hooks even with no unreadable-object callback. Root ownership must therefore cover evaluator-held current values, list cursors, and hash keys unconditionally, rather than depend on `print-unreadable-function` being configured. All 125 printer tests also passed after that root-guard adjustment (`tmp/issue590-printer-final.log`). Ordinary SIGUSR bookkeeping currently arms debugger state or queues counters; this note does not claim it directly runs arbitrary Lisp signal hooks.

The result so far addresses **printer interruptibility**, including the independently reproduced ignored-SIGTERM symptom. It does not justify claiming that the original ETAF value's expansion or the separate teardown/layout observations are resolved. No GNU source files were modified.

## Final verification

The final implementation was verified with the rebuilt development executable
and matching generated runtime image. All Rust test runs used `cargo nextest`;
temporary fixtures, isolated state, and investigation logs stayed in `./tmp/`.

| Check | Result | Local evidence |
| --- | --- | --- |
| Printer unit suite, including callback/GC and GNU parity controls | 125 passed | `tmp/issue590-printer-final.log` |
| Live GNU oracle: quit matrix, print/read, prin1, circle integration cases | 48 unique tests passed | `tmp/issue590-oracle-{print,circle-prin1}-green.log` |
| TUI printer quit plus interactive evaluation controls | 3 passed | `tmp/issue590-printer-quit-tui-green.log` |
| Corrected GUI fixture: X11 and Wayland | Both passed; snapshots contain caught quit and subsequent `(t 42)` | `tmp/issue590-printer-quit-gui-{x11,wayland}-green.log` |
| External SIGTERM during expanded DAG printing | Clean exit 15 and shutdown hook within the five-second bound | `tmp/issue590-etaf/neo-dag-sigterm-green.json` |
| Same SIGTERM with `inhibit-quit=t` | Clean exit 15 and shutdown hook | `tmp/issue590-etaf/neo-dag-sigterm-inhibit-green.json` |
| Shutdown hook detaches the active graph and forces GC | Clean exit; NeoMacs retains the active printer value | `tmp/issue590-etaf/neo-dag-sigterm-gc-root-green.json` |

The GC variant checks NeoMacs' conservative lifetime invariant, not exact weak
reference retention parity: GNU also exits cleanly but collects that weakly
referenced ancestor. Formatting and diff checks passed. No release-PGO build was
tested. Preprocessing, standalone formatting, and large atomic formatting loops
retain their existing behavior; this change introduces evaluator attention
checks between stateful object visits.
