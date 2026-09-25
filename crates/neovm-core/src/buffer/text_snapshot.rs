//! How a buffer-text snapshot holds the text bytes (P3.5 stage A).
//!
//! Redisplay snapshots every displayed buffer's text once per window layout
//! (`Buffer::text_snapshot`). The text bytes live in an `Rc<TextBackend>`
//! inside [`super::buffer_text::BufferText`]'s storage:
//!
//! - [`TextSnapshotMode::Share`] makes the snapshot an `Rc` bump. The live
//!   buffer's next mutation goes through `Rc::make_mut`, which copies only
//!   while a snapshot still holds the old bytes. Snapshots live for one
//!   window layout, so steady-state typing copies nothing;
//!   [`buffer_text_cow_copies`] counts every copy that does happen.
//! - [`TextSnapshotMode::Copy`] deep-copies the bytes into the snapshot, as
//!   every snapshot did before (a 16 MB buffer paid 4,040 page faults per
//!   redisplay).
//!
//! `NEOMACS_TEXT_SNAPSHOT=copy|share` selects the mode for a same-binary
//! A/B. Unset means `copy` until the default flips after measurement. Read
//! once per process, on the first snapshot.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// How [`super::buffer_text::BufferText`]'s snapshot clone holds the bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TextSnapshotMode {
    /// Deep-copy the text bytes into every snapshot (the old behaviour).
    Copy = 1,
    /// Share the bytes copy-on-write; the snapshot is O(1).
    Share = 2,
}

/// [`TEXT_SNAPSHOT_MODE`] before the knob is read.
const MODE_UNREAD: u8 = 0;

/// The mode's discriminant, or [`MODE_UNREAD`].
static TEXT_SNAPSHOT_MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

/// Copies `Rc::make_mut` made because a snapshot still held the live
/// buffer's bytes when the buffer mutated. Process-wide: the layout engine
/// reads and resets it once per accepted frame (`LayoutStats`).
static BUFFER_TEXT_COW_COPIES: AtomicUsize = AtomicUsize::new(0);

/// The mode a value of `NEOMACS_TEXT_SNAPSHOT` selects.
pub fn parse_text_snapshot_knob(value: Option<&str>) -> TextSnapshotMode {
    let Some(value) = value.map(str::trim) else {
        return TextSnapshotMode::Copy;
    };
    match value.to_ascii_lowercase().as_str() {
        "share" | "cow" | "on" | "1" => TextSnapshotMode::Share,
        "" | "copy" | "off" | "0" => TextSnapshotMode::Copy,
        other => {
            tracing::warn!(
                value = other,
                "NEOMACS_TEXT_SNAPSHOT: unknown mode, using copy"
            );
            TextSnapshotMode::Copy
        }
    }
}

thread_local! {
    static MODE_OVERRIDE: std::cell::Cell<Option<TextSnapshotMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force MODE on this thread, overriding the knob; `None` returns to the
/// knob. For tests, including those of crates that snapshot buffers (the
/// layout engine), which is why it is not `cfg(test)`. The mode is read once
/// per snapshot, never on a per-character path, so the check costs nothing
/// measurable.
pub fn set_text_snapshot_mode_override(mode: Option<TextSnapshotMode>) {
    MODE_OVERRIDE.with(|cell| cell.set(mode));
}

/// The active mode. One relaxed byte load once the knob is read.
#[inline]
pub fn text_snapshot_mode() -> TextSnapshotMode {
    if let Some(mode) = MODE_OVERRIDE.with(|cell| cell.get()) {
        return mode;
    }
    match TEXT_SNAPSHOT_MODE.load(Ordering::Relaxed) {
        MODE_UNREAD => read_text_snapshot_knob(),
        2 => TextSnapshotMode::Share,
        _ => TextSnapshotMode::Copy,
    }
}

#[cold]
#[inline(never)]
fn read_text_snapshot_knob() -> TextSnapshotMode {
    let mode = parse_text_snapshot_knob(std::env::var("NEOMACS_TEXT_SNAPSHOT").ok().as_deref());
    TEXT_SNAPSHOT_MODE.store(mode as u8, Ordering::Relaxed);
    mode
}

/// Record one copy-on-write copy of BYTES bytes of buffer text.
#[cold]
#[inline(never)]
pub(crate) fn note_buffer_text_cow_copy(bytes: usize) {
    BUFFER_TEXT_COW_COPIES.fetch_add(1, Ordering::Relaxed);
    tracing::debug!(
        target: "neovm::buffer_text_cow",
        bytes,
        "buffer text copied on write: a snapshot still held the bytes"
    );
}

/// Copy-on-write copies of buffer text since the last [`take_buffer_text_cow_copies`].
pub fn buffer_text_cow_copies() -> usize {
    BUFFER_TEXT_COW_COPIES.load(Ordering::Relaxed)
}

/// Take and reset the copy-on-write copy count.
pub fn take_buffer_text_cow_copies() -> usize {
    BUFFER_TEXT_COW_COPIES.swap(0, Ordering::Relaxed)
}
