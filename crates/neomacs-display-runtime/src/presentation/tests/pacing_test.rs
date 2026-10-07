use super::*;
#[test]
fn latest_frame_mode_is_selected_only_when_supported() {
    use wgpu::PresentMode::*;
    assert_eq!(present_mode(&[Fifo, Immediate, Mailbox]), Mailbox);
    assert_eq!(present_mode(&[Fifo, Immediate]), Fifo);
    assert_eq!(present_mode(&[Fifo]), Fifo);
}
