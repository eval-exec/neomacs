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
#[derive(Clone, Copy)]
pub(crate) struct FrameFaces<'a> {
    resolver: &'a FaceResolver,
}

impl<'a> FrameFaces<'a> {
    pub(crate) fn new(resolver: &'a FaceResolver) -> Self {
        Self { resolver }
    }

    /// Bind this frame face environment to one displayed buffer. Capturing the
    /// remapping value here makes window ownership a construction invariant,
    /// rather than a flag every later face lookup must remember to pass.
    pub(crate) fn for_window(self, buffer: &impl LayoutBufferView) -> WindowFaces<'a> {
        WindowFaces {
            frame: self,
            remapping: BufferFaceRemapping::capture(buffer),
        }
    }

    #[cfg(test)]
    pub(crate) fn unremapped_window_for_test(self) -> WindowFaces<'a> {
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
#[derive(Clone, Copy)]
pub(crate) struct WindowFaces<'a> {
    frame: FrameFaces<'a>,
    remapping: BufferFaceRemapping,
}

impl<'a> WindowFaces<'a> {
    /// Low-level renderer plumbing inside `display_row` still consumes the
    /// frame resolver for shaping and asset publication. Keep that escape
    /// hatch visible only to this parent module; buffer/window callers cannot
    /// use it for semantic face lookup.
    pub(super) fn pipeline_resolver(self) -> &'a FaceResolver {
        self.frame.resolver
    }

    pub(crate) fn default_face(self) -> ResolvedFace {
        self.frame
            .resolver
            .resolve_remapped_default_face(self.remapping)
    }

    pub(crate) fn resolve_named_face(self, face_name: &str) -> ResolvedFace {
        self.frame
            .resolver
            .resolve_remapped_named_face(self.remapping, face_name)
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
            .resolve_remapped_face_value_over(self.remapping, base, face_value)
    }
}
