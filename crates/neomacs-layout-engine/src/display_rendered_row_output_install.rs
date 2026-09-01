use crate::display_row::measured_state::{
    DisplayRowOwner, FrameChromeKind, MeasuredDisplayRow, WindowChromeKind,
};
use crate::output::builder::{DisplayOutputBuilder, FRAME_CHROME_WINDOW_ID};
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_chrome::ChromeDisplayRow;

pub(crate) fn install_measured_window_display_row(
    builder: &mut DisplayOutputBuilder,
    measured: &MeasuredDisplayRow,
) {
    MeasuredWindowDisplayRowInstallRequest { measured }.install(builder);
}

pub(crate) fn install_measured_frame_chrome_display_row(
    builder: &mut DisplayOutputBuilder,
    measured: &MeasuredDisplayRow,
) {
    MeasuredFrameChromeAssetsInstallRequest { measured }.install(builder);
}

pub(crate) fn frame_chrome_display_row(measured: &MeasuredDisplayRow) -> ChromeDisplayRow {
    let mut row = measured.frame_chrome_output_row();
    crate::display_row::finalizer::GlyphRowFinalizationContext::new(
        FRAME_CHROME_WINDOW_ID as u64,
        measured.row_index() as usize,
        measured.bounds(),
    )
    .finalize_row(&mut row, 0, None);
    // Media is embedded in the authoritative GlyphRow and materializes through
    // the same row walk as text.  Chrome no longer owns a second placement list.
    ChromeDisplayRow::new(row)
}

pub(crate) fn install_rendered_display_row_fragment_assets(
    builder: &mut DisplayOutputBuilder,
    faces: &[Face],
) {
    install_faces(builder, faces);
}

struct MeasuredWindowDisplayRowInstallRequest<'a> {
    measured: &'a MeasuredDisplayRow,
}

impl MeasuredWindowDisplayRowInstallRequest<'_> {
    fn install(self, builder: &mut DisplayOutputBuilder) {
        let measured = self.measured;
        let DisplayRowOwner::WindowChrome { window_id, kind } = measured.owner() else {
            panic!("frame chrome rows must install through frame chrome rows");
        };
        debug_assert!(window_id > 0);
        debug_assert_eq!(builder.current_window_id_i64(), window_id as i64);
        debug_assert!(matches!(
            kind,
            WindowChromeKind::TabLine | WindowChromeKind::HeaderLine | WindowChromeKind::ModeLine
        ));
        let display_row_index = measured.row_index() as usize;
        install_faces(builder, measured.rendered().faces());
        let row = measured.window_relative_output_row(builder.current_window_pixel_bounds());
        builder.install_output_row_lifecycle(
            crate::output::row_request::OutputRowLifecycleRequest::complete(
                display_row_index,
                row.role,
                row.mode_line,
                row,
            ),
        );
    }
}

struct MeasuredFrameChromeAssetsInstallRequest<'a> {
    measured: &'a MeasuredDisplayRow,
}

impl MeasuredFrameChromeAssetsInstallRequest<'_> {
    fn install(self, builder: &mut DisplayOutputBuilder) {
        let measured = self.measured;
        let DisplayRowOwner::FrameChrome { kind } = measured.owner() else {
            panic!("window-owned rows must install through window chrome");
        };
        debug_assert!(matches!(kind, FrameChromeKind::TabBar));
        install_faces(builder, measured.rendered().faces());
    }
}

fn install_faces(builder: &mut DisplayOutputBuilder, faces: &[Face]) {
    for face in faces {
        builder.publish_output_face(face.id, face.clone());
    }
}
