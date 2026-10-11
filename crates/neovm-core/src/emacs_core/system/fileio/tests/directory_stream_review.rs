use super::*;
use crate::emacs_core::error::{FlowKind, signal};
#[test]
fn retry_checks_quit_without_requiring_host_signals_or_a_race() {
    let mut attempts = 0;
    let mut polls = 0;
    let result = next_with_retry(
        || {
            attempts += 1;
            Err(io::Error::from_raw_os_error(if attempts % 2 == 0 {
                libc::EINTR
            } else {
                libc::EAGAIN
            }))
        },
        || {
            polls += 1;
            if polls == 3 {
                Err(signal("quit", vec![]))
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(attempts, 3);
    assert_eq!(polls, 3);
    match result {
        Err(DirectoryNextError::Quit(flow)) => match flow.into_kind() {
            FlowKind::Signal(s) => assert_eq!(s.symbol_name(), "quit"),
            other => panic!("unexpected {other:?}"),
        },
        _ => panic!("retry should stop at quit"),
    }
}
#[test]
fn retry_distinguishes_end_of_stream_from_permanent_errors() {
    let mut polls = 0;
    assert!(
        next_with_retry(
            || Ok(None),
            || {
                polls += 1;
                Ok(())
            }
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(polls, 0);
    match next_with_retry(
        || Err(io::Error::from_raw_os_error(libc::EIO)),
        || {
            polls += 1;
            Ok(())
        },
    ) {
        Err(DirectoryNextError::Io(err)) => assert_eq!(err.raw_os_error(), Some(libc::EIO)),
        _ => panic!("expected EIO"),
    }
    assert_eq!(polls, 0);
}

#[test]
fn retry_requires_native_errno_instead_of_a_synthesized_error_kind() {
    for kind in [io::ErrorKind::Interrupted, io::ErrorKind::WouldBlock] {
        let mut polls = 0;
        let result = next_with_retry(
            || Err(io::Error::new(kind, "synthetic error without native errno")),
            || {
                polls += 1;
                Ok(())
            },
        );
        match result {
            Err(DirectoryNextError::Io(error)) => {
                assert_eq!(error.kind(), kind);
                assert_eq!(error.raw_os_error(), None);
            }
            other => panic!("non-OS error must remain permanent: {other:?}"),
        }
        assert_eq!(polls, 0);
    }
}

#[test]
fn native_errors_keep_their_sources_and_gnu_opening_data() {
    use std::error::Error;

    let opening = DirectoryReadError::from(io::Error::from_raw_os_error(libc::EACCES));
    let source = opening
        .source()
        .unwrap()
        .downcast_ref::<io::Error>()
        .unwrap();
    assert_eq!(source.raw_os_error(), Some(libc::EACCES));
    let (action, error) = opening.into_parts();
    assert_eq!(action, "Opening directory");
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));

    let reading = DirectoryNextError::from(io::Error::from_raw_os_error(libc::EIO));
    let source = reading
        .source()
        .unwrap()
        .downcast_ref::<io::Error>()
        .unwrap();
    assert_eq!(source.raw_os_error(), Some(libc::EIO));
}
