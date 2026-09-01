use crate::display_item::RenderFaceRef;
use neomacs_display_protocol::types::FaceId;

pub(crate) fn render_face_ref_id(face: RenderFaceRef, fallback: FaceId) -> FaceId {
    match face {
        RenderFaceRef::FaceId(face_id) => face_id,
        RenderFaceRef::Inherit => fallback,
    }
}
