//! Host boundary contracts shared between NeoVM (the Lisp runtime) and
//! the embedding host (neomacs, tests, tools).
//!
//! This crate is deliberately thin: it only exposes the data types and
//! trait(s) that cross the host ↔ VM boundary. Every entry point in
//! [`HostAbi`] is called by the VM and implemented by the host. Nothing
//! in this crate runs Lisp code; it is pure wire-format vocabulary.
//!
//! The shape of the API reflects the small set of things the VM needs
//! from the host:
//!
//! * metadata about each primitive the VM can invoke
//!   ([`PrimitiveDescriptor`]),
//! * a way to actually invoke primitives and get a result or signal
//!   back ([`HostAbi::call_primitive`]),
//! * snapshot read / patch apply against host-owned state
//!   ([`SnapshotRequest`], [`PatchRequest`], [`PatchResult`]),
//! * enums describing task scheduling and channel select operations
//!   the host-facing runtime layers use.
//!
//! The types are intentionally `Clone + Debug + Eq` where possible so
//! both sides can store them in maps, compare them in tests, and log
//! them for diagnostics. [`LispValue`] carries a raw byte payload so
//! this crate is agnostic to the VM's internal `Value` representation.
#![deny(missing_docs)]

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;

pub mod frontend_event;
mod host;

pub use host::{
    ExecutionEngine, HostKind, HostOperation, HostOperationError, HostProfile, NativeModuleModel,
    ProcessModel, RuntimeImageModel, StorageModel,
};

/// Opaque identifier for a VM-side object handle exposed across the
/// host boundary. Issued by the VM and interpreted only by the VM;
/// the host treats it as an opaque token.
pub type VmHandleId = u64;

/// Opaque identifier for an isolate — a logically independent unit
/// of VM state that owns its own heap, root set, and scheduler
/// queue. Hosts can run multiple isolates concurrently, and patches
/// / snapshots are always scoped to a single isolate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IsolateId(
    /// Raw isolate identifier, assigned by the host.
    pub u64,
);

/// Opaque identifier for a host primitive (one of the built-in
/// functions the VM can invoke via [`HostAbi::call_primitive`]).
/// Registration of primitives is host-side; the VM only passes
/// `PrimitiveId` values that it has previously learned about via
/// [`HostAbi::primitive_descriptor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveId(
    /// Raw primitive identifier, assigned by the host.
    pub u32,
);

/// Opaque identifier for a channel that carries
/// [`LispValue`] messages between isolates or between the VM and
/// host code. Channels are created by the host and referenced by
/// both sides through this ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChannelId(
    /// Raw channel identifier, assigned by the host.
    pub u64,
);

/// Thread affinity requirement for a primitive.
///
/// Some host primitives may only be invoked from the main thread
/// (e.g. because they touch UI state), while others can run on any
/// worker thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    /// Primitive must only be called from the host's main thread.
    MainOnly,
    /// Primitive is safe to call from any worker thread.
    WorkerSafe,
}

/// Effect classification for a primitive. The VM uses this to
/// decide how aggressively to reorder or speculate around a call,
/// and whether to allow the call from a concurrent reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectClass {
    /// Primitive does not read or write any observable state
    /// beyond its arguments and return value.
    PureRead,
    /// Primitive reads host-owned state but does not modify it.
    StateRead,
    /// Primitive mutates host-owned state.
    StateWrite,
    /// Primitive performs a blocking I/O operation (file, socket,
    /// device). Callers should expect the thread to park.
    BlockingIo,
}

/// Static metadata describing a single host primitive. Returned by
/// [`HostAbi::primitive_descriptor`] so the VM can ask the host what
/// it is about to call before dispatching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    /// Human-readable name (used for error messages, logs, and
    /// disassembly). Must be stable across runs so stored bytecode
    /// can reference primitives by name where needed.
    pub name: &'static str,
    /// Thread affinity requirement.
    pub affinity: Affinity,
    /// Effect classification for reordering and concurrency
    /// analysis.
    pub effect: EffectClass,
    /// `true` if calling this primitive may trigger a GC cycle
    /// (directly or transitively through re-entry).
    pub can_trigger_gc: bool,
    /// `true` if calling this primitive may run Lisp code re-entrant
    /// into the VM. Hosts that implement primitive wrappers around
    /// Lisp callbacks set this flag.
    pub can_reenter_elisp: bool,
    /// `true` if calling this primitive with the same arguments in
    /// the same host state always returns the same result.
    pub deterministic: bool,
}

