use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

use crate::MetricName;

/// A semantic count that must agree exactly between editors before their
/// timings may be compared.
///
/// This is deliberately a closed enum rather than an arbitrary `MetricName`:
/// duration metrics cannot accidentally be declared correctness invariants.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CrossEditorParityMetric {
    CompletionCandidateCount,
}

impl CrossEditorParityMetric {
    pub const fn metric_name(self) -> MetricName {
        match self {
            Self::CompletionCandidateCount => MetricName::CompletionCandidateCount,
        }
    }
}

/// Stable identity of a committed performance workload.
///
/// A closed enum prevents a typo from selecting a different fixture or
/// silently creating a new time series.
///
/// The name is stated **once**, by `strum`'s `serialize_all` plus the explicit
/// `serialize` on the rows whose spelling kebab-case cannot derive (the `8K`
/// family and the `64`/`256` pair, which carry a digit boundary).  Display,
/// `IntoStaticStr`, `EnumIter` and `EnumCount` are all generated from that same
/// attribute set, so a name cannot drift between the enum, the CLI parser and
/// the time-series key — and `catalog_test` asserts the serde spelling agrees
/// with it, since serde's `rename` is the one list that is still separate.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Display,
    EnumCount,
    EnumIter,
    Eq,
    IntoStaticStr,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ScenarioId {
    RustLspTyping,
    RustLspTypingHeavy,
    MxTabCompletion,
    BytecodeCallLoop,
    /// Rare-call tier-entry diagnostics, excluded from the whole-editor suite.
    LexicalLoop,
    DynamicBindingLoop,
    DynamicVariableReadLoop,
    DynamicRebindingLoop,
    DynamicAliasReadLoop,
    BufferLocalReadLoop,
    /// First call of fresh functions, including heat-up and OSR compilation.
    FirstHotLoop,
    /// Short first-call controls for native compilation cost.
    #[serde(rename = "first-hot-loop-8k")]
    #[strum(serialize = "first-hot-loop-8k")]
    FirstHotLoop8K,
    #[serde(rename = "first-hot-loop-16k")]
    #[strum(serialize = "first-hot-loop-16k")]
    FirstHotLoop16K,
    #[serde(rename = "first-hot-loop-32k")]
    #[strum(serialize = "first-hot-loop-32k")]
    FirstHotLoop32K,
    /// First calls with many conditional blocks, to expose compilation scaling.
    #[serde(rename = "first-branch-loop-64")]
    #[strum(serialize = "first-branch-loop-64")]
    FirstBranchLoop64,
    #[serde(rename = "first-branch-loop-256")]
    #[strum(serialize = "first-branch-loop-256")]
    FirstBranchLoop256,
    /// Warmed builtin calls from bytecode; excluded from the whole-editor score.
    BuiltinCallPoint,
    BuiltinCallStringBytes,
    BuiltinCallStringLessp,
    BuiltinCallGetTextProperty,
    BuiltinCallMultibyteStringP,
    BuiltinCallCharOrStringP,
    BuiltinCallMaxChar,
    /// Individual warmed searches, excluded from the whole-editor score.
    SearchLiteralForward,
    SearchLiteralBackward,
    SearchRegexpForward,
    SearchRegexpBackward,
    SearchPosixForward,
    SearchPosixBackward,

    EditingSimulation,
    Startup,
    SustainedEditing,
    GuiInputLatency,
    OrgEditing,
    /// `org-editing` over a document carrying what a real Org file carries.
    ///
    /// The plain row builds headings, property drawers and tables and nothing
    /// else, so the `font-lock-ensure` it runs over the whole buffer every
    /// iteration never reaches Org's expensive matchers. Against the Org
    /// manual it has 0 links, 0 emphasis markers, 0 `#+` lines and 0 list
    /// items per 100 lines where the manual has 2.3, 19.3, 15.5 and 6.1.
    /// Same operation, same heading count, realistic surroundings.
    OrgEditingHeavy,
    MagitStatus,
    OrgJournalOpen,
    LargeFileEditing,
    Indentation,
    RegexSearch,
    /// Focused search-view diagnostics; excluded from the whole-editor suite.
    BoundedSearchEditSmall,
    BoundedSearchEditLarge,
    BoundedSearchEditOnly,
    BoundedSearchNoEdit,

    SustainedNativeVideo,
    /// `magit-status` with the package loaded as byte-code, which is what a
    /// user's session does.  The plain row forces `load-suffixes '(".el")`,
    /// inherited from the MELPA parity tests, and so measures loading and
    /// tree-walking source instead.
    MagitStatusCompiled,
    /// `org-journal-open` with the package loaded as byte-code, for the same
    /// reason.
    OrgJournalOpenCompiled,
    /// GNU ELPA's own Elisp benchmark suite, pinned.
    ///
    /// Every other fixture here was written in this repository, and auditing
    /// them found that each either flattered this engine or hid a defect. A
    /// third-party suite cannot be shaped to our strengths.
    ///
    /// NOT an editor benchmark -- twelve of its eighteen members are
    /// arithmetic and list compute -- and deliberately absent from
    /// `STANDARD_SCENARIOS` so it can never enter the board's geometric mean.
    ElispBenchmarks,
    /// Reading a subprocess's output: spawn, read, decode, insert.
    ///
    /// Every compilation, grep and language-server session pays this path and
    /// no other row touches it. It uses `call-process` so the row is
    /// deterministic; that reaches the same decoder as the async filters
    /// (`decode_process_run_in_context`) but does NOT cover filter dispatch or
    /// partial-run carryover.
    ProcessOutput,
    /// Opening a file: decode plus buffer insert, then fontification, timed
    /// apart.
    ///
    /// `insert-file-contents` is on the path of every file a session opens and
    /// NO other row times it -- `large-file-editing` loads its buffer before
    /// the sampling window opens. A 2.0-2.3x deficit against GNU lived there
    /// unseen until it was found by profiling outside the board (`0249de3cd`).
    /// Fontification is the other half of a real open and is large enough to
    /// bury the first, so the row reports both phases.
    FileOpen,
    /// `magit-status` over a repository with real history and a populated
    /// working tree.
    ///
    /// The other Magit rows run a repository of one file, one commit and one
    /// modified line -- the smallest status Magit can render, with no staged
    /// changes, no untracked files, no stashes and no second commit in the
    /// log. Magit's cost is parsing `git diff` output into sections and
    /// propertizing them, and that repository hands it one line to parse.
    ///
    /// This row loads byte-code, like `magit-status-compiled` and unlike
    /// `magit-status`, so comparing it against `magit-status-compiled`
    /// isolates the REPOSITORY as the only variable between them.
    MagitStatusHeavy,
    /// One `jsonrpc` round trip per operation at the size a language server
    /// actually sends: serialize a request, then parse a
    /// `textDocument/publishDiagnostics` reply.
    ///
    /// This is eglot's per-keystroke path. It is a separate row from
    /// `rust-lsp-typing`, which edits a buffer with diagnostics already
    /// applied and whose JSON fixture is 1.2 KB -- small enough that both
    /// engines measure identically, which is exactly why a 3.3x-8.3x
    /// serializer gap (issue #173) survived unseen in the suite. Payload
    /// size is the variable this row exists to hold at a realistic value.
    LspJsonRpc,
}

