# Public API companion gate

This directory vendors lbstamp's canonical PC1 semantic projector. `surface.py`,
`expected.py` and `selftest.py` are byte-for-byte copies of the reviewed
rustdoc-JSON **format 57** implementation. The CLI uses only these tracked Python
standard-library files and explicitly supplied artifacts; it does not read a
lane worktree, a lane's evidence directory, or a producer controller.

Canonical SHA-256 values at import:

| File | SHA-256 |
| --- | --- |
| `surface.py` | `75ea6dbc691e203ac394509ad823b3aee0e429524815c60b30243df1e28deba5` |
| `expected.py` | `bb9015841c6817db9a040417418a04b70289511b625627730dddfe7c590d619e` |
| `selftest.py` | `0affbf05270ddb2cb05d0f54c52baec2c5a3a42134724aeb737b55f821d5fad7` |

The canonical baseline-02 report supersedes the older README's incomplete
hidden-item baseline warning. Its completed extraction established support for
borrowed and builtin aliases. It did not establish candidate equivalence. No
baseline artifact is vendored: each comparison needs artifacts at its actual
base and candidate.

## Invocation after split qualification resumes

Produce both arms locally using the same pinned compiler, rustdoc, feature set
and target. Both `--document-private-items` and `--document-hidden-items` are
mandatory producer flags. The comparator verifies those flags, JSON hashes,
source pins and toolchain receipts. Rustdoc generation itself is not performed
by this directory.

```sh
python3 scripts/split-guards/api/surface.py \
  --baseline A_CORE.json.gz --baseline-receipt A_RECEIPT.json \
  --candidate B_CORE.json.gz --candidate-receipt B_RECEIPT.json \
  --candidate-dep B_VALUE.json.gz --candidate-dep-receipt B_VALUE_RECEIPT.json \
  --path-map APPROVED_MOVED_PATHS.tsv --exceptions EXACT_EXCEPTIONS.tsv \
  --external-invariants PAIRED_EXTERNAL_EVIDENCE.json --out NEW_OUTPUT_DIRECTORY
```

Dependency JSON arguments and producer receipts are parallel repeatable lists.
Supply baseline dependencies too when needed. Existing output directories are
never overwritten. Exit 0 means exact exception equality, exit 1 means an API
delta, and exit 2 means unsupported or unresolved input. Baseline-only extraction
is not a comparison pass. Re-export aliases preserve canonical item identity;
one method's aliases form one exception group.

The path-map TSV has exactly `from` and `to` columns. Its reviewed prefixes map
declaration identity across the move, not exported API paths. Unsupported JSON
schemas, unresolved moved definitions and cyclic or unsupported exports refuse.

The exception TSV has exactly these columns:

```text
owner  method  candidate_signature_sha256  guard  protected_caller  approval
```

The separators are tabs. Fields must be nonempty, hashes exact, and owner/method
groups unique. Added method groups must equal the table and retain the baseline's
previously private signature. Removed items, changed signatures, extra exceptions
and unrelated public additions fail. Approval text does not itself prove a
signature, guard, DATA ledger or protected-caller review.

`expected.py` derives hashes from previously private baseline inherent methods:

```sh
python3 scripts/split-guards/api/expected.py \
  --baseline A_CORE.json.gz --hot-table APPROVED_METHODS.tsv \
  --path-map APPROVED_MOVED_PATHS.tsv --out NEW_EXPECTED_DIRECTORY
```

That input table must provide `canonical_owner` and `method` columns. Expected
signature extraction alone grants no API or exception approval.

Opaque external dependencies need either their complete definition JSON or exact
paired evidence with `toolchain`, `target`, `cargo_lock`, `features` and
`dependencies` keys. Each identity has equal `baseline_sha256` and
`candidate_sha256` values; dependency entries must match the exact unresolved
crate set. Hashes must bind real compiler/sysroot, resolution, feature and source
manifests. The comparator does not manufacture those manifests. Changed workspace
graphs do not justify dummy hashes or relaxing the canonical equality rule.

## Deferred controls and compatibility boundary

`post-split-mutations.sh` records the canonical synthetic negative controls and
their clean identity control. It is for the post-split test pass; importing the
tool does not claim those controls have been run. The script's successful exit
means its assertions observed the expected negative outcomes. The full unchanged
suite is also available as `python3 scripts/split-guards/api/selftest.py`.

A live candidate mutation remains a separate post-split obligation: introduce an
unapproved public method on a public aliased type in a disposable candidate,
produce matching JSON and receipts, observe comparator exit 1, then restore the
candidate and observe exit 0. Never mutate shared baselines or substitute the
synthetic controls for that receipt.

The canonical comparator accepts **inherent-method exception groups only**.
PC1-D or PC1-P rows representing non-method additions remain unsupported and
fail closed. Support for such rows requires a reviewed canonical tool change;
this port neither invents a normalization rule nor broadens accepted item kinds.
Tests and projector executions are deferred under split FAST MODE.