/// Wire representation of a Lisp value crossing the host boundary.
///
/// The format of `bytes` is an implementation detail of the VM
/// serializer; neither this crate nor the host interprets the
/// contents. The host simply stores or forwards the bytes as-is.
///
/// `LispValue` derives `Default` so empty values (zero bytes) can
/// be constructed without knowing the serializer's format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LispValue {
    /// Opaque serialized payload. Format is a VM-internal concern.
    pub bytes: Vec<u8>,
}

/// A Lisp-level condition signal propagated across the host
/// boundary. Produced by host primitives when the host wants to
/// raise a Lisp-visible error, caught by the VM and re-raised
/// through the normal condition-case machinery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    /// Symbol name of the condition (e.g. `"wrong-type-argument"`).
    pub symbol: String,
    /// Optional data payload attached to the signal, serialized as
    /// a string by the producer. Typically a printed representation
    /// of a Lisp value so the host and VM can exchange details
    /// without agreeing on `LispValue` format.
    pub data: Option<String>,
}

/// Non-Lisp error surfaced by the host side of the ABI. Used by
/// snapshot and patch calls that can fail for reasons other than a
/// Lisp-level condition (e.g. unknown handle, I/O error, stale
/// revision).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    /// Human-readable error description.
    pub message: String,
}

impl Display for HostError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for HostError {}

/// Snapshot-read request. Asks the host to serialize the state
/// anchored by `handle` inside `isolate` into an opaque blob so the
/// VM (or a different isolate) can inspect it without taking the
/// host's write lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRequest {
    /// Isolate the target handle lives in.
    pub isolate: IsolateId,
    /// VM handle identifying the object to snapshot.
    pub handle: VmHandleId,
    /// Optional revision hint: if set, the host may short-circuit
    /// and return the cached snapshot when the target has not
    /// advanced past `revision_hint`.
    pub revision_hint: Option<u64>,
}

/// Result of a successful snapshot read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotBlob {
    /// Monotonic revision of the snapshotted state at the moment
    /// the host produced this blob.
    pub revision: u64,
    /// Serialized snapshot payload. Format is a host/VM contract;
    /// this crate does not interpret it.
    pub bytes: Vec<u8>,
}

/// Patch-apply request. Asks the host to apply the delta `patch`
/// against the object identified by `target` inside `isolate`,
/// provided the current revision matches `expected_revision`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchRequest {
    /// Isolate the target handle lives in.
    pub isolate: IsolateId,
    /// VM handle identifying the object to patch.
    pub target: VmHandleId,
    /// Revision the patch was produced against. If this does not
    /// match the current host-side revision, the host rejects the
    /// patch with [`PatchResult::Rejected`].
    pub expected_revision: u64,
    /// Serialized patch payload. Format is a host/VM contract.
    pub patch: Vec<u8>,
}

/// Outcome of a patch-apply request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchResult {
    /// Patch applied cleanly. `new_revision` is the host-assigned
    /// revision of the target after the patch.
    Applied {
        /// Host-assigned revision after the patch.
        new_revision: u64,
    },
    /// Patch rejected because the target had already advanced past
    /// `expected_revision`. The VM should re-read the snapshot and
    /// rebase its edits on top.
    Rejected {
        /// Host's current revision at the moment the patch was
        /// rejected. The VM rebases against this revision.
        current_revision: u64,
    },
}

/// Priority hint for a VM task scheduled against the host. The host
/// scheduler uses this to bias ready-queue ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskPriority {
    /// Highest priority — a task that is blocking the interactive
    /// event loop.
    Interactive,
    /// Default priority for background work.
    Default,
    /// Lowest priority — long-running batch work that should yield
    /// to interactive and default tasks.
    Background,
}

