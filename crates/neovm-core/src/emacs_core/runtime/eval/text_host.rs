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

use super::{Context, VariableView};
use crate::buffer::BufferManager;
use crate::emacs_core::edit_host::EditHost;
use crate::emacs_core::text_host::TextHost;

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
}
