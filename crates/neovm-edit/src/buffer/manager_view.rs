//! Non-swappable exclusive view capabilities for [`BufferManager`] and
//! [`EditRuntime`].
//!
//! `BufferManager` owns the `ColdWorldLease` and per-context buffer selection
//! for its `World`; `EditRuntime` contains `BufferManager` inline alongside
//! `insdel.c` change-combining state. Handing out a raw `&mut BufferManager`
//! or `&mut EditRuntime` allows safe code to `std::mem::swap` two contexts'
//! managers (or swap in a fresh `BufferManager::new()`), cross-linking
//! `World`-owned storage leases and marker/overlay ownership. Likewise,
//! handing out a raw `&mut Buffer` via safe methods allows `std::mem::replace`
//! on a buffer element.
//!
//! `BufferManagerMut` and `EditRuntimeMut` close that surface at compile time:
//!
//! * Both implement `Deref` for shared reads, and deliberately omit
//!   `DerefMut` and any `into_manager` / `into_runtime` escape.
//! * Both forward only the audited in-place mutating methods that callers use.
//! * `BufferManagerMut` exposes at most one `unsafe` element escape
//!   ([`BufferManagerMut::with_buffer_mut_no_swap`]) for sites that do not yet
//!   have a dedicated in-place method; it is closure-scoped with pre/post
//!   `debug_assert!` slot-identity checks, and production call sites are capped
//!   by a shrink-only architecture ratchet.

use std::ops::Deref;

use neovm_value::error::Flow;
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::header::MarkerObj;
use neovm_value::value::{RuntimeBindingValue, Value};

use super::buffer::{
    AccessibleBufferRegionSnapshot, BUFFER_SLOT_COUNT, Buffer, BufferId, BufferManager, BufferSlot,
    BufferSlotInfo, InsertionType, MarkerRef, OutermostRestrictionResetState,
    SavedRestrictionState, UndoBoundaryOutcome,
};
use super::edit_runtime::EditRuntime;
use super::edit_transaction::TranspositionAnchorPolicy;
use super::overlay::OverlayModificationHook;
use super::position::{CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1, TextPositionAnchor};
use super::text::backend::ImplementedBufferTextBackendKind;
use super::text_props::CasingPropertyMode;
use super::undo::undo_list_boundary;
use super::{CasifyExpansion, CasifyStorageShape, TextEditRange, TextExtent, TextTransposition};

/// Non-swappable exclusive capability over a [`BufferManager`].
///
/// Implements [`Deref`] for read access to [`BufferManager`], but deliberately
/// omits `DerefMut` and any `&mut BufferManager` unwrap so safe code cannot
/// `std::mem::swap` or `std::mem::replace` the manager across contexts.
#[must_use = "dropping a BufferManagerMut performs no buffer mutation"]
#[derive(Debug)]
pub struct BufferManagerMut<'a> {
    manager: &'a mut BufferManager,
}

