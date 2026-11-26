//! Sealed slot for `EntryState::image` (value-gc C2, G1).
//!
//! `EntryState` lives inside `mutator_registration` and holds a `Send + Sync`
//! [`PublishedGcImage`] that the stopped collector later restores or traces.
//!
//! Because `EntryState.image` is declared on `EntryState` in parent module
//! `mutator_registration.rs`, descendant modules can still perform whole-field
//! replacement or `std::mem::swap` on `state.image`. What prevents cross-heap
//! or cross-owner UAF is consumer-side validation: every read/take/mutation
//! consumer ([`Self::as_ref_for`], [`Self::take_for`], [`Self::young_mut_for`])
//! AND installation ([`Self::install`]) checks both `HeapIdentity` and
//! `YoungRegistrationId` and fails closed without mutating the slot or
//! materializing any `MarkWord`.

/// Sealed slot wrapping `Option<PublishedGcImage>`.
pub(super) struct EntryImageSlot(Option<super::PublishedGcImage>);

impl EntryImageSlot {
    #[inline(always)]
    pub(super) const fn empty() -> Self {
        Self(None)
    }

    #[inline(always)]
    pub(super) fn is_some(&self) -> bool {
        self.0.is_some()
    }

    #[inline(always)]
    pub(super) fn is_none(&self) -> bool {
        self.0.is_none()
    }

    #[inline]
    fn check_identity(
        image: &super::PublishedGcImage,
        expected_heap: super::HeapIdentity,
        expected_young: super::YoungRegistrationId,
    ) -> Result<(), super::ImageError> {
        if image.young_id().heap_identity() != expected_heap {
            return Err(super::ImageError::Young(
                super::YoungRegistryError::WrongHeap,
            ));
        }
        if image.young_id() != expected_young {
            return Err(super::ImageError::Young(
                super::YoungRegistryError::UnknownRegistration,
            ));
        }
        Ok(())
    }

    /// Read-only access to the resident image after validating both
    /// `expected_heap` and `expected_young`. Fails closed on mismatch.
    #[inline]
    pub(super) fn as_ref_for(
        &self,
        expected_heap: super::HeapIdentity,
        expected_young: super::YoungRegistrationId,
    ) -> Result<Option<&super::PublishedGcImage>, super::ImageError> {
        let Some(image) = self.0.as_ref() else {
            return Ok(None);
        };
        Self::check_identity(image, expected_heap, expected_young)?;
        Ok(Some(image))
    }

    /// Take the resident image after validating both `expected_heap` and
    /// `expected_young`. Fails closed without removing the image on mismatch.
    #[inline]
    pub(super) fn take_for(
        &mut self,
        expected_heap: super::HeapIdentity,
        expected_young: super::YoungRegistrationId,
    ) -> Result<Option<super::PublishedGcImage>, super::ImageError> {
        let Some(image) = self.0.as_ref() else {
            return Ok(None);
        };
        Self::check_identity(image, expected_heap, expected_young)?;
        Ok(self.0.take())
    }

    /// Narrow mutable access to the resident young-block log after validating
    /// both `expected_heap` and `expected_young`, without exposing
    /// `&mut PublishedGcImage`.
    #[inline]
    pub(super) fn young_mut_for(
        &mut self,
        expected_heap: super::HeapIdentity,
        expected_young: super::YoungRegistrationId,
    ) -> Result<Option<&mut super::YoungBlockLog>, super::ImageError> {
        let Some(image) = self.0.as_ref() else {
            return Ok(None);
        };
        Self::check_identity(image, expected_heap, expected_young)?;
        Ok(self.0.as_mut().map(super::PublishedGcImage::young_mut))
    }