impl ScenarioId {
    /// The workload this scenario runs, which is not always its own name.
    ///
    /// The `-compiled` rows differ from the rows they mirror only in which of
    /// the package's files `load` prefers; they execute the same fixture
    /// branch. Fixtures dispatch on this name and end in an
    /// `(error "unknown editor workload")`, so a variant that reported its own
    /// id would fail the run rather than measure it.
    pub fn workload_str(self) -> &'static str {
        match self {
            Self::MagitStatusCompiled | Self::MagitStatusHeavy => Self::MagitStatus.as_str(),
            Self::OrgJournalOpenCompiled => Self::OrgJournalOpen.as_str(),
            other => other.as_str(),
        }
    }

    /// Number of inner iterations in a fresh function's first call.
    /// Other scenarios have a different operation contract.
    pub(crate) const fn first_hot_loop_iterations(self) -> Option<u32> {
        match self {
            Self::FirstHotLoop => Some(65_536),
            Self::FirstHotLoop8K => Some(8_192),
            Self::FirstHotLoop16K => Some(16_384),
            Self::FirstHotLoop32K => Some(32_768),
            Self::FirstBranchLoop64 | Self::FirstBranchLoop256 => Some(4_096),
            _ => None,
        }
    }

    /// Conditional updates per inner iteration of a branch-heavy first call.
    pub(crate) const fn first_call_branches(self) -> Option<u32> {
        match self {
            Self::FirstBranchLoop64 => Some(64),
            Self::FirstBranchLoop256 => Some(256),
            _ => None,
        }
    }

    /// The scenario's stable name — the same string `Display`, `FromStr`, the
    /// CLI and serde all carry, generated from the enum's `strum`/`serde`
    /// attributes rather than restated here.
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownScenarioId(String);