static_assertions::assert_not_impl_any!(BufferManagerMut<'static>: Send, Sync, Copy, Clone);

impl<'a> BufferManagerMut<'a> {
    /// Wrap a mutable [`BufferManager`] borrow in a non-swappable capability.
    #[doc(hidden)]
    #[inline(always)]
    pub fn new(manager: &'a mut BufferManager) -> Self {
        Self { manager }
    }

    /// Reborrow this capability for a shorter lifetime without exposing
    /// `&mut BufferManager`.
    #[inline(always)]
    pub fn reborrow(&mut self) -> BufferManagerMut<'_> {
        BufferManagerMut {
            manager: self.manager,
        }
    }

    /// Audited, closure-scoped escape hatch to a mutable [`Buffer`] element
    /// when no in-place method on [`BufferManagerMut`] covers the mutation yet.
    ///
    /// Closure scoping prevents returning `&mut Buffer` past the call site and
    /// allows debug builds to re-verify slot identity (`id` and `undo_state`)
    /// after `f` returns. Production call sites are capped by a shrink-only
    /// architecture ratchet (`buffer_manager_view_ratchet_test`).
    ///
    /// # Safety
    ///
    /// The closure `f` must use the provided `&mut Buffer` strictly for
    /// in-place mutations of the existing buffer slot in this [`BufferManager`]:
    ///
    /// 1. Unconditionally do not `std::mem::swap`, `std::mem::replace`, or move
    ///    `*Buffer` out of its slot — neither with another `BufferManager`,
    ///    another `World`, a standalone [`Buffer::new_standalone`], nor a
    ///    same-`World` temporary (including across GC). Replacing or moving the
    ///    slot breaks the `World`-owned storage lease and marker/overlay
    ///    invariants.
    /// 2. Do not derive a raw pointer from the `&mut Buffer` that is
    ///    dereferenced after `f` returns.
    #[inline]
    pub unsafe fn with_buffer_mut_no_swap<R>(
        &mut self,
        id: BufferId,
        f: impl FnOnce(&mut Buffer) -> R,
    ) -> Option<R> {
        let buf = self.manager.get_mut(id)?;
        #[cfg(debug_assertions)]
        let expected_id = buf.id;
        #[cfg(debug_assertions)]
        let expected_undo = buf.undo_state.clone();
        #[cfg(debug_assertions)]
        debug_assert_eq!(expected_id, id, "buffer slot id mismatch on entry");
        let result = f(buf);
        #[cfg(debug_assertions)]
        if let Some(after) = self.manager.get(id) {
            debug_assert_eq!(
                after.id, expected_id,
                "with_buffer_mut_no_swap caller violated # Safety: buffer slot id changed"
            );
            debug_assert!(
                after.undo_state.shares_with(&expected_undo),
                "with_buffer_mut_no_swap caller violated # Safety: buffer slot was replaced or swapped"
            );
        }
        Some(result)
    }

    #[inline]
    pub fn create_buffer(&mut self, name: &str) -> BufferId {
        self.manager.create_buffer(name)
    }

    #[inline]
    pub fn create_buffer_with_hook_inhibition(
        &mut self,
        name: &str,
        inhibit_buffer_hooks: bool,
    ) -> BufferId {
        self.manager
            .create_buffer_with_hook_inhibition(name, inhibit_buffer_hooks)
    }

    #[inline]
    pub fn note_buffer_order_tail(&mut self, id: BufferId) -> bool {
        self.manager.note_buffer_order_tail(id)
    }

    #[inline]
    pub fn note_buffer_order_after(&mut self, id: BufferId, after: BufferId) -> bool {
        self.manager.note_buffer_order_after(id, after)
    }

    #[inline]
    pub fn note_buffer_display(&mut self, id: BufferId) {
        self.manager.note_buffer_display(id)
    }

    #[inline]
    pub fn switch_current(&mut self, id: BufferId) -> bool {
        self.manager.switch_current(id)
    }

    #[inline]
    pub fn switch_current_unrecorded(&mut self, id: BufferId) -> bool {
        self.manager.switch_current_unrecorded(id)
    }

    #[inline]
    pub fn set_current(&mut self, id: BufferId) {
        self.manager.set_current(id)
    }

    #[inline]
    pub fn kill_buffer(&mut self, id: BufferId) -> bool {
        self.manager.kill_buffer(id)
    }

    #[inline]
    pub fn kill_buffer_collect(&mut self, id: BufferId) -> Option<Vec<BufferId>> {
        self.manager.kill_buffer_collect(id)
    }

    #[inline]
    pub fn buffer_list_cursor(&mut self) -> impl Iterator<Item = BufferId> + use<> {
        self.manager.buffer_list_cursor()
    }

    #[inline]
    pub fn clear_buffer_labeled_restrictions(&mut self, buffer_id: BufferId) -> Option<()> {
        self.manager.clear_buffer_labeled_restrictions(buffer_id)
    }

    #[inline]
    pub fn goto_buffer_emacs_byte_pos(
        &mut self,
        id: BufferId,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.manager.goto_buffer_emacs_byte_pos(id, pos)
    }

    #[inline]
    pub fn set_buffer_point_anchor(
        &mut self,
        id: BufferId,
        point: TextPositionAnchor,
    ) -> Option<TextPositionAnchor> {
        self.manager.set_buffer_point_anchor(id, point)
    }

    #[inline]
    pub fn note_overlay_modification(&mut self, id: BufferId) -> Option<()> {
        self.manager.note_overlay_modification(id)
    }

    #[inline]
    pub fn delete_all_buffer_overlays(&mut self, id: BufferId) -> Option<()> {
        self.manager.delete_all_buffer_overlays(id)
    }

    #[inline]
    pub fn replace_buffer_contents(&mut self, id: BufferId, text: &str) -> Option<()> {
        self.manager.replace_buffer_contents(id, text)
    }

    #[inline]
    pub fn replace_buffer_contents_lisp_string(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        self.manager.replace_buffer_contents_lisp_string(id, text)
    }

    #[inline]
    pub fn clear_buffer_local_properties(
        &mut self,
        id: BufferId,
        obarray: &mut Obarray,
        kill_permanent: bool,
    ) -> Option<()> {
        self.manager
            .clear_buffer_local_properties(id, obarray, kill_permanent)
    }

    #[inline]
    pub fn clear_inserted_plain_text_properties_in_char_range(
        &mut self,
        id: BufferId,
        char_range: CharRange,
    ) -> Option<()> {
        self.manager
            .clear_inserted_plain_text_properties_in_char_range(id, char_range)
    }

    #[inline]
    pub fn set_buffer_multibyte_flag(&mut self, id: BufferId, flag: bool) -> Option<()> {
        self.manager.set_buffer_multibyte_flag(id, flag)
    }

    #[inline]
    pub fn set_buffer_modified_flag(&mut self, id: BufferId, flag: bool) -> Option<()> {
        self.manager.set_buffer_modified_flag(id, flag)
    }

    #[inline]
    pub fn set_buffer_auto_saved(&mut self, id: BufferId) -> Option<()> {
        self.manager.set_buffer_auto_saved(id)
    }

    #[inline]
    pub fn set_buffer_file_name(&mut self, id: BufferId, file_name: Value) -> Option<()> {
        self.manager.set_buffer_file_name(id, file_name)
    }

    #[inline]
    pub fn rename_buffer(&mut self, id: BufferId, name: Value) -> Option<()> {
        self.manager.rename_buffer(id, name)
    }

    #[inline]
    pub fn set_buffer_mark_emacs_byte_pos(
        &mut self,
        id: BufferId,
        pos: EmacsBytePos,
    ) -> Option<()> {
        self.manager.set_buffer_mark_emacs_byte_pos(id, pos)
    }

    #[inline]
    pub fn set_buffer_last_window_start(
        &mut self,
        id: BufferId,
        lisp_char_pos: LispCharPos1,
    ) -> Option<()> {
        self.manager.set_buffer_last_window_start(id, lisp_char_pos)
    }

    #[inline]
    pub fn clear_buffer_mark(&mut self, id: BufferId) -> Option<()> {
        self.manager.clear_buffer_mark(id)
    }

    #[inline]
    pub fn set_buffer_local_property(
        &mut self,
        id: BufferId,
        name: &str,
        value: Value,
    ) -> Option<()> {
        self.manager.set_buffer_local_property(id, name, value)
    }

    #[inline]
    pub fn set_buffer_local_property_by_sym_id(
        &mut self,
        id: BufferId,
        sym_id: SymId,
        value: Value,
    ) -> Option<()> {
        self.manager
            .set_buffer_local_property_by_sym_id(id, sym_id, value)
    }

    #[inline]
    pub fn set_current_local_map(&mut self, keymap: Value) -> Option<()> {
        self.manager.set_current_local_map(keymap)
    }

    #[inline]
    pub fn set_buffer_local_void_property_by_sym_id(
        &mut self,
        id: BufferId,
        sym_id: SymId,
    ) -> Option<()> {
        self.manager
            .set_buffer_local_void_property_by_sym_id(id, sym_id)
    }

    #[inline]
    pub fn remove_buffer_local_property(
        &mut self,
        id: BufferId,
        name: &str,
    ) -> Option<Option<RuntimeBindingValue>> {
        self.manager.remove_buffer_local_property(id, name)
    }

    #[inline]
    pub fn add_undo_boundary(&mut self, id: BufferId) -> Option<UndoBoundaryOutcome> {
        self.manager.add_undo_boundary(id)
    }

    #[inline]
    pub fn record_undo_point_before_command(&mut self, id: BufferId) -> Option<()> {
        self.manager.record_undo_point_before_command(id)
    }

    #[inline]
    pub fn restore_buffer_emacs_byte_restriction(
        &mut self,
        id: BufferId,
        range: EmacsByteRange,
    ) -> Option<()> {
        self.manager
            .restore_buffer_emacs_byte_restriction(id, range)
    }

    #[inline]
    pub fn restore_buffer_accessible_region(
        &mut self,
        id: BufferId,
        snapshot: AccessibleBufferRegionSnapshot,
    ) -> Option<()> {
        self.manager.restore_buffer_accessible_region(id, snapshot)
    }

    #[inline]
    pub fn restore_buffer_accessible_region_with_current_full_end(
        &mut self,
        id: BufferId,
        snapshot: AccessibleBufferRegionSnapshot,
    ) -> Option<()> {
        self.manager
            .restore_buffer_accessible_region_with_current_full_end(id, snapshot)
    }

    #[inline]
    pub fn save_current_restriction_state(&mut self) -> Option<SavedRestrictionState> {
        self.manager.save_current_restriction_state()
    }

    #[inline]
    pub fn reset_outermost_restrictions(&mut self) -> OutermostRestrictionResetState {
        self.manager.reset_outermost_restrictions()
    }

    #[inline]
    pub fn restore_outermost_restrictions(&mut self, state: OutermostRestrictionResetState) {
        self.manager.restore_outermost_restrictions(state)
    }

    #[inline]
    pub fn restore_saved_restriction_state(&mut self, saved: SavedRestrictionState) {
        self.manager.restore_saved_restriction_state(saved)
    }

    #[inline]
    pub fn internal_labeled_narrow_to_emacs_byte_range(
        &mut self,
        buffer_id: BufferId,
        range: EmacsByteRange,
        label: Value,
    ) -> Option<()> {
        self.manager
            .internal_labeled_narrow_to_emacs_byte_range(buffer_id, range, label)
    }

    #[inline]
    pub fn internal_labeled_widen(&mut self, buffer_id: BufferId, label: &Value) -> Option<()> {
        self.manager.internal_labeled_widen(buffer_id, label)
    }

    #[inline]
    pub fn configure_buffer_undo_list(&mut self, id: BufferId, value: Value) -> Option<()> {
        self.manager.configure_buffer_undo_list(id, value)
    }

    #[inline]
    pub fn create_marker_at_anchor(
        &mut self,
        buffer_id: BufferId,
        position: TextPositionAnchor,
        insertion_type: InsertionType,
    ) -> *mut MarkerObj {
        self.manager
            .create_marker_at_anchor(buffer_id, position, insertion_type)
    }

    #[inline]
    pub fn move_marker_ref_to_anchor(
        &mut self,
        buffer_id: BufferId,
        marker: MarkerRef,
        position: TextPositionAnchor,
    ) -> Option<()> {
        self.manager
            .move_marker_ref_to_anchor(buffer_id, marker, position)
    }

    #[inline]
    pub fn set_buffer_default_slot(&mut self, info: &BufferSlotInfo, value: Value) {
        self.manager.set_buffer_default_slot(info, value)
    }

    #[inline]
    pub fn remove_marker(&mut self, marker: MarkerRef) {
        self.manager.remove_marker(marker)
    }

    #[inline]
    pub fn insert_into_buffer(&mut self, id: BufferId, text: &str) -> Option<()> {
        self.manager.insert_into_buffer(id, text)
    }

    #[inline]
    pub fn insert_lisp_string_into_buffer(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        self.manager.insert_lisp_string_into_buffer(id, text)
    }

    #[inline]
    pub fn insert_into_buffer_before_markers(&mut self, id: BufferId, text: &str) -> Option<()> {
        self.manager.insert_into_buffer_before_markers(id, text)
    }

    #[inline]
    pub fn insert_lisp_string_into_buffer_before_markers(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        self.manager
            .insert_lisp_string_into_buffer_before_markers(id, text)
    }

    #[inline]
    pub fn delete_buffer_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<()> {
        self.manager.delete_buffer_emacs_byte_range(id, byte_range)
    }

    #[inline]
    pub fn delete_buffer_measured_region(
        &mut self,
        id: BufferId,
        range: TextEditRange,
    ) -> Option<()> {
        self.manager.delete_buffer_measured_region(id, range)
    }

    #[inline]
    pub fn replace_buffer_emacs_byte_range_lisp_string(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        text: &LispString,
    ) -> Option<()> {
        self.manager
            .replace_buffer_emacs_byte_range_lisp_string(id, byte_range, text)
    }

    #[inline]
    pub fn replace_buffer_measured_region_lisp_string(
        &mut self,
        id: BufferId,
        range: TextEditRange,
        text: &LispString,
    ) -> Option<TextExtent> {
        self.manager
            .replace_buffer_measured_region_lisp_string(id, range, text)
    }

    #[doc(hidden)]
    #[inline]
    pub fn casify_replace_buffer_region_with_expansions(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        text: &LispString,
        expansions: &[CasifyExpansion],
        storage_shape: CasifyStorageShape,
        properties: &CasingPropertyMode<'_>,
    ) -> Result<(), Flow> {
        self.manager.casify_replace_buffer_region_with_expansions(
            id,
            byte_range,
            text,
            expansions,
            storage_shape,
            properties,
        )
    }

    #[inline]
    pub fn expose_empty_unibyte_buffer_bytes(&mut self, id: BufferId, bytes: &[u8]) -> Option<()> {
        self.manager.expose_empty_unibyte_buffer_bytes(id, bytes)
    }

    #[inline]
    pub fn seed_default_slot_into_unset_buffers(&mut self, slot: BufferSlot, value: Value) {
        self.manager
            .seed_default_slot_into_unset_buffers(slot, value)
    }

    #[inline]
    pub fn set_default_text_backend_kind(&mut self, kind: ImplementedBufferTextBackendKind) {
        self.manager.set_default_text_backend_kind(kind)
    }

    #[inline]
    pub fn transpose_buffer_regions(
        &mut self,
        id: BufferId,
        transposition: TextTransposition,
        anchor_policy: TranspositionAnchorPolicy,
    ) -> Option<()> {
        self.manager
            .transpose_buffer_regions(id, transposition, anchor_policy)
    }

    #[inline]
    pub fn with_preserved_modification_state<R>(
        &mut self,
        id: BufferId,
        operation: impl FnOnce(BufferManagerMut<'_>) -> R,
    ) -> Option<R> {
        self.manager
            .with_preserved_modification_state(id, |manager| {
                operation(BufferManagerMut::new(manager))
            })
    }

    #[inline]
    pub fn widen_buffer(&mut self, id: BufferId) -> Option<()> {
        self.manager.get_mut(id)?.widen();
        Some(())
    }

    #[inline]
    pub fn widen_and_goto_end(&mut self, id: BufferId) -> Option<()> {
        let buf = self.manager.get_mut(id)?;
        buf.widen();
        let end = buf.point_max_emacs_byte_pos();
        buf.goto_emacs_byte_pos(end);
        Some(())
    }

    #[inline]
    pub fn clear_buffer_text_and_reset_point(&mut self, id: BufferId) -> Option<()> {
        let buf = self.manager.get_mut(id)?;
        buf.widen();
        let len = buf.total_emacs_byte_len().get();
        if len > 0 {
            buf.delete_emacs_byte_range(EmacsByteRange::new(
                EmacsBytePos::ZERO,
                EmacsBytePos::new(len),
            ));
        }
        buf.goto_emacs_byte_pos(EmacsBytePos::ZERO);
        Some(())
    }

    #[inline]
    pub fn append_undo_list_boundary_to_current(&mut self) {
        if let Some(buf) = self.manager.current_buffer_mut() {
            let mut ul = buf.get_undo_list();
            undo_list_boundary(&mut ul);
            buf.set_undo_list(ul);
        }
    }

    #[inline]
    pub fn set_buffer_undo_list(&mut self, id: BufferId, value: Value) -> Option<()> {
        self.manager.get_mut(id)?.set_undo_list(value);
        Some(())
    }

    #[inline]
    pub fn kill_all_buffer_local_variables(
        &mut self,
        id: BufferId,
        obarray: &mut Obarray,
        kill_permanent: bool,
        buffer_defaults: &[Value; BUFFER_SLOT_COUNT],
    ) -> Option<()> {
        self.manager.get_mut(id)?.kill_all_local_variables(
            obarray,
            kill_permanent,
            buffer_defaults,
        );
        Some(())
    }

    #[inline]
    pub fn put_buffer_text_property_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        range: EmacsByteRange,
        prop: Value,
        val: Value,
    ) -> Option<()> {
        self.manager
            .get_mut(id)?
            .text_props_put_property_in_emacs_byte_range(range, prop, val);
        Some(())
    }

    #[inline]
    pub fn set_buffer_slot(&mut self, id: BufferId, slot: BufferSlot, value: Value) -> Option<()> {
        self.manager.get_mut(id)?.slots_mut()[slot.index()] = value;
        Some(())
    }

    #[inline]
    pub fn set_buffer_slot_local(
        &mut self,
        id: BufferId,
        slot: BufferSlot,
        value: Value,
    ) -> Option<()> {
        let buf = self.manager.get_mut(id)?;
        buf.slots_mut()[slot.index()] = value;
        buf.set_slot_local_flag(slot, true);
        Some(())
    }

    #[inline]
    pub(crate) fn register_marker_value_at_emacs_byte_pos(
        &mut self,
        marker: &Value,
        buffer_id: Option<BufferId>,
        byte_pos: Option<EmacsBytePos>,
    ) {
        self.manager
            .register_marker_value_at_emacs_byte_pos(marker, buffer_id, byte_pos);
    }
}

