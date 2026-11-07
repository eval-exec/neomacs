//! GNU `print.c`'s `print_buffer`, one per World.
//!
//! GNU keeps ONE process-global print accumulator (`print_buffer`,
//! `src/print.c:79` area, with `print_buffer.pos` and `.pos_byte`):
//!
//! * `print_prepare` (`src/print.c:132-194`) switches to the destination
//!   buffer; for a buffer/marker destination it saves a COPY of the current
//!   content (`make_string_from_bytes (buffer, pos, pos_byte)`) and records
//!   `print_unwind` (`src/print.c:109-113`) as an unwind-protect, then resets
//!   `pos = pos_byte = 0`.
//! * `print_finish` (`src/print.c:198-228`) inserts `buffer[0..pos]` into the
//!   destination and runs `unbind_to`, which executes the recorded unwind.
//! * `print_unwind` restores the saved data with
//!   `memcpy (print_buffer.buffer, SDATA (saved), SCHARS (saved))` -- the
//!   saved BYTES, leaving `pos` untouched. A nested print therefore leaks its
//!   bytes into the outer output exactly as GNU's static does.
//!
//! GNU has one static because its Lisp is serialized on the global lock. The
//! faithful mapping here is one accumulator per WORLD: a World is Neo's
//! isolation boundary, prints inside one World serialize on that World's view
//! rules (sole owner, or the Green Baton holder), and Worlds never share the
//! accumulator. This is the design's named divergence class D1 (cross-World
//! sharing); no Lisp observable can reach it.
//!
//! The type is reached through a [`WorldPrintAccess`] token minted only from a
//! live `&mut Context` in `global_state`: a mutator touching the accumulator
//! holds the World (design A2), so there is no unsynchronized path. The
//! accumulator's lifetime across a `print-unreadable-function` callback is
//! carried by a specpdl entry ([`super::SpecBinding`]'s
//! `NativeUnwindAction::PrintBufferUnwind`), not by a borrow: the unwind runs
//! from `unbind_to` on every non-local exit, exactly like GNU's
//! `record_unwind_protect (print_unwind, ...)`.
//!
//! No TLS, no process static, no per-Context state.

/// `print_buffer.buffer` while a print is in progress (`buffer != NULL` in
/// GNU). `saves` is GNU's LIFO unwind-protect stack: one entry per NESTED
/// print, in `print_prepare` order.
#[derive(Debug, Default)]
pub(crate) struct ActivePrintBuffer {
    /// The accumulator bytes (GNU's `print_buffer.buffer` + `size`).
    bytes: Vec<u8>,
    /// GNU `print_buffer.pos`: characters written since `prepare`.
    pub(crate) pos: usize,
    /// GNU `print_buffer.pos_byte`: bytes written since `prepare`.
    pub(crate) pos_byte: usize,
}

/// One saved accumulator state: `print_prepare`'s
/// `make_string_from_bytes (buffer, pos, pos_byte)`.
#[derive(Debug, Default)]
struct PrintBufferSave {
    /// Bytes `0..pos_byte` of the accumulator at save time.
    bytes: Vec<u8>,
    /// `SCHARS (saved)` -- the CHARACTER count at save time. `print_unwind`
    /// memcpy's exactly this many BYTES (a GNU quirk that truncates a
    /// multibyte save; reproduced deliberately by design A1).
    chars: usize,
}

/// GNU's `print_buffer` for one World: an explicit Idle/Active state instead
/// of GNU's null pointer, so the owner is type-level (design rev 2, A2).
///
/// `Idle` keeps the retained capacity (GNU's free-on-unwind is not
/// observable); `Active` is a print in progress, with the LIFO saves of the
/// nested prints under it.
#[derive(Debug)]
pub(crate) enum WorldPrintBuffer {
    /// `print_buffer.buffer == NULL`.
    Idle { retained: Vec<u8> },
    /// `print_buffer.buffer != NULL`.
    Active {
        buffer: ActivePrintBuffer,
        saves: Vec<PrintBufferSave>,
    },
}

impl Default for WorldPrintBuffer {
    fn default() -> Self {
        Self::Idle {
            retained: Vec::new(),
        }
    }
}

impl WorldPrintBuffer {
    #[doc(hidden)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// GNU `print_prepare`'s accumulator part: become active, or (nested)
    /// save a copy of the current content. `pos`/`pos_byte` reset to 0.
    /// Steady-state Idle -> Active performs no allocation: the retained Vec
    /// is reused, matching GNU's pre-allocated first buffer.
    #[doc(hidden)]
    pub(crate) fn prepare(&mut self) {
        match self {
            Self::Idle { retained } => {
                *self = Self::Active {
                    buffer: ActivePrintBuffer {
                        bytes: std::mem::take(retained),
                        pos: 0,
                        pos_byte: 0,
                    },
                    saves: Vec::new(),
                };
            }
            Self::Active { buffer, saves } => {
                // GNU allocates the saved string here and only here
                // (`make_string_from_bytes`); the outer print's own prepare
                // saves nothing.
                saves.push(PrintBufferSave {
                    bytes: buffer.bytes[..buffer.pos_byte].to_vec(),
                    chars: buffer.pos,
                });
                buffer.pos = 0;
                buffer.pos_byte = 0;
            }
        }
    }