impl fmt::Display for UnknownScenarioId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown performance scenario `{}`", self.0)
    }
}

impl std::error::Error for UnknownScenarioId {}

impl FromStr for ScenarioId {
    type Err = UnknownScenarioId;

    /// A search over the generated iteration rather than a second table of
    /// strings: the spelling lives on the enum's `strum` attributes, and this
    /// cannot disagree with it.  A few dozen string comparisons, on a path
    /// taken a handful of times per process.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::iter()
            .find(|id| id.as_str() == value)
            .ok_or_else(|| UnknownScenarioId(value.to_string()))
    }
}

/// Display adapter selected for a workload run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frontend {
    Batch,
    Tui { rows: u16, columns: u16 },
    Gui { width: u32, height: u32 },
}

/// Immutable definition of one committed performance workload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioSpec {
    pub id: ScenarioId,
    pub description: &'static str,
    pub default_frontend: Frontend,
    pub default_iterations: NonZeroU32,
    pub primary_metric: MetricName,
    pub cross_editor_parity_metrics: &'static [CrossEditorParityMetric],
}

const SCENARIOS: &[ScenarioSpec] = &[
    ScenarioSpec {
        id: ScenarioId::RustLspTyping,
        description: "Rust Tree-sitter typing with revision-pinned LSP Mode and deterministic diagnostic replay",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerEditCpuTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::MxTabCompletion,
        description: "M-x TAB over 1,024 controlled commands through a real minibuffer and completion window",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(5).expect("non-zero scenario default"),
        primary_metric: MetricName::PerCompletionCpuTime,
        cross_editor_parity_metrics: &[CrossEditorParityMetric::CompletionCandidateCount],
    },
    ScenarioSpec {
        id: ScenarioId::BytecodeCallLoop,
        description: "Tier-0 bytecode-to-bytecode call and return loop with the Neomacs JIT disabled",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerBytecodeCallCpuTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::EditingSimulation,
        description: "Composite editing simulation with typed phase timings",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::Startup,
        description: "Clean editor startup through the complete process lifecycle",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1).expect("non-zero scenario default"),
        primary_metric: MetricName::ProcessWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SustainedEditing,
        description: "Long-running insert, fontification, redisplay, and deletion cycle",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerEditWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::GuiInputLatency,
        description: "GUI self-insert-command-to-forced-redisplay latency distribution",
        default_frontend: Frontend::Gui {
            width: 1200,
            height: 800,
        },
        // The ranked metric is a p99: over 100 samples that is the 2nd-largest
        // value, an extreme-value statistic decided by whether a handful of
        // scheduling or GC events land inside the timed window. Over 1000 it
        // is the 10th-largest. The workload costs ~2-5 ms per keystroke, so
        // this adds a few seconds per run, not the minutes a percentile of
        // 100 would need to become trustworthy by repetition.
        default_iterations: NonZeroU32::new(1000).expect("non-zero scenario default"),
        primary_metric: MetricName::P99InputToRedisplayLatency,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgEditing,
        description: "Org headings, TODO state, tables, fontification, and edits",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::MagitStatus,
        description: "Revision-pinned Magit status refresh in a deterministic Git repository",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgJournalOpen,
        description: "Revision-pinned org-journal yearly file open with org-superstar and git-gutter overlays",
        default_frontend: Frontend::Batch,
        // One operation is a full journal-open cycle (kill the buffer, let
        // org-journal find-file, fontify, and lay out the yearly file). At the
        // real workload's scale that is seconds, not milliseconds, so five
        // iterations keep the run bounded while still giving the median
        // something to work with.
        default_iterations: NonZeroU32::new(5).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::LargeFileEditing,
        description: "Editing, fontification, and navigation in a deterministic large file",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::Indentation,
        description: "Repeated Emacs Lisp region indentation with state restoration",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(50).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::RegexSearch,
        description: "Repeated regular-expression searches over realistic Emacs Lisp",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(50).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SustainedNativeVideo,
        description: "Sustained native video decode, zero-copy import, GPU composition, pacing, and pool reuse on the caller's physical Linux display",
        default_frontend: Frontend::Gui {
            width: 1920,
            height: 1080,
        },
        // One operation is a 100 ms observation tick: 300 gives a 30 second
        // measurement window after decoder and renderer warmup.
        default_iterations: NonZeroU32::new(300).expect("non-zero scenario default"),
        primary_metric: MetricName::P99VideoPresentationInterval,
        cross_editor_parity_metrics: &[],
    },
    // The two byte-code rows below exist because the plain `magit-status` and
    // `org-journal-open` rows force `load-suffixes '(".el")` -- deliberate for
    // the MELPA parity tests, where reading source keeps a package comparable
    // between engines without either byte-compiler in the picture, but wrong
    // for performance, because no user's session runs that way. They are added
    // as new ids rather than by flipping the existing rows so the published
    // instruction series stays comparable and the parity rationale survives.
    ScenarioSpec {
        id: ScenarioId::MagitStatusCompiled,
        description: "Revision-pinned Magit status refresh with the package loaded as byte-code, as a user's session loads it",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgJournalOpenCompiled,
        description: "Revision-pinned org-journal yearly file open with the packages loaded as byte-code, as a user's session loads them",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(5).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::RustLspTypingHeavy,
        description: "Rust Tree-sitter typing with a whole-file diagnostic set, the overlay load a real language-server session carries",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerEditCpuTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgEditingHeavy,
        description: "Org editing over links, emphasis, source blocks and lists -- the markup a real Org file carries",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::MagitStatusHeavy,
        description: "Magit status over a repository with real history, staged and unstaged diffs, untracked files and a stash",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FileOpen,
        description: "Open a source file: decode and buffer insert, then fontification, timed as separate phases",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::ProcessOutput,
        description: "Read a subprocess's output into a buffer: spawn, read, decode, insert",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::ElispBenchmarks,
        description: "GNU ELPA elisp-benchmarks: the upstream Elisp suite, pinned -- a workload this repository did not write",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationCpuTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::LspJsonRpc,
        description: "jsonrpc round trip at language-server message size: serialize a request, parse a diagnostics reply",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(200).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BoundedSearchEditSmall,
        description: "Bounded search after distant edits in a 1 KiB multibyte buffer",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BoundedSearchEditLarge,
        description: "Bounded search after distant edits in an 8 MiB multibyte buffer",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BoundedSearchEditOnly,
        description: "Distant edit control in an 8 MiB multibyte buffer",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BoundedSearchNoEdit,
        description: "Bounded search without edits in an 8 MiB multibyte buffer",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::LexicalLoop,
        description: "One long lexical sum loop under the editor default tier policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::DynamicBindingLoop,
        description: "One long sum loop with a live dynamic binding under the editor default tier policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstHotLoop,
        description: "First call of fresh bytecode functions, each looping 65,536 times under the editor default tier policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallPoint,
        description: "Warmed point calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallStringBytes,
        description: "Warmed string-bytes calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallStringLessp,
        description: "Warmed string-lessp calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallGetTextProperty,
        description: "Warmed get-text-property calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallMultibyteStringP,
        description: "Warmed multibyte-string-p calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallCharOrStringP,
        description: "Warmed char-or-string-p calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BuiltinCallMaxChar,
        description: "Warmed max-char calls through a bytecode alias under the editor default tier policy, including loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(500_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchLiteralForward,
        description: "Warmed forward literal searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchLiteralBackward,
        description: "Warmed backward literal searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchRegexpForward,
        description: "Warmed forward regexp searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchRegexpBackward,
        description: "Warmed backward regexp searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchPosixForward,
        description: "Warmed forward POSIX regexp searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SearchPosixBackward,
        description: "Warmed backward POSIX regexp searches over multibyte text, including point repositioning and bytecode loop overhead",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(2_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstHotLoop8K,
        description: "First call of fresh bytecode functions, each looping 8,192 times, including heat-up and native compilation",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstHotLoop16K,
        description: "First call of fresh bytecode functions, each looping 16,384 times, including heat-up and native compilation",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstHotLoop32K,
        description: "First call of fresh bytecode functions, each looping 32,768 times, including heat-up and native compilation",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstBranchLoop64,
        description: "First call of fresh bytecode functions with 64 conditional updates per iteration and 4,096 iterations; includes any compilation selected by the recorded execution policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::FirstBranchLoop256,
        description: "First call of fresh bytecode functions with 256 conditional updates per iteration and 4,096 iterations; includes any compilation selected by the recorded execution policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::DynamicVariableReadLoop,
        description: "Read a dynamically bound special variable on every loop iteration under the editor default tier policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::DynamicRebindingLoop,
        description: "Bind, read and unwind a special variable on every loop iteration under the editor default tier policy",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::DynamicAliasReadLoop,
        description: "Read a variable alias on every loop iteration while its target is dynamically bound",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::BufferLocalReadLoop,
        description: "Read a dynamically bound buffer-local variable on every loop iteration",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
];

pub fn scenarios() -> &'static [ScenarioSpec] {
    SCENARIOS
}

