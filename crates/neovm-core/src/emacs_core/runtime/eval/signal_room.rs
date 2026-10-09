//! GNU's temporary evaluator-depth reserve for signal-time Lisp callbacks.

use super::*;
use crate::emacs_core::forward::LispInteger;

impl Context {
    /// Finish a rejected call while its depth is still counted, as in GNU
    /// eval.c:2604-2614, 3196-3205 and bytecode.c:779-788. VM callers must
    /// publish their operand cursor before this can run callback Lisp or GC.
    #[cold]
    #[inline(never)]
    pub(crate) fn finish_lisp_depth_overflow(&mut self, flow: Flow) -> Flow {
        let flow = self
            .dispatch_signal_flow_cold(flow)
            .expect_err("a rejected call cannot return a value");
        self.depth -= 1;
        flow
    }

    /// Borrow evaluator depth on this Context's mutator and record its return
    /// on the same Context's specpdl. No state is shared between mutators.
    ///
    /// GNU `max_ensure_room` (`src/eval.c:266-277`) writes the DEFVAR_INT
    /// cells directly: these temporary changes do not invoke variable watchers.
    #[cold]
    #[inline(never)]
    pub(crate) fn ensure_lisp_eval_depth_room(&mut self, room: i64) {
        let limit_id = max_lisp_eval_depth_symbol();
        let old_limit = self.signal_depth_integer_value(limit_id);
        let wanted = (self.depth as i64).saturating_add(room);
        // The visible cell can differ from max_depth's effective floor.
        // With no shortfall GNU's check neither borrows nor registers cleanup.
        if wanted <= old_limit {
            return;
        }
        let reserve_id = lisp_eval_depth_reserve_symbol();
        let old_reserve = self.signal_depth_integer_value(reserve_id);
        let borrowed = wanted.saturating_sub(old_limit).min(old_reserve);
        if borrowed <= 0 {
            return;
        }
        let new_limit = old_limit.saturating_add(borrowed);
        self.set_signal_depth_integer(limit_id, new_limit);
        self.set_signal_depth_integer(reserve_id, old_reserve - borrowed);
        self.max_depth = new_limit.max(100) as usize;
        self.record_native_unwind(NativeUnwindAction::RestoreEvalDepth { old_limit });
    }

    /// GNU `restore_stack_limits` (`src/eval.c:254-261`) returns the current
    /// limit's excess, including any change made by the callback itself.
    #[cold]
    #[inline(never)]
    pub(super) fn restore_lisp_eval_depth_room(&mut self, old_limit: i64) {
        let limit_id = max_lisp_eval_depth_symbol();
        let reserve_id = lisp_eval_depth_reserve_symbol();
        let current_limit = self.signal_depth_integer_value(limit_id);
        let current_reserve = self.signal_depth_integer_value(reserve_id);
        self.set_signal_depth_integer(
            reserve_id,
            current_reserve.saturating_add(current_limit.saturating_sub(old_limit)),
        );
        self.set_signal_depth_integer(limit_id, old_limit);
        self.max_depth = old_limit.max(100) as usize;
    }

    /// The current buffer's binding of a localized native depth variable.
    /// GNU swaps that binding into its C scalar (`data.c:1574-1605`); this
    /// runtime keeps the scalar and binding cons separately, so native signal
    /// writes must update the authoritative cons as well. The cache and cons
    /// belong to this Context's mutator; no reference escapes a signal-time
    /// operation or is shared with a concurrent Lisp mutator.
    #[cold]
    #[inline(never)]
    fn signal_depth_localized_cell(&self, id: SymId) -> Option<Value> {
        if !self.obarray.is_localized(id) {
            return None;
        }
        if let Some(buffer) = self.buffers.current_buffer() {
            self.obarray
                .read_localized_in_buffer(id, buffer)
                .expect("a localized depth variable has its current binding");
            Some(
                self.obarray
                    .blv(id)
                    .expect("localized depth variable")
                    .valcell,
            )
        } else {
            Some(
                self.obarray
                    .blv(id)
                    .expect("localized depth variable")
                    .defcell,
            )
        }
    }

    #[cold]
    #[inline(never)]
    fn signal_depth_integer_value(&self, id: SymId) -> i64 {
        match self.signal_depth_localized_cell(id) {
            Some(cell) => LispInteger::check(cell.cons_cdr())
                .expect("native depth variables hold integers")
                .as_i64(),
            None => self
                .obarray
                .int_forwarder(id)
                .expect("native depth variable has its integer cell")
                .get_i64(),
        }
    }

    /// Write the active binding without variable watchers or creation of an
    /// automatic local binding. Re-resolve it on restoration: GNU restores
    /// the active buffer's scalar even if the callback switched buffers,
    /// leaving the former buffer's borrowed binding intact.
    #[cold]
    #[inline(never)]
    fn set_signal_depth_integer(&mut self, id: SymId, value: i64) {
        let cell = self.signal_depth_localized_cell(id);
        let value = LispInteger::from_i64(value);
        self.obarray
            .int_forwarder(id)
            .expect("native depth variable has its integer cell")
            .set(value);
        if let Some(cell) = cell {
            cell.set_cdr(value.value());
        }
    }
}
