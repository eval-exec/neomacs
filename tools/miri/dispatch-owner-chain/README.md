# Dispatch owner-chain Miri model

This independent, dependency-free workspace checks scalar pointer ordering
with a u64 counter. It does not interpret a fabricated Context, heap or JIT.
The tests are opt-in so normal repository test jobs do not run Miri models.

Recorded toolchain: nightly-2026-07-03, rustc/Miri commit
`c397dae808f70caebab1fc4e11b3edf7e59f58c7`, and nextest 0.9.137.
From this directory, with that Miri toolchain and nextest installed:

```sh
MIRIFLAGS=-Zmiri-backtrace=full cargo +nightly-2026-07-03 miri nextest run \
  --offline --test owner_chain --features owner-chain-model
MIRIFLAGS='-Zmiri-tree-borrows -Zmiri-backtrace=full' \
  cargo +nightly-2026-07-03 miri nextest run --offline --test owner_chain \
  --features owner-chain-model
```

The fixed path derives a fresh raw callback pointer from the current typed
owner before each callback. Adding the explicit `baseline-ancestor` feature
instead selects the old ancestor pointer for the eight counterexamples;
those eight rows are expected to fail under each alias model. Four preserved
apply/read/direct controls remain passing. The same twelve test names run
under either feature selection; no permanently failing test is shipped in
the default fresh target. A compile_error blocks baseline-ancestor outside
cfg(miri), preventing native execution of the archived invalid model.

The finish shape creates a typed owner, calls through raw, then reuses that
owner for release, followed by a second callback and final bookkeeping. It
does not activate the owner before the first callback. Macro/source shapes
also include the typed counter increment corresponding to their real
pre-entry depth bookkeeping. Each of their three alternatives is tested
independently. Numbered scalar callbacks represent pointer ancestry only;
the model does not invent native ABIs or simulate execution machinery.

The eighth counterexample passes an older raw pointer beside a new protected
mutable argument; the fresh helper takes only the owner and derives raw
inside. Its protected function-argument retag differs from the local owner
in finish. The four archived controls retain their original raw casts and
assertions; in particular, an older pointer derived inside the same typed
owner function can pass the inspected normalization/read model. Do not
rewrite those preserved casts to from_mut before subsequent owner reborrows:
that is a different provenance experiment, not a formatting change.

Miri establishes results for these executed model paths, not the soundness
of every full-runtime path. Production source regressions, actual fixture
behavior and code generation still need their separate verification.

The evidence directory preserves the finished Stacked/Tree runs: twelve
fresh cases pass under each model, while the baseline selection yields
eight expected alias failures and four passing controls. The original
six-case experiment is archived separately. `model-results.json` records
the executed source and log hashes; the checked-in model differs from the
archived executed bytes only in macro-block whitespace.
Logs are stored as lossless `.log.gz` archives; `gzip -cd` displays the
original bytes, whose hashes remain in the metadata. The small source
formatting diff is archived the same way.

`owner-formations.json` combines the seven-file source audit. Locations
refer to pre-repair commit `06bef6ee903a84307b59bd636ac14ba6fc626d41`.
Each record checks owner formation, an older-raw write during its live
extent, and later owner use. Three formations require nine callback
repairs. Other records identify the absent condition; non-Context loans
and typed reborrows are recorded explicitly rather than changed for
symmetry. The component tables and exclusion inventories retain the
source reasoning, including the two VM scopes and the cache probe scopes.


## Historical main22 re-cut

A historical member was re-cut onto parent
`22f437c00b85c6464991d792aec349ca0a67ff55` after the first local landing.
`evidence/main22-recut-audit.json` records that historical main22 source
comparison, nine callback arguments and historical-to-current line mapping.
The original owner-formations archive remains explicitly based on06bef:
its108 records are107 source expressions and one no-formation sentinel.
All107 original expressions are unchanged in the new parent. Seventeen
shim coordinates shift by2 and five cache coordinates by11; the new
stack-fallback branches form no new Context owner. The five-file production
delta within the seven audited source files is identical to the earlier repair, including all nine immediate
fresh-owner callback pointers. No old source/log is relabelled as main22.

The existing model sources and archived observations are unchanged by this
re-cut; the new metadata/README do not create a new executed result. Fresh
post-tip Miri, correctness and performance evidence is required separately.

## Current main7a source audit

The current parent is
`7a7d474cd8c229eaaf778750c6cab95318817a04`.
`evidence/main7a-recut-audit.json` records a source-only comparison against
historical main22 parent `22f437c00b85c6464991d792aec349ca0a67ff55`
and member `e6c49c2ea01bebaaa12f42fdc829320276982a70`.
Its provisional source-view label identifies the source examined before
this metadata amendment; it is not a final member SHA or an executed tip.
The audited production blob IDs carry the stable source comparison.

Seven production source files are audited; five have changes in the
production repair delta: dispatch.rs, shims.rs, source_slots.rs, cache.rs
and vm.rs. The audited switch_dispatch.rs and apply.rs have no member
delta. All seven parent files are byte-identical main22 to main7a, and all
seven repaired source files match the historical e6 member. The five-file
production delta is therefore exactly unchanged, including every added
and removed line. All nine callback pointers remain immediate first
arguments derived from the current typed owner; later arguments contain
no intervening Context-owner access. Pointer-expression coordinates and
all 107 source-expression coordinates retain their main22 mappings; the
108th record remains the no-formation module sentinel. Historical06 to
main22 shifts (17 shim rows by2 and five cache rows by11) remain recorded.
The old all-three repair witnesses describe the pre-repair source, not a
claim that the repaired source still contains the older-raw pattern.

The entire neovm-eval and neovm-value parent subtrees are unchanged between
main22 and main7a. The endpoint source changes are confined to JSON
serialization/insertion, live window margins/fringes queries, and their
related tests. Those external builtin paths may still affect behavior,
allocation or callback frequency; this source audit does not certify
whole-call-graph behavior or performance. It identifies no new immediate
D1a owner-chain contract change or re-cut defect.

The existing model sources, archive bytes and historical main22 addendum
were unchanged before this new provenance addendum. Adding metadata does
not create a newly executed model result. Historical06 and main22 logs,
including the old compiler/diagnostic identity evidence, remain explicitly
historical. New final-member fresh-build, local real-.git architecture /
history guards, Miri, correctness and required performance evidence are
separate obligations; this source comparison is not READY evidence.