    /// Append printed bytes (canonical Emacs internal encoding) at the
    /// current position, advancing `pos` by the CHARACTER count and
    /// `pos_byte` by the byte count.
    #[doc(hidden)]
    pub(crate) fn append(&mut self, bytes: &[u8]) {
        let Self::Active { buffer, .. } = self else {
            debug_assert!(false, "print bytes appended outside print_prepare");
            return;
        };
        if bytes.is_empty() {
            return;
        }
        let end = buffer.pos_byte + bytes.len();
        if end > buffer.bytes.len() {
            buffer.bytes.resize(end, 0);
        }
        buffer.bytes[buffer.pos_byte..end].copy_from_slice(bytes);
        buffer.pos_byte = end;
        buffer.pos += count_chars(bytes);
    }

    /// The bytes `print_finish` inserts into the destination:
    /// `buffer[0..pos_byte]`.
    #[doc(hidden)]
    pub(crate) fn pending(&self) -> &[u8] {
        match self {
            Self::Active { buffer, .. } => &buffer.bytes[..buffer.pos_byte],
            Self::Idle { retained } => {
                debug_assert!(false, "print_finish outside print_prepare");
                retained
            }
        }
    }

    /// Install a complete rendering as the accumulator content, advancing
    /// `pos` by its character count. Used when no Lisp callback interrupted
    /// the render, so the accumulator cursor is untouched at 0: GNU's
    /// `printchar` would have filled exactly these bytes. Avoids the
    /// render -> accumulator copy on the common path.
    #[doc(hidden)]
    pub(crate) fn replace_active_bytes(&mut self, bytes: Vec<u8>) {
        let Self::Active { buffer, .. } = self else {
            debug_assert!(false, "print replace outside print_prepare");
            return;
        };
        buffer.pos = count_chars(&bytes);
        buffer.pos_byte = bytes.len();
        buffer.bytes = bytes;
    }

    /// GNU `print_finish`'s `unbind_to` (the recorded unwind):
    /// `memcpy (buffer, SDATA (saved), SCHARS (saved))` for a nested print,
    /// or the outermost free (which leaves the retained capacity in place).
    /// `pos`/`pos_byte` are deliberately NOT restored.
    #[doc(hidden)]
    pub(crate) fn unwind(&mut self) {
        let Self::Active { buffer, saves } = self else {
            debug_assert!(false, "print unwind without an active print");
            return;
        };
        match saves.pop() {
            Some(save) => {
                // SCHARS (saved) BYTES, exactly as GNU's memcpy does.
                let n = save.chars.min(save.bytes.len()).min(buffer.bytes.len());
                if n != 0 {
                    buffer.bytes[..n].copy_from_slice(&save.bytes[..n]);
                }
            }
            None => {
                let buffer = std::mem::take(buffer);
                *self = Self::Idle {
                    retained: buffer.bytes,
                };
            }
        }
    }

    /// True while a print is in progress (test/diagnostic probe).
    #[doc(hidden)]
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// The nesting depth of saved prints (test/diagnostic probe).
    #[doc(hidden)]
    pub(crate) fn depth(&self) -> usize {
        match self {
            Self::Active { saves, .. } => saves.len(),
            Self::Idle { .. } => 0,
        }
    }
}

/// The World-owned print state (design rev 2 A3): GNU's `print_buffer`
/// accumulator plus the `print_number_index` counter that print.c keeps as a
/// separate file static (`src/print.c:89`).
///
/// `print_number_index` is an `Arc<AtomicI64>` so the printer can hold a
/// handle across a `print-unreadable-function` callback without borrowing the
/// World: a nested print on the SAME World (including one started by another
/// Lisp thread of that World) clones the same handle and advances the same
/// counter. That is GNU's single process static under the global lock. Ordering
/// is `Relaxed`: only the one World owner at a time touches it.
#[derive(Debug)]
pub(crate) struct WorldPrintState {
    pub(crate) buffer: WorldPrintBuffer,
    number_index: std::sync::Arc<std::sync::atomic::AtomicI64>,
}

impl Default for WorldPrintState {
    fn default() -> Self {
        Self {
            buffer: WorldPrintBuffer::new(),
            number_index: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
        }
    }
}

impl WorldPrintState {
    #[doc(hidden)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A handle to GNU's `print_number_index` for one print. Cloning the Arc
    /// never borrows the World, so the printer may hold it across callbacks.
    #[doc(hidden)]
    pub(crate) fn number_index_handle(&self) -> std::sync::Arc<std::sync::atomic::AtomicI64> {
        std::sync::Arc::clone(&self.number_index)
    }

    /// GNU's `print_number_index = 0` (print.c:1296-1303, :1524).
    #[doc(hidden)]
    pub(crate) fn reset_number_index(&self) {
        self.number_index
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The next label number from GNU's `print_number_index` (`src/print.c:1458`).
///
/// `index` is the World-owned counter for an evaluator-backed print; the
/// `None` fallback is the Context-less printer (`print_value_bytes` and the
/// diagnostics), which is not Lisp-observable and keeps the historical
/// thread-local counter until the P3 follow-up removes it.
#[doc(hidden)]
pub(crate) fn next_print_number_index(index: Option<&std::sync::atomic::AtomicI64>) -> i64 {
    match index {
        Some(cell) => cell.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
        None => crate::emacs_core::print::next_print_number_index(),
    }
}

/// The number of Emacs characters in a canonical internal-encoding byte run.
fn count_chars(bytes: &[u8]) -> usize {
    if bytes.is_ascii() {
        return bytes.len();
    }
    let mut chars = 0;
    let mut pos = 0;
    while pos < bytes.len() {
        let (_, len) = neovm_value::emacs_char::string_char(&bytes[pos..]);
        pos += len;
        chars += 1;
    }
    chars
}