/// Return the definition for a typed scenario identity.
///
/// `ScenarioId` is closed, so absence is not a representable state. Keeping
/// this match exhaustive makes adding an enum variant fail to compile until
/// its workload definition is also registered.
pub const fn scenario(id: ScenarioId) -> &'static ScenarioSpec {
    match id {
        ScenarioId::RustLspTyping => &SCENARIOS[0],
        ScenarioId::MxTabCompletion => &SCENARIOS[1],
        ScenarioId::BytecodeCallLoop => &SCENARIOS[2],
        ScenarioId::LexicalLoop => &SCENARIOS[27],
        ScenarioId::DynamicBindingLoop => &SCENARIOS[28],
        ScenarioId::DynamicVariableReadLoop => &SCENARIOS[48],
        ScenarioId::DynamicRebindingLoop => &SCENARIOS[49],
        ScenarioId::DynamicAliasReadLoop => &SCENARIOS[50],
        ScenarioId::BufferLocalReadLoop => &SCENARIOS[51],
        ScenarioId::FirstHotLoop => &SCENARIOS[29],
        ScenarioId::FirstHotLoop8K => &SCENARIOS[43],
        ScenarioId::FirstHotLoop16K => &SCENARIOS[44],
        ScenarioId::FirstHotLoop32K => &SCENARIOS[45],
        ScenarioId::FirstBranchLoop64 => &SCENARIOS[46],
        ScenarioId::FirstBranchLoop256 => &SCENARIOS[47],
        ScenarioId::BuiltinCallPoint => &SCENARIOS[30],
        ScenarioId::BuiltinCallStringBytes => &SCENARIOS[31],
        ScenarioId::BuiltinCallStringLessp => &SCENARIOS[32],
        ScenarioId::BuiltinCallGetTextProperty => &SCENARIOS[33],
        ScenarioId::BuiltinCallMultibyteStringP => &SCENARIOS[34],
        ScenarioId::BuiltinCallCharOrStringP => &SCENARIOS[35],
        ScenarioId::BuiltinCallMaxChar => &SCENARIOS[36],
        ScenarioId::SearchLiteralForward => &SCENARIOS[37],
        ScenarioId::SearchLiteralBackward => &SCENARIOS[38],
        ScenarioId::SearchRegexpForward => &SCENARIOS[39],
        ScenarioId::SearchRegexpBackward => &SCENARIOS[40],
        ScenarioId::SearchPosixForward => &SCENARIOS[41],
        ScenarioId::SearchPosixBackward => &SCENARIOS[42],

        ScenarioId::EditingSimulation => &SCENARIOS[3],
        ScenarioId::Startup => &SCENARIOS[4],
        ScenarioId::SustainedEditing => &SCENARIOS[5],
        ScenarioId::GuiInputLatency => &SCENARIOS[6],
        ScenarioId::OrgEditing => &SCENARIOS[7],
        ScenarioId::MagitStatus => &SCENARIOS[8],
        ScenarioId::OrgJournalOpen => &SCENARIOS[9],
        ScenarioId::LargeFileEditing => &SCENARIOS[10],
        ScenarioId::Indentation => &SCENARIOS[11],
        ScenarioId::RegexSearch => &SCENARIOS[12],
        ScenarioId::BoundedSearchEditSmall => &SCENARIOS[23],
        ScenarioId::BoundedSearchEditLarge => &SCENARIOS[24],
        ScenarioId::BoundedSearchEditOnly => &SCENARIOS[25],
        ScenarioId::BoundedSearchNoEdit => &SCENARIOS[26],

        ScenarioId::SustainedNativeVideo => &SCENARIOS[13],
        ScenarioId::MagitStatusCompiled => &SCENARIOS[14],
        ScenarioId::OrgJournalOpenCompiled => &SCENARIOS[15],
        ScenarioId::RustLspTypingHeavy => &SCENARIOS[16],
        ScenarioId::LspJsonRpc => &SCENARIOS[22],
        ScenarioId::ElispBenchmarks => &SCENARIOS[21],
        ScenarioId::ProcessOutput => &SCENARIOS[20],
        ScenarioId::FileOpen => &SCENARIOS[19],
        ScenarioId::MagitStatusHeavy => &SCENARIOS[18],
        ScenarioId::OrgEditingHeavy => &SCENARIOS[17],
    }
}

#[cfg(test)]
mod tests;
