//! GNU-global state, owned by the World (round 3w, plan-3w.md).
//!
//! GNU keeps all of this in C globals (or KBOARD/terminal fields, not per
//! thread either), so every owner of a World must see the same values. The
//! World holds one [`GlobalState`]; each owner's Context reaches a field
//! through its [`WorldSlot`] (the one access path), and accessor-owned native fields
//! through the narrow methods below.

use super::*;
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
            pub(crate) mode_line_output: crate::emacs_core::xdisp::ModeLineOutputOwner,
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
            pub(super) unsafe fn over(cell: NonNull<GlobalState>) -> Self {
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
    xwidgets: super::super::xwidget::XwidgetState,
    /// process.c `Vprocess_alist`, `chan_process[]`.
    processes: ProcessManager,
    /// keymap.c `current_global_map`.
    selected_global_map: super::super::keymap::SelectedGlobalMap,
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
    current_message: Option<crate::heap_types::LispString>,
    /// xdisp.c `echo_area_buffer[]`.
    echo_area_buffers: EchoAreaBuffers,
    /// minibuf.c `minibuf_selected_window`.
    minibuffer_selected_window: Option<crate::window::WindowId>,
    /// minibuf.c `minibuf_window` in use.
    active_minibuffer_window: Option<crate::window::WindowId>,
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
    command_loop: crate::keyboard::CommandLoop,
    /// xfaces.c `face_alist` / frame face caches.
    face_table: FaceTable,
    /// The face table's materialized source (xfaces.c frame caches).
    materialized_face_table_source: Option<(crate::window::FrameId, u64)>,
    /// xmenu.c menu-bar rebuild generation.
    menu_bar_rebuild_generation: u64,
    /// xdisp.c last redisplay's inputs.
    last_redisplay_signature: Option<RedisplaySignature>,
    /// xdisp.c `safe_call` queue.
    pending_safe_funcalls: Vec<PendingSafeFuncall>,
    /// fringe.c `fringe_bitmaps[]`.
    fringe_bitmaps: super::super::builtins::fringe_bitmap::FringeBitmapRegistry,
    /// composite.c `gstring_hash_table`.
    gstring_shape_cache: HashMap<crate::emacs_core::font::GstringShapeCacheKey, Value>,
}

impl GlobalState {
    /// A new World's state, COMMAND_LOOP being its first owner's.
    pub(super) fn fresh(command_loop: crate::keyboard::CommandLoop) -> Self {
        Self {
            cached_system_name: Value::NIL,
            xwidgets: super::super::xwidget::XwidgetState::new(),
            processes: ProcessManager::new(),
            selected_global_map: super::super::keymap::SelectedGlobalMap::default(),
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
            message_log_need_newline: false,
            message_buf_print: false,
            input_mode_interrupt: true,
            debugging_output_file: None,
            mode_line_output: Default::default(),
        }
    }
}

impl Context {
    pub(crate) fn mode_line_output_owner(&self) -> crate::emacs_core::xdisp::ModeLineOutputOwner {
        self.global_state().mode_line_output.clone()
    }

    /// The World's GNU-global state, reached from this owner's `quit_char`
    /// slot (accessor-owned fields have no Context slot of their own).
    #[inline]
    fn global_state(&self) -> &GlobalState {
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
    fn global_state_mut(&mut self) -> &mut GlobalState {
        let field = self.quit_char.cell().as_ptr();
        // SAFETY: as `global_state`, with this owner borrowed exclusively.
        unsafe {
            &mut *field
                .cast::<u8>()
                .sub(std::mem::offset_of!(GlobalState, quit_char))
                .cast::<GlobalState>()
        }
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
    pub(crate) fn message_log_need_newline(&self) -> bool {
        self.global_state().message_log_need_newline
    }

    #[inline]
    pub(crate) fn set_message_log_need_newline(&mut self, value: bool) {
        self.global_state_mut().message_log_need_newline = value;
    }

    /// xdisp.c `message_buf_print`.
    #[inline]
    pub(crate) fn message_buf_print(&self) -> bool {
        self.global_state().message_buf_print
    }

    #[inline]
    pub(crate) fn set_message_buf_print(&mut self, value: bool) {
        self.global_state_mut().message_buf_print = value;
    }

    /// keyboard.c `interrupt_input`.
    #[inline]
    pub(crate) fn input_mode_interrupt(&self) -> bool {
        self.global_state().input_mode_interrupt
    }

    #[inline]
    pub(crate) fn set_input_mode_interrupt_flag(&mut self, value: bool) {
        self.global_state_mut().input_mode_interrupt = value;
    }

    /// print.c `external_debugging_output`'s stream.
    #[inline]
    pub(crate) fn debugging_output_file_mut(&mut self) -> &mut Option<std::fs::File> {
        &mut self.global_state_mut().debugging_output_file
    }
}

#[cfg(test)]
#[path = "tests/context_layout_test.rs"]
mod context_layout_tests;
