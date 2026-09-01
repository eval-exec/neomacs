use crate::display_origin::DisplayOrigin;
use neomacs_display_protocol::face::BasicFaceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BaseFacePolicy {
    BufferFaceIncludingOverlays,
    OverlayStringAtAnchor,
    DisplayPropertyUnderlyingFace,
    /// A window-owned basic face whose effective definition can be replaced
    /// buffer-locally through `face-remapping-alist`.
    BufferRemappedBasicFace(BasicFaceId),
    /// A frame-owned basic face with no associated buffer remapping context.
    FrameBasicFace(BasicFaceId),
}

impl From<DisplayOrigin> for BaseFacePolicy {
    fn from(origin: DisplayOrigin) -> Self {
        match origin {
            DisplayOrigin::BufferText { .. } => Self::BufferFaceIncludingOverlays,
            DisplayOrigin::OverlayString { .. } => Self::OverlayStringAtAnchor,
            DisplayOrigin::DisplayPropertyString { .. } => Self::DisplayPropertyUnderlyingFace,
            DisplayOrigin::LinePrefix { .. } | DisplayOrigin::WrapPrefix { .. } => {
                Self::BufferRemappedBasicFace(BasicFaceId::Default)
            }
            DisplayOrigin::ModeLine { selected } => Self::BufferRemappedBasicFace(if selected {
                BasicFaceId::ModeLineActive
            } else {
                BasicFaceId::ModeLineInactive
            }),
            DisplayOrigin::HeaderLine { selected } => Self::BufferRemappedBasicFace(if selected {
                BasicFaceId::HeaderLineActive
            } else {
                BasicFaceId::HeaderLineInactive
            }),
            DisplayOrigin::TabLine => Self::BufferRemappedBasicFace(BasicFaceId::TabLine),
            DisplayOrigin::TabBar => Self::FrameBasicFace(BasicFaceId::TabBar),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomacs_display_protocol::face::BasicFaceId;
    use neovm_core::buffer::CharPos0;
    use neovm_core::emacs_core::Value;

    #[test]
    fn base_face_policy_derives_from_buffer_text_origin() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::BufferText {
                charpos: CharPos0::new(3),
            }),
            BaseFacePolicy::BufferFaceIncludingOverlays
        );
    }

    #[test]
    fn base_face_policy_derives_from_overlay_string_origin() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::OverlayString {
                overlay_id: Value::fixnum(1),
                anchor_charpos: CharPos0::new(4),
                kind: crate::display_origin::OverlayStringKind::Before,
            }),
            BaseFacePolicy::OverlayStringAtAnchor
        );
    }

    #[test]
    fn base_face_policy_derives_from_display_property_string_origin() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::DisplayPropertyString {
                anchor_charpos: CharPos0::new(5),
                source: crate::display_origin::DisplayPropertySource::TextProperty,
            }),
            BaseFacePolicy::DisplayPropertyUnderlyingFace
        );
    }

    #[test]
    fn base_face_policy_derives_from_prefix_origins() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::LinePrefix {
                anchor_charpos: CharPos0::new(6),
            }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default)
        );
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::WrapPrefix {
                anchor_charpos: CharPos0::new(7),
            }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default)
        );
    }

    #[test]
    fn base_face_policy_derives_from_mode_line_origin() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::ModeLine { selected: true }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::ModeLineActive)
        );
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::ModeLine { selected: false }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::ModeLineInactive)
        );
    }

    #[test]
    fn base_face_policy_derives_from_header_line_origin() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::HeaderLine { selected: true }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::HeaderLineActive)
        );
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::HeaderLine { selected: false }),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::HeaderLineInactive)
        );
    }

    #[test]
    fn base_face_policy_derives_from_tab_line_and_tab_bar_origins() {
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::TabLine),
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::TabLine)
        );
        assert_eq!(
            BaseFacePolicy::from(DisplayOrigin::TabBar),
            BaseFacePolicy::FrameBasicFace(BasicFaceId::TabBar)
        );
    }
}
