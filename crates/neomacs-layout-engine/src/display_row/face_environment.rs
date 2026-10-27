//! Ownership-scoped face resolution for display layout.
//!
//! GNU's face lookup is window-aware for anything rendered on behalf of a
//! displayed buffer: `face-remapping-alist` participates in named-face and
//! inheritance lookup. Frame-owned UI must use the canonical frame namespace
//! instead. These capabilities make that ownership choice at construction
//! time and keep the raw resolver private afterward.

use crate::neovm_bridge::{BufferFaceRemapping, FaceResolver, LayoutBufferView, ResolvedFace};
use neovm_core::emacs_core::Value;

/// Face capability for frame-owned chrome.
pub(crate) struct FrameFaces<'a, FS: crate::neovm_bridge::FaceServices = FaceResolver> {
    resolver: &'a FS,
}

impl<'a, FS: crate::neovm_bridge::FaceServices> FrameFaces<'a, FS> {
    pub(crate) fn new(resolver: &'a FS) -> Self {
        Self { resolver }
    }

    /// Bind this frame face environment to one displayed buffer. Capturing the
    /// remapping value here makes window ownership a construction invariant,
    /// rather than a flag every later face lookup must remember to pass.
    pub(crate) fn for_window(self, buffer: &impl LayoutBufferView) -> WindowFaces<'a, FS> {
        WindowFaces {
            frame: self,
            remapping: BufferFaceRemapping::capture(buffer),
        }
    }

    #[cfg(test)]
    pub(crate) fn unremapped_window_for_test(self) -> WindowFaces<'a, FS> {
        WindowFaces {
            frame: self,
            remapping: BufferFaceRemapping::empty(),
        }
    }
}

/// Face capability for anything owned by a displayed window's buffer.
///
/// Consumers can ask GNU-shaped questions (`default`, named lookup, or
/// merge-over-base), but cannot accidentally use the frame-only namespace and
/// bypass `face-remapping-alist`.
pub(crate) struct WindowFaces<'a, FS: crate::neovm_bridge::FaceServices = FaceResolver> {
    frame: FrameFaces<'a, FS>,
    remapping: BufferFaceRemapping,
}

impl<'a, FS: crate::neovm_bridge::FaceServices> WindowFaces<'a, FS> {
    /// Low-level renderer plumbing inside `display_row` still consumes the
    /// frame resolver for shaping and asset publication. Keep that escape
    /// hatch visible only to this parent module; buffer/window callers cannot
    /// use it for semantic face lookup.
    pub(super) fn pipeline_resolver(self) -> &'a FaceResolver {
        self.frame.resolver.resolver()
    }

    pub(super) fn pipeline_services(self) -> &'a FS {
        self.frame.resolver
    }

    pub(crate) fn default_face(self) -> ResolvedFace {
        self.frame
            .resolver
            .resolver()
            .resolve_remapped_default_face(self.remapping)
    }

    pub(crate) fn resolve_named_face(self, face_name: &str) -> ResolvedFace {
        self.frame
            .resolver
            .resolver()
            .resolve_remapped_named_face(self.remapping, face_name)
    }

    pub(crate) fn resolve_fixed_face(
        self,
        handle: neovm_core::emacs_core::intern::WellKnown,
    ) -> ResolvedFace {
        self.frame
            .resolver
            .resolver()
            .resolve_remapped_fixed_face(self.remapping, handle)
    }

    pub(crate) fn merge_named_face_over(
        self,
        base: &ResolvedFace,
        face_name: &str,
    ) -> ResolvedFace {
        self.resolve_face_value_over(base, &Value::symbol(face_name))
            .unwrap_or_else(|| base.clone())
    }

    pub(crate) fn resolve_face_value_over(
        self,
        base: &ResolvedFace,
        face_value: &Value,
    ) -> Option<ResolvedFace> {
        self.frame
            .resolver
            .resolver()
            .resolve_remapped_face_value_over(self.remapping, base, face_value)
    }
}

impl<FS: crate::neovm_bridge::FaceServices> Copy for FrameFaces<'_, FS> {}
impl<FS: crate::neovm_bridge::FaceServices> Clone for FrameFaces<'_, FS> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<FS: crate::neovm_bridge::FaceServices> Copy for WindowFaces<'_, FS> {}
impl<FS: crate::neovm_bridge::FaceServices> Clone for WindowFaces<'_, FS> {
    fn clone(&self) -> Self {
        *self
    }
}