    /// Install `image` into an empty entry slot after verifying both
    /// `expected_heap` and `expected_young`.
    ///
    /// Fails closed on heap-identity mismatch, young-registration mismatch, or
    /// an already-occupied slot, leaving `self` unchanged and returning the
    /// untouched `image` inside [`RestoreImageFailure`] so the caller can
    /// retain or return it conservatively.
    ///
    /// # Safety
    /// `image` must have been published by the owner bound to
    /// `(expected_heap, expected_young)` in the current live era, and every
    /// `MarkWord` stored inside `image` must belong to `expected_heap` and
    /// remain live until restored, collected, or conservatively retained.
    pub(super) unsafe fn install(
        &mut self,
        expected_heap: super::HeapIdentity,
        expected_young: super::YoungRegistrationId,
        image: super::PublishedGcImage,
    ) -> Result<(), super::RestoreImageFailure> {
        if let Err(error) = Self::check_identity(&image, expected_heap, expected_young) {
            return Err(super::RestoreImageFailure { error, image });
        }
        if self.0.is_some() {
            return Err(super::RestoreImageFailure {
                error: super::ImageError::DestinationOccupied,
                image,
            });
        }
        self.0 = Some(image);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::EntryImageSlot;
    use crate::tagged::gc::HeapIdentity;
    use crate::tagged::gc::mutator_registration::image::{ImageError, OwnerGcState};
    use crate::tagged::gc::young_registry::{YoungRegistrationId, YoungRegistryError};

    fn published_owner_image(
        heap: HeapIdentity,
    ) -> (YoungRegistrationId, super::super::PublishedGcImage) {
        let young = YoungRegistrationId::issue_parallel(heap).unwrap();
        let mut owner = OwnerGcState::new(heap, young, 0).unwrap();
        // SAFETY: empty owner state on a fresh synthetic HeapIdentity for unit testing.
        let image = unsafe { owner.publish() }.unwrap();
        (young, image)
    }

    #[test]
    fn install_rejects_wrong_heap_and_wrong_young_id_fail_closed() {
        let heap_a = HeapIdentity::issue();
        let heap_b = HeapIdentity::issue();
        let young_a1 = YoungRegistrationId::issue_parallel(heap_a).unwrap();
        let young_a2 = YoungRegistrationId::issue_parallel(heap_a).unwrap();
        let (young_b, image_b) = published_owner_image(heap_b);

        let mut slot = EntryImageSlot::empty();

        // 1. Wrong HeapIdentity fails closed with WrongHeap and leaves slot empty.
        // SAFETY: unit test exercising the runtime HeapIdentity guard.
        let err_heap = unsafe { slot.install(heap_a, young_a1, image_b) }.unwrap_err();
        assert!(matches!(
            err_heap.error,
            ImageError::Young(YoungRegistryError::WrongHeap)
        ));
        assert_eq!(err_heap.image.young_id(), young_b);
        assert!(slot.is_none());

        // 2. Same HeapIdentity, wrong YoungRegistrationId fails closed with
        //    UnknownRegistration and leaves slot empty.
        let (young_a_actual, image_a) = published_owner_image(heap_a);
        assert_ne!(young_a_actual, young_a2);
        // SAFETY: unit test exercising the runtime YoungRegistrationId guard.
        let err_young = unsafe { slot.install(heap_a, young_a2, image_a) }.unwrap_err();
        assert!(matches!(
            err_young.error,
            ImageError::Young(YoungRegistryError::UnknownRegistration)
        ));
        assert_eq!(err_young.image.young_id(), young_a_actual);
        assert!(slot.is_none());

        // 3. Matching (heap_a, young_a_actual) succeeds.
        // SAFETY: matching synthetic image returned intact from the failed install above.
        unsafe { slot.install(heap_a, young_a_actual, err_young.image) }.unwrap();
        assert!(slot.is_some());
        assert_eq!(
            slot.as_ref_for(heap_a, young_a_actual)
                .unwrap()
                .unwrap()
                .young_id(),
            young_a_actual
        );

        // 4. Second install into occupied slot fails closed with DestinationOccupied
        //    without replacing the resident image.
        let dup_image = {
            let mut dup_owner = OwnerGcState::new(heap_a, young_a_actual, 0).unwrap();
            unsafe { dup_owner.publish() }.unwrap()
        };
        let err_occ = unsafe { slot.install(heap_a, young_a_actual, dup_image) }.unwrap_err();
        assert!(matches!(err_occ.error, ImageError::DestinationOccupied));
        assert!(slot.is_some());
    }

    #[test]
    fn swapped_entry_image_slots_fail_closed_at_every_consumer() {
        let heap_a = HeapIdentity::issue();
        let heap_b = HeapIdentity::issue();
        let (young_a, image_a) = published_owner_image(heap_a);
        let (young_b, image_b) = published_owner_image(heap_b);

        let mut slot_a = EntryImageSlot::empty();
        let mut slot_b = EntryImageSlot::empty();
        // SAFETY: each slot installed with its own matching synthetic image.
        unsafe {
            slot_a.install(heap_a, young_a, image_a).unwrap();
            slot_b.install(heap_b, young_b, image_b).unwrap();
        }

        // Simulate the safe whole-field `std::mem::swap` bypass across heaps.
        std::mem::swap(&mut slot_a, &mut slot_b);

        // Every consumer on slot_a refuses heap_b's image without mutating slot_a.
        assert!(matches!(
            slot_a.as_ref_for(heap_a, young_a),
            Err(ImageError::Young(YoungRegistryError::WrongHeap))
        ));
        assert!(matches!(
            slot_a.young_mut_for(heap_a, young_a),
            Err(ImageError::Young(YoungRegistryError::WrongHeap))
        ));
        assert!(matches!(
            slot_a.take_for(heap_a, young_a),
            Err(ImageError::Young(YoungRegistryError::WrongHeap))
        ));
        assert!(slot_a.is_some(), "failed take_for must leave slot intact");

        // Same-heap cross-owner swap is also refused with UnknownRegistration.
        let (young_a2, image_a2) = published_owner_image(heap_a);
        let mut slot_a2 = EntryImageSlot::empty();
        unsafe { slot_a2.install(heap_a, young_a2, image_a2).unwrap() };
        std::mem::swap(&mut slot_a, &mut slot_b); // restore slot_a -> image_a
        std::mem::swap(&mut slot_a, &mut slot_a2);
        assert!(matches!(
            slot_a.take_for(heap_a, young_a),
            Err(ImageError::Young(YoungRegistryError::UnknownRegistration))
        ));
        assert!(slot_a.is_some());
    }
}
