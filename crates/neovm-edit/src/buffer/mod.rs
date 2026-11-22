// The public `buffer::buffer` path is established across the evaluator.
#[allow(clippy::module_inception)]
pub mod buffer;
#[doc(hidden)]
pub mod buffer_text;
#[doc(hidden)]
pub mod composition;
mod edit_runtime;
#[cfg(not(any(test, feature = "test-support")))]
mod edit_transaction;
#[cfg(any(test, feature = "test-support"))]
pub mod edit_transaction;
#[doc(hidden)]
pub mod storage_string;
#[doc(hidden)]
pub mod syntax_cache;
#[doc(hidden)]
pub mod syntax_class;
#[doc(hidden)]
pub use edit_transaction::{CasifyExpansion, CasifyStorageShape};
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod gap_buffer;
#[cfg(any(test, feature = "test-support"))]
pub mod gap_buffer;
mod marker_data;
pub mod overlay;
mod overlay_bplus;
pub(crate) mod overlay_index;
mod overlay_order;
pub mod position;
pub mod shared;
pub mod text;
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod text_index;
#[cfg(any(test, feature = "test-support"))]
pub mod text_index;
pub mod text_props;
pub mod text_snapshot;
pub mod undo;
pub mod visited_file_modtime;

pub use buffer::{
    Buffer, BufferId, BufferManager, BufferTextSnapshot, InsertionType, LineIndexBuild, MarkerRef,
    SavedRestrictionState, TextPropertyRoots,
};
#[cfg(feature = "test-support")]
pub use buffer::{LabeledRestriction, SavedLabeledRestrictions};
pub use buffer::{LabeledRestrictionLabel, SavedRestrictionKind};
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) use buffer_text::BufferText;
#[cfg(any(test, feature = "test-support"))]
pub use buffer_text::BufferText;
pub use buffer_text::{BufferSourceAuthority, BufferSourceReadLease};
pub use edit_runtime::EditRuntime;
#[cfg(all(test, not(feature = "test-support")))]
pub(crate) use edit_transaction::TranspositionAnchorPolicy;
#[cfg(feature = "test-support")]
pub use edit_transaction::TranspositionAnchorPolicy;
#[doc(hidden)]
pub use edit_transaction::{PreparedBufferEdit, TextMeasurement};
pub use overlay::{Overlay, OverlayList};
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) use position::BufferLispPos;
#[cfg(any(test, feature = "test-support"))]
pub use position::BufferLispPos;
pub use position::{
    AccessibleCharRange, AccessibleEmacsByteRange, CharLen, CharPos0, CharRange, DisplayColumn,
    EmacsByteLen, EmacsBytePos, EmacsByteRange, LispBytePos1, LispCharPos1, TextPositionAnchor,
};
pub use shared::{SavedPointBeforeCommand, SharedUndoState};
pub use text::TextMetrics;
pub use text::{BufferTextBackendKind, TextChange, TextEditRange, TextExtent};
#[cfg(not(feature = "test-support"))]
pub(crate) use text::{TextInsertion, TextReplacement, TextTransposition};
#[cfg(feature = "test-support")]
pub use text::{TextInsertion, TextReplacement, TextTransposition};
pub use text_props::{ObjectIntervalPlistRun, ObjectIntervalRun, TextPropertyTable};
#[cfg(not(feature = "test-support"))]
pub(crate) use undo::truncate_undo_list;
#[cfg(feature = "test-support")]
pub use undo::truncate_undo_list;
pub(crate) use undo::undo_first_group_bytes;
pub use undo::{
    UndoLimitBindings, undo_list_boundary, undo_list_is_disabled, undo_list_record_insert,
};
#[cfg(not(feature = "test-support"))]
pub(crate) use undo::{UndoLimits, UndoRecording};
#[cfg(feature = "test-support")]
pub use undo::{UndoLimits, UndoRecording, undo_list_has_trailing_boundary};
pub use visited_file_modtime::VisitedFileModtime;