impl Deref for BufferManagerMut<'_> {
    type Target = BufferManager;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.manager
    }
}

/// Non-swappable exclusive capability over an [`EditRuntime`].
///
/// Implements [`Deref`] for read access to [`EditRuntime`], but deliberately
/// omits `DerefMut` and any `&mut EditRuntime` / `&mut BufferManager` unwrap.
/// Mutable buffer access goes through [`EditRuntimeMut::buffers_mut`], which
/// returns a [`BufferManagerMut`].
#[must_use = "dropping an EditRuntimeMut performs no edit mutation"]
#[derive(Debug)]
pub struct EditRuntimeMut<'a> {
    runtime: &'a mut EditRuntime,
}

static_assertions::assert_not_impl_any!(EditRuntimeMut<'static>: Send, Sync, Copy, Clone);

impl<'a> EditRuntimeMut<'a> {
    /// Wrap a mutable [`EditRuntime`] borrow in a non-swappable capability.
    #[doc(hidden)]
    #[inline(always)]
    pub fn new(runtime: &'a mut EditRuntime) -> Self {
        Self { runtime }
    }

    /// Reborrow this capability for a shorter lifetime without exposing
    /// `&mut EditRuntime`.
    #[inline(always)]
    pub fn reborrow(&mut self) -> EditRuntimeMut<'_> {
        EditRuntimeMut {
            runtime: self.runtime,
        }
    }

    /// Mutable capability over the contained [`BufferManager`].
    #[inline(always)]
    pub fn buffers_mut(&mut self) -> BufferManagerMut<'_> {
        BufferManagerMut::new(&mut self.runtime.buffers)
    }

    #[inline]
    pub fn set_combine_after_change_buffer(&mut self, value: Option<BufferId>) {
        self.runtime.set_combine_after_change_buffer(value)
    }

    #[inline]
    pub fn push_combine_after_change_record(&mut self, record: (i64, i64, i64)) {
        self.runtime.combine_after_change_list_mut().push(record);
    }

    #[inline]
    pub fn clear_combine_after_change_list(&mut self) {
        self.runtime.combine_after_change_list_mut().clear();
    }

    #[inline]
    pub fn set_combine_after_change_list(&mut self, value: Vec<(i64, i64, i64)>) {
        self.runtime.set_combine_after_change_list(value)
    }

    #[inline]
    pub fn clear_last_overlay_modification_hooks(&mut self) {
        self.runtime.last_overlay_modification_hooks_mut().clear();
    }

    #[inline]
    pub fn set_last_overlay_modification_hooks(&mut self, value: Vec<OverlayModificationHook>) {
        self.runtime.set_last_overlay_modification_hooks(value)
    }

    #[inline]
    pub fn set_interval_insert_behind_hooks(&mut self, value: Value) {
        self.runtime.set_interval_insert_behind_hooks(value)
    }

    #[inline]
    pub fn set_interval_insert_in_front_hooks(&mut self, value: Value) {
        self.runtime.set_interval_insert_in_front_hooks(value)
    }
}

