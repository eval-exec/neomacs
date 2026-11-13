//! The one production `TextHost` implementation (stage-3b R0).
//!
//! The text layer declares `TextHost: EditHost` (`emacs_core::text_host`) and
//! calls its methods generically; the evaluator implements it for `Context`
//! right here, so the trait never needs a second impl and the text side never
//! names `Context`. `variables()` returns this Context's `EditHost::Vars`
//! (`VariableView`), built from DISJOINT field borrows
//! (`&self.obarray`, `&self.owner`), never through a helper that borrows all
//! of `self` (edit V.2 rule), so the text phase holds only the variable state
//! and no stale view can outlive it. Static dispatch, never `dyn`, one
//! production impl. Precedent: `runtime/eval/edit_host.rs`.

use neovm_value::heap_types::{LispString, ProcessId};

use super::{Context, VariableView};
use neovm_edit::buffer::BufferManager;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_text::code_conversion_workspace::{CodeConversionBufferLease, ConversionBufferEncoding};
use neovm_text::emacs_core::coding::{CodingSystemManager, EolConversion};
use neovm_text::emacs_core::text_host::TextHost;
use neovm_text::encoding::{CodingEncoderState, RuntimeCodingSystem};
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::value::Value;

impl TextHost for Context {
    #[inline(always)]
    fn variables(&self) -> <Self as EditHost>::Vars<'_> {
        VariableView::new(&self.obarray, &self.owner)
    }

    #[inline(always)]
    fn buffers(&self) -> &BufferManager {
        &self.edit.buffers
    }

    #[inline(always)]
    fn buffers_mut(&mut self) -> &mut BufferManager {
        &mut self.edit.buffers
    }

    fn eval_sub(&mut self, form: Value) -> EvalResult {
        Context::eval_sub(self, form)
    }

    #[inline(always)]
    fn eol_conversion(&self) -> EolConversion {
        Context::eol_conversion(self)
    }

    #[inline(always)]
    fn coding_systems(&self) -> &CodingSystemManager {
        &self.coding_systems
    }

    fn process_encoder_state(
        &mut self,
        process: ProcessId,
    ) -> Result<&mut CodingEncoderState, Flow> {
        self.processes
            .get_any_mut(process)
            .and_then(|proc| proc.encoding_state.encoder_mut())
            .ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Process output coding is not initialized")],
                )
            })
    }

    fn process_encoding_coding(&mut self, process: ProcessId) -> Result<RuntimeCodingSystem, Flow> {
        // The process's encoding coding, re-read after a pre-write hook may
        // have replaced the descriptor (GNU send_process recalibration).
        self.processes
            .get_any(process)
            .and_then(|proc| proc.encoding_state.coding())
            .ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Process output coding is not initialized")],
                )
            })
    }

    fn process_encoding_bypasses(&mut self, process: ProcessId) -> bool {
        self.processes
            .get_any(process)
            .is_some_and(|proc| !proc.encoding_state.requires_encoding())
    }

    fn current_buffer_is_displayed(&self) -> bool {
        let Some(buffer_id) = self.edit.buffers.current_buffer_id() else {
            return false;
        };
        self.frames.selected_frame().is_some_and(|frame| {
            frame.window_list().into_iter().any(|window_id| {
                frame
                    .find_window(window_id)
                    .and_then(|window| window.buffer_id())
                    == Some(buffer_id)
            })
        })
    }

    fn acquire_code_conversion_buffer(
        &mut self,
        encoding: ConversionBufferEncoding,
    ) -> Result<CodeConversionBufferLease, Flow> {
        // GNU `code_conversion_save` through the evaluator so
        // `Fmake_local_variable`'s localized forwarding metadata is updated.
        // Body moved verbatim from code_conversion_workspace's impl Context
        // (same statement order and rooting); text owns the lifecycle row.
        let lease = self
            .code_conversion_workspace
            .acquire(&mut self.edit.buffers, encoding);
        if let Err(flow) = self.set_buffer_local_binding_by_id(
            lease.buffer_id(),
            neovm_value::intern::intern("inhibit-modification-hooks"),
            Value::T,
        ) {
            self.code_conversion_workspace
                .release(&mut self.edit.buffers, lease);
            return Err(flow);
        }
        Ok(lease)
    }

    fn set_code_conversion_buffer_contents(
        &mut self,
        lease: &CodeConversionBufferLease,
        text: &LispString,
    ) {
        self.code_conversion_workspace
            .replace_contents(&mut self.edit.buffers, lease, text);
    }

    fn release_code_conversion_buffer(&mut self, lease: CodeConversionBufferLease) {
        self.code_conversion_workspace
            .release(&mut self.edit.buffers, lease);
    }
}
