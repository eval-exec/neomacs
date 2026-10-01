use super::CaptureRoute;
use crate::Frontend;

#[test]
fn gui_launches_are_direct_the_display_session_is_harness_owned() {
    let gui = Frontend::Gui {
        width: 1920,
        height: 1080,
    };
    // Both GUI flavors launch the editor as a direct child of the harness:
    // the bench display session lives in neomacs-infra and is a sibling of
    // the editor, never its parent, so perf wraps only the editor.
    assert_eq!(CaptureRoute::for_frontend(gui, false), CaptureRoute::Direct);
    assert_eq!(CaptureRoute::for_frontend(gui, true), CaptureRoute::Direct);
}

#[test]
fn batch_is_direct_and_tui_names_its_adapter() {
    assert_eq!(
        CaptureRoute::for_frontend(Frontend::Batch, false),
        CaptureRoute::Direct
    );
    assert_eq!(
        CaptureRoute::for_frontend(
            Frontend::Tui {
                rows: 40,
                columns: 120,
            },
            false,
        ),
        CaptureRoute::Adapter("PTY")
    );
}

#[test]
fn direct_native_display_failures_are_not_described_as_adapter_failures() {
    assert_eq!(CaptureRoute::Direct.process_role(), "workload process");
    assert_eq!(CaptureRoute::Adapter("GUI").process_role(), "adapter");
}
