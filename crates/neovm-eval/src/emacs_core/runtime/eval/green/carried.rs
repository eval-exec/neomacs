//! GNU-global state that lives in thread-locals (round 3w, plan-3w.md
//! section 7, C1/C2): the Baton holder's thread-locals hold the World's
//! value, and the World holds it while the holder is parked. A park swaps it
//! out of the parking thread's thread-locals for placeholders; the next
//! resume (or a new thread's start) swaps it back in, on whichever OS thread
//! resumes, and keeps that thread's placeholders for the next park. So a
//! switch moves values and allocates nothing. Sim Worlds never move it, so
//! their thread-local fast paths are unchanged.

use std::any::Any;

/// A piece's placeholder, which the thread-locals hold while the World is
/// elsewhere.
type Placeholder = fn() -> Box<dyn Any>;
/// Exchange a piece with this OS thread's thread-locals.
type Swap = fn(&mut dyn Any);

/// Every carried piece of World state, with its GNU anchor.
const CARRIERS: &[(Placeholder, Swap)] = &[
    // composite.c `composition_table`.
    (
        neovm_text::emacs_core::composite::world_state_placeholder,
        neovm_text::emacs_core::composite::swap_world_state,
    ),
    // process.c process names (`Vprocess_alist`).
    (
        crate::emacs_core::eval::data::process_state::world_state_placeholder,
        crate::emacs_core::eval::data::process_state::swap_world_state,
    ),
    // sqlite.c connections and statements (Lisp objects, global).
    (
        crate::emacs_core::eval::lifecycle::sqlite_world_state_placeholder,
        crate::emacs_core::eval::lifecycle::sqlite_swap_world_state,
    ),
    // dbusbind.c buses and registered fds.
    (
        crate::emacs_core::eval::lifecycle::dbusbind_world_state_placeholder,
        crate::emacs_core::eval::lifecycle::dbusbind_swap_world_state,
    ),
    // The clipboard, the primary selection and the monitor list.
    (
        crate::emacs_core::builtins::selection_world_state_placeholder,
        crate::emacs_core::builtins::swap_selection_world_state,
    ),
    // keyboard.c `lossage_limit`.
    (
        crate::emacs_core::builtins::symbols::world_state_placeholder,
        crate::emacs_core::builtins::symbols::swap_world_state,
    ),
    // undo.c's compaction latch.
    (
        neovm_edit::emacs_core::undo::world_state_placeholder,
        neovm_edit::emacs_core::undo::swap_world_state,
    ),
    // terminal.c `terminal_list`.
    (
        crate::emacs_core::eval::lifecycle::terminal_world_state_placeholder,
        crate::emacs_core::eval::lifecycle::terminal_swap_world_state,
    ),
    // alloc.c consing counters (`memory-use-counts`).
    (
        neovm_value::value::world_state_placeholder,
        neovm_value::value::swap_world_state,
    ),
    // World-owned collection revision, journal and capture state.
    (
        neovm_value::tagged::collection_reads::world_state_placeholder,
        neovm_value::tagged::collection_reads::swap_world_state,
    ),
    // Property-list write revision (observation).
    (
        neovm_value::symbol::property_revision_world_state_placeholder,
        neovm_value::symbol::swap_property_revision_world_state,
    ),
    // Display-table and window-parameter layout epochs.
    (
        crate::emacs_core::eval::data::window_state::world_state_placeholder,
        crate::emacs_core::eval::data::window_state::swap_world_state,
    ),
    // The time-zone rule (GNU's TZ, process-wide).
    (
        crate::emacs_core::eval::lifecycle::timefns_world_state_placeholder,
        crate::emacs_core::eval::lifecycle::timefns_swap_world_state,
    ),
];

/// The World's thread-local state while its holder is parked, and the
/// placeholders the running holder's thread-locals gave up for it otherwise.
pub(crate) struct CarriedState {
    pieces: Vec<Box<dyn Any>>,
    /// Whether `pieces` holds the World's state (its holder is parked).
    holds_world: bool,
}

impl std::fmt::Debug for CarriedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CarriedState")
            .field("pieces", &self.pieces.len())
            .field("holds_world", &self.holds_world)
            .finish()
    }
}

impl CarriedState {
    /// Placeholders, while the World's state is in the running holder's
    /// thread-locals (when the Green runtime starts).
    #[cold]
    pub(crate) fn placeholders() -> Self {
        Self {
            pieces: CARRIERS
                .iter()
                .map(|(placeholder, _)| placeholder())
                .collect(),
            holds_world: false,
        }
    }

    /// Take the World's state out of this OS thread's thread-locals.
    #[cold]
    pub(crate) fn stow(&mut self) {
        assert!(
            !self.holds_world,
            "the World's carried state is in two places"
        );
        self.swap();
        self.holds_world = true;
    }

