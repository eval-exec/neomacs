//! GNU-global state, owned by the World (round 3w, plan-3w.md).
//!
//! GNU keeps all of this in C globals (or KBOARD/terminal fields, not per
//! thread either), so every owner of a World must see the same values. The
//! World holds one [`GlobalState`]; each owner's Context reaches a field
//! through its [`WorldSlot`] (the one access path), and accessor-owned native fields
//! through the narrow methods below.

use super::*;
use crate::emacs_core::eval::data::minibuffer::MinibufferManager;
use crate::emacs_core::eval::data::minibuffer_entry::MinibufferManagerExt;
use std::ptr::NonNull;

macro_rules! global_state {
    ($($(#[$doc:meta])* $field:ident: $ty:ty,)*) => {
        /// The World's GNU-global state (plan-3w.md section 1.2; each
        /// field's GNU anchor is on it).
        pub(crate) struct GlobalState {
            $($(#[$doc])* pub(crate) $field: $ty,)*
            /// xdisp.c `message_log_need_newline`.
            pub(crate) message_log_need_newline: bool,
            /// xdisp.c `message_buf_print`.
            pub(crate) message_buf_print: bool,
            /// keyboard.c `interrupt_input`.
            pub(crate) input_mode_interrupt: bool,
            /// print.c `external_debugging_output`'s stream.
            pub(crate) debugging_output_file: Option<std::fs::File>,
            /// xdisp.c `mode_line_elt_no` and its active output scope. Accessor
            /// owned so adding formatter state does not move Context/JIT fields.
            pub(crate) mode_line_output: crate::emacs_core::eval::data::redisplay::ModeLineOutputOwner,
            /// print.c print state, one per World: the `print_buffer`
            /// accumulator every buffer/marker print shares, plus the
            /// `print_number_index` counter (design rev 2 A2/A3). PRIVATE to
            /// this module: the only route to the buffer is
            /// [`WorldPrintAccess`], minted from a live `&mut Context`
            /// (type-level World-lock proof). Per World (not
            /// process/TLS/Context): a World is Neo's isolation boundary.
            /// See [`super::print_buffer`].
            print_state: super::print_buffer::WorldPrintState,
        }

        /// One owner's slots over a World's [`GlobalState`].
        pub(crate) struct GlobalSlots {
            $(pub(crate) $field: WorldSlot<$ty>,)*
        }

        impl GlobalSlots {
            /// Slots over every field of the state at CELL.
            ///
            /// # Safety
            /// CELL is the World's stable `GlobalState` allocation; see
            /// [`WorldSlot::over`].
            #[doc(hidden)]
            pub unsafe fn over(cell: NonNull<GlobalState>) -> Self {
                let cell = cell.as_ptr();
                // SAFETY: each field pointer is derived from the whole
                // allocation (`addr_of_mut!` through CELL), never from a
                // reference, so it keeps the allocation's provenance.
                unsafe {
                    Self {
                        $($field: WorldSlot::over(NonNull::new_unchecked(
                            std::ptr::addr_of_mut!((*cell).$field),
                        )),)*
                    }
                }
            }
        }
    };
}

global_state! {
    /// `Vsystem_name` (sysdep.c).
    cached_system_name: Value,
    /// xwidget.c `internal_xwidget_list`.
    xwidgets: super::data::xwidget::XwidgetState,
    /// process.c `Vprocess_alist`, `chan_process[]`.
    processes: ProcessManager,
    /// keymap.c `current_global_map`.
    selected_global_map: crate::emacs_core::eval::keymap_lookup::SelectedGlobalMap,
    /// `register-alist` (a global variable).
    registers: RegisterManager,
    /// bookmark.el globals.
    bookmarks: BookmarkManager,
    /// abbrev.c `Vabbrev_table_name_list` and its global tables.
    abbrevs: AbbrevManager,
    /// `killed-rectangle` (a global variable).
    rectangle: RectangleState,
    /// callint.c: a function's `interactive` form, global.
    interactive: InteractiveRegistry,
    /// minibuf.c `minibuf_level`, `Vminibuffer_list`.
    minibuffers: MinibufferManager,
    /// minibuf.c reads (global with `minibuf_level`).
    interactive_minibuffer_read_count: u64,
    /// xdisp.c echo area message.
    current_message: Option<neovm_value::heap_types::LispString>,
    /// xdisp.c `echo_area_buffer[]`.
    echo_area_buffers: EchoAreaBuffers,
    /// minibuf.c `minibuf_selected_window`.
    minibuffer_selected_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    /// minibuf.c `minibuf_window` in use.
    active_minibuffer_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    /// emacs.c daemon state (process-global).
    daemon: Option<DaemonState>,
    /// keyboard.c `quit_char`.
    quit_char: i64,
    /// The major/minor mode registry (`minor-mode-alist`, global).
    modes: ModeRegistry,
    /// thread.c `all_threads`.
    threads: ThreadManager,
    /// KBOARD `kbd_macro_*` (per terminal, one terminal in Phase 0).
    kmacro: KmacroManager,
    /// keyboard.c `command_loop_level`, KBOARD input state.
    command_loop: crate::emacs_core::eval::data::command_loop::CommandLoop,
    /// xfaces.c `face_alist` / frame face caches.
    face_table: FaceTable,
    /// The face table's materialized source (xfaces.c frame caches).
    materialized_face_table_source: Option<(neovm_value::display_vocab::FrameId, u64)>,
    /// xmenu.c menu-bar rebuild generation.
    menu_bar_rebuild_generation: u64,
    /// xdisp.c last redisplay's inputs.
    last_redisplay_signature: Option<RedisplaySignature>,
    /// xdisp.c `safe_call` queue.
    pending_safe_funcalls: Vec<PendingSafeFuncall>,
    /// fringe.c `fringe_bitmaps[]`.
    fringe_bitmaps: super::super::builtins::fringe_bitmap::FringeBitmapRegistry,
    /// composite.c `gstring_hash_table`.
    gstring_shape_cache: HashMap<neovm_value::display_vocab::GstringShapeCacheKey, Value>,
}

impl GlobalState {
    /// A fresh World's state with the caller's drained-image fields applied.
    ///
    /// pdump reconstruction cannot use a struct literal here: the print
    /// accumulator is private to this module (design rev 2 A2), and Rust's
    /// functional-update syntax needs every field's visibility. Each field the
    /// image carries is still assigned by the caller through its normal
    /// visibility; the private print state keeps its fresh value.
    #[doc(hidden)]
    pub(crate) fn fresh_then(
        command_loop: crate::emacs_core::eval::data::command_loop::CommandLoop,
        apply: impl FnOnce(&mut Self),
    ) -> Self {
        let mut state = Self::fresh(command_loop);
        apply(&mut state);
        state
    }

    /// A new World's state, COMMAND_LOOP being its first owner's.
    #[doc(hidden)]
    pub fn fresh(command_loop: crate::emacs_core::eval::data::command_loop::CommandLoop) -> Self {
        Self {
            cached_system_name: Value::NIL,
            xwidgets: super::data::xwidget::XwidgetState::new(),
            processes: ProcessManager::new(),
            selected_global_map: crate::emacs_core::eval::keymap_lookup::SelectedGlobalMap::default(
            ),
            registers: RegisterManager::new(),
            bookmarks: BookmarkManager::new(),
            abbrevs: AbbrevManager::new(),
            rectangle: RectangleState::new(),
            interactive: InteractiveRegistry::new(),
            minibuffers: MinibufferManager::new(),
            interactive_minibuffer_read_count: 0,
            current_message: None,
            echo_area_buffers: EchoAreaBuffers::default(),
            minibuffer_selected_window: None,
            active_minibuffer_window: None,
            daemon: None,
            quit_char: 7,
            modes: ModeRegistry::new(),
            threads: ThreadManager::new(),
            kmacro: KmacroManager::new(),
            command_loop,
            face_table: FaceTable::new(),
            materialized_face_table_source: None,
            menu_bar_rebuild_generation: 0,
            last_redisplay_signature: None,
            pending_safe_funcalls: Vec::new(),
            fringe_bitmaps: super::super::builtins::fringe_bitmap::FringeBitmapRegistry::new(),
            gstring_shape_cache: HashMap::new(),
            print_state: super::print_buffer::WorldPrintState::new(),
            message_log_need_newline: false,
            message_buf_print: false,
            input_mode_interrupt: true,
            debugging_output_file: None,
            mode_line_output: Default::default(),
        }
    }
}

/// Proof that the World's exclusive print view is held; see
/// [`Context::world_print`]. The private field keeps this token the only
/// route to `&mut WorldPrintBuffer`.
#[doc(hidden)]
pub(crate) struct WorldPrintAccess<'ctx> {
    context: &'ctx mut Context,
}

static_assertions::assert_not_impl_any!(WorldPrintAccess<'static>: Send, Sync, Clone);

impl WorldPrintAccess<'_> {
    /// GNU `print_prepare`'s accumulator part. The returned token is the
    /// type-level proof every other accumulator operation requires.
    #[inline]
    pub(crate) fn prepare(&mut self) -> super::print_buffer::PrintActive {
        self.context.global_state_mut().print_state.prepare()
    }

    /// GNU `print_buffer`'s byte append.
    #[inline]
    pub(crate) fn append(&mut self, active: super::print_buffer::PrintActive, bytes: &[u8]) {
        self.context
            .global_state_mut()
            .print_state
            .buffer
            .append(active, bytes);
    }

    /// The bytes `print_finish` inserts: `buffer[0..pos_byte]`.
    #[inline]
    pub(crate) fn pending(&self, active: super::print_buffer::PrintActive) -> Vec<u8> {
        self.context
            .global_state()
            .print_state
            .buffer
            .pending(active)
            .to_vec()
    }

    /// The number of bytes the accumulator currently holds (GNU
    /// `print_buffer.pos_byte`). A caller that pre-filled the accumulator
    /// (e.g. `print`'s leading newline) makes the "install without a copy"
    /// path unsafe.
    #[inline]
    pub(crate) fn pending_len(&self, active: super::print_buffer::PrintActive) -> usize {
        self.context
            .global_state()
            .print_state
            .buffer
            .pending(active)
            .len()
    }

    /// Install a complete rendering as the accumulator content (no callback
    /// flushed during the render, so the cursor is untouched at 0).
    #[inline]
    pub(crate) fn replace_pending_bytes(
        &mut self,
        active: super::print_buffer::PrintActive,
        bytes: Vec<u8>,
    ) {
        self.context
            .global_state_mut()
            .print_state
            .buffer
            .replace_active_bytes(active, bytes);
    }

    /// GNU `print_finish`'s recorded unwind (`print_unwind`).
    #[inline]
    pub(crate) fn unwind(&mut self, active: super::print_buffer::PrintActive) {
        self.context
            .global_state_mut()
            .print_state
            .buffer
            .unwind(active);
    }

    /// The specpdl-recorded `PrintBufferUnwind` action: no token exists at
    /// that point (the entry was recorded by the only `prepare`), so the
    /// accumulator checks the Active state and fails loudly if it is Idle.
    #[inline]
    pub(crate) fn unwind_recorded(&mut self) {
        self.context
            .global_state_mut()
            .print_state
            .buffer
            .unwind_recorded();
    }

    /// A handle to GNU's `print_number_index` for one print; see
    /// [`super::print_buffer::WorldPrintState`].
    #[inline]
    pub(crate) fn number_index_handle(&self) -> std::sync::Arc<std::sync::atomic::AtomicI64> {
        self.context
            .global_state()
            .print_state
            .number_index_handle()
    }

    /// GNU's `print_number_index = 0` on the World-owned counter.
    #[inline]
    pub(crate) fn reset_number_index(&self) {
        self.context.global_state().print_state.reset_number_index();
    }

    /// Test/diagnostic probe: is a print in progress?
    #[inline]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn is_active(&self) -> bool {
        self.context.global_state().print_state.buffer.is_active()
    }

    /// Test/diagnostic probe: the nested-print save depth.
    #[inline]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn depth(&self) -> usize {
        self.context.global_state().print_state.buffer.depth()
    }
}

impl Context {
    #[doc(hidden)]
    pub fn mode_line_output_owner(
        &self,
    ) -> crate::emacs_core::eval::data::redisplay::ModeLineOutputOwner {
        self.global_state().mode_line_output.clone()
    }

    /// The World's GNU-global state, reached from this owner's `quit_char`
    /// slot (accessor-owned fields have no Context slot of their own).
    #[inline]
    #[doc(hidden)]
    pub fn global_state(&self) -> &GlobalState {
        let field = self.quit_char.cell().as_ptr();
        // SAFETY: every slot points at a field of the World's one
        // `GlobalState` with the allocation's provenance (`GlobalSlots::over`),
        // so stepping back by the field's offset names that allocation; the
        // World outlives this Context.
        unsafe {
            &*field
                .cast::<u8>()
                .sub(std::mem::offset_of!(GlobalState, quit_char))
                .cast::<GlobalState>()
        }
    }

    #[inline]
    #[doc(hidden)]
    pub fn global_state_mut(&mut self) -> &mut GlobalState {
        let field = self.quit_char.cell().as_ptr();
        // SAFETY: as `global_state`, with this owner borrowed exclusively.
        unsafe {
            &mut *field
                .cast::<u8>()
                .sub(std::mem::offset_of!(GlobalState, quit_char))
                .cast::<GlobalState>()
        }
    }

    /// A handle to the World-owned GNU `print_number_index` (design rev 2
    /// A3), for renderers that only hold `&Context` (the error/message
    /// printer). The handle never borrows the Context.
    #[inline]
    #[doc(hidden)]
    pub(crate) fn print_number_index_handle(&self) -> std::sync::Arc<std::sync::atomic::AtomicI64> {
        self.global_state().print_state.number_index_handle()
    }

    /// GNU `print()`'s `print_number_index = 0` (print.c:1296-1303), on the
    /// World-owned counter. Takes `&self`: the counter is an atomic and the
    /// World view is already held by the caller.
    #[inline]
    #[doc(hidden)]
    pub(crate) fn reset_print_number_index_world(&self) {
        self.global_state().print_state.reset_number_index();
    }

    /// The World's exclusive print-accumulator view (design rev 2 A2).
    ///
    /// This is the ONLY way to reach `&mut WorldPrintBuffer`: the field is
    /// private to this module, and the token can be minted only from a live
    /// `&mut Context` -- the sole World owner, or the holder of the Green
    /// Baton. Holding the token is therefore the type-level proof that the
    /// World's global-lock view is held; there is no unsynchronized path.
    /// The token is deliberately short-lived: the accumulator stays Active
    /// across a Lisp callback, carried by its specpdl unwind entry, never by
    /// a borrow of the Context.
    #[doc(hidden)]
    pub(crate) fn world_print(&mut self) -> WorldPrintAccess<'_> {
        WorldPrintAccess { context: self }
    }

    /// GNU's `face_change_count`: bumped on every face change.
    #[inline]
    pub fn face_change_count(&self) -> u64 {
        self.face_change_count
    }

    /// Bumped when a display-affecting variable changes.
    #[inline]
    pub fn display_var_change_count(&self) -> u64 {
        self.display_var_change_count
    }

    /// xdisp.c `message_log_need_newline`.
    #[inline]
    #[doc(hidden)]
    pub fn message_log_need_newline(&self) -> bool {
        self.global_state().message_log_need_newline
    }

    #[inline]
    #[doc(hidden)]
    pub fn set_message_log_need_newline(&mut self, value: bool) {
        self.global_state_mut().message_log_need_newline = value;
    }

    /// xdisp.c `message_buf_print`.
    #[inline]
    #[doc(hidden)]
    pub fn message_buf_print(&self) -> bool {
        self.global_state().message_buf_print
    }

    #[inline]
    #[doc(hidden)]
    pub fn set_message_buf_print(&mut self, value: bool) {
        self.global_state_mut().message_buf_print = value;
    }

    /// keyboard.c `interrupt_input`.
    #[inline]
    #[doc(hidden)]
    pub fn input_mode_interrupt(&self) -> bool {
        self.global_state().input_mode_interrupt
    }

    #[inline]
    #[doc(hidden)]
    pub fn set_input_mode_interrupt_flag(&mut self, value: bool) {
        self.global_state_mut().input_mode_interrupt = value;
    }

    /// print.c `external_debugging_output`'s stream.
    #[inline]
    #[doc(hidden)]
    pub fn debugging_output_file_mut(&mut self) -> &mut Option<std::fs::File> {
        &mut self.global_state_mut().debugging_output_file
    }
}

#[cfg(test)]
#[path = "tests/context_layout_test.rs"]
mod context_layout_tests;
