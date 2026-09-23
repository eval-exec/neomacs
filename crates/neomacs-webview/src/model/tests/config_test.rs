use std::path::{Path, PathBuf};

use super::{WebViewFrameTransport, frame_transport_from_source, profile_root_from_sources};

#[cfg(target_os = "linux")]
struct TestDmaBufLease;

#[cfg(target_os = "linux")]
impl super::DmaBufLease for TestDmaBufLease {
    fn request_pixels(&self) {}
}

#[test]
fn rejected_frame_requests_pixels_before_releasing_its_native_lease() {
    use std::sync::{Arc, Mutex};
    struct Lease(Arc<Mutex<Vec<&'static str>>>);
    impl super::DmaBufLease for Lease {
        fn request_pixels(&self) {
            self.0.lock().unwrap().push("pixels");
        }
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("release");
        }
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let frame = super::DmaBufFrame::new(
        Vec::new(),
        None,
        0,
        0,
        1,
        1,
        super::DmaBufFrameLease::new(Lease(events.clone())),
    );
    frame.request_pixel_fallback();
    assert_eq!(*events.lock().unwrap(), ["pixels", "release"]);
}

#[test]
fn explicit_profile_root_wins_over_platform_directories() {
    assert_eq!(
        profile_root_from_sources(
            Some(PathBuf::from("/explicit/webview")),
            Some(PathBuf::from("/data")),
            Some(PathBuf::from("/home/user")),
            Path::new(".local/share/neomacs/webview"),
        ),
        Some(PathBuf::from("/explicit/webview"))
    );
}

#[test]
fn relative_environment_directories_cannot_make_profiles_cwd_dependent() {
    assert_eq!(
        profile_root_from_sources(
            None,
            Some(PathBuf::from("relative-data")),
            Some(PathBuf::from("/home/user")),
            Path::new(".local/share/neomacs/webview"),
        ),
        Some(PathBuf::from("/home/user/.local/share/neomacs/webview"))
    );
}

#[test]
fn frame_transport_configuration_is_parsed_into_a_closed_enum() {
    assert_eq!(
        frame_transport_from_source(Some("dmabuf")),
        WebViewFrameTransport::DmaBuf
    );
    assert_eq!(
        frame_transport_from_source(Some("pixels")),
        WebViewFrameTransport::SoftwarePixels
    );
    assert_eq!(
        frame_transport_from_source(Some("auto")),
        WebViewFrameTransport::Auto
    );
    assert_eq!(
        frame_transport_from_source(Some("future-unknown-value")),
        WebViewFrameTransport::Auto
    );
}

#[cfg(target_os = "linux")]
#[test]
fn dma_buf_readiness_is_gated_by_the_producer_fence() {
    use std::fs::File;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::time::Duration;

    let mut descriptors = [-1; 2];
    // SAFETY: `descriptors` has the two slots required by `pipe`.
    assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
    // SAFETY: `pipe` initialized both descriptors, each of which moves
    // into exactly one RAII owner.
    let read_end = unsafe { File::from_raw_fd(descriptors[0]) };
    let write_end = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
    let frame = super::DmaBufFrame::new(
        Vec::new(),
        Some(read_end),
        0,
        0,
        1,
        1,
        super::DmaBufFrameLease::new(TestDmaBufLease),
    );

    assert_eq!(
        frame.wait_until_ready(Duration::ZERO).unwrap(),
        super::DmaBufReadiness::TimedOut
    );
    let byte = [1u8];
    // SAFETY: the write descriptor is live and `byte` is a one-byte buffer.
    assert_eq!(
        unsafe { libc::write(write_end.as_raw_fd(), byte.as_ptr().cast(), byte.len()) },
        1
    );
    assert_eq!(
        frame.wait_until_ready(Duration::ZERO).unwrap(),
        super::DmaBufReadiness::Ready
    );
}