    /// Put the World's state into this OS thread's thread-locals.
    #[cold]
    pub(crate) fn unstow(&mut self) {
        if self.holds_world {
            self.swap();
            self.holds_world = false;
        }
    }

    #[doc(hidden)]
    pub fn swap(&mut self) {
        for ((_, swap), piece) in CARRIERS.iter().zip(&mut self.pieces) {
            swap(&mut **piece);
        }
    }
}

#[cfg(test)]
mod carriers_round_trip {
    use super::CARRIERS;
    use crate::emacs_core::eval::{Context, ThreadingMode};

    /// Every carrier a green park executes, including one added later.
    /// A panicking stub fails here instead of in the 213-test bucket.
    #[test]
    fn every_carrier_round_trips_in_the_unit_runtime() {
        let _green =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        assert!(!CARRIERS.is_empty());
        for (placeholder, swap) in CARRIERS {
            let mut slot = placeholder();
            swap(&mut *slot);
            swap(&mut *slot);
        }
    }
}

/// Empty carried slots for the unlinked upper layers (peer rulings 5685/5690).
///
/// A park stores the placeholder and a resume swaps it back: the same pair
/// `green/carried.rs` runs for sqlite, dbus, terminal, and timefns.
#[cfg(test)]
mod absent_carried_world_state {
    use std::any::Any;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use crate::emacs_core::eval::lifecycle::{
        dbusbind_swap_world_state, dbusbind_world_state_placeholder,
        live_terminal_ids_in_keyboard_poll_order, sqlite_swap_world_state,
        sqlite_world_state_placeholder, terminal_swap_world_state,
        terminal_world_state_placeholder, timefns_swap_world_state,
        timefns_world_state_placeholder,
    };
    use crate::emacs_core::eval::{Context, ThreadingMode};

    fn green() -> Context {
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
    }

    fn panic_text(payload: Box<dyn Any + Send>) -> String {
        match payload.downcast::<String>() {
            Ok(text) => *text,
            Err(payload) => match payload.downcast::<&str>() {
                Ok(text) => (*text).to_owned(),
                Err(_) => "<non-string panic>".to_owned(),
            },
        }
    }

    #[test]
    fn a_green_park_resume_round_trips_the_empty_slots() {
        let _green = green();
        let carriers: [(fn() -> Box<dyn Any>, fn(&mut dyn Any)); 4] = [
            (sqlite_world_state_placeholder, sqlite_swap_world_state),
            (dbusbind_world_state_placeholder, dbusbind_swap_world_state),
            (terminal_world_state_placeholder, terminal_swap_world_state),
            (timefns_world_state_placeholder, timefns_swap_world_state),
        ];
        for (placeholder, swap) in carriers {
            let mut slot = placeholder();
            swap(&mut *slot);
            swap(&mut *slot);
        }
    }

    #[test]
    fn a_terminal_behaviour_hook_still_panics_absent() {
        let _green = green();
        let err = catch_unwind(AssertUnwindSafe(|| {
            let _ = live_terminal_ids_in_keyboard_poll_order();
        }));
        let text = panic_text(err.expect_err("terminal live-ids must still panic"));
        assert!(
            text.contains("ABSENT-LAYER display::terminal.live_terminal_ids"),
            "terminal hook panic was {text}"
        );
    }

    #[test]
    fn a_sqlite_subr_still_panics_absent() {
        let mut ctx = green();
        let err = catch_unwind(AssertUnwindSafe(|| {
            let _ = ctx.eval_str("(sqlitep nil)");
        }));
        let text = panic_text(err.expect_err("sqlitep must still panic"));
        assert!(
            text.contains("ABSENT-LAYER system::sqlitep"),
            "sqlite subr panic was {text}"
        );
    }

    #[test]
    fn a_mismatched_slot_panics() {
        let _green = green();
        let swaps: [(fn(&mut dyn Any), &str); 4] = [
            (
                sqlite_swap_world_state,
                "ABSENT-LAYER system::sqlite.world_state_swap (non-empty slot)",
            ),
            (
                dbusbind_swap_world_state,
                "ABSENT-LAYER system::dbusbind.world_state_swap (non-empty slot)",
            ),
            (
                terminal_swap_world_state,
                "ABSENT-LAYER display::terminal.world_state_swap (non-empty slot)",
            ),
            (
                timefns_swap_world_state,
                "ABSENT-LAYER system::timefns.world_state_swap (non-empty slot)",
            ),
        ];
        for (swap, marker) in swaps {
            let err = catch_unwind(AssertUnwindSafe(|| {
                let mut foreign: Box<dyn Any> = Box::new(0x5151_u64);
                swap(&mut *foreign);
            }));
            let text = panic_text(err.expect_err(marker));
            assert!(text.contains(marker), "panic was {text}, wanted {marker}");
        }
    }
}