/// Scheduling options for a VM task submitted through the host
/// runtime. Non-default fields customize affinity, priority, name,
/// and timeout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskOptions {
    /// Optional diagnostic name for the task.
    pub name: Option<String>,
    /// Priority hint for the host scheduler. Default:
    /// [`TaskPriority::Default`].
    pub priority: TaskPriority,
    /// Thread affinity. Default: [`Affinity::WorkerSafe`].
    pub affinity: Affinity,
    /// Optional wall-clock timeout. If set and the timeout expires
    /// before the task completes, the task is cancelled and the
    /// result is [`TaskError::TimedOut`].
    pub timeout: Option<Duration>,
}

impl Default for TaskOptions {
    fn default() -> Self {
        Self {
            name: None,
            priority: TaskPriority::Default,
            affinity: Affinity::WorkerSafe,
            timeout: None,
        }
    }
}

/// Why a VM task executed through the host runtime produced no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskError {
    /// Task was cancelled by the host or by the requester.
    Cancelled,
    /// Task was cancelled because the wall-clock timeout expired.
    TimedOut,
    /// Task raised a Lisp-level condition before completing.
    Failed(
        /// The condition signal the task raised.
        Signal,
    ),
    /// Task called `kill-emacs`. Not a failure: an order. GNU exits the
    /// process when a Lisp thread calls `kill-emacs`, so a host that treats
    /// this as one task's error keeps running against the program's wishes.
    /// Its own variant so every host boundary decides at compile time instead
    /// of pattern-matching a signal named "kill-emacs".
    Shutdown(ShutdownOrder),
}

/// A task's request that the process exit. Carried by
/// [`TaskError::Shutdown`] and latched by the runtime, so a host learns of it
/// whether or not it inspects the task's own result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutdownOrder {
    /// Process exit status the task asked for.
    pub exit_code: i32,
    /// Whether the exit should restart the editor.
    pub restart: bool,
}

/// One operation in a channel-select batch. The host picks whichever
/// operation becomes ready first (or times out if none do).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectOp {
    /// Receive from the given channel.
    Recv(
        /// Channel to receive from.
        ChannelId,
    ),
    /// Send the given value to the given channel.
    Send(
        /// Channel to send to.
        ChannelId,
        /// Payload to send.
        LispValue,
    ),
}

/// Outcome of a channel-select batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectResult {
    /// One of the submitted operations became ready.
    Ready {
        /// Index into the original `SelectOp` slice.
        op_index: usize,
        /// For a receive operation, the value that was dequeued.
        /// For a send operation, `None` (the payload was delivered
        /// and needs no return).
        value: Option<LispValue>,
    },
    /// No operation became ready before the timeout expired.
    TimedOut,
    /// The select was cancelled before any operation became ready.
    Cancelled,
}

/// Host-side boundary trait implemented by the embedding runtime.
/// The VM holds a `&mut dyn HostAbi` and calls into it whenever it
/// needs to invoke a primitive, read a snapshot, or apply a patch.
pub trait HostAbi {
    /// Return the static metadata for `primitive`. The VM calls this
    /// before dispatching a primitive call so it can decide whether
    /// the caller's context (thread, reader/writer lock state) is
    /// compatible with the primitive's requirements.
    fn primitive_descriptor(&self, primitive: PrimitiveId) -> PrimitiveDescriptor;

    /// Invoke `primitive` inside `isolate` with `args`. Returns the
    /// primitive's result value on success or a Lisp-level signal
    /// on failure.
    fn call_primitive(
        &mut self,
        isolate: IsolateId,
        primitive: PrimitiveId,
        args: &[LispValue],
    ) -> Result<LispValue, Signal>;

    /// Clone the state anchored by `request.handle` into an opaque
    /// snapshot blob the caller can inspect without holding the
    /// host's write lock. The host may return a cached blob when
    /// the hinted revision is still current.
    fn clone_snapshot(&self, request: SnapshotRequest) -> Result<SnapshotBlob, HostError>;

    /// Apply `request.patch` to the target identified by
    /// `request.target`, provided the target is still at
    /// `request.expected_revision`. Returns
    /// [`PatchResult::Applied`] with the new revision on success,
    /// or [`PatchResult::Rejected`] with the current revision if
    /// the target has moved on.
    fn submit_patch(&mut self, request: PatchRequest) -> Result<PatchResult, HostError>;
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