impl Deref for EditRuntimeMut<'_> {
    type Target = EditRuntime;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.runtime
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_manager_mut_supports_in_place_edits_and_deref_reads() {
        crate::test_utils::init_test_tracing();
        let mut manager = BufferManager::new();
        let mut view = BufferManagerMut::new(&mut manager);

        let id = view.create_buffer("view-test");
        view.set_current(id);
        assert_eq!(view.current_buffer_id(), Some(id));
        assert_eq!(view.replace_buffer_contents(id, "hello world"), Some(()));
        assert_eq!(
            view.goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(5)),
            Some(EmacsBytePos::new(5))
        );
        // SAFETY: in-place read/mutation of the existing buffer slot only.
        let byte_len = unsafe { view.with_buffer_mut_no_swap(id, |b| b.buffer_string().len()) };
        assert_eq!(byte_len, Some(11));

        let buf = view.get(id).expect("buffer exists");
        assert_eq!(buf.buffer_string(), "hello world");
        assert_eq!(buf.point_emacs_byte_pos(), EmacsBytePos::new(5));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "buffer slot was replaced or swapped")]
    fn with_buffer_mut_no_swap_debug_assert_catches_same_id_replace() {
        crate::test_utils::init_test_tracing();
        let mut manager = BufferManager::new();
        let mut view = BufferManagerMut::new(&mut manager);
        let id = view.create_buffer("debug-replace-check");

        // SAFETY: intentionally violating # Safety in a debug-only `#[should_panic]`
        // test to verify the post-closure `debug_assert!` detects same-id replacement.
        unsafe {
            let _ = view.with_buffer_mut_no_swap(id, |buf| {
                let _old =
                    std::mem::replace(buf, Buffer::new_standalone(id, Value::string("fake")));
            });
        }
    }

    #[test]
    fn edit_runtime_mut_forwards_buffers_and_change_hooks() {
        crate::test_utils::init_test_tracing();
        let mut runtime = EditRuntime::new(BufferManager::new());
        let mut edit = EditRuntimeMut::new(&mut runtime);

        let id = edit.buffers_mut().create_buffer("edit-view-test");
        edit.set_combine_after_change_buffer(Some(id));
        edit.push_combine_after_change_record((0, 0, 5));
        edit.set_interval_insert_behind_hooks(Value::T);
        edit.set_interval_insert_in_front_hooks(Value::NIL);

        assert_eq!(edit.combine_after_change_buffer(), Some(id));
        assert_eq!(edit.combine_after_change_list().as_slice(), &[(0, 0, 5)]);
        assert_eq!(edit.interval_insert_behind_hooks(), Value::T);
        assert_eq!(edit.interval_insert_in_front_hooks(), Value::NIL);
        assert!(edit.buffers.get(id).is_some());

        edit.clear_combine_after_change_list();
        assert!(edit.combine_after_change_list().is_empty());
        edit.clear_last_overlay_modification_hooks();
        assert!(edit.last_overlay_modification_hooks().is_empty());
    }
}
