//! Evaluator-thread text conversion; platform byte counts never index Lisp text directly.

impl crate::Context {
    pub(crate) fn delete_ime_surrounding(
        &mut self,
        mut before: usize,
        mut after: usize,
    ) -> crate::emacs_core::error::EvalResult {
        let Some(buffer) = self.buffers.current_buffer() else {
            return Ok(crate::Value::NIL);
        };
        let accessible = buffer.accessible_emacs_byte_region();
        let mut start = buffer.point_emacs_byte_pos();
        let mut end = start;
        while before > 0 && start > accessible.start() {
            let Some(character) = buffer.char_before_emacs_byte_pos(start) else {
                break;
            };
            let Some(remaining) = before.checked_sub(character.len_utf8()) else {
                break;
            };
            let Some(length) = buffer.char_before_emacs_byte_len(start) else {
                break;
            };
            start = start.saturating_sub_len(length);
            before = remaining;
        }
        while after > 0 && end < accessible.end() {
            let Some(character) = buffer.char_after_emacs_byte_pos(end) else {
                break;
            };
            let Some(remaining) = after.checked_sub(character.len_utf8()) else {
                break;
            };
            let Some(length) = buffer.char_after_emacs_byte_len(end) else {
                break;
            };
            end = end.add_len(length);
            after = remaining;
        }
        let start = crate::Value::fixnum(buffer.emacs_byte_pos_to_lisp_char_pos(start).as_i64());
        let end = crate::Value::fixnum(buffer.emacs_byte_pos_to_lisp_char_pos(end).as_i64());
        // Reuse normal editing semantics: narrowing, read-only checks, change
        // hooks, undo and marker movement. Android callbacks never touch Context.
        crate::emacs_core::editfns::builtin_delete_region_2(self, start, end)
    }
}
